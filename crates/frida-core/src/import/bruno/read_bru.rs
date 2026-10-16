//! Reads a `.bru` collection folder (`bruno.json`, `collection.bru`, `folder.bru`, `*.bru`,
//! `environments/*.bru`) into the in-between model.

use std::path::Path;

use super::bru::{self, BruFile, Pair};
use super::model::{
    Auth, Body, Collection, Common, Env, EnvVar, Folder, Kv, Node, OAuth2, Part, Request,
};
use super::{list_dir, read_text};
use crate::error::Result;

pub(crate) fn read(dir: &Path) -> Result<Collection> {
    let mut collection = Collection::default();
    let config: serde_json::Value = match read_text(&dir.join("bruno.json")) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => serde_json::Value::Null,
    };
    collection.name = config
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "Bruno collection".into());
    let ignore: Vec<String> = config
        .get("ignore")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if let Ok(text) = read_text(&dir.join("collection.bru")) {
        let file = bru::parse(&text);
        collection.common = common(&file, true);
    }
    collection.children = read_dir(dir, dir, &ignore)?;

    let env_dir = dir.join("environments");
    for path in list_dir(&env_dir)? {
        if path.extension().is_some_and(|e| e == "bru") {
            let text = read_text(&path)?;
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            collection
                .environments
                .push(environment(&name, &bru::parse(&text)));
        }
    }
    Ok(collection)
}

fn read_dir(root: &Path, dir: &Path, ignore: &[String]) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for path in list_dir(dir)? {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.starts_with('.') || ignore.contains(&file_name) {
            continue;
        }
        if path.is_dir() {
            if (dir == root && file_name == "environments") || file_name == "node_modules" {
                continue;
            }
            let folder_file = read_text(&path.join("folder.bru"))
                .map(|t| bru::parse(&t))
                .unwrap_or_default();
            let folder = Folder {
                name: folder_file
                    .value("meta", "name")
                    .unwrap_or_else(|| file_name.clone()),
                seq: folder_file
                    .value("meta", "seq")
                    .and_then(|s| s.parse().ok()),
                common: common(&folder_file, true),
                children: read_dir(root, &path, ignore)?,
            };
            nodes.push(Node::Folder(folder));
        } else if path.extension().is_some_and(|e| e == "bru")
            && file_name != "collection.bru"
            && file_name != "folder.bru"
        {
            let text = read_text(&path)?;
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            nodes.push(Node::Request(request(&stem, &bru::parse(&text))));
        }
    }
    Ok(nodes)
}

fn kvs(pairs: Vec<Pair>) -> Vec<Kv> {
    pairs
        .into_iter()
        .map(|p| Kv {
            name: p.name,
            value: p.value,
            enabled: p.enabled,
        })
        .collect()
}

/// Headers, auth, variables, scripts, tests, asserts and docs. `auth_block`: collections and
/// folders keep the mode in an `auth { mode: ... }` block; requests in their method block.
fn common(file: &BruFile, auth_block: bool) -> Common {
    let mode = if auth_block {
        file.value("auth", "mode")
    } else {
        file.http()
            .and_then(|(_, pairs)| pairs.into_iter().find(|p| p.name == "auth"))
            .map(|p| p.value)
    };
    Common {
        headers: kvs(file.dict("headers")),
        auth: auth(file, mode.as_deref().unwrap_or("none")),
        vars: kvs(file.dict("vars:pre-request"))
            .into_iter()
            .map(|kv| Kv {
                name: kv.name.trim_start_matches('@').to_string(),
                ..kv
            })
            .collect(),
        post_vars: kvs(file.dict("vars:post-response"))
            .into_iter()
            .map(|kv| Kv {
                name: kv.name.trim_start_matches('@').to_string(),
                ..kv
            })
            .collect(),
        script_pre: file.text("script:pre-request").unwrap_or_default(),
        script_post: file.text("script:post-response").unwrap_or_default(),
        tests: file.text("tests").unwrap_or_default(),
        asserts: kvs(file.dict("assert")),
        docs: file.text("docs").unwrap_or_default(),
    }
}

