//! OpenAPI 3.0 / 3.1 import (PLAN.md §11.1).
//!
//! The spec is parsed into a generic `serde_json::Value` and walked by hand instead of using a
//! typed OpenAPI crate: that handles 3.0 and 3.1 with one code path and tolerates the small
//! mistakes real-world specs contain. (A typed model is like a strict JAXB binding; this is
//! more like walking a Jackson `JsonNode`.)
//!
//! [`map_openapi`] only computes what to create (an [`ImportPlan`], easy to snapshot-test);
//! [`write_plan`] writes it to disk.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;

use crate::error::{CoreError, Result};
use crate::model::{
    ApiKeyLocation, Auth, AuthScheme, Body, BodyMode, Collection, EnvVariable, Environment, Folder,
    KeyValue, MultipartField, MultipartKind, OAuth2Config, OAuth2Grant, PathParam, Request,
};
use crate::storage;

/// Generated example bodies stop at this nesting depth (recursive schemas would never end).
const MAX_DEPTH: usize = 8;

/// The HTTP methods an OpenAPI path item can have, in the order they are imported.
const METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

/// What the import created, shown to the user afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub collection_name: String,
    pub requests: u32,
    pub folders: u32,
    pub environments: u32,
    /// The `baseUrl` of the created `default` environment (empty when not applicable).
    pub base_url: String,
    /// Secret variables created without a value, for the user to fill in.
    pub secrets_to_fill: Vec<String>,
    pub warnings: Vec<String>,
}

/// How imported operations are put into folders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiGrouping {
    /// One nested folder per static path segment, like Postman and Bruno:
    /// `/legs/resolve/{legNumber}` goes into `legs/resolve`. Path parameters get no folder.
    #[default]
    Paths,
    /// One folder per operation tag (its first tag); untagged operations go to the root.
    Tags,
}

/// A folder to create, with its requests in order and its subfolders.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedFolder {
    pub folder: Folder,
    pub requests: Vec<Request>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<PlannedFolder>,
}

impl PlannedFolder {
    fn new(name: String) -> Self {
        Self {
            folder: Folder::new(name),
            requests: Vec::new(),
            folders: Vec::new(),
        }
    }

    /// This folder and all folders below it.
    fn count(&self) -> usize {
        1 + self.folders.iter().map(PlannedFolder::count).sum::<usize>()
    }

    /// The requests in this folder and all folders below it.
    fn request_count(&self) -> usize {
        self.requests.len()
            + self
                .folders
                .iter()
                .map(PlannedFolder::request_count)
                .sum::<usize>()
    }
}

/// Everything an import will create.
#[derive(Debug, Clone, Serialize)]
pub struct ImportPlan {
    pub collection: Collection,
    pub environment: Environment,
    pub folders: Vec<PlannedFolder>,
    /// Operations at the collection root (untagged ones, or `/` when grouping by path).
    pub requests: Vec<Request>,
    pub summary: ImportSummary,
}

/// Reads an OpenAPI 3.x document (YAML or JSON) and plans the collection for it.
pub fn map_openapi(text: &str, grouping: OpenApiGrouping) -> Result<ImportPlan> {
    let root = parse(text)?;
    let version = root.get("openapi").and_then(Value::as_str).unwrap_or("");
    if !version.starts_with("3.") {
        let what = if root.get("swagger").is_some() {
            "Swagger 2.0 is not supported; convert it to OpenAPI 3 first (e.g. with swagger-converter)."
                .to_string()
        } else {
            format!("this is not an OpenAPI 3 document (openapi: {version:?})")
        };
        return Err(CoreError::InvalidInput(what));
    }
    let mut mapper = Mapper {
        root: &root,
        warnings: Vec::new(),
        env_vars: Vec::new(),
    };
    Ok(mapper.plan(grouping))
}

/// Writes `plan` as a new collection in `dir` (which may exist but must not be a collection).
pub fn write_plan(dir: &Path, plan: &ImportPlan) -> Result<()> {
    let created = storage::create_collection(dir, &plan.collection.name)?;
    let collection = Collection {
        id: created.id,
        ..plan.collection.clone()
    };
    storage::save_collection(dir, &collection)?;

    storage::create_environment(dir, &plan.environment.name)?;
    storage::save_environment(dir, &plan.environment.name, &plan.environment)?;

    for request in &plan.requests {
        storage::create_request(dir, "", request)?;
    }
    write_folders(dir, "", &plan.folders)
}

