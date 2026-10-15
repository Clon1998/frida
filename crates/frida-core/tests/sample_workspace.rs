//! The sample workspace in `testdata/sample-workspace` (also used for trying Frida by hand)
//! loads cleanly, and every request in it succeeds against the local test server.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use frida_core::http;
use frida_core::storage::{self, TreeNode};
use tokio_util::sync::CancellationToken;

fn sample_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/sample-workspace")
}

fn request_paths(nodes: &[TreeNode], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            TreeNode::Folder { children, .. } => request_paths(children, out),
            TreeNode::Request { path, .. } => out.push(path.clone()),
            TreeNode::Invalid { path, error } => panic!("{path} is invalid: {error}"),
        }
    }
}

#[test]
fn sample_workspace_loads_without_errors() {
    let overview = storage::load_overview(&sample_workspace()).unwrap();
    assert_eq!(overview.collections.len(), 1);
    assert!(
        overview.collections[0].error.is_none(),
        "{:?}",
        overview.collections[0].error
    );
}

#[tokio::test]
async fn every_sample_request_succeeds_against_the_test_server() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let collection = sample_workspace().join("collections/echo-api");

    let mut paths = Vec::new();
    request_paths(&storage::scan_tree(&collection).unwrap(), &mut paths);
    assert!(paths.len() >= 10, "expected the full sample, got {paths:?}");

    // The sample points at the default port; the test server runs on a random one.
    let port = |text: &str| text.replace("localhost:8787", &format!("127.0.0.1:{}", addr.port()));
    let mut local_env = storage::find_environment(&collection, "local")
        .unwrap()
        .unwrap();
    for var in &mut local_env.variables {
        var.value = var.value.as_deref().map(port);
    }
    let shared_env = storage::find_environment(&sample_workspace(), "shared")
        .unwrap()
        .unwrap();

    for path in paths.iter().filter(|p| !p.contains("slow")) {
        let mut request = storage::read_request(&collection, path).unwrap();
        request.url = port(&request.url);

        let runtime = std::collections::HashMap::new();
        let input = http::ExecuteInput {
            collection_dir: &collection,
            request_rel: path,
            request: &request,
            collection_env: Some(&local_env),
            global_env: Some(&shared_env),
            runtime: &runtime,
            cookies: None,
        };
        let response = http::execute(input, CancellationToken::new())
            .await
            .unwrap_or_else(|e| panic!("{path} failed: {e}"))
            .response;
        assert_eq!(response.status, 200, "{path}");
    }
}

#[tokio::test]
async fn variables_sample_resolves_every_layer() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let collection = sample_workspace().join("collections/echo-api");
    let mut local_env = storage::find_environment(&collection, "local")
        .unwrap()
        .unwrap();
    for var in &mut local_env.variables {
        if var.name == "baseUrl" {
            var.value = Some(format!("http://127.0.0.1:{}", addr.port()));
        }
        if var.name == "token" {
            // What the keychain would provide.
            var.value = Some("test-token".into());
        }
    }
    let shared_env = storage::find_environment(&sample_workspace(), "shared")
        .unwrap()
        .unwrap();
    let path = "variables/get-with-variables.yaml";
    let request = storage::read_request(&collection, path).unwrap();
    let runtime = std::collections::HashMap::new();

    let executed = http::execute(
        http::ExecuteInput {
            collection_dir: &collection,
            request_rel: path,
            request: &request,
            collection_env: Some(&local_env),
            global_env: Some(&shared_env),
            runtime: &runtime,
            cookies: None,
        },
        CancellationToken::new(),
    )
    .await
    .unwrap();

    let reply: serde_json::Value = serde_json::from_slice(&executed.response.body).unwrap();
    assert_eq!(reply["path"], "/orders/42");
    assert_eq!(reply["headers"]["x-team"], "platform");
    assert_eq!(reply["headers"]["authorization"], "Bearer test-token");
    assert!(reply["query"].as_str().unwrap().starts_with("trace="));
    assert!(
        executed
            .report
            .secret_values
            .contains(&"test-token".to_string())
    );
}
