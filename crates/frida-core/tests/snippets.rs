//! Every script and test example offered in the editors (`frida_script::snippets`) runs through
//! the real send pipeline against the test server's `/sample` route: no script errors, every
//! test passes, and each example does what its title says. Test examples must also *fail*
//! against a response they don't fit, so none of them passes vacuously.

use std::net::SocketAddr;
use std::sync::Mutex;

use frida_core::http::{self, ExecuteInput, ExecutionMode, ScriptLog, SessionStore};
use frida_core::model::{Body, BodyMode, EnvVariable, Environment, Request, Scripts};
use frida_core::{CoreError, storage};
use frida_script::LogLevel;
use frida_script::snippets::{Snippet, SnippetKind, snippets};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn var(name: &str, value: &str, secret: bool) -> EnvVariable {
    EnvVariable {
        name: name.into(),
        value: Some(value.into()),
        secret,
        source: None,
        enabled: true,
    }
}

/// The environment the examples expect: a token, credentials and a signing secret.
fn environment(addr: SocketAddr) -> Environment {
    let mut env = Environment::new("dev");
    env.variables = vec![
        var("baseUrl", &format!("http://{addr}"), false),
        var("token", "env-token", false),
        var("username", "ann", false),
        var("password", "pw", true),
        var("apiSecret", "s3cr3t", true),
    ];
    env
}

/// A JSON POST to `path` with `snippet` in its place (pre-request, post-response or tests).
fn request_with(snippet: &Snippet, path: &str) -> Request {
    let mut request = Request::new("Get pet");
    request.method = "POST".into();
    request.url = format!("{{{{baseUrl}}}}{path}");
    request.body = Body {
        mode: BodyMode::Json,
        json: r#"{"name": "Rex"}"#.into(),
        ..Body::default()
    };
    match snippet.kind {
        SnippetKind::PreRequest => {
            request.scripts = Scripts {
                pre: snippet.code.clone(),
                ..Scripts::default()
            }
        }
        SnippetKind::PostResponse => {
            request.scripts = Scripts {
                post: snippet.code.clone(),
                ..Scripts::default()
            }
        }
        SnippetKind::Tests => request.tests = snippet.code.clone(),
    }
    request
}

struct Sent {
    result: frida_core::Result<http::Executed>,
    logs: Vec<ScriptLog>,
    session: SessionStore,
    env: Environment,
}

impl Sent {
    fn ok(&self) -> &http::Executed {
        match &self.result {
            Ok(executed) => executed,
            Err(err) => panic!("send failed: {err}"),
        }
    }

    /// The `/sample` route's echo of the request as the server received it.
    fn echo(&self) -> Value {
        let body: Value = serde_json::from_slice(&self.ok().response.body).unwrap();
        body["echo"].clone()
    }

    fn header(&self, name: &str) -> Option<String> {
        self.echo()["headers"][name].as_str().map(str::to_string)
    }

    fn runtime(&self, name: &str) -> Option<Value> {
        self.session.runtime.lock().unwrap().get(name).cloned()
    }
}

async fn send(addr: SocketAddr, snippet: &Snippet, path: &str, mode: ExecutionMode) -> Sent {
    let dir = tempfile::tempdir().unwrap();
    storage::create_collection(dir.path(), "Snippets").unwrap();
    let env = environment(addr);
    let session = SessionStore::default();
    let logs = Mutex::new(Vec::new());
    let request = request_with(snippet, path);
    let on_log = |log: ScriptLog| logs.lock().unwrap().push(log);
    let result = http::execute_as(
        ExecuteInput {
            collection_dir: dir.path(),
            request_rel: "get-pet.yaml",
            request: &request,
            collection_env: Some(&env),
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &frida_core::auth::OAuth2Manager::new(std::sync::Arc::new(|_| Ok(()))),
        },
        mode,
        CancellationToken::new(),
        &on_log,
    )
    .await;
    let logs = logs.into_inner().unwrap();
    Sent {
        result,
        logs,
        session,
        env,
    }
}

