//! Reads a Bruno YAML collection (OpenCollection, the default since Bruno 3.1:
//! `opencollection.yml`, `folder.yml`, `*.yml`, `environments/*.yml`) into the in-between model.
//! Field names follow Bruno's `packages/bruno-filestore/src/formats/yml`.

use std::path::Path;

use serde_json::Value;

use super::model::{
    Auth, Body, Collection, Common, Env, EnvVar, Folder, Kv, Node, OAuth2, Part, Request,
};
use super::{list_dir, read_text};
use crate::error::{CoreError, Result};

fn parse(path: &Path) -> Result<Value> {
    let text = read_text(path)?;
    serde_saphyr::from_str::<Value>(&text).map_err(|e| CoreError::InvalidFile {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn s(value: &Value, key: &str) -> String {
    value.get(key).map(text).unwrap_or_default()
}

/// A scalar or a typed value (`{ type: number, data: "1" }`) as text.
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Object(map) if map.contains_key("data") => text(&map["data"]),
        Value::Object(map) if map.contains_key("content") => text(&map["content"]),
        other => other.to_string(),
    }
}

fn enabled(value: &Value) -> bool {
    value.get("disabled").and_then(Value::as_bool) != Some(true)
}

fn array<'v>(value: &'v Value, key: &str) -> &'v [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn kvs(list: &[Value]) -> Vec<Kv> {
    list.iter()
        .map(|v| Kv {
            name: s(v, "name"),
            value: v.get("value").map(text).unwrap_or_default(),
            enabled: enabled(v),
            description: s(v, "description"),
        })
        .collect()
}

pub(crate) fn read(dir: &Path) -> Result<Collection> {
    let root = parse(&dir.join("opencollection.yml"))?;
    let mut collection = Collection {
        name: root
            .get("info")
            .map(|i| s(i, "name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Bruno collection".into()),
        common: common(root.get("request").unwrap_or(&Value::Null), &root),
        ..Collection::default()
    };
    collection.children = read_dir(dir, dir, &mut collection.warnings)?;
    for path in list_dir(&dir.join("environments"))? {
        if path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            collection
                .environments
                .push(environment(&parse(&path)?, &path));
        }
    }
    Ok(collection)
}

fn read_dir(root: &Path, dir: &Path, warnings: &mut Vec<String>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for path in list_dir(dir)? {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if (dir == root && file_name == "environments") || file_name == "node_modules" {
                continue;
            }
            let folder_file = path.join("folder.yml");
            let meta = if folder_file.exists() {
                parse(&folder_file)?
            } else {
                Value::Null
            };
            let info = meta.get("info").unwrap_or(&Value::Null);
            nodes.push(Node::Folder(Folder {
                name: Some(s(info, "name"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| file_name.clone()),
                seq: info.get("seq").and_then(Value::as_u64).map(|n| n as u32),
                common: common(meta.get("request").unwrap_or(&Value::Null), &meta),
                children: read_dir(root, &path, warnings)?,
            }));
        } else if path.extension().is_some_and(|e| e == "yml" || e == "yaml")
            && file_name != "opencollection.yml"
            && file_name != "folder.yml"
        {
            let item = match parse(&path) {
                Ok(item) => item,
                Err(err) => {
                    warnings.push(format!("{file_name} was skipped: {err}"));
                    continue;
                }
            };
            let kind = item.get("info").map(|i| s(i, "type")).unwrap_or_default();
            match kind.as_str() {
                "http" | "graphql" => nodes.push(Node::Request(request(&item, &kind, &file_name))),
                "" => {}
                other => {
                    let name = item.get("info").map(|i| s(i, "name")).unwrap_or_default();
                    nodes.push(Node::Request(Request {
                        name,
                        kind: other.to_string(),
                        ..Request::default()
                    }));
                }
            }
        }
    }
    Ok(nodes)
}

