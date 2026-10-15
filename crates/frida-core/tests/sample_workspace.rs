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

    for path in paths.iter().filter(|p| !p.contains("slow")) {
        let mut request = storage::read_request(&collection, path).unwrap();
        // The sample points at the default port; the test server runs on a random one.
        request.url = request
            .url
            .replace("localhost:8787", &format!("127.0.0.1:{}", addr.port()));

        let response = http::execute(&collection, path, &request, CancellationToken::new())
            .await
            .unwrap_or_else(|e| panic!("{path} failed: {e}"));
        assert_eq!(response.status, 200, "{path}");
    }
}
