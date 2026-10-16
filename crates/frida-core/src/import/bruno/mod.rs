//! Bruno collection import (PLAN.md §11.2). Both of Bruno's on-disk formats are supported:
//! the classic `.bru` files and the YAML "OpenCollection" format (default since Bruno 3.1).
//!
//! Like the OpenAPI importer this first builds a [`BrunoPlan`] (pure, snapshot-tested), then
//! [`write_bruno_plan`] writes it. Scripts are copied as they are: Frida's `bru` object is an
//! alias of `fri`, so most Bruno scripts run unchanged; calls Frida lacks are listed as warnings.

mod asserts;
mod bru;
mod model;
mod read_bru;
mod read_yml;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ImportSummary;
use crate::error::{CoreError, Result};
use crate::model::{
    ApiKeyLocation, Auth, AuthScheme, Body as FridaBody, BodyMode, Collection, EnvVariable,
    Environment, Folder, KeyValue, MultipartField, MultipartKind, OAuth2ClientAuth, OAuth2Config,
    OAuth2Grant, PathParam, Request, Scripts, TokenPlacement,
};
use crate::storage;

/// A folder (or the collection root) and what it contains, in order.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedDir {
    /// `None` for the collection root.
    pub folder: Option<Folder>,
    pub items: Vec<PlannedItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlannedItem {
    /// Boxed (like `Request`) so the enum stays small whichever variant it holds.
    Folder(Box<PlannedDir>),
    Request(Box<Request>),
}

/// Everything a Bruno import will create.
#[derive(Debug, Clone, Serialize)]
pub struct BrunoPlan {
    pub collection: Collection,
    pub environments: Vec<Environment>,
    pub root: PlannedDir,
    pub summary: ImportSummary,
}

/// Which Bruno format a folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Bru,
    Yaml,
}

fn detect(dir: &Path) -> Result<Format> {
    if dir.join("opencollection.yml").exists() {
        Ok(Format::Yaml)
    } else if dir.join("bruno.json").exists() || dir.join("collection.bru").exists() {
        Ok(Format::Bru)
    } else {
        Err(CoreError::InvalidInput(format!(
            "{} is not a Bruno collection (no bruno.json or opencollection.yml found)",
            dir.display()
        )))
    }
}

/// Reads the Bruno collection in `dir` and plans the Frida collection.
pub fn map_bruno(dir: &Path) -> Result<BrunoPlan> {
    let format = detect(dir)?;
    let source = match format {
        Format::Bru => read_bru::read(dir)?,
        Format::Yaml => read_yml::read(dir)?,
    };
    let mut mapper = Mapper {
        source_dir: dir.to_path_buf(),
        warnings: source.warnings.clone(),
        requests: 0,
        folders: 0,
        graphql: 0,
        files: false,
    };
    if dir.join(".env").exists() {
        mapper.warnings.push(
            "The collection has a .env file. Frida reads {{process.env.NAME}} from the OS \
             environment only; set those variables before starting Frida."
                .into(),
        );
    }

    let mut collection = Collection::new(source.name.clone());
    let common = mapper.common(&source.common, &format!("collection {:?}", source.name));
    collection.headers = common.headers;
    collection.auth = match common.auth {
        Auth::Inherit => Auth::None,
        other => other,
    };
    collection.vars = common.vars;
    collection.scripts = common.scripts;
    collection.tests = common.tests;
    collection.docs = common.docs;

    let root = PlannedDir {
        folder: None,
        items: mapper.items(&source.children),
    };

    let mut secrets = Vec::new();
    let environments = source
        .environments
        .iter()
        .map(|env| {
            let mut variables = Vec::new();
            for var in &env.vars {
                if !var.enabled {
                    mapper.warnings.push(format!(
                        "Environment {:?}: the disabled variable {:?} was left out.",
                        env.name, var.name
                    ));
                    continue;
                }
                if var.secret {
                    secrets.push(format!("{}: {}", env.name, var.name));
                }
                variables.push(EnvVariable {
                    name: var.name.clone(),
                    value: (!var.secret).then(|| var.value.clone()),
                    secret: var.secret,
                    source: None,
                });
            }
            Environment {
                variables,
                ..Environment::new(env.name.clone())
            }
        })
        .collect::<Vec<_>>();

    if mapper.graphql > 0 {
        mapper.warnings.push(format!(
            "{} GraphQL request(s) were imported as JSON POST requests ({{\"query\", \"variables\"}}).",
            mapper.graphql
        ));
    }
    if mapper.files {
        mapper.warnings.push(format!(
            "File bodies and multipart files point to files in the Bruno collection ({}); \
             keep them there or update the paths.",
            dir.display()
        ));
    }

    let summary = ImportSummary {
        collection_name: source.name,
        requests: mapper.requests,
        folders: mapper.folders,
        environments: u32::try_from(environments.len()).unwrap_or(u32::MAX),
        base_url: String::new(),
        secrets_to_fill: secrets,
        warnings: dedup(mapper.warnings),
    };
    Ok(BrunoPlan {
        collection,
        environments,
        root,
        summary,
    })
}

