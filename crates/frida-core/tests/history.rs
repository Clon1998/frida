//! M4 acceptance (PLAN.md §15): history entries contain no secret values, end to end.

use std::net::SocketAddr;
use std::sync::Arc;

use frida_core::auth::OAuth2Manager;
use frida_core::codegen::{CurlOptions, Shell};
use frida_core::history::{DEFAULT_RETENTION, HistoryStore, NewEntry};
use frida_core::http::{self, ExecuteInput};
use frida_core::model::{Auth, AuthScheme, EnvVariable, Environment, KeyValue, Request};
use frida_core::storage;
use tokio_util::sync::CancellationToken;

const SECRET: &str = "very-secret-token-123";
const API_KEY: &str = "literal-api-key-456";

#[tokio::test]
async fn a_send_with_secrets_leaves_no_trace_in_history_or_curl() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let collection = storage::create_collection(dir.path(), "C").unwrap();

    // A secret environment variable (as filled in from the keychain) used in a header and the
    // body, plus a literal API key in the auth settings.
    let mut env = Environment::new("dev");
    env.variables = vec![EnvVariable {
        name: "token".into(),
        value: Some(SECRET.into()),
        secret: true,
        source: None,
        enabled: true,
    }];
    let mut request = Request::new("Echo");
    request.method = "POST".into();
    request.url = format!("http://{addr}/echo");
    request.headers = vec![KeyValue::new("Authorization", "Bearer {{token}}")];
    request.body.mode = frida_core::model::BodyMode::Json;
    request.body.json = r#"{"token": "{{token}}"}"#.into();
    request.auth = Auth::Custom {
        schemes: vec![AuthScheme::ApiKey {
            location: frida_core::model::ApiKeyLocation::Header,
            name: "x-api-key".into(),
            value: API_KEY.into(),
        }],
    };
    let rel = storage::create_request(dir.path(), "", &request).unwrap();

    let session = http::SessionStore::default();
    let oauth = OAuth2Manager::new(Arc::new(|_| Ok(())));
    let input = || ExecuteInput {
        collection_dir: dir.path(),
        request_rel: &rel,
        request: &request,
        collection_env: Some(&env),
        global_env: None,
        session: session.session(None),
        cookies: None,
        oauth: &oauth,
    };

    let executed = http::execute(input(), CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    // The echo server reflects the secrets back in the response body, so they appear in the
    // request *and* the response.
    assert!(String::from_utf8_lossy(&executed.response.body).contains(SECRET));

    let store = HistoryStore::open_in_memory().unwrap();
    let entry = NewEntry::from_send(
        &collection.id,
        &rel,
        "Echo",
        &executed.sent,
        Ok(&executed.response),
    );
    let id = store
        .add(&entry, &executed.sensitive, DEFAULT_RETENTION)
        .unwrap();
    let stored = serde_json::to_string(&store.get(id).unwrap()).unwrap();
    let body = String::from_utf8_lossy(&store.response_body(id).unwrap()).to_string();
    for secret in [SECRET, API_KEY] {
        assert!(
            !stored.contains(secret),
            "history contains {secret}: {stored}"
        );
        assert!(!body.contains(secret), "stored body contains {secret}");
    }

    // curl export masks the same values unless secrets are explicitly included.
    let options = CurlOptions {
        shell: Shell::Bash,
        resolve_variables: true,
        include_secrets: false,
    };
    let curl = http::export_curl(&input(), options).unwrap();
    assert!(!curl.contains(SECRET) && !curl.contains(API_KEY), "{curl}");
    assert!(curl.contains("Bearer ***"), "{curl}");
    let with_secrets = http::export_curl(
        &input(),
        CurlOptions {
            include_secrets: true,
            ..options
        },
    )
    .unwrap();
    assert!(with_secrets.contains(SECRET) && with_secrets.contains(API_KEY));

    // With placeholders, the variable stays a placeholder.
    let raw = http::export_curl(
        &input(),
        CurlOptions {
            resolve_variables: false,
            ..options
        },
    )
    .unwrap();
    assert!(
        raw.contains("Bearer {{token}}") && !raw.contains(API_KEY),
        "{raw}"
    );
}

