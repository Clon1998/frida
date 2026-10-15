//! Turning a pasted curl command into a request (PLAN.md §11.3).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::model::{Auth, AuthScheme, BodyMode, KeyValue, MultipartField, MultipartKind, Request};

/// The imported request plus anything that couldn't be carried over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CurlImport {
    pub request: Request,
    pub warnings: Vec<String>,
}

/// Options that take no value. Known so that combined short flags (`-sSL`) can be split and
/// so they don't produce warnings.
const IGNORED_FLAGS: &[&str] = &[
    "-s",
    "--silent",
    "-S",
    "--show-error",
    "-L",
    "--location",
    "-v",
    "--verbose",
    "-i",
    "--include",
    "-f",
    "--fail",
    "--fail-with-body",
    "--compressed",
    "-#",
    "--progress-bar",
    "-N",
    "--no-buffer",
    "--http1.1",
    "--http2",
    "-g",
    "--globoff",
];
/// Options that take a value we don't use.
const IGNORED_WITH_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-w",
    "--write-out",
    "--connect-timeout",
    "--retry",
    "-x",
    "--proxy",
    "--cacert",
    "-E",
    "--cert",
    "--key",
    "-c",
    "--cookie-jar",
    "-r",
    "--range",
];

/// Parses a curl command line. Fails only if it isn't a curl command at all; everything that
/// can't be mapped becomes a warning.
pub fn import_curl(command: &str) -> Result<CurlImport, String> {
    // Join continuation lines of bash (`\`), PowerShell (`` ` ``) and cmd (`^`).
    let mut text = command.trim().to_string();
    for continuation in ["\\\r\n", "\\\n", "`\r\n", "`\n", "^\r\n", "^\n"] {
        text = text.replace(continuation, " ");
    }
    let tokens =
        shell_words::split(&text).map_err(|e| format!("could not read the command: {e}"))?;
    let mut tokens = tokens.into_iter().peekable();
    match tokens.next().as_deref() {
        Some("curl") | Some("curl.exe") => {}
        _ => return Err("the command must start with `curl`".into()),
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut urlencoded: Vec<(String, String)> = Vec::new();
    let mut binary_file: Option<String> = None;
    let mut form: Vec<MultipartField> = Vec::new();
    let mut basic: Option<(String, String)> = None;
    let mut get = false;
    let mut timeout_ms: Option<u32> = None;
    let mut warnings = Vec::new();

    while let Some(token) = tokens.next() {
        // `--header=Value` → `--header`, `Value`
        let (flag, inline) = match token.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (token.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| tokens.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };

        match flag.as_str() {
            "-X" | "--request" => method = Some(value(&flag)?.to_uppercase()),
            "--url" => url = Some(value(&flag)?),
            "-H" | "--header" => {
                let raw = value(&flag)?;
                match raw.split_once(':') {
                    Some((name, v)) => headers.push(KeyValue::new(name.trim(), v.trim())),
                    None => warnings.push(format!("ignored header without a value: {raw}")),
                }
            }
            "-d" | "--data" | "--data-ascii" | "--data-raw" => data.push(value(&flag)?),
            "--data-binary" => {
                let v = value(&flag)?;
                match v.strip_prefix('@') {
                    Some(path) => binary_file = Some(path.to_string()),
                    None => data.push(v),
                }
            }
            "--data-urlencode" => {
                let v = value(&flag)?;
                match v.split_once('=') {
                    Some((name, val)) if !name.is_empty() => {
                        urlencoded.push((name.into(), val.into()))
                    }
                    _ => data.push(v.trim_start_matches('=').to_string()),
                }
            }
            "-F" | "--form" => {
                let v = value(&flag)?;
                let (name, val) = v.split_once('=').unwrap_or((v.as_str(), ""));
                // Drop `;type=...` / `;filename=...` attributes.
                let val = val.split(';').next().unwrap_or_default();
                let (kind, val) = match val.strip_prefix('@') {
                    Some(path) => (MultipartKind::File, path),
                    None => (MultipartKind::Text, val),
                };
                form.push(MultipartField {
                    name: name.to_string(),
                    kind,
                    value: val.to_string(),
                    enabled: true,
                });
            }
            "-u" | "--user" => {
                let v = value(&flag)?;
                let (user, pass) = v.split_once(':').unwrap_or((v.as_str(), ""));
                basic = Some((user.to_string(), pass.to_string()));
            }
            "-b" | "--cookie" => {
                let v = value(&flag)?;
                if v.contains('=') {
                    headers.push(KeyValue::new("Cookie", v));
                } else {
                    warnings.push(format!(
                        "ignored cookie file {v:?}; Frida keeps cookies per collection"
                    ));
                }
            }
            "-A" | "--user-agent" => headers.push(KeyValue::new("User-Agent", value(&flag)?)),
            "-e" | "--referer" => headers.push(KeyValue::new("Referer", value(&flag)?)),
            "-I" | "--head" => method = Some("HEAD".into()),
            "-G" | "--get" => get = true,
            "-k" | "--insecure" => warnings.push(
                "-k: TLS verification is a collection setting in Frida (Collection settings → TLS)"
                    .into(),
            ),
            "-m" | "--max-time" => {
                let v = value(&flag)?;
                match v.parse::<f64>() {
                    Ok(secs) => timeout_ms = Some((secs * 1000.0) as u32),
                    Err(_) => warnings.push(format!("ignored invalid --max-time {v:?}")),
                }
            }
            f if IGNORED_FLAGS.contains(&f) => {}
            f if IGNORED_WITH_VALUE.contains(&f) => {
                let _ = value(f)?;
            }
            // Combined short flags such as `-sSL`.
            f if f.starts_with('-')
                && !f.starts_with("--")
                && f.len() > 2
                && f[1..]
                    .chars()
                    .all(|c| IGNORED_FLAGS.contains(&format!("-{c}").as_str())) => {}
            f if f.starts_with('-') => warnings.push(format!("ignored unsupported option {f}")),
            _ => {
                if url.is_none() {
                    url = Some(token);
                } else {
                    warnings.push(format!("ignored extra argument {token:?}"));
                }
            }
        }
    }

    let url = url.ok_or("the command has no URL")?;
    let mut request = Request::new("");
    request.settings.timeout_ms = timeout_ms;
    if let Some((username, password)) = basic {
        request.auth = Auth::Custom {
            schemes: vec![AuthScheme::Basic { username, password }],
        };
    }

    let content_type = headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_lowercase())
        .unwrap_or_default();
    let has_body =
        !data.is_empty() || !urlencoded.is_empty() || !form.is_empty() || binary_file.is_some();

    let mut final_url = url;
    if get && (!data.is_empty() || !urlencoded.is_empty()) {
        // -G moves the data into the query string.
        let mut pairs: Vec<String> = std::mem::take(&mut data);
        pairs.extend(urlencoded.drain(..).map(|(n, v)| format!("{n}={v}")));
        let sep = if final_url.contains('?') { '&' } else { '?' };
        final_url = format!("{final_url}{sep}{}", pairs.join("&"));
    } else if !form.is_empty() {
        request.body.mode = BodyMode::Multipart;
        request.body.multipart = form;
        // reqwest sets the multipart Content-Type with the boundary itself.
        headers.retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
    } else if let Some(path) = binary_file {
        request.body.mode = BodyMode::Binary;
        request.body.binary_path = path;
    } else if !urlencoded.is_empty()
        || (content_type.contains("x-www-form-urlencoded") && !data.is_empty())
    {
        request.body.mode = BodyMode::FormUrlencoded;
        let mut fields: Vec<KeyValue> = data
            .iter()
            .flat_map(|d| d.split('&'))
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (n, v) = pair.split_once('=').unwrap_or((pair, ""));
                KeyValue::new(url_decode(n), url_decode(v))
            })
            .collect();
        fields.extend(urlencoded.into_iter().map(|(n, v)| KeyValue::new(n, v)));
        request.body.form_url_encoded = fields;
    } else if !data.is_empty() {
        let body = data.join("&");
        let trimmed = body.trim_start();
        if content_type.contains("json")
            || (content_type.is_empty() && (trimmed.starts_with('{') || trimmed.starts_with('[')))
        {
            request.body.mode = BodyMode::Json;
            request.body.json = body;
        } else if content_type.contains("xml")
            || (content_type.is_empty() && trimmed.starts_with('<'))
        {
            request.body.mode = BodyMode::Xml;
            request.body.xml = body;
        } else if content_type.is_empty() && looks_like_form(&body) {
            // curl's own default for -d is application/x-www-form-urlencoded.
            request.body.mode = BodyMode::FormUrlencoded;
            request.body.form_url_encoded = body
                .split('&')
                .map(|pair| {
                    let (n, v) = pair.split_once('=').unwrap_or((pair, ""));
                    KeyValue::new(url_decode(n), url_decode(v))
                })
                .collect();
        } else {
            request.body.mode = BodyMode::Text;
            request.body.text = body;
        }
    }

    // Frida adds the default Content-Type of each body mode itself (see `http::prepare`), so an
    // exact match is redundant; dropping it keeps imported requests clean.
    let default_type = match request.body.mode {
        BodyMode::Json => Some("application/json"),
        BodyMode::Xml => Some("application/xml"),
        BodyMode::Text => Some("text/plain"),
        BodyMode::Binary => Some("application/octet-stream"),
        BodyMode::FormUrlencoded => Some("application/x-www-form-urlencoded"),
        BodyMode::Multipart | BodyMode::None => None,
    };
    if let Some(default_type) = default_type {
        headers.retain(|h| {
            !(h.name.eq_ignore_ascii_case("content-type")
                && h.value.eq_ignore_ascii_case(default_type))
        });
    }

    request.method =
        method.unwrap_or_else(|| if has_body && !get { "POST" } else { "GET" }.to_string());
    request.params.query = query_params(&final_url);
    request.name = default_name(&request.method, &final_url);
    request.url = final_url;
    request.headers = headers;
    Ok(CurlImport { request, warnings })
}

/// `a=1&b=2` (every part has a `=` and no spaces) is treated as form data.
fn looks_like_form(body: &str) -> bool {
    !body.is_empty()
        && body
            .split('&')
            .all(|p| p.contains('=') && !p.contains(char::is_whitespace))
}

fn url_decode(text: &str) -> String {
    let text = text.replace('+', " ");
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Query rows from the URL, kept raw like the UI does.
fn query_params(url: &str) -> Vec<KeyValue> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (n, v) = pair.split_once('=').unwrap_or((pair, ""));
            KeyValue::new(n, v)
        })
        .collect()
}

/// `GET /orders/42` from the method and the URL path.
fn default_name(method: &str, url: &str) -> String {
    let path = reqwest::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.split('?').next().unwrap_or(url).to_string());
    format!("{method} {path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_curl_input() {
        assert!(import_curl("wget https://x").is_err());
        assert!(import_curl("curl -H 'A: b'").is_err());
    }

    #[test]
    fn url_decoding() {
        assert_eq!(url_decode("a%20b+c%2"), "a b c%2");
    }
}
