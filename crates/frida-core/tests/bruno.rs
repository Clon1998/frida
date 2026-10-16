//! M9 acceptance (PLAN.md §15): Bruno collections in both formats import as expected
//! (snapshots of `testdata/bruno`), and an imported collection works end to end.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use frida_core::http;
use frida_core::import::{BrunoPlan, PlannedItem, map_bruno, write_bruno_plan};
use frida_core::model::{Auth, AuthScheme, BodyMode, MultipartKind, Request};
use frida_core::storage;
use frida_core::yaml::to_yaml_string;
use tokio_util::sync::CancellationToken;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/bruno")
        .join(name)
}

/// Collection ids are random and file paths depend on the checkout location. Paths are replaced
/// *before* writing YAML: the YAML writer wraps long lines, so replacing afterwards would give
/// different line breaks on machines with longer or shorter paths.
fn snapshot(plan: &BrunoPlan) -> String {
    let mut plan = plan.clone();
    plan.collection.id = "<generated>".into();
    let roots = [
        fixture("")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        fixture("").to_string_lossy().to_string(),
    ];
    let mut value = serde_json::to_value(&plan).unwrap();
    replace_strings(&mut value, &roots);
    to_yaml_string(&value).unwrap()
}

fn replace_strings(value: &mut serde_json::Value, roots: &[String]) {
    match value {
        serde_json::Value::String(s) => {
            for root in roots {
                let root = root.trim_end_matches('/');
                *s = s.replace(root, "<testdata/bruno>");
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| replace_strings(v, roots)),
        serde_json::Value::Object(map) => map.values_mut().for_each(|v| replace_strings(v, roots)),
        _ => {}
    }
}

fn requests(items: &[PlannedItem], out: &mut Vec<Request>) {
    for item in items {
        match item {
            PlannedItem::Request(r) => out.push((**r).clone()),
            PlannedItem::Folder(dir) => requests(&dir.items, out),
        }
    }
}

fn find(plan: &BrunoPlan, name: &str) -> Request {
    let mut all = Vec::new();
    requests(&plan.root.items, &mut all);
    all.into_iter().find(|r| r.name == name).unwrap()
}

#[test]
fn bru_collection_matches_snapshot() {
    let plan = map_bruno(&fixture("shop")).unwrap();
    insta::assert_snapshot!("bruno_shop", snapshot(&plan));
}

#[test]
fn yaml_collection_matches_snapshot() {
    let plan = map_bruno(&fixture("yml-collection")).unwrap();
    assert!(plan.summary.requests >= 5, "{:?}", plan.summary);
    insta::assert_snapshot!("bruno_yml_collection", snapshot(&plan));
}

#[test]
fn brunos_own_testbench_imports() {
    let plan = map_bruno(&fixture("testbench")).unwrap();
    let s = &plan.summary;
    assert!(s.requests > 150, "{s:?}");
    assert_eq!(s.environments, 2);
    // Warnings only (counts and messages), so the snapshot stays readable.
    insta::assert_snapshot!(
        "bruno_testbench_summary",
        format!(
            "requests: {}\nfolders: {}\nenvironments: {}\nsecrets: {:?}\nwarnings:\n{}",
            s.requests,
            s.folders,
            s.environments,
            s.secrets_to_fill,
            s.warnings.join("\n")
        )
        .replace(
            &fixture("").to_string_lossy().to_string(),
            "<testdata/bruno>/"
        )
    );
}

#[test]
fn shop_maps_the_tricky_parts() {
    let plan = map_bruno(&fixture("shop")).unwrap();
    let s = &plan.summary;
    assert_eq!((s.requests, s.folders, s.environments), (5, 3, 1));
    assert_eq!(s.secrets_to_fill, vec!["local: password"]);
    let warnings = s.warnings.join("\n");
    assert!(warnings.contains("digest auth"), "{warnings}");
    assert!(warnings.contains("bru.cwd"), "{warnings}");
    assert!(warnings.contains("legacyHost"), "{warnings}");
    assert!(warnings.contains("GraphQL"), "{warnings}");

    assert!(
        matches!(&plan.collection.auth, Auth::Custom { schemes } if matches!(schemes[0], AuthScheme::Bearer { .. }))
    );

    let login = find(&plan, "Login");
    assert!(
        login
            .scripts
            .post
            .contains("fri.setVar(\"token\", JSON.parse(res.body.body).token);")
    );
    assert!(login.tests.contains("expect(res.status).to.eql(200)"));
    assert!(login.tests.contains("// (disabled) res.body.never: eq 1"));
    assert!(login.tests.starts_with("test(\"echoes the user\""));

    let list = find(&plan, "List orders");
    assert_eq!(list.settings.timeout_ms, Some(5000));
    assert!(!list.params.query[1].enabled);

    let create = find(&plan, "Create order");
    assert_eq!(create.body.mode, BodyMode::Multipart);
    assert_eq!(create.body.multipart[0].value, "hello");
    assert_eq!(create.body.multipart[1].kind, MultipartKind::File);
    assert!(
        create.body.multipart[1]
            .value
            .ends_with("shop/files/invoice.pdf")
    );
    assert!(!create.body.multipart[2].enabled);
    assert_eq!(create.params.path[0].value, "eu");

    let search = find(&plan, "Search (GraphQL)");
    let body: serde_json::Value = serde_json::from_str(&search.body.json).unwrap();
    assert_eq!(body["variables"]["limit"], 5);
    assert!(
        body["query"]
            .as_str()
            .unwrap()
            .contains("orders(limit: $limit)")
    );

    assert_eq!(find(&plan, "Digest protected").auth, Auth::None);
}

/// The imported collection runs: the login's post-response variable carries the token into the
/// next request via inherited bearer auth, and the converted assertions pass.
#[tokio::test(flavor = "multi_thread")]
async fn imported_collection_runs_end_to_end() {
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let plan = map_bruno(&fixture("shop")).unwrap();
    write_bruno_plan(dir.path(), &plan).unwrap();

    let mut env = storage::find_environment(dir.path(), "local")
        .unwrap()
        .unwrap();
    env.variables[0].value = Some(format!("http://{addr}"));
    let session = http::SessionStore::default();
    for path in ["auth/login.yaml", "orders/list-orders.yaml"] {
        let request =
            storage::read_request(dir.path(), path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let executed = http::execute(
            http::ExecuteInput {
                collection_dir: dir.path(),
                request_rel: path,
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
        assert!(
            executed.scripts.errors.is_empty(),
            "{path}: {:?}",
            executed.scripts.errors
        );
        assert!(!executed.scripts.tests.is_empty(), "{path}");
        for test in &executed.scripts.tests {
            assert!(test.passed, "{path}: {} {:?}", test.name, test.error);
        }
    }
}