/// Writes `plan` as a new collection in `target`.
pub fn write_bruno_plan(target: &Path, plan: &BrunoPlan) -> Result<()> {
    let created = storage::create_collection(target, &plan.collection.name)?;
    storage::save_collection(
        target,
        &Collection {
            id: created.id,
            ..plan.collection.clone()
        },
    )?;
    for env in &plan.environments {
        storage::create_environment(target, &env.name)?;
        storage::save_environment(target, &env.name, env)?;
    }
    write_dir(target, "", &plan.root)
}

fn write_dir(root: &Path, rel: &str, dir: &PlannedDir) -> Result<()> {
    for item in &dir.items {
        match item {
            PlannedItem::Request(request) => {
                storage::create_request(root, rel, request)?;
            }
            PlannedItem::Folder(sub) => {
                let Some(planned) = &sub.folder else { continue };
                let sub_rel = storage::create_folder(root, rel, &planned.name)?;
                let created = storage::read_folder(root, &sub_rel)?;
                storage::save_folder(
                    root,
                    &sub_rel,
                    &Folder {
                        seq: created.seq,
                        ..planned.clone()
                    },
                )?;
                write_dir(root, &sub_rel, sub)?;
            }
        }
    }
    Ok(())
}

/// What importing the Bruno collection in `dir` would create.
pub fn preview_bruno_dir(dir: &Path) -> Result<ImportSummary> {
    Ok(map_bruno(dir)?.summary)
}

/// Imports the Bruno collection in `source` as a new collection in `target` (default:
/// `<workspace>/collections/<name>`) and adds it to the workspace.
pub fn import_bruno_dir(
    workspace_dir: &Path,
    source: &Path,
    target: Option<&Path>,
) -> Result<(PathBuf, ImportSummary)> {
    let plan = map_bruno(source)?;
    let dir = match target {
        Some(dir) => dir.to_path_buf(),
        None => storage::new_collection_dir(workspace_dir, &plan.collection.name)?,
    };
    write_bruno_plan(&dir, &plan)?;
    storage::add_collection(workspace_dir, &dir)?;
    tracing::info!(
        requests = plan.summary.requests,
        "Bruno collection imported"
    );
    Ok((dir, plan.summary))
}

/// The pieces of a collection, folder or request after mapping.
struct MappedCommon {
    headers: Vec<KeyValue>,
    auth: Auth,
    vars: Vec<KeyValue>,
    scripts: Scripts,
    tests: String,
    docs: String,
}

struct Mapper {
    source_dir: PathBuf,
    warnings: Vec<String>,
    requests: u32,
    folders: u32,
    graphql: u32,
    /// Some body refers to a file.
    files: bool,
}

impl Mapper {
    /// Folders first, then requests, each by Bruno's `seq` and name (Bruno's sidebar order).
    fn items(&mut self, nodes: &[model::Node]) -> Vec<PlannedItem> {
        let mut folders: Vec<&model::Folder> = Vec::new();
        let mut requests: Vec<&model::Request> = Vec::new();
        for node in nodes {
            match node {
                model::Node::Folder(f) => folders.push(f),
                model::Node::Request(r) => requests.push(r),
            }
        }
        folders.sort_by_key(|f| (f.seq.unwrap_or(u32::MAX), f.name.to_lowercase()));
        requests.sort_by_key(|r| (r.seq, r.name.to_lowercase()));

        let mut items = Vec::new();
        for folder in folders {
            self.folders += 1;
            let common = self.common(&folder.common, &format!("folder {:?}", folder.name));
            let planned = Folder {
                headers: common.headers,
                auth: common.auth,
                vars: common.vars,
                scripts: common.scripts,
                tests: common.tests,
                docs: common.docs,
                ..Folder::new(folder.name.clone())
            };
            items.push(PlannedItem::Folder(Box::new(PlannedDir {
                folder: Some(planned),
                items: self.items(&folder.children),
            })));
        }
        for request in requests {
            if let Some(mapped) = self.request(request) {
                self.requests += 1;
                items.push(PlannedItem::Request(Box::new(mapped)));
            }
        }
        items
    }

