//! The console (like Postman's and Bruno's): the raw request as sent and the raw response of
//! every send, for debugging. Secret values are masked, as everywhere outside the send itself.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::prepare::{PreparedBody, PreparedRequest};
use super::send::HttpResponse;
use crate::mask::Masker;
use crate::model::KeyValue;
use crate::vars::iso_timestamp;

/// Bodies are shown up to this size in the console; it is a log, not the response viewer.
pub const MAX_CONSOLE_BODY: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleEntry {
    pub id: String,
    pub timestamp: String,
    pub request_name: String,
    pub request_path: String,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<KeyValue>,
    pub request_body: Option<ConsoleBody>,
    /// `None` when there was no response (see `error`).
    pub response: Option<ConsoleResponse>,
    pub error: Option<String>,
    /// `via proxy http://proxy:8080` when the request went through a manual proxy (never
    /// with the proxy's login).
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleResponse {
    pub http_version: String,
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    pub body: Option<ConsoleBody>,
    pub time_ms: u32,
    pub size_bytes: u32,
    /// The URL after redirects.
    pub final_url: String,
}

/// A body as text. Binary bodies are summarized instead of shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleBody {
    pub text: String,
    /// The body was longer than [`MAX_CONSOLE_BODY`] and was cut.
    pub truncated: bool,
}

impl ConsoleEntry {
    /// From the request exactly as sent and its outcome; every `sensitive` value is masked.
    pub fn from_send(
        id: String,
        request_name: &str,
        request_path: &str,
        sent: &PreparedRequest,
        outcome: Result<&HttpResponse, String>,
        sensitive: &[String],
    ) -> Self {
        let m = Masker::new(sensitive);
        let request_body = match &sent.body {
            PreparedBody::None => None,
            PreparedBody::Bytes(bytes) => Some(body(&m, bytes)),
            PreparedBody::Form(fields) => {
                let text: Vec<String> = fields.iter().map(|(k, v)| format!("{k}={v}")).collect();
                Some(body(&m, text.join("&").as_bytes()))
            }
            PreparedBody::Multipart(parts) => {
                let text: Vec<String> = parts
                    .iter()
                    .map(|part| match part {
                        super::PreparedPart::Text { name, value } => format!("{name}={value}"),
                        super::PreparedPart::File { name, path } => {
                            format!("{name}=@{}", path.display())
                        }
                    })
                    .collect();
                Some(body(
                    &m,
                    format!("(multipart form)\n{}", text.join("\n")).as_bytes(),
                ))
            }
        };
        // The headers as they went out (with `host`, `user-agent`, ...) when known.
        let request_headers = match outcome {
            Ok(r) if !r.request_headers.is_empty() => headers(&m, &r.request_headers),
            _ => headers(&m, &sent.headers),
        };
        let (response, error) = match outcome {
            Ok(r) => (
                Some(ConsoleResponse {
                    http_version: r.http_version.clone(),
                    status: r.status,
                    status_text: r.status_text.clone(),
                    headers: headers(&m, &r.headers),
                    body: (!r.body.is_empty()).then(|| body(&m, &r.body)),
                    time_ms: u32::try_from(r.elapsed.as_millis()).unwrap_or(u32::MAX),
                    size_bytes: u32::try_from(r.body.len()).unwrap_or(u32::MAX),
                    final_url: m.text(&r.final_url),
                }),
                None,
            ),
            Err(error) => (None, Some(m.text(&error))),
        };
        Self {
            id,
            timestamp: now_iso(),
            request_name: request_name.to_string(),
            request_path: request_path.to_string(),
            method: sent.method.clone(),
            url: m.text(&sent.url),
            request_headers,
            request_body,
            response,
            error,
            proxy: sent.proxy.describe_for(&sent.url),
        }
    }