fn auth(file: &BruFile, mode: &str) -> Auth {
    let v = |block: &str, key: &str| file.value(block, key).unwrap_or_default();
    match mode {
        "inherit" => Auth::Inherit,
        "none" | "" => Auth::None,
        "basic" => Auth::Basic {
            username: v("auth:basic", "username"),
            password: v("auth:basic", "password"),
        },
        "bearer" => Auth::Bearer {
            token: v("auth:bearer", "token"),
        },
        "apikey" => Auth::ApiKey {
            key: v("auth:apikey", "key"),
            value: v("auth:apikey", "value"),
            in_query: v("auth:apikey", "placement") == "queryparams",
        },
        "oauth2" => {
            let o = |key: &str| v("auth:oauth2", key);
            let placement = o("token_placement");
            Auth::OAuth2(Box::new(OAuth2 {
                grant: o("grant_type"),
                token_url: o("access_token_url"),
                auth_url: o("authorization_url"),
                callback_url: o("callback_url"),
                client_id: o("client_id"),
                client_secret: o("client_secret"),
                scope: o("scope"),
                pkce: o("pkce") == "true",
                credentials_in_body: o("credentials_placement") != "basic_auth_header",
                username: o("username"),
                password: o("password"),
                token_query_key: (placement == "url").then(|| {
                    let key = o("token_query_key");
                    if key.is_empty() {
                        "access_token".into()
                    } else {
                        key
                    }
                }),
                header_prefix: o("token_header_prefix"),
            }))
        }
        other => Auth::Unsupported(other.to_string()),
    }
}

fn request(stem: &str, file: &BruFile) -> Request {
    let name = file
        .value("meta", "name")
        .unwrap_or_else(|| stem.to_string());
    let kind = file.value("meta", "type").unwrap_or_else(|| "http".into());
    let (method, http) = file.http().unwrap_or_else(|| ("GET".into(), Vec::new()));
    let http_value = |key: &str| {
        http.iter()
            .find(|p| p.name == key)
            .map(|p| p.value.clone())
            .unwrap_or_default()
    };
    let mut query = kvs(file.dict("params:query"));
    query.extend(kvs(file.dict("query")));
    let body = match http_value("body").as_str() {
        "json" => Body::Json(
            file.text("body:json")
                .or_else(|| file.text("body"))
                .unwrap_or_default(),
        ),
        "text" => Body::Text(file.text("body:text").unwrap_or_default()),
        "xml" => Body::Xml(file.text("body:xml").unwrap_or_default()),
        "sparql" => Body::Sparql(file.text("body:sparql").unwrap_or_default()),
        "graphql" => Body::GraphQl {
            query: file.text("body:graphql").unwrap_or_default(),
            variables: file.text("body:graphql:vars").unwrap_or_default(),
        },
        "formUrlEncoded" => Body::Form(kvs(file.dict("body:form-urlencoded"))),
        "multipartForm" => Body::Multipart(
            file.dict("body:multipart-form")
                .into_iter()
                .map(|p| {
                    let value = strip_content_type(&p.value);
                    match value
                        .strip_prefix("@file(")
                        .and_then(|v| v.strip_suffix(')'))
                    {
                        Some(paths) => Part {
                            name: p.name,
                            value: String::new(),
                            files: paths
                                .split('|')
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                                .collect(),
                            enabled: p.enabled,
                        },
                        None => Part {
                            name: p.name,
                            value,
                            files: Vec::new(),
                            enabled: p.enabled,
                        },
                    }
                })
                .collect(),
        ),
        "file" => Body::File(
            file.dict("body:file")
                .into_iter()
                .filter(|p| p.enabled)
                .find_map(|p| {
                    strip_content_type(&p.value)
                        .strip_prefix("@file(")
                        .and_then(|v| v.strip_suffix(')'))
                        .map(str::to_string)
                }),
        ),
        _ => Body::None,
    };
    let timeout_ms = file
        .value("settings", "timeout")
        .and_then(|t| t.parse::<u32>().ok())
        .filter(|t| *t > 0);
    Request {
        name,
        seq: file
            .value("meta", "seq")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
        kind,
        method,
        url: http_value("url"),
        query,
        path_params: kvs(file.dict("params:path")),
        body,
        timeout_ms,
        common: common(file, false),
    }
}

/// `value @contentType(x)` → `value` (Frida picks content types itself).
fn strip_content_type(value: &str) -> String {
    match value.rfind("@contentType(") {
        Some(pos) if value.trim_end().ends_with(')') => value[..pos].trim_end().to_string(),
        _ => value.to_string(),
    }
}

fn environment(name: &str, file: &BruFile) -> Env {
    let mut vars: Vec<EnvVar> = file
        .dict("vars")
        .into_iter()
        .map(|p| EnvVar {
            name: p.name,
            value: p.value,
            secret: false,
            enabled: p.enabled,
        })
        .collect();
    for secret in file.list("vars:secret") {
        let (name, enabled) = match secret.strip_prefix('~') {
            Some(n) => (n.to_string(), false),
            None => (secret.clone(), true),
        };
        match vars.iter_mut().find(|v| v.name == name) {
            Some(v) => {
                v.secret = true;
                v.value.clear();
            }
            None => vars.push(EnvVar {
                name,
                value: String::new(),
                secret: true,
                enabled,
            }),
        }
    }
    Env {
        name: name.to_string(),
        vars,
    }
}