    fn request(&mut self, r: &model::Request) -> Option<Request> {
        let label = format!("request {:?}", r.name);
        if r.kind != "http" && r.kind != "graphql" {
            self.warnings.push(format!(
                "{label} is a {} request, which Frida doesn't support; it was skipped.",
                r.kind
            ));
            return None;
        }
        let common = self.common(&r.common, &label);
        let mut request = Request::new(r.name.clone());
        request.method = r.method.clone();
        request.url = r.url.clone();
        request.params.query = r.query.iter().map(kv).collect();
        request.params.path = r
            .path_params
            .iter()
            .map(|p| PathParam {
                name: p.name.clone(),
                value: p.value.clone(),
            })
            .collect();
        request.headers = common.headers;
        request.auth = common.auth;
        request.vars = common.vars;
        request.scripts = common.scripts;
        request.tests = common.tests;
        request.docs = common.docs;
        request.settings.timeout_ms = r.timeout_ms;
        request.body = self.body(&r.body, &label, &mut request.headers);
        Some(request)
    }

    fn body(&mut self, body: &model::Body, label: &str, headers: &mut Vec<KeyValue>) -> FridaBody {
        let mut b = FridaBody::default();
        match body {
            model::Body::None => {}
            model::Body::Json(json) => {
                b.mode = BodyMode::Json;
                b.json = json.clone();
            }
            model::Body::Text(text) => {
                b.mode = BodyMode::Text;
                b.text = text.clone();
            }
            model::Body::Xml(xml) => {
                b.mode = BodyMode::Xml;
                b.xml = xml.clone();
            }
            model::Body::Sparql(query) => {
                b.mode = BodyMode::Text;
                b.text = query.clone();
                if !headers
                    .iter()
                    .any(|h| h.name.eq_ignore_ascii_case("content-type"))
                {
                    headers.push(KeyValue::new("Content-Type", "application/sparql-query"));
                }
            }
            model::Body::GraphQl { query, variables } => {
                self.graphql += 1;
                let variables: serde_json::Value = if variables.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(variables)
                        .unwrap_or_else(|_| serde_json::Value::String(variables.clone()))
                };
                b.mode = BodyMode::Json;
                b.json = serde_json::to_string_pretty(&serde_json::json!({
                    "query": query,
                    "variables": variables,
                }))
                .unwrap_or_default();
            }
            model::Body::Form(fields) => {
                b.mode = BodyMode::FormUrlencoded;
                b.form_url_encoded = fields.iter().map(kv).collect();
            }
            model::Body::Multipart(parts) => {
                b.mode = BodyMode::Multipart;
                for part in parts {
                    if part.files.is_empty() {
                        b.multipart.push(MultipartField {
                            name: part.name.clone(),
                            kind: MultipartKind::Text,
                            value: part.value.clone(),
                            enabled: part.enabled,
                        });
                    } else {
                        if part.files.len() > 1 {
                            self.warnings.push(format!(
                                "{label}: the multipart field {:?} has several files; each became its own field.",
                                part.name
                            ));
                        }
                        for file in &part.files {
                            self.files = true;
                            b.multipart.push(MultipartField {
                                name: part.name.clone(),
                                kind: MultipartKind::File,
                                value: self.source_path(file),
                                enabled: part.enabled,
                            });
                        }
                    }
                }
            }
            model::Body::File(path) => {
                b.mode = BodyMode::Binary;
                if let Some(path) = path {
                    self.files = true;
                    b.binary_path = self.source_path(path);
                }
            }
        }
        b
    }

    /// Bruno file paths are relative to the collection; make them absolute so they still work
    /// from the new collection.
    fn source_path(&self, path: &str) -> String {
        if path.is_empty() || Path::new(path).is_absolute() {
            path.to_string()
        } else {
            self.source_dir.join(path).to_string_lossy().to_string()
        }
    }

    fn common(&mut self, c: &model::Common, label: &str) -> MappedCommon {
        // Post-response variables (`token: res.body.token`) become a few lines of script.
        let mut post_lines: Vec<String> = c
            .post_vars
            .iter()
            .filter(|v| v.enabled && !v.name.is_empty())
            .map(|v| format!("fri.setVar({}, {});", js_string(&v.name), v.value.trim()))
            .collect();
        let post = if post_lines.is_empty() {
            c.script_post.clone()
        } else {
            post_lines.insert(0, "// Converted from Bruno post-response variables.".into());
            let generated = post_lines.join("\n");
            if c.script_post.trim().is_empty() {
                generated
            } else {
                format!("{generated}\n\n{}", c.script_post)
            }
        };

        let (assert_tests, skipped) = asserts::to_tests(&c.asserts);
        if !skipped.is_empty() {
            self.warnings.push(format!(
                "{label}: assertions that couldn't be converted were kept as comments in its tests: {}",
                skipped.join("; ")
            ));
        }
        let tests = match (c.tests.trim().is_empty(), assert_tests.is_empty()) {
            (_, true) => c.tests.clone(),
            (true, false) => assert_tests,
            (false, false) => format!("{}\n\n{assert_tests}", c.tests),
        };

        for (part, code) in [
            ("pre-request script", &c.script_pre),
            ("post-response script", &post),
            ("tests", &tests),
        ] {
            let unsupported = unsupported_calls(code);
            if !unsupported.is_empty() {
                self.warnings.push(format!(
                    "{label}, {part} uses {}, which Frida doesn't support (see docs/scripting.md).",
                    unsupported.into_iter().collect::<Vec<_>>().join(", ")
                ));
            }
        }

        MappedCommon {
            headers: c.headers.iter().map(kv).collect(),
            auth: self.auth(&c.auth, label),
            vars: c.vars.iter().map(kv).collect(),
            scripts: Scripts {
                pre: c.script_pre.clone(),
                post,
            },
            tests,
            docs: c.docs.clone(),
        }
    }

    fn auth(&mut self, auth: &model::Auth, label: &str) -> Auth {
        let scheme = match auth {
            model::Auth::None => return Auth::None,
            model::Auth::Inherit => return Auth::Inherit,
            model::Auth::Basic { username, password } => AuthScheme::Basic {
                username: username.clone(),
                password: password.clone(),
            },
            model::Auth::Bearer { token } => AuthScheme::Bearer {
                token: token.clone(),
            },
            model::Auth::ApiKey {
                key,
                value,
                in_query,
            } => AuthScheme::ApiKey {
                location: if *in_query {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
                name: key.clone(),
                value: value.clone(),
            },
            model::Auth::OAuth2(o) => {
                let grant = match o.grant.as_str() {
                    "client_credentials" => OAuth2Grant::ClientCredentials,
                    "password" => OAuth2Grant::Password,
                    "authorization_code" => OAuth2Grant::AuthorizationCode,
                    other => {
                        self.warnings.push(format!(
                            "{label}: the OAuth2 {other:?} grant isn't supported; its auth was set to none."
                        ));
                        return Auth::None;
                    }
                };
                let token_placement = match &o.token_query_key {
                    Some(key) => TokenPlacement {
                        location: ApiKeyLocation::Query,
                        name: key.clone(),
                        prefix: String::new(),
                    },
                    None => TokenPlacement {
                        prefix: if o.header_prefix.is_empty() {
                            "Bearer".into()
                        } else {
                            o.header_prefix.clone()
                        },
                        ..TokenPlacement::default()
                    },
                };
                AuthScheme::OAuth2(Box::new(OAuth2Config {
                    grant,
                    token_url: o.token_url.clone(),
                    auth_url: o.auth_url.clone(),
                    redirect_uri: o.callback_url.clone(),
                    client_id: o.client_id.clone(),
                    client_secret: o.client_secret.clone(),
                    scope: o.scope.clone(),
                    use_pkce: o.pkce,
                    username: o.username.clone(),
                    password: o.password.clone(),
                    client_auth: if o.credentials_in_body {
                        OAuth2ClientAuth::Body
                    } else {
                        OAuth2ClientAuth::BasicHeader
                    },
                    token_placement,
                    preset: None,
                }))
            }
            model::Auth::Unsupported(mode) => {
                self.warnings.push(format!(
                    "{label} uses {mode} auth, which Frida doesn't support; its auth was set to none."
                ));
                return Auth::None;
            }
        };
        Auth::Custom {
            schemes: vec![scheme],
        }
    }
}