    /// For a request that failed before it could be sent (e.g. an invalid URL): shows it as
    /// typed, with `{{placeholders}}`, so it contains no secret values.
    pub fn not_sent(
        id: String,
        request_name: &str,
        request_path: &str,
        method: &str,
        url: &str,
        error: String,
    ) -> Self {
        Self {
            id,
            timestamp: now_iso(),
            request_name: request_name.to_string(),
            request_path: request_path.to_string(),
            method: method.to_string(),
            url: url.to_string(),
            request_headers: Vec::new(),
            request_body: None,
            response: None,
            error: Some(error),
            proxy: None,
        }
    }
}

fn headers(m: &Masker, headers: &[(String, String)]) -> Vec<KeyValue> {
    headers
        .iter()
        .map(|(name, value)| KeyValue::new(name.clone(), m.text(value)))
        .collect()
}

fn body(m: &Masker, bytes: &[u8]) -> ConsoleBody {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let masked = m.text(text);
            let truncated = masked.len() > MAX_CONSOLE_BODY;
            let mut cut = MAX_CONSOLE_BODY.min(masked.len());
            // Never cut inside a multi-byte character.
            while !masked.is_char_boundary(cut) {
                cut -= 1;
            }
            ConsoleBody {
                text: masked[..cut].to_string(),
                truncated,
            }
        }
        Err(_) => ConsoleBody {
            text: format!("<{} bytes of binary data>", bytes.len()),
            truncated: false,
        },
    }
}

fn now_iso() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    iso_timestamp(millis)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn sent(body: PreparedBody) -> PreparedRequest {
        PreparedRequest {
            method: "POST".into(),
            url: "http://x/?key=SECRET".into(),
            headers: vec![("Authorization".into(), "Bearer SECRET".into())],
            body,
            timeout: Duration::from_secs(1),
            tls_verify: true,
            ca_cert_paths: vec![],
            proxy: Default::default(),
        }
    }

    fn response(body: Vec<u8>) -> HttpResponse {
        HttpResponse {
            status: 201,
            status_text: "Created".into(),
            headers: vec![("x-echo".into(), "SECRET".into())],
            body,
            elapsed: Duration::from_millis(7),
            final_url: "http://x/".into(),
            http_version: "HTTP/1.1".into(),
            request_headers: Vec::new(),
        }
    }

    #[test]
    fn masks_secrets_everywhere() {
        let entry = ConsoleEntry::from_send(
            "1".into(),
            "Create",
            "c.yaml",
            &sent(PreparedBody::Bytes(b"{\"t\":\"SECRET\"}".to_vec())),
            Ok(&response(b"echo SECRET".to_vec())),
            &["SECRET".into()],
        );
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("SECRET"), "{json}");
        let response = entry.response.unwrap();
        assert_eq!((response.status, response.time_ms), (201, 7));
        assert_eq!(response.http_version, "HTTP/1.1");
    }

    #[test]
    fn large_and_binary_bodies_are_shortened() {
        let big = "é".repeat(MAX_CONSOLE_BODY); // 2 bytes each: forces a cut at a char boundary
        let entry = ConsoleEntry::from_send(
            "1".into(),
            "A",
            "a.yaml",
            &sent(PreparedBody::Bytes(big.into_bytes())),
            Ok(&response(vec![0xff, 0xfe, 0x00])),
            &[],
        );
        let request_body = entry.request_body.unwrap();
        assert!(request_body.truncated);
        assert!(request_body.text.len() <= MAX_CONSOLE_BODY);
        assert_eq!(
            entry.response.unwrap().body.unwrap().text,
            "<3 bytes of binary data>"
        );
    }

    #[test]
    fn errors_without_a_response() {
        let entry = ConsoleEntry::from_send(
            "1".into(),
            "A",
            "a.yaml",
            &sent(PreparedBody::None),
            Err("connection refused (key SECRET)".into()),
            &["SECRET".into()],
        );
        assert!(entry.response.is_none());
        assert_eq!(entry.error.as_deref(), Some("connection refused (key ***)"));
    }
}
