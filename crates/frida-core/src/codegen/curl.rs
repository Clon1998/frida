//! "Copy as curl" (PLAN.md §11.3): a readable, multi-line curl command for bash or PowerShell.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::auth::OAuth2Manager;
use crate::http::{RequestContext, apply_path_params, merge_headers, normalize_url};
use crate::model::{ApiKeyLocation, AuthScheme, BodyMode, MultipartKind, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    /// bash, zsh, sh: single quotes, `\` line continuation.
    Bash,
    /// PowerShell: single quotes, `` ` `` line continuation, and `curl.exe` (in Windows
    /// PowerShell, `curl` is an alias for `Invoke-WebRequest`).
    PowerShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CurlOptions {
    pub shell: Shell,
    /// Replace `{{variables}}` with their values (default) or keep the placeholders.
    pub resolve_variables: bool,
    /// Off by default: secret values are replaced by `***`. Turning it on requires an explicit
    /// confirmation in the UI (PLAN.md §6).
    pub include_secrets: bool,
}

/// A request in the shape curl needs. Built by [`curl_request`], rendered by [`render_curl`],
/// and produced by the curl importer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurlRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// `-u user:password`
    pub basic_auth: Option<(String, String)>,
    pub body: CurlBody,
    /// `-k`
    pub insecure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurlBody {
    None,
    /// `--data-raw`
    Raw(String),
    /// `--data-urlencode name=value`, one per field
    Form(Vec<(String, String)>),
    /// `-F`, one per field
    Multipart(Vec<(String, CurlPart)>),
    /// `--data-binary @path`
    File(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurlPart {
    Text(String),
    File(String),
}

/// Builds the curl view of a request: merged headers, path params, body and auth. No I/O:
/// files are referenced by path. OAuth2 uses a cached token if there is one, otherwise a
/// placeholder. Returns the request and the auth values that must be masked as secrets.
pub fn curl_request(
    ctx: RequestContext<'_>,
    request: &Request,
    auth: &[AuthScheme],
    oauth: &OAuth2Manager,
) -> (CurlRequest, Vec<String>) {
    let mut headers = merge_headers(ctx, request);
    let mut url = apply_path_params(&normalize_url(&request.url), request);
    let mut basic_auth = None;
    let mut sensitive = Vec::new();

    let set_header = |headers: &mut Vec<(String, String)>, name: &str, value: String| {
        headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        headers.push((name.to_string(), value));
    };
    for scheme in auth {
        match scheme {
            AuthScheme::Basic { username, password } => {
                sensitive.push(password.clone());
                basic_auth = Some((username.clone(), password.clone()));
            }
            AuthScheme::Bearer { token } => {
                sensitive.push(token.clone());
                set_header(&mut headers, "Authorization", format!("Bearer {token}"));
            }
            AuthScheme::ApiKey {
                location,
                name,
                value,
            } => {
                sensitive.push(value.clone());
                match location {
                    ApiKeyLocation::Header => set_header(&mut headers, name, value.clone()),
                    ApiKeyLocation::Query => url = with_query(&url, name, value),
                }
            }
            AuthScheme::OAuth2(config) => {
                let token = match oauth.cached(config) {
                    Some(token) => {
                        sensitive.push(token.access_token.clone());
                        token.access_token
                    }
                    None => "<oauth2-access-token>".to_string(),
                };
                let placement = &config.token_placement;
                match placement.location {
                    ApiKeyLocation::Header => {
                        let value = if placement.prefix.trim().is_empty() {
                            token
                        } else {
                            format!("{} {token}", placement.prefix.trim())
                        };
                        set_header(&mut headers, &placement.name, value);
                    }
                    ApiKeyLocation::Query => url = with_query(&url, &placement.name, &token),
                }
            }
        }
    }

    let body = &request.body;
    let curl_body = match body.mode {
        BodyMode::None => CurlBody::None,
        BodyMode::Json => CurlBody::Raw(body.json.clone()),
        BodyMode::Xml => CurlBody::Raw(body.xml.clone()),
        BodyMode::Text => CurlBody::Raw(body.text.clone()),
        BodyMode::FormUrlencoded => CurlBody::Form(
            body.form_url_encoded
                .iter()
                .filter(|f| f.enabled && !f.name.is_empty())
                .map(|f| (f.name.clone(), f.value.clone()))
                .collect(),
        ),
        BodyMode::Multipart => CurlBody::Multipart(
            body.multipart
                .iter()
                .filter(|f| f.enabled && !f.name.is_empty())
                .map(|f| {
                    let part = match f.kind {
                        MultipartKind::Text => CurlPart::Text(f.value.clone()),
                        MultipartKind::File => CurlPart::File(f.value.clone()),
                    };
                    (f.name.clone(), part)
                })
                .collect(),
        ),
        BodyMode::Binary => CurlBody::File(body.binary_path.clone()),
    };

    let curl = CurlRequest {
        method: request.method.trim().to_uppercase(),
        url,
        headers,
        basic_auth,
        body: curl_body,
        insecure: !ctx.collection.settings.tls.verify,
    };
    sensitive.retain(|s| !s.is_empty());
    (curl, sensitive)
}

fn with_query(url: &str, name: &str, value: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut parsed) => {
            parsed.query_pairs_mut().append_pair(name, value);
            parsed.to_string()
        }
        Err(_) => {
            let sep = if url.contains('?') { '&' } else { '?' };
            format!("{url}{sep}{name}={value}")
        }
    }
}

/// Renders the command, one option per line. `masked` values (secrets) are replaced by `***`.
pub fn render_curl(request: &CurlRequest, shell: Shell, masked: &[String]) -> String {
    let q = |text: &str| quote(&mask(text, masked), shell);
    // The method and URL go on the first line; every other option on its own line.
    let mut first: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    let method = request.method.as_str();
    let has_body = request.body != CurlBody::None;
    match method {
        "HEAD" => first.push("--head".into()),
        // GET is curl's default; POST is implied by a body.
        "GET" => {}
        "POST" if has_body => {}
        _ => first.push(format!("-X {method}")),
    }
    first.push(q(&request.url));
    for (name, value) in &request.headers {
        parts.push(format!("-H {}", q(&format!("{name}: {value}"))));
    }
    if let Some((user, password)) = &request.basic_auth {
        parts.push(format!("-u {}", q(&format!("{user}:{password}"))));
    }
    match &request.body {
        CurlBody::None => {}
        CurlBody::Raw(text) => parts.push(format!("--data-raw {}", q(text))),
        CurlBody::Form(fields) => {
            for (name, value) in fields {
                parts.push(format!(
                    "--data-urlencode {}",
                    q(&format!("{name}={value}"))
                ));
            }
        }
        CurlBody::Multipart(fields) => {
            for (name, part) in fields {
                let value = match part {
                    CurlPart::Text(text) => format!("{name}={text}"),
                    CurlPart::File(path) => format!("{name}=@{path}"),
                };
                parts.push(format!("-F {}", q(&value)));
            }
        }
        CurlBody::File(path) => parts.push(format!("--data-binary {}", q(&format!("@{path}")))),
    }
    if request.insecure {
        parts.push("-k".into());
    }

    let (program, continuation) = match shell {
        Shell::Bash => ("curl", " \\\n  "),
        Shell::PowerShell => ("curl.exe", " `\n  "),
    };
    let mut out = format!("{program} {}", first.join(" "));
    for part in parts {
        out.push_str(continuation);
        out.push_str(&part);
    }
    out
}

fn mask(text: &str, masked: &[String]) -> String {
    let mut out = text.to_string();
    // Longest first, so a secret that contains another one is masked as a whole.
    let mut secrets: Vec<&String> = masked.iter().filter(|s| !s.is_empty()).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        out = out.replace(secret.as_str(), "***");
    }
    out
}