/// Headers, auth, variables and scripts of a collection/folder (`request:` block) or of a
/// request (`http:` + `runtime:`). `doc` holds `docs` at its top level.
fn common(block: &Value, doc: &Value) -> Common {
    let runtime_like = block;
    let mut c = Common {
        headers: kvs(array(block, "headers")),
        auth: auth(block.get("auth")),
        vars: kvs(array(runtime_like, "variables")),
        docs: doc.get("docs").map(text).unwrap_or_default(),
        ..Common::default()
    };
    scripts(array(runtime_like, "scripts"), &mut c);
    actions(array(runtime_like, "actions"), &mut c);
    c.asserts = assertions(array(runtime_like, "assertions"));
    c
}

fn scripts(list: &[Value], c: &mut Common) {
    for script in list {
        let code = s(script, "code");
        match s(script, "type").as_str() {
            "before-request" => c.script_pre = code,
            "after-response" => c.script_post = code,
            "tests" => c.tests = code,
            _ => {}
        }
    }
}

/// `actions: [{ type: set-variable, phase: after-response, variable: {name}, selector: {expression} }]`
/// are Bruno's post-response variables.
fn actions(list: &[Value], c: &mut Common) {
    for action in list {
        if s(action, "type") == "set-variable" && s(action, "phase") == "after-response" {
            c.post_vars.push(Kv {
                name: action
                    .get("variable")
                    .map(|v| s(v, "name"))
                    .unwrap_or_default(),
                value: action
                    .get("selector")
                    .map(|v| s(v, "expression"))
                    .unwrap_or_default(),
                enabled: enabled(action),
                description: String::new(),
            });
        }
    }
}

/// `{ expression, operator, value }` → the `.bru` form `expression: operator value`.
fn assertions(list: &[Value]) -> Vec<Kv> {
    list.iter()
        .map(|a| {
            let operator = s(a, "operator");
            let value = a.get("value").map(text).unwrap_or_default();
            Kv {
                name: s(a, "expression"),
                value: if value.is_empty() {
                    operator
                } else {
                    format!("{operator} {value}")
                },
                enabled: enabled(a),
                description: String::new(),
            }
        })
        .collect()
}

fn auth(value: Option<&Value>) -> Auth {
    let Some(value) = value else {
        return Auth::None;
    };
    if value.as_str() == Some("inherit") {
        return Auth::Inherit;
    }
    match s(value, "type").as_str() {
        "" | "none" => Auth::None,
        "inherit" => Auth::Inherit,
        "basic" => Auth::Basic {
            username: s(value, "username"),
            password: s(value, "password"),
        },
        "bearer" => Auth::Bearer {
            token: s(value, "token"),
        },
        "apikey" => Auth::ApiKey {
            key: s(value, "key"),
            value: s(value, "value"),
            in_query: s(value, "placement") == "query",
        },
        "oauth2" => {
            let credentials = value.get("credentials").unwrap_or(&Value::Null);
            let owner = value.get("resourceOwner").unwrap_or(&Value::Null);
            let placement = value
                .get("tokenConfig")
                .and_then(|t| t.get("placement"))
                .unwrap_or(&Value::Null);
            let grant = match s(value, "flow").as_str() {
                "resource_owner_password_credentials" => "password".to_string(),
                other => other.to_string(),
            };
            Auth::OAuth2(Box::new(OAuth2 {
                grant,
                token_url: s(value, "accessTokenUrl"),
                auth_url: s(value, "authorizationUrl"),
                callback_url: s(value, "callbackUrl"),
                client_id: s(credentials, "clientId"),
                client_secret: s(credentials, "clientSecret"),
                scope: s(value, "scope"),
                pkce: value
                    .get("pkce")
                    .is_some_and(|p| p.get("disabled").and_then(Value::as_bool) != Some(true)),
                credentials_in_body: s(credentials, "placement") != "basic_auth_header",
                username: s(owner, "username"),
                password: s(owner, "password"),
                token_query_key: placement.get("query").map(text),
                header_prefix: placement.get("header").map(text).unwrap_or_default(),
            }))
        }
        other => Auth::Unsupported(other.to_string()),
    }
}

