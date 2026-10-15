//! End-to-end HTTP tests against the local echo server (`frida-testserver`).

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use frida_core::http::{self, HttpResponse, RequestContext, SendError};
use frida_core::model::{BodyMode, Collection, KeyValue, MultipartField, MultipartKind, Request};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// Starts an echo server on a free port.
async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

fn request(method: &str, url: String) -> Request {
    let mut request = Request::new("test");
    request.method = method.to_string();
    request.url = url;
    request
}

async fn send_in(dir: &Path, request: &Request) -> Result<HttpResponse, SendError> {
    let collection = Collection::new("Test");
    let ctx = RequestContext {
        collection_dir: dir,
        collection: &collection,
        folders: &[],
    };
    let prepared = http::prepare(ctx, request).unwrap();
    http::send(&prepared, CancellationToken::new()).await
}

async fn send(request: &Request) -> Result<HttpResponse, SendError> {
    send_in(Path::new("."), request).await
}

/// Sends and parses the echo server's JSON reply.
async fn echo(request: &Request) -> Value {
    let response = send(request).await.unwrap();
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    serde_json::from_slice(&response.body).unwrap()
}

#[tokio::test]
async fn sends_every_method_including_custom_ones() {
    let addr = server().await;
    for method in [
        "GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PROPFIND",
    ] {
        let reply = echo(&request(method, format!("http://{addr}/echo"))).await;
        assert_eq!(reply["method"], method);
    }
    // HEAD has no response body, so only the status is checked.
    let head = send(&request("HEAD", format!("http://{addr}/echo")))
        .await
        .unwrap();
    assert_eq!(head.status, 200);
}

#[tokio::test]
async fn sends_query_path_params_and_headers() {
    let addr = server().await;
    let mut req = request("GET", format!("{addr}/orders/:id?expand=items"));
    req.params.path = vec![frida_core::model::PathParam {
        name: "id".to_string(),
        value: "42".to_string(),
    }];
    req.headers = vec![KeyValue::new("X-Correlation-Id", "abc")];

    let reply = echo(&req).await;
    assert_eq!(reply["path"], "/orders/42");
    assert_eq!(reply["query"], "expand=items");
    assert_eq!(reply["headers"]["x-correlation-id"], "abc");
}

#[tokio::test]
async fn sends_text_like_bodies_with_content_type() {
    let addr = server().await;
    let cases = [
        (BodyMode::Json, "{\"id\":1}", "application/json"),
        (BodyMode::Xml, "<id>1</id>", "application/xml"),
        (BodyMode::Text, "hello", "text/plain"),
    ];
    for (mode, content, content_type) in cases {
        let mut req = request("POST", format!("http://{addr}/echo"));
        req.body.mode = mode;
        req.body.json = content.to_string();
        req.body.xml = content.to_string();
        req.body.text = content.to_string();

        let reply = echo(&req).await;
        assert_eq!(reply["body"], content, "{mode:?}");
        assert_eq!(reply["headers"]["content-type"], content_type, "{mode:?}");
    }
}

#[tokio::test]
async fn sends_form_urlencoded_body() {
    let addr = server().await;
    let mut req = request("POST", format!("http://{addr}/echo"));
    req.body.mode = BodyMode::FormUrlencoded;
    req.body.form_url_encoded = vec![KeyValue::new("a", "1"), KeyValue::new("b", "x y")];

    let reply = echo(&req).await;
    assert_eq!(reply["body"], "a=1&b=x+y");
    assert_eq!(
        reply["headers"]["content-type"],
        "application/x-www-form-urlencoded"
    );
}

#[tokio::test]
async fn sends_multipart_and_binary_bodies_from_files() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("upload.txt"), "file content").unwrap();

    let mut req = request("POST", format!("http://{addr}/echo"));
    req.body.mode = BodyMode::Multipart;
    req.body.multipart = vec![
        MultipartField {
            name: "note".into(),
            kind: MultipartKind::Text,
            value: "hi".into(),
            enabled: true,
        },
        MultipartField {
            name: "doc".into(),
            kind: MultipartKind::File,
            value: "upload.txt".into(),
            enabled: true,
        },
    ];
    let response = send_in(dir.path(), &req).await.unwrap();
    let reply: Value = serde_json::from_slice(&response.body).unwrap();
    let body = reply["body"].as_str().unwrap();
    assert!(
        reply["headers"]["content-type"]
            .as_str()
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(body.contains("name=\"note\"") && body.contains("hi"));
    assert!(body.contains("filename=\"upload.txt\"") && body.contains("file content"));

    let mut req = request("PUT", format!("http://{addr}/echo"));
    req.body.mode = BodyMode::Binary;
    req.body.binary_path = "upload.txt".to_string();
    let response = send_in(dir.path(), &req).await.unwrap();
    let reply: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(reply["body"], "file content");
    assert_eq!(reply["headers"]["content-type"], "application/octet-stream");
}