fn kv(kv: &model::Kv) -> KeyValue {
    KeyValue {
        name: kv.name.clone(),
        value: kv.value.clone(),
        enabled: kv.enabled,
    }
}

fn js_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// What scripts may call on each object (see `frida-script/src/prelude.js`). Anything else
/// on `bru`/`fri`/`req`/`res` is reported.
const SUPPORTED: &[(&str, &[&str])] = &[
    (
        "bru",
        &[
            "hasVar",
            "getVar",
            "setVar",
            "deleteVar",
            "getAllVars",
            "deleteAllVars",
            "getEnvName",
            "hasEnvVar",
            "getEnvVar",
            "setEnvVar",
            "deleteEnvVar",
            "getAllEnvVars",
            "getGlobalEnvName",
            "hasGlobalEnvVar",
            "getGlobalEnvVar",
            "setGlobalEnvVar",
            "deleteGlobalEnvVar",
            "getAllGlobalEnvVars",
            "getCollectionName",
            "getCollectionVar",
            "hasCollectionVar",
            "getFolderVar",
            "getRequestVar",
            "getProcessEnv",
            "interpolate",
            "sleep",
            "isSafeMode",
            "getTestResults",
            "getAssertionResults",
            "sendRequest",
            "setNextRequest",
            "runner",
        ],
    ),
    (
        "req",
        &[
            "getUrl",
            "setUrl",
            "getHost",
            "getPath",
            "getQueryString",
            "getMethod",
            "setMethod",
            "getName",
            "getAuthMode",
            "getHeader",
            "getHeaders",
            "setHeader",
            "setHeaders",
            "deleteHeader",
            "deleteHeaders",
            "getBody",
            "setBody",
            "getTimeout",
            "setTimeout",
            "getExecutionMode",
            "getTags",
            "url",
            "method",
            "headers",
            "body",
            "timeout",
        ],
    ),
    (
        "res",
        &[
            "status",
            "statusText",
            "headers",
            "body",
            "responseTime",
            "url",
            "getStatus",
            "getStatusText",
            "getHeader",
            "getHeaders",
            "getBody",
            "setBody",
            "getResponseTime",
            "getUrl",
            "getSize",
        ],
    ),
];

