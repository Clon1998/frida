//! M4 acceptance (PLAN.md §15): history entries contain no secret values, end to end.

use std::collections::HashMap;
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

    let runtime = HashMap::new();
    let oauth = OAuth2Manager::new(Arc::new(|_| Ok(())));
    let input = || ExecuteInput {
        collection_dir: dir.path(),
        request_rel: &rel,
        request: &request,
        collection_env: Some(&env),
        global_env: None,
        runtime: &runtime,
        cookies: None,
        oauth: &oauth,
    };

    let executed = http::execute(input(), CancellationToken::new())
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