/// Creates `folders` (and their subfolders, recursively) inside the folder `parent`.
fn write_folders(dir: &Path, parent: &str, folders: &[PlannedFolder]) -> Result<()> {
    for planned in folders {
        let rel = storage::create_folder(dir, parent, &planned.folder.name)?;
        if !planned.folder.docs.is_empty() {
            let mut folder = storage::read_folder(dir, &rel)?;
            folder.docs = planned.folder.docs.clone();
            storage::save_folder(dir, &rel, &folder)?;
        }
        for request in &planned.requests {
            storage::create_request(dir, &rel, request)?;
        }
        write_folders(dir, &rel, &planned.folders)?;
    }
    Ok(())
}

/// Reads a spec file and shows what importing it would create, without writing anything.
pub fn preview_openapi_file(file: &Path, grouping: OpenApiGrouping) -> Result<ImportSummary> {
    let text = std::fs::read_to_string(file).map_err(|e| CoreError::io(file, e))?;
    Ok(map_openapi(&text, grouping)?.summary)
}

/// Imports a spec file as a new collection and adds it to the workspace. `target` is the
/// folder for the collection; by default `<workspace>/collections/<title>`.
pub fn import_openapi_file(
    workspace_dir: &Path,
    file: &Path,
    target: Option<&Path>,
    grouping: OpenApiGrouping,
) -> Result<(std::path::PathBuf, ImportSummary)> {
    let text = std::fs::read_to_string(file).map_err(|e| CoreError::io(file, e))?;
    let plan = map_openapi(&text, grouping)?;
    let dir = match target {
        Some(dir) => dir.to_path_buf(),
        None => storage::new_collection_dir(workspace_dir, &plan.collection.name)?,
    };
    write_plan(&dir, &plan)?;
    storage::add_collection(workspace_dir, &dir)?;
    tracing::info!(requests = plan.summary.requests, "OpenAPI spec imported");
    Ok((dir, plan.summary))
}

fn parse(text: &str) -> Result<Value> {
    let trimmed = text.trim_start();
    let parsed = if trimmed.starts_with('{') {
        serde_json::from_str(trimmed).map_err(|e| e.to_string())
    } else {
        serde_saphyr::from_str::<Value>(text).map_err(|e| e.to_string())
    };
    parsed.map_err(|e| CoreError::InvalidInput(format!("could not read the OpenAPI file: {e}")))
}

/// Walks one document. Holds the warnings and the environment variables found on the way.
struct Mapper<'a> {
    root: &'a Value,
    warnings: Vec<String>,
    /// Variables for the `default` environment (besides `baseUrl`), e.g. auth credentials.
    env_vars: Vec<EnvVariable>,
}

/// An operation found under `paths`, before it is placed in a folder.
struct Operation {
    /// The path as written in the spec, e.g. `/legs/{legNumber}`.
    path: String,
    tag: Option<String>,
    request: Request,
}

impl<'a> Mapper<'a> {
    fn plan(&mut self, grouping: OpenApiGrouping) -> ImportPlan {
        let info = self.root.get("info").unwrap_or(&Value::Null);
        let name = str_of(info, "title")
            .unwrap_or("Imported API")
            .trim()
            .to_string();

        let mut collection = Collection::new(name.clone());
        collection.docs = str_of(info, "description").unwrap_or("").to_string();
        let global_security = self.root.get("security");
        collection.auth = match global_security {
            Some(requirements) => self.auth_for(requirements, "the API"),
            None => self.fallback_auth(),
        };

        let base_url = self.base_url();
        let operations = self.operations(global_security);

        let (folders, root_requests) = match grouping {
            OpenApiGrouping::Tags => self.group_by_tag(operations),
            OpenApiGrouping::Paths => group_by_path(operations),
        };

        let mut environment = Environment::new("default");
        environment.variables.push(EnvVariable {
            name: "baseUrl".into(),
            value: Some(base_url.clone()),
            secret: false,
            source: None,
            enabled: true,
        });
        environment.variables.append(&mut self.env_vars);

        let requests = root_requests.len()
            + folders
                .iter()
                .map(PlannedFolder::request_count)
                .sum::<usize>();
        let summary = ImportSummary {
            collection_name: name,
            requests: u32::try_from(requests).unwrap_or(u32::MAX),
            folders: u32::try_from(folders.iter().map(PlannedFolder::count).sum::<usize>())
                .unwrap_or(u32::MAX),
            environments: 1,
            base_url,
            secrets_to_fill: environment
                .variables
                .iter()
                .filter(|v| v.secret)
                .map(|v| v.name.clone())
                .collect(),
            warnings: dedup(std::mem::take(&mut self.warnings)),
        };
        ImportPlan {
            collection,
            environment,
            folders,
            requests: root_requests,
            summary,
        }
    }

