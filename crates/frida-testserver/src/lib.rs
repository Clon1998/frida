//! A tiny local HTTP server for tests and for trying Frida by hand.
//!
//! Routes:
//! - `/slow?ms=N`      waits N ms (default 2000), then echoes
//! - `/redirect`       302 to `/echo`
//! - `/image.png`      a 1x1 PNG
//! - `/status/{code}`  responds with that status code
//! - `/cookies/set?k=v` sets each query pair as a cookie (`Set-Cookie`)
//! - `/gzip`           echoes like `/echo`, gzip-compressed (`Content-Encoding: gzip`)
//! - `/sample`         a sample JSON resource (a pet with items, a token and a date) plus the
//!   request under `"echo"`; what the script and test snippets are written against
//! - anything else     echoes the request back as JSON: method, path, query, headers, body
//!
//! - `/oauth/*`        a mock OAuth2 provider, see the `oauth` module
//!
//! [`spawn_tls`] serves the same routes over HTTPS with a throwaway CA.

mod oauth;
mod proxy;
mod tls;

pub use proxy::{ProxyServer, spawn_proxy};
pub use tls::{TlsServer, spawn_tls};

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, Request};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, get};
use serde_json::{Map, Value, json};

/// The smallest valid PNG: one transparent pixel.
const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

pub fn router() -> Router {
    Router::new()
        .route("/slow", any(slow))
        .route("/redirect", any(|| async { Redirect::to("/echo") }))
        .route(
            "/image.png",
            get(|| async { ([(header::CONTENT_TYPE, "image/png")], PIXEL_PNG) }),
        )
        .route("/status/{code}", any(status))
        .route("/cookies/set", get(set_cookies))
        .route("/gzip", any(gzip))
        .route("/sample", any(sample))
        .merge(oauth::router())
        .fallback(echo)
}

/// Starts the server on `addr` in the background and returns the bound address.
/// Pass port 0 to get a free port (what the tests do).
pub async fn spawn(addr: SocketAddr) -> std::io::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router())
            .await
            .expect("test server crashed");
    });
    Ok(bound)
}

async fn echo(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let headers: Map<String, Value> = headers
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                Value::String(String::from_utf8_lossy(value.as_bytes()).to_string()),
            )
        })
        .collect();
    let payload = json!({
        "method": method.as_str(),
        "path": uri.path(),
        "query": uri.query().unwrap_or(""),
        "headers": headers,
        "body": String::from_utf8_lossy(&body),
        "bodyLength": body.len(),
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        payload.to_string(),
    )
        .into_response()
}

/// A typical JSON API response, for the script/test snippets (`frida_script::snippets`).
/// `echo` holds the request as received, so tests can see what pre-request scripts changed.
async fn sample(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let echoed = echo(method, uri, headers, body).await;
    let request: Value = serde_json::from_slice(
        &axum::body::to_bytes(echoed.into_body(), usize::MAX)
            .await
            .unwrap_or_default(),
    )
    .unwrap_or(Value::Null);
    let payload = json!({
        "id": 42,
        "name": "Rex",
        "status": "available",
        "tags": ["friendly", "small"],
        "items": [{ "id": 1, "price": 9.5 }, { "id": 2, "price": 3 }],
        "token": "sample-token-123",
        "createdAt": "2026-10-16T10:00:00Z",
        "echo": request,
    });
    (
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::HeaderName::from_static("x-request-id"), "req-42"),
        ],
        payload.to_string(),
    )
        .into_response()
}

async fn gzip(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    use std::io::Write;
    let echoed = echo(method, uri, headers, body).await;
    let plain = axum::body::to_bytes(echoed.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let compressed = encoder
        .write_all(&plain)
        .and_then(|()| encoder.finish())
        .unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_ENCODING, "gzip"),
        ],
        compressed,
    )
        .into_response()
}

async fn slow(
    Query(query): Query<std::collections::HashMap<String, u64>>,
    request: Request,
) -> Response {
    let ms = query.get("ms").copied().unwrap_or(2_000);
    tokio::time::sleep(Duration::from_millis(ms)).await;
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .unwrap_or_default();
    echo(parts.method, parts.uri, parts.headers, bytes).await
}

async fn status(Path(code): Path<u16>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST);
    (status, format!("status {code}")).into_response()
}

async fn set_cookies(Query(pairs): Query<std::collections::BTreeMap<String, String>>) -> Response {
    let mut response = "cookies set".into_response();
    for (name, value) in pairs {
        let cookie = format!("{name}={value}; Path=/");
        if let Ok(header) = header::HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, header);
        }
    }
    response
}