/// Quotes one argument for the shell. Single quotes keep everything literal in both shells;
/// only the quote itself needs escaping.
fn quote(text: &str, shell: Shell) -> String {
    match shell {
        Shell::Bash => format!("'{}'", text.replace('\'', r"'\''")),
        Shell::PowerShell => format!("'{}'", text.replace('\'', "''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, body: CurlBody) -> CurlRequest {
        CurlRequest {
            method: method.into(),
            url: "https://api.example.com/orders?x=1".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            basic_auth: None,
            body,
            insecure: false,
        }
    }

    #[test]
    fn bash_output_is_multi_line_and_quoted() {
        let out = render_curl(
            &request("POST", CurlBody::Raw(r#"{"name":"O'Brien"}"#.into())),
            Shell::Bash,
            &[],
        );
        assert_eq!(
            out,
            "curl 'https://api.example.com/orders?x=1' \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{\"name\":\"O'\\''Brien\"}'"
        );
    }

    #[test]
    fn powershell_uses_curl_exe_and_backticks() {
        let out = render_curl(
            &request("PUT", CurlBody::Raw("it's".into())),
            Shell::PowerShell,
            &[],
        );
        assert!(out.starts_with("curl.exe -X PUT 'https://"), "{out}");
        assert!(out.contains(" `\n  --data-raw 'it''s'"), "{out}");
    }

    #[test]
    fn secrets_are_masked() {
        let mut req = request("GET", CurlBody::None);
        req.headers
            .push(("Authorization".into(), "Bearer s3cr3t-token".into()));
        req.basic_auth = Some(("alice".into(), "pw-123".into()));
        let out = render_curl(&req, Shell::Bash, &["s3cr3t-token".into(), "pw-123".into()]);
        assert!(!out.contains("s3cr3t") && !out.contains("pw-123"), "{out}");
        assert!(
            out.contains("Bearer ***") && out.contains("-u 'alice:***'"),
            "{out}"
        );
    }

    #[test]
    fn head_get_and_flags() {
        assert!(render_curl(&request("HEAD", CurlBody::None), Shell::Bash, &[]).contains("--head"));
        assert!(!render_curl(&request("GET", CurlBody::None), Shell::Bash, &[]).contains("-X"));
        let mut req = request("DELETE", CurlBody::None);
        req.insecure = true;
        let out = render_curl(&req, Shell::Bash, &[]);
        assert!(out.contains("-X DELETE") && out.ends_with("-k"), "{out}");
    }
}