    /// One folder per tag, in the order of the top-level `tags` list, then first use.
    fn group_by_tag(&self, operations: Vec<Operation>) -> (Vec<PlannedFolder>, Vec<Request>) {
        let mut tag_order: Vec<String> = self
            .root
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| str_of(t, "name").map(str::to_string))
            .collect();
        for op in &operations {
            if let Some(tag) = &op.tag
                && !tag_order.contains(tag)
            {
                tag_order.push(tag.clone());
            }
        }
        let mut folders: Vec<PlannedFolder> = tag_order
            .iter()
            .map(|tag| {
                let mut folder = PlannedFolder::new(tag.clone());
                folder.folder.docs = self.tag_description(tag);
                folder
            })
            .collect();
        let mut root_requests = Vec::new();
        for op in operations {
            match op.tag {
                Some(tag) => {
                    let index = tag_order
                        .iter()
                        .position(|t| *t == tag)
                        .expect("tag listed");
                    folders[index].requests.push(op.request);
                }
                None => root_requests.push(op.request),
            }
        }
        folders.retain(|f| !f.requests.is_empty());
        (folders, root_requests)
    }

    fn tag_description(&self, tag: &str) -> String {
        self.root
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|t| str_of(t, "name") == Some(tag))
            .and_then(|t| str_of(t, "description"))
            .unwrap_or("")
            .to_string()
    }

    /// `servers[0].url` with its variables replaced by their defaults.
    fn base_url(&mut self) -> String {
        let Some(server) = self
            .root
            .get("servers")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
        else {
            self.warnings
                .push("The spec has no servers; set baseUrl in the default environment.".into());
            return "http://localhost".into();
        };
        let mut url = str_of(server, "url").unwrap_or("/").to_string();
        if let Some(vars) = server.get("variables").and_then(Value::as_object) {
            for (name, var) in vars {
                let default = var.get("default").map(plain).unwrap_or_default();
                url = url.replace(&format!("{{{name}}}"), &default);
            }
        }
        let url = url.trim_end_matches('/').to_string();
        if !url.contains("://") {
            self.warnings.push(format!(
                "The server URL {url:?} is relative; add the host to baseUrl in the default environment."
            ));
        }
        url
    }

    fn operations(&mut self, global_security: Option<&Value>) -> Vec<Operation> {
        let mut out = Vec::new();
        let Some(paths) = self.root.get("paths").and_then(Value::as_object) else {
            self.warnings.push("The spec has no paths.".into());
            return out;
        };
        for (path, item) in paths {
            let item = self.resolve(item);
            let shared_params: Vec<Value> = array_of(&item, "parameters")
                .iter()
                .map(|p| self.resolve(p))
                .collect();
            for method in METHODS {
                let Some(op) = item.get(method) else { continue };
                let tag = op
                    .get("tags")
                    .and_then(Value::as_array)
                    .and_then(|tags| tags.first())
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let op = op.clone();
                let request = self.request(path, method, &op, &shared_params, global_security);
                out.push(Operation {
                    path: path.clone(),
                    tag,
                    request,
                });
            }
        }
        out
    }

    fn request(
        &mut self,
        path: &str,
        method: &str,
        op: &Value,
        shared_params: &[Value],
        global_security: Option<&Value>,
    ) -> Request {
        let upper = method.to_uppercase();
        let name = str_of(op, "summary")
            .or_else(|| str_of(op, "operationId"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{upper} {path}"));
        let mut request = Request::new(name);
        request.method = upper.clone();

        // `{id}` → `:id`, Frida's path parameter syntax.
        let frida_path = path
            .split('/')
            .map(
                |seg| match seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    Some(name) => format!(":{name}"),
                    None => seg.to_string(),
                },
            )
            .collect::<Vec<_>>()
            .join("/");

        // Operation parameters override path-level ones with the same name and location.
        let mut params: Vec<Value> = shared_params.to_vec();
        for p in array_of(op, "parameters") {
            let p = self.resolve(p);
            params.retain(|q| !(q.get("name") == p.get("name") && q.get("in") == p.get("in")));
            params.push(p);
        }
        for p in &params {
            let name = str_of(p, "name").unwrap_or("").to_string();
            let required = p.get("required").and_then(Value::as_bool).unwrap_or(false);
            let value = self.param_value(p);
            match str_of(p, "in") {
                Some("path") => request.params.path.push(PathParam { name, value }),
                Some("query") => request.params.query.push(KeyValue {
                    name,
                    value,
                    enabled: required,
                }),
                Some("header") => {
                    // OpenAPI says these three are described elsewhere and must be ignored.
                    let lower = name.to_lowercase();
                    if !matches!(lower.as_str(), "accept" | "content-type" | "authorization") {
                        request.headers.push(KeyValue {
                            name,
                            value,
                            enabled: required,
                        });
                    }
                }
                Some("cookie") => self.warnings.push(format!(
                    "{upper} {path}: cookie parameter {name:?} was skipped (set it as a Cookie header if needed)."
                )),
                _ => {}
            }
        }

        let query: Vec<String> = request
            .params
            .query
            .iter()
            .filter(|q| q.enabled)
            .map(|q| format!("{}={}", q.name, q.value))
            .collect();
        request.url = format!("{{{{baseUrl}}}}{frida_path}");
        if !query.is_empty() {
            request.url = format!("{}?{}", request.url, query.join("&"));
        }

        if let Some(body) = op.get("requestBody") {
            let body = self.resolve(body);
            self.body(&mut request, &body, &format!("{upper} {path}"));
        }

        // Operation-level security overrides the API-wide one.
        if let Some(security) = op.get("security")
            && Some(security) != global_security
        {
            request.auth = self.auth_for(security, &format!("{upper} {path}"));
        }

        let mut docs = Vec::new();
        if op.get("deprecated").and_then(Value::as_bool) == Some(true) {
            docs.push("**Deprecated.**".to_string());
        }
        if let Some(description) = str_of(op, "description") {
            docs.push(description.trim().to_string());
        }
        if let Some(id) = str_of(op, "operationId") {
            docs.push(format!("Operation id: `{id}`"));
        }
        request.docs = docs.join("\n\n");
        request
    }

    /// A parameter's example, default or first enum value; empty when none is given.
    fn param_value(&mut self, param: &Value) -> String {
        if let Some(example) = param.get("example") {
            return plain(example);
        }
        if let Some(first) = param
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|m| m.values().next())
        {
            let example = self.resolve(first);
            if let Some(value) = example.get("value") {
                return plain(value);
            }
        }
        let schema = self.resolve(param.get("schema").unwrap_or(&Value::Null));
        let schema = match schema.get("items") {
            Some(items) if schema.get("type").and_then(Value::as_str) == Some("array") => {
                self.resolve(items)
            }
            _ => schema,
        };
        for key in ["example", "default"] {
            if let Some(v) = schema.get(key) {
                return plain(v);
            }
        }
        schema
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
            .map(plain)
            .unwrap_or_default()
    }

    fn body(&mut self, request: &mut Request, body: &Value, where_: &str) {
        let Some(content) = body.get("content").and_then(Value::as_object) else {
            return;
        };
        let is_json = |ct: &str| ct == "application/json" || ct.ends_with("+json");
        // Prefer the media types Frida edits best.
        let pick = content
            .keys()
            .find(|ct| is_json(ct))
            .or_else(|| {
                content
                    .keys()
                    .find(|ct| *ct == "application/x-www-form-urlencoded")
            })
            .or_else(|| content.keys().find(|ct| *ct == "multipart/form-data"))
            .or_else(|| {
                content
                    .keys()
                    .find(|ct| ct.ends_with("/xml") || ct.ends_with("+xml"))
            })
            .or_else(|| content.keys().find(|ct| ct.starts_with("text/")))
            .or_else(|| content.keys().next())
            .cloned();
        let Some(content_type) = pick else { return };
        let media = self.resolve(&content[&content_type]);
        let schema = media.get("schema").cloned().unwrap_or(Value::Null);
        let explicit = self.media_example(&media);

        let mut b = Body::default();
        if is_json(&content_type) {
            b.mode = BodyMode::Json;
            let value = explicit.unwrap_or_else(|| self.example(&schema, 0, &mut Vec::new()));
            b.json = serde_json::to_string_pretty(&value).unwrap_or_default();
            if content_type != "application/json" {
                request
                    .headers
                    .push(KeyValue::new("Content-Type", content_type.clone()));
            }
        } else if content_type == "application/x-www-form-urlencoded"
            || content_type == "multipart/form-data"
        {
            let value = explicit.unwrap_or_else(|| self.example(&schema, 0, &mut Vec::new()));
            let schema = self.resolve(&schema);
            let fields = value.as_object().cloned().unwrap_or_default();
            if content_type == "multipart/form-data" {
                b.mode = BodyMode::Multipart;
                b.multipart = fields
                    .iter()
                    .map(|(name, v)| {
                        let prop = schema.get("properties").and_then(|p| p.get(name));
                        let is_file = prop
                            .and_then(|p| str_of(p, "format"))
                            .is_some_and(|f| f == "binary" || f == "byte");
                        MultipartField {
                            name: name.clone(),
                            kind: if is_file {
                                MultipartKind::File
                            } else {
                                MultipartKind::Text
                            },
                            value: if is_file { String::new() } else { plain(v) },
                            enabled: true,
                        }
                    })
                    .collect();
            } else {
                b.mode = BodyMode::FormUrlencoded;
                b.form_url_encoded = fields
                    .iter()
                    .map(|(name, v)| KeyValue::new(name.clone(), plain(v)))
                    .collect();
            }
        } else if content_type.ends_with("/xml") || content_type.ends_with("+xml") {
            b.mode = BodyMode::Xml;
            match explicit {
                Some(Value::String(xml)) => b.xml = xml,
                _ => {
                    let value = self.example(&schema, 0, &mut Vec::new());
                    let root = schema_name(&schema).unwrap_or("root");
                    b.xml = to_xml(root, &value, 0);
                }
            }
        } else if content_type.starts_with("text/") {
            b.mode = BodyMode::Text;
            b.text = explicit.map(|v| plain(&v)).unwrap_or_default();
        } else {
            b.mode = BodyMode::Binary;
            self.warnings.push(format!(
                "{where_}: the {content_type} body needs a file; choose it in the Body tab."
            ));
        }
        if content.len() > 1 {
            // Not a warning: just which one was picked.
            tracing::debug!(%where_, %content_type, "picked one of several request media types");
        }
        request.body = b;
    }

    fn media_example(&mut self, media: &Value) -> Option<Value> {
        if let Some(example) = media.get("example") {
            return Some(example.clone());
        }
        let first = media
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|m| m.values().next())?;
        self.resolve(first).get("value").cloned()
    }

    /// An example value for `schema`: its own example/default/enum, else generated.
    fn example(&mut self, schema: &Value, depth: usize, refs: &mut Vec<String>) -> Value {
        if depth > MAX_DEPTH {
            return Value::Null;
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            if refs.iter().any(|r| r == reference) {
                // A recursive schema (e.g. a category with a parent category): stop here.
                return Value::Null;
            }
            refs.push(reference.to_string());
            let target = self.resolve(schema);
            let value = self.example(&target, depth, refs);
            refs.pop();
            return value;
        }
        for key in ["example", "default", "const"] {
            if let Some(v) = schema.get(key) {
                return v.clone();
            }
        }
        if let Some(first) = schema
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
        {
            return first.clone();
        }
        if let Some(first) = schema
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
        {
            return first.clone();
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            for part in all {
                match self.example(part, depth + 1, refs) {
                    Value::Object(map) => merged.extend(map),
                    other if merged.is_empty() && all.len() == 1 => return other,
                    _ => {}
                }
            }
            return Value::Object(merged);
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(first) = schema
                .get(key)
                .and_then(Value::as_array)
                .and_then(|a| a.first())
            {
                return self.example(first, depth + 1, refs);
            }
        }
        let ty = schema_type(schema);
        match ty.as_deref() {
            Some("object") | None if schema.get("properties").is_some() => {
                let mut out = Map::new();
                if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                    for (name, prop) in props {
                        // Request bodies don't send server-generated fields.
                        if self.resolve(prop).get("readOnly").and_then(Value::as_bool) == Some(true)
                        {
                            continue;
                        }
                        let value = self.example(prop, depth + 1, refs);
                        if !value.is_null() || prop.get("type").is_some() {
                            out.insert(name.clone(), value);
                        }
                    }
                }
                Value::Object(out)
            }
            Some("object") => Value::Object(Map::new()),
            Some("array") => {
                let item = schema
                    .get("items")
                    .map(|items| self.example(items, depth + 1, refs))
                    .unwrap_or(Value::Null);
                if item.is_null() {
                    Value::Array(Vec::new())
                } else {
                    Value::Array(vec![item])
                }
            }
            Some("integer") => {
                Value::from(schema.get("minimum").and_then(Value::as_i64).unwrap_or(0))
            }
            Some("number") => {
                Value::from(schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0))
            }
            Some("boolean") => Value::Bool(true),
            Some("string") => Value::String(
                match str_of(schema, "format") {
                    Some("date-time") => "2026-01-01T12:00:00Z",
                    Some("date") => "2026-01-01",
                    Some("time") => "12:00:00",
                    Some("email") => "user@example.com",
                    Some("uuid") => "00000000-0000-0000-0000-000000000000",
                    Some("uri" | "url") => "https://example.com",
                    Some("hostname") => "example.com",
                    Some("ipv4") => "192.0.2.1",
                    Some("password") => "",
                    Some("byte") => "",
                    Some("binary") => "",
                    _ => "string",
                }
                .to_string(),
            ),
            _ => Value::Null,
        }
    }

    /// Follows `$ref`s (local ones only). External or broken refs give `null` and a warning.
    /// Returns a copy: simpler than tracking which part of the document it borrows from.
    fn resolve(&mut self, value: &Value) -> Value {
        let root = self.root;
        let mut current = value;
        for _ in 0..32 {
            let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
                return current.clone();
            };
            match reference.strip_prefix('#') {
                Some(pointer) => match root.pointer(pointer) {
                    Some(target) => current = target,
                    None => {
                        self.warnings
                            .push(format!("The reference {reference:?} points nowhere."));
                        return Value::Null;
                    }
                },
                None => {
                    self.warnings.push(format!(
                        "External reference {reference:?} was not followed (only references inside the file are)."
                    ));
                    return Value::Null;
                }
            }
        }
        Value::Null
    }

    /// Auth for a list of security requirements. The first requirement is used; its schemes
    /// are combined (e.g. OAuth2 + API key). `[]` means no auth.
    fn auth_for(&mut self, requirements: &Value, where_: &str) -> Auth {
        let Some(list) = requirements.as_array() else {
            return Auth::Inherit;
        };
        let Some(first) = list.iter().find_map(Value::as_object) else {
            return Auth::None;
        };
        if first.is_empty() {
            return Auth::None; // `{}`: authentication is optional.
        }
        if list.len() > 1 {
            self.warnings.push(format!(
                "{where_} allows several security alternatives; the first one was used."
            ));
        }
        let mut schemes = Vec::new();
        for (name, scopes) in first {
            let scopes: Vec<&str> = scopes
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if let Some(scheme) = self.scheme(name, &scopes) {
                schemes.push(scheme);
            }
        }
        if schemes.is_empty() {
            return Auth::None;
        }
        // The auth editor shows the first scheme as the "primary" one and only API keys after it
        // (PLAN.md §4.5), so put the primary scheme first, whatever order the spec lists them in.
        // `partition` keeps the original order inside both halves.
        let (api_keys, mut primary): (Vec<AuthScheme>, Vec<AuthScheme>) = schemes
            .into_iter()
            .partition(|s| matches!(s, AuthScheme::ApiKey { .. }));
        if primary.len() > 1 {
            self.warnings.push(format!(
                "{where_} combines several non-API-key schemes; only the first one was used."
            ));
            primary.truncate(1);
        }
        primary.extend(api_keys);
        Auth::Custom { schemes: primary }
    }

    /// No API-wide `security`: use the only scheme, if there is exactly one.
    fn fallback_auth(&mut self) -> Auth {
        let names: Vec<String> = self
            .root
            .pointer("/components/securitySchemes")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        if names.len() > 1 {
            self.warnings.push(format!(
                "The spec defines {} security schemes but no default; set the collection's auth by hand if needed.",
                names.len()
            ));
        }
        match names.as_slice() {
            [only] => match self.scheme(only, &[]) {
                Some(scheme) => Auth::Custom {
                    schemes: vec![scheme],
                },
                None => Auth::None,
            },
            _ => Auth::None,
        }
    }

    /// One security scheme as a Frida auth scheme, with `{{variables}}` for the credentials.
    fn scheme(&mut self, name: &str, scopes: &[&str]) -> Option<AuthScheme> {
        let pointer = format!(
            "/components/securitySchemes/{}",
            name.replace('~', "~0").replace('/', "~1")
        );
        let Some(raw) = self.root.pointer(&pointer) else {
            self.warnings
                .push(format!("Unknown security scheme {name:?}."));
            return None;
        };
        let def = self.resolve(raw);
        match str_of(&def, "type") {
            Some("apiKey") => {
                let location = match str_of(&def, "in") {
                    Some("query") => ApiKeyLocation::Query,
                    Some("cookie") => {
                        self.warnings.push(format!(
                            "Security scheme {name:?} uses a cookie API key; it was set up as a header instead."
                        ));
                        ApiKeyLocation::Header
                    }
                    _ => ApiKeyLocation::Header,
                };
                let var = var_name(name);
                self.env_var(&var, true);
                Some(AuthScheme::ApiKey {
                    location,
                    name: str_of(&def, "name").unwrap_or("X-API-Key").to_string(),
                    value: format!("{{{{{var}}}}}"),
                })
            }
            Some("http") => match str_of(&def, "scheme").map(str::to_lowercase).as_deref() {
                Some("basic") => {
                    self.env_var("username", false);
                    self.env_var("password", true);
                    Some(AuthScheme::Basic {
                        username: "{{username}}".into(),
                        password: "{{password}}".into(),
                    })
                }
                Some("bearer") => {
                    self.env_var("token", true);
                    Some(AuthScheme::Bearer {
                        token: "{{token}}".into(),
                    })
                }
                other => {
                    self.warnings.push(format!(
                        "Security scheme {name:?} uses HTTP {:?} auth, which Frida doesn't support; it was skipped.",
                        other.unwrap_or("")
                    ));
                    None
                }
            },
            Some("oauth2") => {
                let flows = def.get("flows").unwrap_or(&Value::Null);
                let (grant, flow) = if let Some(f) = flows.get("clientCredentials") {
                    (OAuth2Grant::ClientCredentials, f)
                } else if let Some(f) = flows.get("authorizationCode") {
                    (OAuth2Grant::AuthorizationCode, f)
                } else if let Some(f) = flows.get("password") {
                    (OAuth2Grant::Password, f)
                } else {
                    self.warnings.push(format!(
                        "Security scheme {name:?} only has the implicit OAuth2 flow, which Frida doesn't support; it was skipped."
                    ));
                    return None;
                };
                let scope = if scopes.is_empty() {
                    flow.get("scopes")
                        .and_then(Value::as_object)
                        .map(|m| m.keys().cloned().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default()
                } else {
                    scopes.join(" ")
                };
                self.env_var("clientId", false);
                self.env_var("clientSecret", true);
                let mut config = OAuth2Config {
                    grant,
                    token_url: str_of(flow, "tokenUrl").unwrap_or("").to_string(),
                    auth_url: str_of(flow, "authorizationUrl").unwrap_or("").to_string(),
                    client_id: "{{clientId}}".into(),
                    client_secret: "{{clientSecret}}".into(),
                    scope,
                    ..OAuth2Config::default()
                };
                if grant == OAuth2Grant::Password {
                    self.env_var("username", false);
                    self.env_var("password", true);
                    config.username = "{{username}}".into();
                    config.password = "{{password}}".into();
                }
                Some(AuthScheme::OAuth2(Box::new(config)))
            }
            other => {
                self.warnings.push(format!(
                    "Security scheme {name:?} ({}) isn't supported; it was skipped.",
                    other.unwrap_or("unknown type")
                ));
                None
            }
        }
    }

    /// Adds a variable to the default environment once. Secrets get no value.
    fn env_var(&mut self, name: &str, secret: bool) {
        if self.env_vars.iter().any(|v| v.name == name) {
            return;
        }
        self.env_vars.push(EnvVariable {
            name: name.to_string(),
            value: if secret { None } else { Some(String::new()) },
            secret,
            source: None,
            enabled: true,
        });
    }
}

