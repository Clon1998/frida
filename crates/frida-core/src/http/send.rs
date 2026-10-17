//! Sending a [`PreparedRequest`] with reqwest (PLAN.md §7, step 7).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use reqwest::cookie::CookieStore;
use reqwest::header::{self, HeaderMap, HeaderValue};

use super::cookies::CookieJar;
use super::prepare::{PreparedBody, PreparedPart, PreparedRequest};

/// Redirects are followed up to this many hops (PLAN.md §7).
const MAX_REDIRECTS: usize = 10;

/// Sent unless the request sets its own `User-Agent` (Postman sends `PostmanRuntime/x.y`).
/// Some APIs, e.g. GitHub's, reject requests without one.
const USER_AGENT: &str = concat!("frida-http/", env!("CARGO_PKG_VERSION"));

/// The encodings [`super::decode`] can undo.
const ACCEPT_ENCODING: &str = "gzip, deflate, br, zstd";

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
    /// e.g. `HTTP/1.1` or `HTTP/2.0`, for the console's status line.
    pub http_version: String,
    /// The request headers as they went out, including those added on the way (`host`,
    /// `user-agent`, `cookie`, `content-length`, ...), for the console. Empty when unknown.
    pub request_headers: Vec<(String, String)>,
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

    /// The request was to go through the proxy, which couldn't be reached.
    #[error("could not reach the proxy {proxy}: {message}")]
    Proxy { proxy: String, message: String },

    /// The proxy wants a (different) login: `407 Proxy Authentication Required`.
    #[error("the proxy {proxy} refused the request: 407 Proxy Authentication Required")]
    ProxyAuth { proxy: String },

    #[error("{0}")]
    Other(String),
}

/// Sends the request and reads the full response body. With a cookie jar, cookies are sent
/// and the response's `Set-Cookie` headers are stored in it.
///
/// `cancel` lets the UI abort an in-flight request. `tokio::select!` waits for whichever
/// finishes first, the request or the cancellation, and drops the other one. Dropping a
/// future in Rust cancels it (unlike a Java `Future`, which you must cancel explicitly), so
/// the connection is closed as soon as the user clicks "Cancel".
pub async fn send(
    prepared: &PreparedRequest,
    cookies: Option<Arc<CookieJar>>,
    cancel: CancellationToken,
) -> Result<HttpResponse, SendError> {
    tokio::select! {
        result = send_inner(prepared, cookies) => result,
        () = cancel.cancelled() => Err(SendError::Cancelled),
    }
}

async fn send_inner(
    prepared: &PreparedRequest,
    cookies: Option<Arc<CookieJar>>,
) -> Result<HttpResponse, SendError> {
    // Decompression is done by `decode` below, so the response keeps its `content-encoding`.
    let client = build_client(prepared, cookies.clone())
        .await?
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .map_err(|e| SendError::Tls(error_chain(&e)))?;

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
    // Built first (instead of `builder.send()`) so the final headers can be read for the console.
    let mut request = builder
        .build()
        .map_err(|e| SendError::Other(error_chain(&e)))?;
    add_default_headers(request.headers_mut());
    let request_headers = wire_headers(&request, cookies.as_deref());

    // The proxy this request goes through (manual proxies only), to explain proxy errors.
    let via = prepared
        .proxy
        .proxy_for(&prepared.url)
        .map(|p| p.url.clone());
    let started = Instant::now();
    let response = client.execute(request).await.map_err(|e| {
        let error = map_error(e, prepared.timeout);
        match &via {
            Some(proxy) => proxy_error(proxy, error),
            None => error,
        }
    })?;

    let status = response.status();
    // Plain-HTTP requests travel inside the proxy's own response, so its 407 arrives as an
    // ordinary response; HTTPS requests get it as a failed CONNECT (see `proxy_error`).
    if let Some(proxy) = &via
        && status == reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED
    {
        return Err(SendError::ProxyAuth {
            proxy: proxy.clone(),
        });
    }
    let final_url = response.url().to_string();
    // `Version`'s Debug output is already the protocol name, e.g. "HTTP/1.1".
    let http_version = format!("{:?}", response.version());
    let headers = header_pairs(response.headers());
    let content_encoding = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let raw_body = response
        .bytes()
        .await
        .map_err(|e| map_error(e, prepared.timeout))?
        .to_vec();
    let elapsed = started.elapsed();
    // Decompressing is CPU work, so it runs on a blocking thread when there is any.
    let body = match content_encoding {
        None => raw_body,
        Some(encoding) => {
            tokio::task::spawn_blocking(move || super::decode::decode(Some(&encoding), raw_body))
                .await
                .map_err(|e| SendError::Other(e.to_string()))?
        }
    };

    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body,
        elapsed,
        final_url,
        http_version,
        request_headers,
    })
}