fn request(item: &Value, kind: &str, file_name: &str) -> Request {
    let info = item.get("info").unwrap_or(&Value::Null);
    let http = item.get(kind).unwrap_or(&Value::Null);
    let runtime = item.get("runtime").unwrap_or(&Value::Null);

    let mut common = common(runtime, item);
    common.headers = kvs(array(http, "headers"));
    common.auth = auth(http.get("auth"));

    let params = array(http, "params");
    let pick = |ty: &str| -> Vec<Kv> {
        params
            .iter()
            .filter(|p| s(p, "type") == ty)
            .map(|p| Kv {
                name: s(p, "name"),
                value: s(p, "value"),
                enabled: enabled(p),
                description: s(p, "description"),
            })
            .collect()
    };
    let body_value = http.get("body").unwrap_or(&Value::Null);
    let body = if kind == "graphql" {
        Body::GraphQl {
            query: s(body_value, "query"),
            variables: s(body_value, "variables"),
        }
    } else {
        let data = body_value.get("data").unwrap_or(&Value::Null);
        match s(body_value, "type").as_str() {
            "json" => Body::Json(text(data)),
            "text" => Body::Text(text(data)),
            "xml" => Body::Xml(text(data)),
            "sparql" => Body::Sparql(text(data)),
            "form-urlencoded" => Body::Form(kvs(data.as_array().map(Vec::as_slice).unwrap_or(&[]))),
            "multipart-form" => Body::Multipart(
                data.as_array()
                    .into_iter()
                    .flatten()
                    .map(|entry| {
                        let is_file = s(entry, "type") == "file";
                        let value = entry.get("value").unwrap_or(&Value::Null);
                        Part {
                            name: s(entry, "name"),
                            value: if is_file { String::new() } else { text(value) },
                            files: if is_file {
                                match value {
                                    Value::Array(paths) => paths.iter().map(text).collect(),
                                    other => vec![text(other)],
                                }
                            } else {
                                Vec::new()
                            },
                            enabled: enabled(entry),
                        }
                    })
                    .collect(),
            ),
            "file" => Body::File(
                data.as_array()
                    .into_iter()
                    .flatten()
                    .find(|f| f.get("selected").and_then(Value::as_bool) == Some(true))
                    .map(|f| s(f, "filePath")),
            ),
            _ => Body::None,
        }
    };
    let timeout_ms = item
        .get("settings")
        .and_then(|st| st.get("timeout"))
        .and_then(Value::as_u64)
        .and_then(|t| u32::try_from(t).ok())
        .filter(|t| *t > 0);
    Request {
        name: Some(s(info, "name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| file_name.trim_end_matches(".yml").to_string()),
        seq: info
            .get("seq")
            .and_then(Value::as_u64)
            .map(|n| n as u32)
            .unwrap_or(1),
        kind: kind.to_string(),
        method: Some(s(http, "method").to_uppercase())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| {
                if kind == "graphql" {
                    "POST".into()
                } else {
                    "GET".into()
                }
            }),
        url: s(http, "url"),
        query: pick("query"),
        path_params: pick("path"),
        body,
        timeout_ms,
        common,
    }
}

fn environment(doc: &Value, path: &Path) -> Env {
    let name = Some(s(doc, "name"))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        });
    Env {
        name,
        vars: array(doc, "variables")
            .iter()
            .map(|v| {
                let secret = v.get("secret").and_then(Value::as_bool) == Some(true);
                EnvVar {
                    name: s(v, "name"),
                    value: if secret {
                        String::new()
                    } else {
                        v.get("value").map(text).unwrap_or_default()
                    },
                    secret,
                    enabled: enabled(v),
                }
            })
            .collect(),
    }
}
