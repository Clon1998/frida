//! M2 acceptance (PLAN.md §15): the inspector shows the correct source and shadowing for a
//! fixture that defines the same variable in all six layers (`testdata/six-layers`).

use std::path::{Path, PathBuf};

use frida_core::http::{ExecuteInput, inspect};
use frida_core::storage;
use frida_core::vars::{ResolvedVar, VarSource};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/six-layers")
}

fn inspect_x(runtime: &[(&str, &str)]) -> ResolvedVar {
    let session = frida_core::http::SessionStore::with_runtime(runtime);
    let ws = workspace();
    let collection = ws.join("collections/api");
    let request_rel = "orders/eu/get-x.yaml";
    let request = storage::read_request(&collection, request_rel).unwrap();
    let global_env = storage::find_environment(&ws, "global").unwrap().unwrap();
    let collection_env = storage::find_environment(&collection, "dev")
        .unwrap()
        .unwrap();

    let report = inspect(&ExecuteInput {
        collection_dir: &collection,
        request_rel,
        request: &request,
        collection_env: Some(&collection_env),
        global_env: Some(&global_env),
        session: session.session(None),
        cookies: None,
        oauth: &frida_core::auth::OAuth2Manager::new(std::sync::Arc::new(|_| Ok(()))),
    })
    .unwrap();
    report.vars.into_iter().find(|v| v.name == "x").unwrap()
}

fn folder(path: &str, name: &str) -> VarSource {
    VarSource::Folder {
        path: path.into(),
        name: name.into(),
    }
}

#[test]
fn runtime_wins_and_every_other_layer_is_shadowed_in_order() {
    let x = inspect_x(&[("x", "runtime")]);

    assert_eq!(x.value.as_deref(), Some("runtime"));
    assert_eq!(x.source, Some(VarSource::Runtime));
    let shadowed: Vec<(VarSource, Option<&str>)> = x
        .shadowed
        .iter()
        .map(|s| (s.source.clone(), s.value.as_deref()))
        .collect();
    assert_eq!(
        shadowed,
        [
            (VarSource::Request, Some("request")),
            (folder("orders/eu", "EU"), Some("inner folder")),
            (folder("orders", "Orders"), Some("outer folder")),
            (
                VarSource::CollectionEnv { name: "dev".into() },
                Some("collection env")
            ),
            (VarSource::Collection, Some("collection")),
            (
                VarSource::GlobalEnv {
                    name: "global".into()
                },
                Some("global env")
            ),
        ]
    );
}

#[test]
fn without_runtime_vars_the_request_layer_wins() {
    let x = inspect_x(&[]);
    assert_eq!(x.source, Some(VarSource::Request));
    assert_eq!(x.value.as_deref(), Some("request"));
    assert_eq!(x.shadowed.len(), 5);
}
