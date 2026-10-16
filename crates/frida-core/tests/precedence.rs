//! Variable precedence (PLAN.md §5.1) through a real send: `{{who}}` is defined in every layer,
//! and the winner is removed one layer at a time. Also: a script's `setEnvVar` beats the
//! environment file for the rest of the session, and disabled rows never win.

use std::net::SocketAddr;
use std::path::Path;

use frida_core::http::{self, ExecuteInput, SessionStore};
use frida_core::model::{EnvVariable, Environment, KeyValue, Request, Scripts};
use frida_core::storage;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

fn env(name: &str, vars: &[(&str, &str)]) -> Environment {
    let mut env = Environment::new(name);
    env.variables = vars
        .iter()
        .map(|(n, v)| EnvVariable {
            name: (*n).into(),
            value: Some((*v).into()),
            secret: false,
            source: None,
            enabled: true,
        })
        .collect();
    env
}

/// collection vars `who=collection`; folders `outer` and `outer/inner` with their own `who`.
fn collection(dir: &Path) {
    storage::create_collection(dir, "Precedence").unwrap();
    let mut c = storage::load_collection(dir).unwrap();
    c.vars = vec![KeyValue::new("who", "collection")];
    storage::save_collection(dir, &c).unwrap();
    for (parent, name) in [("", "outer"), ("outer", "inner")] {
        let rel = storage::create_folder(dir, parent, name).unwrap();
        let mut folder = storage::read_folder(dir, &rel).unwrap();
        folder.vars = vec![KeyValue::new("who", name)];
        storage::save_folder(dir, &rel, &folder).unwrap();
    }
}

/// Which value `{{who}}` resolved to, read from the echo of the `X-Who` header.
async fn who(
    dir: &Path,
    request: &Request,
    collection_env: Option<&Environment>,
    global_env: Option<&Environment>,
    session: &SessionStore,
) -> String {
    let executed = http::execute(
        ExecuteInput {
            collection_dir: dir,
            request_rel: "outer/inner/req.yaml",
            request,
            collection_env,
            global_env,
            session: session.session(None),
            cookies: None,
            oauth: &frida_core::auth::OAuth2Manager::new(std::sync::Arc::new(|_| Ok(()))),
        },
        CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap();
    let echo: Value = serde_json::from_slice(&executed.response.body).unwrap();
    echo["headers"]["x-who"]
        .as_str()
        .unwrap_or("<unresolved>")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_layer_wins_when_the_ones_above_it_are_gone() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    collection(dir);
    let mut request = Request::new("Req");
    request.url = format!("http://{addr}/echo");
    request.headers = vec![KeyValue::new("X-Who", "{{who}}")];
    request.vars = vec![KeyValue::new("who", "request")];
    let mut col_env = env("dev", &[("who", "collection-env")]);
    let global = env("global", &[("who", "global-env")]);
    let session = SessionStore::with_runtime(&[("who", "runtime")]);

    let mut expected = vec![];
    // 1. runtime
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "runtime",
    ));
    session.runtime.lock().unwrap().clear();
    // 2. request vars
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "request",
    ));
    request.vars.clear();
    // 3. nearest folder, then its parent
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "inner",
    ));
    let mut inner = storage::read_folder(dir, "outer/inner").unwrap();
    inner.vars.clear();
    storage::save_folder(dir, "outer/inner", &inner).unwrap();
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "outer",
    ));
    let mut outer = storage::read_folder(dir, "outer").unwrap();
    // A disabled row is the same as none.
    outer.vars[0].enabled = false;
    storage::save_folder(dir, "outer", &outer).unwrap();
    // 4. collection environment
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "collection-env",
    ));
    col_env.variables[0].enabled = false;
    // 5. collection vars
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "collection",
    ));
    let mut c = storage::load_collection(dir).unwrap();
    c.vars.clear();
    storage::save_collection(dir, &c).unwrap();
    // 6. global environment
    expected.push((
        who(dir, &request, Some(&col_env), Some(&global), &session).await,
        "global-env",
    ));
    // Nothing left: the placeholder is sent as written.
    expected.push((
        who(dir, &request, Some(&col_env), None, &session).await,
        "{{who}}",
    ));

    let (got, want): (Vec<_>, Vec<_>) = expected.into_iter().unzip();
    assert_eq!(got, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn set_env_var_beats_the_environment_file_for_the_session() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    collection(tmp.path());
    let col_env = env("dev", &[("who", "from-file")]);
    let session = SessionStore::default();

    let mut setter = Request::new("Set");
    setter.url = format!("http://{addr}/echo");
    setter.scripts = Scripts {
        pre: String::new(),
        post: r#"fri.setEnvVar("who", "from-script");"#.into(),
    };
    // Folder and collection vars are below the environment; remove them for this test.
    let mut c = storage::load_collection(tmp.path()).unwrap();
    c.vars.clear();
    storage::save_collection(tmp.path(), &c).unwrap();
    for rel in ["outer", "outer/inner"] {
        let mut f = storage::read_folder(tmp.path(), rel).unwrap();
        f.vars.clear();
        storage::save_folder(tmp.path(), rel, &f).unwrap();
    }

    let mut reader = Request::new("Read");
    reader.url = format!("http://{addr}/echo");
    reader.headers = vec![KeyValue::new("X-Who", "{{who}}")];
    assert_eq!(
        who(tmp.path(), &reader, Some(&col_env), None, &session).await,
        "from-file"
    );
    who(tmp.path(), &setter, Some(&col_env), None, &session).await;
    assert_eq!(
        who(tmp.path(), &reader, Some(&col_env), None, &session).await,
        "from-script"
    );
    // The file itself is untouched (no `persist: true`), and a new session starts from it.
    assert_eq!(col_env.variables[0].value.as_deref(), Some("from-file"));
    assert_eq!(
        who(
            tmp.path(),
            &reader,
            Some(&col_env),
            None,
            &SessionStore::default()
        )
        .await,
        "from-file"
    );
}