/// Nested folders from the static path segments, in order of first use.
/// `/legs/resolve/{legNumber}` lands in `legs` → `resolve`; `/` lands at the root.
fn group_by_path(operations: Vec<Operation>) -> (Vec<PlannedFolder>, Vec<Request>) {
    let mut folders: Vec<PlannedFolder> = Vec::new();
    let mut root_requests = Vec::new();
    for op in operations {
        let segments: Vec<&str> = op
            .path
            .split('/')
            .filter(|s| !s.is_empty() && !(s.starts_with('{') && s.ends_with('}')))
            .collect();
        if segments.is_empty() {
            root_requests.push(op.request);
        } else {
            insert_at_path(&mut folders, &segments, op.request);
        }
    }
    (folders, root_requests)
}

/// Puts `request` into the folder `segments` below `folders`, creating folders as needed.
///
/// Recursion keeps the borrowing simple: each call holds a `&mut` to one level of folders and
/// hands a `&mut` to the chosen child's list down to the next call.
fn insert_at_path(folders: &mut Vec<PlannedFolder>, segments: &[&str], request: Request) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    let index = match folders.iter().position(|f| f.folder.name == *first) {
        Some(i) => i,
        None => {
            folders.push(PlannedFolder::new((*first).to_string()));
            folders.len() - 1
        }
    };
    let folder = &mut folders[index];
    if rest.is_empty() {
        folder.requests.push(request);
    } else {
        insert_at_path(&mut folder.folders, rest, request);
    }
}

