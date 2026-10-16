//! The console (Ctrl/Cmd+J): script output from every level arrives in order, with its level and
//! where it came from; secret values are masked in log lines and in the request/response entry;
//! the entry shows the request exactly as sent (after pre-request scripts).

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Mutex;

use frida_core::http::{self, ConsoleEntry, ExecuteInput, ScriptLog, SessionStore};
use frida_core::model::{
    ApiKeyLocation, Auth, AuthScheme, EnvVariable, Environment, Request, Scripts,
};
use frida_core::storage;
use frida_script::LogLevel;
use tokio_util::sync::CancellationToken;

const SECRET: &str = "s3cr3t-value-123";

fn env(addr: SocketAddr) -> Environment {
    let mut env = Environment::new("dev");
    env.variables = vec![
        EnvVariable {
            name: "baseUrl".into(),
            value: Some(format!("http://{addr}")),
            secret: false,
            source: None,
            enabled: true,
        },
        EnvVariable {
            name: "apiKey".into(),
            value: Some(SECRET.into()),
            secret: true,
            source: None,
            enabled: true,
        },
    ];
    env
}

/// A collection whose collection, folder and request all have scripts.
fn collection(dir: &Path) {
    storage::create_collection(dir, "Console").unwrap();
    let mut collection = storage::load_collection(dir).unwrap();
    collection.scripts = Scripts {
        pre: r#"console.log("collection pre");"#.into(),
        post: r#"console.log("collection post");"#.into(),
    };
    storage::save_collection(dir, &collection).unwrap();
    let rel = storage::create_folder(dir, "", "Orders").unwrap();
    let mut folder = storage::read_folder(dir, &rel).unwrap();
    folder.scripts = Scripts {
        pre: r#"console.info("folder pre");"#.into(),
        post: String::new(),
    };
    storage::save_folder(dir, &rel, &folder).unwrap();
}

fn request(pre: &str, post: &str) -> Request {
    let mut request = Request::new("Create order");
    request.method = "POST".into();
    request.url = "{{baseUrl}}/echo".into();
    request.scripts = Scripts {
        pre: pre.into(),
        post: post.into(),
    };
    // The secret also goes out as an API key header, so the entry has something to mask.
    request.auth = Auth::Custom {
        schemes: vec![AuthScheme::ApiKey {
            location: ApiKeyLocation::Header,
            name: "X-Api-Key".into(),
            value: "{{apiKey}}".into(),
        }],
    };
    request
}

async fn send(
    dir: &Path,
    addr: SocketAddr,
    request: &Request,
) -> (frida_core::Result<http::Executed>, Vec<ScriptLog>) {
    let env = env(addr);
    let session = SessionStore::default();
    let logs = Mutex::new(Vec::new());
    let on_log = |log: ScriptLog| logs.lock().unwrap().push(log);
    let result = http::execute(
        ExecuteInput {
            collection_dir: dir,
            request_rel: "orders/create-order.yaml",
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
    .await;
    (result, logs.into_inner().unwrap())
}

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn logs_arrive_in_order_with_level_and_location() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    collection(dir.path());
    let request = request(
        r#"console.warn("request pre", { n: 1 }); console.debug("debug line");"#,
        r#"console.error("request post", res.status);"#,
    );
    let (result, logs) = send(dir.path(), addr, &request).await;
    result.unwrap();

    let lines: Vec<(LogLevel, &str, &str)> = logs
        .iter()
        .map(|l| (l.level, l.message.as_str(), l.location.as_str()))
        .collect();
    assert_eq!(
        lines,
        vec![
            (
                LogLevel::Log,
                "collection pre",
                "collection \"Console\", pre-request script"
            ),
            (
                LogLevel::Info,
                "folder pre",
                "folder \"Orders\", pre-request script"
            ),
            (
                LogLevel::Warn,
                // Objects are printed as indented JSON.
                "request pre {\n  \"n\": 1\n}",
                "request \"Create order\", pre-request script"
            ),
            (
                LogLevel::Debug,
                "debug line",
                "request \"Create order\", pre-request script"
            ),
            (
                LogLevel::Log,
                "collection post",
                "collection \"Console\", post-response script"
            ),
            (
                LogLevel::Error,
                "request post 200",
                "request \"Create order\", post-response script"
            ),
        ]
    );
    assert!(logs.iter().all(|l| l.request_name == "Create order"));
}

#[tokio::test(flavor = "multi_thread")]
async fn secret_values_are_masked_in_logs_and_in_the_console_entry() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    collection(dir.path());
    let request = request(
        // A script that (carelessly) logs a secret and puts it into a header and the body.
        r#"const key = fri.getEnvVar("apiKey");
console.log("key is", key);
req.setHeader("X-Debug", "prefix-" + key);
req.setBody({ key });"#,
        r#"console.log("echoed", res.body.headers["x-api-key"]);"#,
    );
    let (result, logs) = send(dir.path(), addr, &request).await;
    let executed = result.unwrap();

    for log in &logs {
        assert!(
            !log.message.contains(SECRET),
            "log leaks the secret: {}",
            log.message
        );
    }
    assert!(logs.iter().any(|l| l.message == "key is ***"));
    assert!(logs.iter().any(|l| l.message == "echoed ***"));

    let entry = ConsoleEntry::from_send(
        "1".into(),
        "Create order",
        "orders/create-order.yaml",
        &executed.sent,
        Ok(&executed.response),
        &executed.sensitive,
    );
    let text = serde_json::to_string(&entry).unwrap();
    assert!(
        !text.contains(SECRET),
        "console entry leaks the secret: {text}"
    );
    // The entry shows the request as sent: the script's header and body are there, masked.
    let header = |name: &str| {
        entry
            .request_headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.clone())
    };
    assert_eq!(header("x-debug").as_deref(), Some("prefix-***"));
    assert_eq!(header("x-api-key").as_deref(), Some("***"));
    assert!(entry.request_body.as_ref().unwrap().text.contains("***"));
    assert_eq!(entry.method, "POST");
    assert!(entry.url.ends_with("/echo"), "{}", entry.url);
    let response = entry.response.as_ref().unwrap();
    assert_eq!(response.status, 200);
    assert!(!response.body.as_ref().unwrap().text.contains(SECRET));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_pre_request_script_logs_up_to_the_error_and_sends_nothing() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    collection(dir.path());
    let request = request(
        r#"console.log("before"); throw new Error("boom"); console.log("after");"#,
        r#"console.log("never");"#,
    );
    let (result, logs) = send(dir.path(), addr, &request).await;
    let err = result.err().expect("the send must fail");
    assert!(err.to_string().contains("boom"), "{err}");
    let messages: Vec<&str> = logs.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(messages, vec!["collection pre", "folder pre", "before"]);

    // What the console shows for a request that wasn't sent: as typed, no secret values.
    let entry = ConsoleEntry::not_sent(
        "2".into(),
        &request.name,
        "orders/create-order.yaml",
        &request.method,
        &request.url,
        err.to_string(),
    );
    assert_eq!(entry.url, "{{baseUrl}}/echo");
    assert!(entry.response.is_none());
    assert!(entry.error.unwrap().contains("boom"));
}