/// Headers every request gets unless it sets them itself, like Postman's.
fn add_default_headers(headers: &mut HeaderMap) {
    for (name, value) in [
        (header::USER_AGENT, USER_AGENT),
        (header::ACCEPT, "*/*"),
        (header::ACCEPT_ENCODING, ACCEPT_ENCODING),
    ] {
        headers
            .entry(name)
            .or_insert_with(|| HeaderValue::from_static(value));
    }
}

/// The request's headers plus those the HTTP stack adds while sending: `host` and
/// `content-length` (hyper) and `cookie` (from the jar, unless the request sets one).
/// Lowercase, as hyper writes them. Covers the first hop only, not redirects.
fn wire_headers(request: &reqwest::Request, cookies: Option<&CookieJar>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(host) = request.url().host_str() {
        // `port()` is `None` for the scheme's default port, which is left out of `host` too.
        let host = match request.url().port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        headers.push(("host".to_string(), host));
    }
    headers.extend(header_pairs(request.headers()));
    if !request.headers().contains_key(header::COOKIE)
        && let Some(cookie) = cookies.and_then(|jar| jar.cookies(request.url()))
    {
        headers.push((
            "cookie".to_string(),
            String::from_utf8_lossy(cookie.as_bytes()).to_string(),
        ));
    }
    if !request.headers().contains_key(header::CONTENT_LENGTH)
        && let Some(bytes) = request.body().and_then(|b| b.as_bytes())
    {
        headers.push(("content-length".to_string(), bytes.len().to_string()));
    }
    headers
}

fn header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                String::from_utf8_lossy(value.as_bytes()).to_string(),
            )
        })
        .collect()
}

/// A client builder with the request's TLS settings and timeout; callers may tweak it before
/// `build()`. A new client per send keeps this simple: TLS settings can differ per collection. Cookies
/// still persist across sends because the jar is shared. (Connection reuse could be added by
/// caching clients per collection later.)
pub(crate) async fn build_client(
    prepared: &PreparedRequest,
    cookies: Option<Arc<CookieJar>>,
) -> Result<reqwest::ClientBuilder, SendError> {
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

    let mut builder = reqwest::Client::builder();
    if let Some(jar) = cookies {
        builder = builder.cookie_provider(jar);
    }
    builder = builder
        .timeout(prepared.timeout)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        // Custom CAs are trusted *in addition to* the OS roots (PLAN.md §7).
        .tls_certs_merge(certs)
        .tls_danger_accept_invalid_certs(!prepared.tls_verify);
    Ok(prepared.proxy.apply(builder))
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

/// Explains a failed request that went to the proxy: it refused the login (407 on the
/// `CONNECT` of an HTTPS request), or it couldn't be reached at all.
fn proxy_error(proxy: &str, error: SendError) -> SendError {
    let message = error.to_string();
    let lower = message.to_lowercase();
    if lower.contains("proxy authorization required")
        || lower.contains("proxy authentication required")
        || lower.contains("407")
    {
        return SendError::ProxyAuth {
            proxy: proxy.to_string(),
        };
    }
    match error {
        // "unsuccessful tunnel": the proxy answered, but couldn't reach the server itself.
        SendError::Connect(message) if lower.contains("tunnel") => SendError::Connect(message),
        SendError::Connect(message) => SendError::Proxy {
            proxy: proxy.to_string(),
            message,
        },
        other => other,
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