fn str_of<'v>(value: &'v Value, key: &str) -> Option<&'v str> {
    value.get(key).and_then(Value::as_str)
}

fn array_of<'v>(value: &'v Value, key: &str) -> &'v [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// A JSON value as plain text: strings without quotes, arrays comma-separated.
fn plain(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    }
}

/// `type` as a single name; 3.1 allows a list like `["string", "null"]`.
fn schema_type(schema: &Value) -> Option<String> {
    match schema.get("type") {
        Some(Value::String(t)) => Some(t.clone()),
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .map(str::to_string),
        _ => None,
    }
}

/// `#/components/schemas/Pet` → `Pet`, for the XML root element.
fn schema_name(schema: &Value) -> Option<&str> {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.rsplit('/').next())
}

/// A simple XML rendering of an example value.
fn to_xml(name: &str, value: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match value {
        Value::Object(map) => {
            let children: String = map.iter().map(|(k, v)| to_xml(k, v, depth + 1)).collect();
            format!("{pad}<{name}>\n{children}{pad}</{name}>\n")
        }
        Value::Array(items) => items.iter().map(|v| to_xml(name, v, depth)).collect(),
        other => format!("{pad}<{name}>{}</{name}>\n", plain(other)),
    }
}

/// `api_key` / `defaultApiKey` → a variable name usable in `{{...}}`.
fn var_name(scheme: &str) -> String {
    let cleaned: String = scheme
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "apiKey".into()
    } else {
        cleaned
    }
}

