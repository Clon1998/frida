//! M5 acceptance: scripts in the send pipeline, against the echo server.
//! `testdata/scripting` chains a login (post-response `bru.setVar("token")`) into an
//! authenticated request whose folder script uses `bru.sendRequest`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use frida_core::http::{self, ExecuteInput, ScriptLog, SessionStore};
use frida_core::{CoreError, storage};
use tokio_util::sync::CancellationToken;

fn collection() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/scripting")
}

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

async fn send(
    session: &SessionStore,
    path: &str,
    request: &frida_core::model::Request,
    logs: &Mutex<Vec<ScriptLog>>,
) -> frida_core::Result<http::Executed> {
    let dir = collection();
    let env = storage::find_environment(&dir, "dev").unwrap().unwrap();
    let on_log = |log: ScriptLog| logs.lock().unwrap().push(log);
    http::execute(
        ExecuteInput {
            collection_dir: &dir,
            request_rel: path,
            request,
            collection_env: Some(&env),
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &frida_core::auth::OAuth2Manager::new(std::sync::Arc::new(|_| Ok(()))),
        },
        CancellationToken::new(),
        &on_log,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn login_token_flows_into_the_next_request() {
    let addr = server().await;
    let session = SessionStore::with_runtime(&[("baseUrl", &format!("http://{addr}"))]);
    let logs = Mutex::new(Vec::new());

    let login = storage::read_request(&collection(), "login.yaml").unwrap();
    let executed = send(&session, "login.yaml", &login, &logs).await.unwrap();
    assert!(
        executed.scripts.errors.is_empty(),
        "{:?}",
        executed.scripts.errors
    );
    let passed: Vec<_> = executed
        .scripts
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed))
        .collect();
    assert_eq!(passed, vec![("no server error", true), ("login ok", true)]);
    assert_eq!(
        session.runtime.lock().unwrap()["token"],
        serde_json::json!("t-123")
    );
    let logs_now = logs.lock().unwrap().clone();
    assert_eq!(logs_now[0].message, "logging in as ann");
    assert_eq!(
        logs_now[0].location,
        "request \"Login\", pre-request script"
    );

    let orders = storage::read_request(&collection(), "orders/list-orders.yaml").unwrap();
    let executed = send(&session, "orders/list-orders.yaml", &orders, &logs)
        .await
        .unwrap();
    assert!(
        executed.scripts.errors.is_empty(),
        "{:?}",
        executed.scripts.errors
    );
    for test in &executed.scripts.tests {
        assert!(test.passed, "{} failed: {:?}", test.name, test.error);
    }
    assert_eq!(executed.scripts.tests.len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_pre_request_script_stops_the_send() {
    let addr = server().await;
    let session = SessionStore::with_runtime(&[("baseUrl", &format!("http://{addr}"))]);
    let mut request = storage::read_request(&collection(), "login.yaml").unwrap();
    request.scripts.pre = "const a = 1;\nthrow new Error('nope');".into();
    let err = send(&session, "login.yaml", &request, &Mutex::new(Vec::new()))
        .await;
    let Err(CoreError::Script(error)) = err else {
        panic!("expected a script error");
    };
    assert_eq!(error.location, "request \"Login\", pre-request script");
    assert_eq!(error.line, Some(2));
    assert_eq!(error.message, "nope");
}

#[tokio::test(flavor = "multi_thread")]
async fn pre_request_scripts_can_rewrite_the_request() {
    let addr = server().await;
    let session = SessionStore::with_runtime(&[("baseUrl", &format!("http://{addr}"))]);
    let mut request = storage::read_request(&collection(), "login.yaml").unwrap();
    request.scripts.pre = r#"
        req.setUrl(req.getUrl() + "?v={{user}}");
        req.deleteHeader("X-Client");
        const body = req.getBody();
        body.extra = true;
        req.setBody(body);
        bru.setEnvVar("user", "bob");
    "#
    .into();
    request.scripts.post = String::new();
    request.tests = String::new();
    let executed = send(&session, "login.yaml", &request, &Mutex::new(Vec::new()))
        .await
        .unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&executed.response.body).unwrap();
    // Changed values are resolved after the script, with the script's env change.
    assert_eq!(reply["query"], "v=bob");
    assert!(reply["headers"].get("x-client").is_none());
    // The header the collection script added before is kept.
    assert_eq!(reply["headers"]["x-request-name"], "Login");
    let body: serde_json::Value = serde_json::from_str(reply["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["extra"], true);
    assert_eq!(body["user"], "bob");
    // The env change is a session override, not written to the file.
    let env = storage::find_environment(&collection(), "dev")
        .unwrap()
        .unwrap();
    assert_eq!(env.variables[0].value.as_deref(), Some("ann"));
}