/// `bru.cwd`, `req.headerList`, `require(...)`: calls Frida's sandbox doesn't offer.
fn unsupported_calls(code: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let bytes = code.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    // `fri` is the same object as `bru`.
    let objects = SUPPORTED
        .iter()
        .copied()
        .chain(std::iter::once(("fri", SUPPORTED[0].1)));
    for (object, members) in objects {
        let pattern = format!("{object}.");
        let mut start = 0;
        while let Some(pos) = code[start..].find(&pattern) {
            let at = start + pos;
            start = at + pattern.len();
            if at > 0 && (is_ident(bytes[at - 1]) || bytes[at - 1] == b'.') {
                continue; // e.g. `myres.x` or `a.res.x`
            }
            let member: String = code[start..]
                .bytes()
                .take_while(|b| is_ident(*b))
                .map(char::from)
                .collect();
            // Only calls and chains (`bru.cwd()`, `req.headerList.get`): a bare `res.data` is
            // usually a local variable (e.g. a `sendRequest` result), and `bru.set*` in a
            // string is just text.
            let next = code[start + member.len()..].trim_start().chars().next();
            let used = matches!(next, Some('(' | '.'));
            if used && !member.is_empty() && !members.contains(&member.as_str()) {
                found.insert(format!("{object}.{member}"));
            }
        }
    }
    if code.contains("require(") {
        found.insert("require(...)".into());
    }
    found
}

fn dedup(warnings: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    warnings
        .into_iter()
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// `Vec` of a directory's entries, sorted by name; empty when it doesn't exist.
pub(crate) fn list_dir(dir: &Path) -> Result<Vec<PathBuf>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    Ok(paths)
}

pub(crate) fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_unsupported_script_calls() {
        let code = r#"
            bru.setVar("a", 1);
            const x = req.headerList.get("a");
            bru.cwd();
            res.getBody();
            const myres = 1; myres.foo;
            require("lodash");
        "#;
        let found: Vec<String> = unsupported_calls(code).into_iter().collect();
        assert_eq!(found, vec!["bru.cwd", "req.headerList", "require(...)"]);
    }
}