fn dedup(warnings: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    warnings
        .into_iter()
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_swagger_2_with_a_hint() {
        let err =
            map_openapi("swagger: '2.0'\ninfo: {title: x}", OpenApiGrouping::Tags).unwrap_err();
        assert!(err.to_string().contains("Swagger 2.0"));
    }

    #[test]
    fn recursive_schemas_and_external_refs_do_not_loop() {
        let plan = map_openapi(
            r##"
openapi: 3.0.3
info: { title: T, version: "1" }
paths:
  /a:
    post:
      requestBody:
        content:
          application/json:
            schema: { $ref: "#/components/schemas/Node" }
  /b:
    post:
      requestBody:
        content:
          application/json:
            schema: { $ref: "other.yaml#/X" }
components:
  schemas:
    Node:
      type: object
      properties:
        name: { type: string }
        child: { $ref: "#/components/schemas/Node" }
"##,
            OpenApiGrouping::Tags,
        )
        .unwrap();
        assert_eq!(plan.requests.len(), 2);
        // The recursion stops: `child` (a reference back to `Node`) is left out.
        assert_eq!(plan.requests[0].body.json, "{\n  \"name\": \"string\"\n}");
        assert!(
            plan.summary
                .warnings
                .iter()
                .any(|w| w.contains("other.yaml"))
        );
    }

    #[test]
    fn oauth2_plus_api_key_puts_oauth2_first() {
        // The API key is listed first in each alternative, as in many real specs.
        let plan = map_openapi(
            r#"
openapi: 3.0.3
info: { title: T, version: "1" }
security:
  - api_key: []
    OAuth2Dev: []
  - api_key: []
    OAuth2Prod: []
paths: {}
components:
  securitySchemes:
    api_key: { type: apiKey, in: header, name: X-Api-Key }
    OAuth2Dev:
      type: oauth2
      flows:
        clientCredentials: { tokenUrl: "https://auth.dev.example.com/oauth2/token", scopes: {} }
    OAuth2Prod:
      type: oauth2
      flows:
        clientCredentials: { tokenUrl: "https://auth.example.com/oauth2/token", scopes: {} }
"#,
            OpenApiGrouping::Tags,
        )
        .unwrap();
        let Auth::Custom { schemes } = &plan.collection.auth else {
            panic!("expected custom auth, got {:?}", plan.collection.auth);
        };
        assert_eq!(schemes.len(), 2);
        let AuthScheme::OAuth2(config) = &schemes[0] else {
            panic!(
                "expected OAuth2 as the primary scheme, got {:?}",
                schemes[0]
            );
        };
        assert_eq!(
            config.token_url,
            "https://auth.dev.example.com/oauth2/token"
        );
        assert!(matches!(&schemes[1], AuthScheme::ApiKey { name, .. } if name == "X-Api-Key"));
    }

    #[test]
    fn grouping_by_path_nests_static_segments() {
        let spec = r#"
openapi: 3.0.3
info: { title: T, version: "1" }
paths:
  /:
    get: { operationId: root }
  /legs:
    get: { operationId: listLegs, tags: [legs] }
  /legs/resolve/{legNumber}:
    get: { operationId: resolveLeg, tags: [legs] }
  /legs/{id}/crew:
    get: { operationId: legCrew, tags: [legs] }
"#;
        let plan = map_openapi(spec, OpenApiGrouping::Paths).unwrap();
        assert_eq!(plan.requests[0].name, "root");
        let legs = &plan.folders[0];
        assert_eq!(legs.folder.name, "legs");
        assert_eq!(legs.requests[0].name, "listLegs");
        let names: Vec<_> = legs
            .folders
            .iter()
            .map(|f| f.folder.name.as_str())
            .collect();
        assert_eq!(names, vec!["resolve", "crew"]);
        assert_eq!(legs.folders[0].requests[0].name, "resolveLeg");
        assert_eq!(plan.summary.folders, 3);
        assert_eq!(plan.summary.requests, 4);

        let by_tag = map_openapi(spec, OpenApiGrouping::Tags).unwrap();
        assert_eq!(by_tag.folders.len(), 1);
        assert_eq!(by_tag.folders[0].requests.len(), 3);
    }

    #[test]
    fn plain_values() {
        assert_eq!(plain(&serde_json::json!(["a", 1])), "a,1");
        assert_eq!(plain(&serde_json::json!(null)), "");
    }
}
