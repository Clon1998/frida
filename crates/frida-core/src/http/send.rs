//! Sending a [`PreparedRequest`] with reqwest (PLAN.md §7, step 7).

use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::prepare::{PreparedBody, PreparedPart, PreparedRequest};

/// Redirects are followed up to this many hops (PLAN.md §7).
const MAX_REDIRECTS: usize = 10;

/// What came back from the server. The body is kept as raw bytes; [`super::ResponseView`]
/// turns it into something the UI can display.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// From just before sending until the whole body was read.
    pub elapsed: Duration,
    /// The URL after redirects.
    pub final_url: String,
}

/// Why sending failed. Each variant maps to a clear message in the UI.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("the URL {url:?} is not valid: {message}")]
    InvalidUrl { url: String, message: String },

    #[error("the request timed out after {} ms", .0.as_millis())]
    Timeout(Duration),

    #[error("the request was cancelled")]
    Cancelled,

    #[error("could not connect: {0}")]
    Connect(String),

    #[error("TLS problem: {0}")]
    Tls(String),

    #[error("could not read {path}: {message}")]
    File { path: String, message: String },

    #[error("{0}")]
    Other(String),
}

/// Sends the request and reads the full response body.
///
/// `cancel` lets the UI abort an in-flight request. `tokio::select!` waits for whichever
/// finishes first, the request or the cancellation, and drops the other one. Dropping a
/// future in Rust cancels it (unlike a Java `Future`, which you must cancel explicitly), so
/// the connection is closed as soon as the user clicks "Cancel".
pub async fn send(
    prepared: &PreparedRequest,
    cancel: CancellationToken,
) -> Result<HttpResponse, SendError> {
    tokio::select! {
        result = send_inner(prepared) => result,
        () = cancel.cancelled() => Err(SendError::Cancelled),
    }
}

async fn send_inner(prepared: &PreparedRequest) -> Result<HttpResponse, SendError> {
    let client = build_client(prepared).await?;

    let url = reqwest::Url::parse(&prepared.url).map_err(|e| SendError::InvalidUrl {
        url: prepared.url.clone(),
        message: e.to_string(),
    })?;
    let method = reqwest::Method::from_bytes(prepared.method.as_bytes())
        .map_err(|e| SendError::Other(format!("invalid method: {e}")))?;

    let mut builder = client.request(method, url);
    for (name, value) in &prepared.headers {
        builder = builder.header(name, value);
    }
    builder = match &prepared.body {
        PreparedBody::None => builder,
        PreparedBody::Bytes(bytes) => builder.body(bytes.clone()),
        PreparedBody::Form(fields) => builder.form(fields),
        PreparedBody::Multipart(parts) => builder.multipart(build_multipart(parts).await?),
    };

    let started = Instant::now();
    let response = builder
        .send()
        .await
        .map_err(|e| map_error(e, prepared.timeout))?;

    let status = response.status();
    let final_url = response.url().to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                String::from_utf8_lossy(value.as_bytes()).to_string(),
            )
        })
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|e| map_error(e, prepared.timeout))?
        .to_vec();

    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body,
        elapsed: started.elapsed(),
        final_url,
    })
}

/// A new client per send keeps M1 simple: TLS settings can differ per collection. Connection
/// reuse and per-collection cookie jars come with M2.
async fn build_client(prepared: &PreparedRequest) -> Result<reqwest::Client, SendError> {
    let mut certs = Vec::new();
    for path in &prepared.ca_cert_paths {
        let pem = tokio::fs::read(path).await.map_err(|e| SendError::File {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let mut bundle = reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| {
            SendError::Tls(format!(
                "{} is not a valid PEM certificate: {e}",
                path.display()
            ))
        })?;
        certs.append(&mut bundle);
    }

    reqwest::Client::builder()
        .timeout(prepared.timeout)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        // Custom CAs are trusted *in addition to* the OS roots (PLAN.md §7).
        .tls_certs_merge(certs)
        .tls_danger_accept_invalid_certs(!prepared.tls_verify)
        .build()
        .map_err(|e| SendError::Tls(error_chain(&e)))
}

async fn build_multipart(parts: &[PreparedPart]) -> Result<reqwest::multipart::Form, SendError> {
    let mut form = reqwest::multipart::Form::new();
    for part in parts {
        form = match part {
            PreparedPart::Text { name, value } => form.text(name.clone(), value.clone()),
            PreparedPart::File { name, path } => {
                let bytes = tokio::fs::read(path).await.map_err(|e| SendError::File {
                    path: path.display().to_string(),
                    message: e.to_string(),
                })?;
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                form.part(
                    name.clone(),
                    reqwest::multipart::Part::bytes(bytes).file_name(file_name),
                )
            }
        };
    }
    Ok(form)
}

fn map_error(error: reqwest::Error, timeout: Duration) -> SendError {
    let message = error_chain(&error);
    if error.is_timeout() {
        SendError::Timeout(timeout)
    } else if message.to_lowercase().contains("certificate") || message.contains("tls") {
        SendError::Tls(message)
    } else if error.is_connect() {
        SendError::Connect(message)
    } else {
        SendError::Other(message)
    }
}

/// reqwest errors wrap their cause (e.g. "error sending request" → "connection refused").
/// This joins the whole chain so the user sees the actual reason.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}