#[tokio::test]
async fn captures_status_headers_time_and_final_url_after_redirect() {
    let addr = server().await;
    let response = send(&request("GET", format!("http://{addr}/redirect")))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.final_url, format!("http://{addr}/echo"));
    assert!(
        response
            .headers
            .iter()
            .any(|(n, v)| n == "content-type" && v == "application/json")
    );

    let not_found = send(&request("GET", format!("http://{addr}/status/404")))
        .await
        .unwrap();
    assert_eq!(not_found.status, 404);
    assert_eq!(not_found.status_text, "Not Found");
}

#[tokio::test]
async fn binary_responses_are_kept_as_bytes() {
    let addr = server().await;
    let response = send(&request("GET", format!("http://{addr}/image.png")))
        .await
        .unwrap();
    assert_eq!(&response.body[..4], b"\x89PNG");
    let view = http::ResponseView::new("1".to_string(), &response);
    assert!(view.body_base64.is_some());
    assert_eq!(view.content_type.as_deref(), Some("image/png"));
}

#[tokio::test]
async fn times_out() {
    let addr = server().await;
    let mut req = request("GET", format!("http://{addr}/slow?ms=2000"));
    req.settings.timeout_ms = Some(200);
    let err = send(&req).await.unwrap_err();
    assert!(matches!(err, SendError::Timeout(_)), "{err}");
}

#[tokio::test]
async fn can_be_cancelled() {
    let addr = server().await;
    let req = request("GET", format!("http://{addr}/slow?ms=5000"));
    let ctx_collection = Collection::new("Test");
    let ctx = RequestContext {
        collection_dir: Path::new("."),
        collection: &ctx_collection,
        folders: &[],
    };
    let prepared = http::prepare(ctx, &req).unwrap();

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });

    let started = std::time::Instant::now();
    let err = http::send(&prepared, cancel).await.unwrap_err();
    assert!(matches!(err, SendError::Cancelled), "{err}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn connection_refused_is_a_connect_error() {
    // Bind and immediately drop a listener to get a port nobody listens on.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let err = send(&request("GET", format!("http://127.0.0.1:{port}/")))
        .await
        .unwrap_err();
    assert!(matches!(err, SendError::Connect(_)), "{err}");
}

#[tokio::test]
async fn invalid_urls_are_reported() {
    let err = send(&request("GET", "http://exa mple.com/".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(err, SendError::InvalidUrl { .. }), "{err}");
}

/// Sends `request` with the given collection TLS settings.
async fn send_with_tls(
    dir: &Path,
    request: &Request,
    verify: bool,
    ca_cert_paths: Vec<String>,
) -> Result<HttpResponse, SendError> {
    let mut collection = Collection::new("Test");
    collection.settings.tls.verify = verify;
    collection.settings.tls.ca_cert_paths = ca_cert_paths;
    let ctx = RequestContext {
        collection_dir: dir,
        collection: &collection,
        folders: &[],
    };
    http::send(
        &http::prepare(ctx, request).unwrap(),
        CancellationToken::new(),
    )
    .await
}

#[tokio::test]
async fn tls_untrusted_certificate_is_rejected_by_default() {
    let server = frida_testserver::spawn_tls().await.unwrap();
    let req = request(
        "GET",
        format!("https://localhost:{}/echo", server.addr.port()),
    );
    let err = send_with_tls(Path::new("."), &req, true, vec![])
        .await
        .unwrap_err();
    assert!(matches!(err, SendError::Tls(_)), "{err}");
}

#[tokio::test]
async fn tls_verification_can_be_disabled() {
    let server = frida_testserver::spawn_tls().await.unwrap();
    let req = request(
        "GET",
        format!("https://localhost:{}/echo", server.addr.port()),
    );
    let response = send_with_tls(Path::new("."), &req, false, vec![])
        .await
        .unwrap();
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn tls_custom_ca_is_trusted_with_verification_on() {
    let server = frida_testserver::spawn_tls().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("certs")).unwrap();
    std::fs::write(dir.path().join("certs/test-ca.pem"), &server.ca_pem).unwrap();

    let req = request(
        "GET",
        format!("https://localhost:{}/echo", server.addr.port()),
    );
    let response = send_with_tls(
        dir.path(),
        &req,
        true,
        vec!["certs/test-ca.pem".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(response.status, 200);
}
