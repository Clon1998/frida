//! M6 acceptance (PLAN.md §15): OpenAPI specs in `testdata/openapi` import as expected
//! (snapshots), and an imported collection loads and sends.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use frida_core::http;
use frida_core::import::{OpenApiGrouping, map_openapi, write_plan};
use frida_core::model::{Auth, AuthScheme, BodyMode, MultipartKind};
use frida_core::storage::{self, TreeNode};
use frida_core::yaml::to_yaml_string;
use tokio_util::sync::CancellationToken;

fn spec(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/openapi")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn specs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/openapi");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml" || e == "json"))
        .collect();
    files.sort();
    files
}

#[test]
fn specs_match_snapshots() {
    let files = specs();
    assert!(files.len() >= 3);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut plan = map_openapi(&text, OpenApiGrouping::Tags)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        plan.collection.id = "<generated>".into();
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        insta::assert_snapshot!(name, to_yaml_string(&plan).unwrap());
    }
}

#[test]
fn internal_spec_maps_the_tricky_parts() {
    let plan = map_openapi(&spec("internal-orders-3.1.json"), OpenApiGrouping::Tags).unwrap();
    let s = &plan.summary;
    assert_eq!(s.base_url, "https://eu.orders.example.com/v2");
    assert_eq!(s.requests, 8);
    // Tags in the order of the `tags` list; "reports" only appears on an operation.
    let folders: Vec<_> = plan
        .folders
        .iter()
        .map(|f| f.folder.name.as_str())
        .collect();
    assert_eq!(folders, vec!["orders", "files", "reports"]);
    assert_eq!(s.secrets_to_fill, vec!["token", "clientSecret", "password"]);
    assert!(
        s.warnings
            .iter()
            .any(|w| w.contains("cookie parameter \"session\""))
    );
    assert!(s.warnings.iter().any(|w| w.contains("other-file.yaml")));

    // API-wide bearer auth on the collection; `security: []` turns it off.
    assert!(
        matches!(&plan.collection.auth, Auth::Custom { schemes } if matches!(schemes[0], AuthScheme::Bearer { .. }))
    );
    let health = &plan.requests[0];
    assert_eq!(health.name, "getHealth");
    assert_eq!(health.auth, Auth::None);

    let orders = &plan.folders[0].requests;
    let list = &orders[0];
    assert_eq!(list.url, "{{baseUrl}}/orders?limit=20");
    assert!(
        !list.params.query[0].enabled,
        "optional params start disabled"
    );
    assert_eq!(list.headers[0].name, "X-Tenant-Id");
    assert!(list.headers.iter().all(|h| h.name != "Accept"));

    let create = &orders[1];
    assert_eq!(create.body.mode, BodyMode::Json);
    let body: serde_json::Value = serde_json::from_str(&create.body.json).unwrap();
    assert!(body.get("id").is_none(), "readOnly fields are left out");
    assert_eq!(body["items"][0]["price"]["currency"], "EUR");
    assert_eq!(body["delivery"]["kind"], "pickup");

    let get = &orders[2];
    assert_eq!(get.url, "{{baseUrl}}/orders/:orderId");
    assert_eq!(
        get.params.path[0].value,
        "3f6c1a52-0d5e-4b8e-9b7a-0c2f6d1e8a11"
    );
    assert!(get.docs.starts_with("**Deprecated.**"));
    let Auth::Custom { schemes } = &get.auth else {
        panic!("expected oauth2")
    };
    let AuthScheme::OAuth2(config) = &schemes[0] else {
        panic!("expected oauth2")
    };
    assert_eq!(config.scope, "orders:read");
    assert_eq!(config.token_url, "https://auth.example.com/oauth2/token");

    let upload = &plan.folders[1].requests[0];
    assert_eq!(upload.body.mode, BodyMode::Multipart);
    assert_eq!(upload.body.multipart[0].kind, MultipartKind::File);
}

#[tokio::test]
async fn imported_collection_loads_and_sends() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let plan = map_openapi(&spec("petstore-expanded-3.0.yaml"), OpenApiGrouping::Paths).unwrap();
    write_plan(dir.path(), &plan).unwrap();

    let tree = storage::scan_tree(dir.path()).unwrap();
    let mut paths = Vec::new();
    fn collect(nodes: &[TreeNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                TreeNode::Folder { children, .. } => collect(children, out),
                TreeNode::Request { path, .. } => out.push(path.clone()),
                TreeNode::Invalid { path, error } => panic!("{path}: {error}"),
            }
        }
    }
    collect(&tree, &mut paths);
    assert_eq!(paths.len(), plan.summary.requests as usize);

    let mut env = storage::find_environment(dir.path(), "default")
        .unwrap()
        .unwrap();
    env.variables[0].value = Some(format!("http://{addr}/api"));
    let session = http::SessionStore::default();
    let request = storage::read_request(dir.path(), &paths[0]).unwrap();
    let executed = http::execute(
        http::ExecuteInput {
            collection_dir: dir.path(),
            request_rel: &paths[0],
            request: &request,
            collection_env: Some(&env),
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &frida_core::auth::OAuth2Manager::new(std::sync::Arc::new(|_| Ok(()))),
        },
        CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&executed.response.body).unwrap();
    assert!(reply["path"].as_str().unwrap().starts_with("/api/pets"));
}