/// Secret values in their *encoded* forms: a Basic auth password goes out base64-encoded in the
/// `Authorization` header, and a secret in the query string is URL-encoded. Masking only the
/// raw value would leak both into history.
#[tokio::test]
async fn encoded_secrets_are_masked_too() {
    use base64::Engine as _;

    const PASSWORD: &str = "p@ss w0rd&more";
    const QUERY_SECRET: &str = "q s3cret/äö";
    const BEARER: &str = "bearer-token-789";
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let collection = storage::create_collection(dir.path(), "C").unwrap();
    let secret = |name: &str, value: &str| EnvVariable {
        name: name.into(),
        value: Some(value.into()),
        secret: true,
        source: None,
        enabled: true,
    };
    let mut env = Environment::new("dev");
    env.variables = vec![
        secret("password", PASSWORD),
        secret("querySecret", QUERY_SECRET),
        secret("bearer", BEARER),
    ];

    let mut request = Request::new("Encoded");
    request.url = format!("http://{addr}/echo?key={{{{querySecret}}}}");
    request.params.query = vec![KeyValue::new("key", "{{querySecret}}")];
    request.headers = vec![KeyValue::new("X-Token", "Bearer {{bearer}}")];
    request.auth = Auth::Custom {
        schemes: vec![AuthScheme::Basic {
            username: "ann".into(),
            password: "{{password}}".into(),
        }],
    };
    let rel = storage::create_request(dir.path(), "", &request).unwrap();
    let session = http::SessionStore::default();
    let oauth = OAuth2Manager::new(Arc::new(|_| Ok(())));
    let executed = http::execute(
        ExecuteInput {
            collection_dir: dir.path(),
            request_rel: &rel,
            request: &request,
            collection_env: Some(&env),
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &oauth,
        },
        CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap();

    let store = HistoryStore::open_in_memory().unwrap();
    let entry = NewEntry::from_send(
        &collection.id,
        &rel,
        "Encoded",
        &executed.sent,
        Ok(&executed.response),
    );
    let id = store
        .add(&entry, &executed.sensitive, DEFAULT_RETENTION)
        .unwrap();
    let stored = serde_json::to_string(&store.get(id).unwrap()).unwrap();
    let body = String::from_utf8_lossy(&store.response_body(id).unwrap()).to_string();

    let basic = base64::engine::general_purpose::STANDARD.encode(format!("ann:{PASSWORD}"));
    // `application/x-www-form-urlencoded`: space as `+`, other non-alphanumerics as `%XX`.
    let encoded_query: String = QUERY_SECRET
        .bytes()
        .map(|b| match b {
            b' ' => "+".to_string(),
            b if b.is_ascii_alphanumeric() || b"-._*".contains(&b) => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    let percent_query = QUERY_SECRET
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    // The request as sent really contains the encoded forms (else this test proves nothing).
    // The prepared URL holds the raw value (the HTTP client encodes it while sending), so the
    // encoded form to look for is what the echo server actually received.
    let sent = format!("{} {:?}", executed.sent.url, executed.sent.headers);
    assert!(sent.contains(&basic), "basic credentials not sent: {sent}");
    let echoed: serde_json::Value = serde_json::from_slice(&executed.response.body).unwrap();
    let received_query = echoed["query"]
        .as_str()
        .unwrap()
        .trim_start_matches("key=")
        .to_string();
    assert_ne!(
        received_query, QUERY_SECRET,
        "expected the query to arrive encoded"
    );
    let response = String::from_utf8_lossy(&executed.response.body);
    assert!(
        response.contains(&basic),
        "the echo should reflect the credentials"
    );

    for (what, value) in [
        ("password", PASSWORD),
        ("basic credentials", basic.as_str()),
        ("query secret", QUERY_SECRET),
        ("form-encoded query secret", encoded_query.as_str()),
        ("percent-encoded query secret", percent_query.as_str()),
        ("query secret as received", received_query.as_str()),
        ("bearer", BEARER),
    ] {
        assert!(
            !stored.contains(value),
            "history contains the {what} ({value}): {stored}"
        );
        assert!(
            !body.contains(value),
            "stored response contains the {what} ({value})"
        );
    }
}