/// What each example is supposed to do, beyond "no errors and every test passes".
fn check(snippet: &Snippet, sent: &Sent) {
    let messages: Vec<&str> = sent.logs.iter().map(|l| l.message.as_str()).collect();
    match snippet.id.as_str() {
        "pre.set-header" => {
            let id = sent.header("x-request-id").expect("X-Request-Id sent");
            assert_eq!(id.len(), 36, "a UUID: {id}");
        }
        "pre.bearer-from-env" => {
            assert_eq!(
                sent.header("authorization").as_deref(),
                Some("Bearer env-token")
            );
        }
        "pre.timestamp-var" => {
            let value = sent.runtime("timestamp").expect("timestamp set");
            assert!(value.as_str().unwrap().starts_with("20"), "{value}");
        }
        "pre.edit-json-body" => {
            let body: Value = serde_json::from_str(sent.echo()["body"].as_str().unwrap()).unwrap();
            assert_eq!(body["name"], "Rex");
            assert!(body["sentAt"].is_string(), "{body}");
        }
        "pre.hmac-signature" => {
            let signature = sent.header("x-signature").unwrap();
            assert_eq!(signature.len(), 64);
            assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(sent.header("x-timestamp").unwrap().parse::<u64>().is_ok());
        }
        "pre.basic-auth-header" => {
            // base64("ann:pw")
            assert_eq!(
                sent.header("authorization").as_deref(),
                Some("Basic YW5uOnB3")
            );
        }
        "pre.fetch-token" => {
            assert_eq!(
                sent.header("authorization").as_deref(),
                Some("Bearer sample-token-123")
            );
            assert_eq!(sent.runtime("token"), Some(json!("sample-token-123")));
        }
        "pre.skip-in-runner" => {} // checked separately in runner mode below
        "pre.log-request" => assert_eq!(messages, vec!["POST {{baseUrl}}/sample"]),
        "post.save-var" => assert_eq!(sent.runtime("petId"), Some(json!(42))),
        "post.save-token-env" => {
            let env = sent.session.env_overrides.apply(&sent.env);
            let token = env.variables.iter().find(|v| v.name == "token").unwrap();
            assert_eq!(token.value.as_deref(), Some("sample-token-123"));
        }
        "post.save-nested" => assert_eq!(sent.runtime("firstItemId"), Some(json!(1))),
        "post.log-response" => {
            assert_eq!(messages.len(), 1);
            assert!(messages[0].starts_with("200 "), "{}", messages[0]);
            assert!(messages[0].contains("Rex"), "{}", messages[0]);
        }
        "post.warn-slow" => assert!(
            sent.logs.iter().all(|l| l.level != LogLevel::Warn),
            "a fast response is not slow"
        ),
        "post.next-request" => {
            assert_eq!(sent.ok().flow.next_request.as_deref(), Some("Get pet"));
        }
        id if id.starts_with("tests.") => {}
        id => panic!("no check for snippet {id}; add one"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_snippet_runs_and_does_what_it_says() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let all = snippets();
    assert!(
        all.len() >= 25,
        "fewer examples than expected: {}",
        all.len()
    );

    for snippet in &all {
        let sent = send(addr, snippet, "/sample", ExecutionMode::Standalone).await;
        let executed = sent.ok();
        assert!(
            executed.scripts.errors.is_empty(),
            "{}: {:?}",
            snippet.id,
            executed.scripts.errors
        );
        for test in &executed.scripts.tests {
            assert!(
                test.passed,
                "{}: test {:?} failed: {:?}",
                snippet.id, test.name, test.error
            );
        }
        if snippet.kind == SnippetKind::Tests {
            assert!(
                !executed.scripts.tests.is_empty(),
                "{} ran no test",
                snippet.id
            );
        }
        check(snippet, &sent);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_snippets_fail_when_the_response_does_not_fit() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    // These only look at timing or a variable that isn't set, so any response passes them.
    let always_pass = ["tests.response-time", "tests.date", "tests.matches-request"];
    for snippet in snippets()
        .iter()
        .filter(|s| s.kind == SnippetKind::Tests && !always_pass.contains(&s.id.as_str()))
    {
        // A plain-text 404: wrong status, no JSON, no headers the examples look for.
        let sent = send(addr, snippet, "/status/404", ExecutionMode::Standalone).await;
        let tests = &sent.ok().scripts.tests;
        assert!(
            tests.iter().any(|t| !t.passed),
            "{} passed against a 404: {tests:?}",
            snippet.id
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_skip_snippet_skips_in_a_collection_run() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let skip = snippets()
        .into_iter()
        .find(|s| s.id == "pre.skip-in-runner")
        .unwrap();
    let standalone = send(addr, &skip, "/sample", ExecutionMode::Standalone).await;
    assert_eq!(standalone.ok().response.status, 200);
    let run = send(addr, &skip, "/sample", ExecutionMode::Runner).await;
    assert!(
        matches!(run.result, Err(CoreError::Skipped { .. })),
        "not skipped in a run"
    );
}
