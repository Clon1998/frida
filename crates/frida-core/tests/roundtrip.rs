//! Nothing gets lost on the way to disk and back:
//! - an import (OpenAPI, Bruno) written to disk reads back as exactly the planned requests,
//!   folders, collection and environments;
//! - every Frida file in `testdata` survives load → save → load → save unchanged (the second
//!   save is byte-identical to the first, and the values are equal).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use frida_core::import::{
    OpenApiGrouping, PlannedFolder, PlannedItem, map_bruno, map_openapi, write_bruno_plan,
    write_plan,
};
use frida_core::model::{Collection, Environment, Folder, Request, Workspace};
use frida_core::storage::{self, TreeNode};
use frida_core::yaml::{from_yaml_str, to_yaml_string};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name)
}

/// Every request of a collection on disk, by tree path of names (`Orders/Get order`), with
/// `seq` cleared: the order is checked separately, by the tree order.
fn requests_on_disk(dir: &Path) -> BTreeMap<String, Request> {
    fn walk(dir: &Path, nodes: &[TreeNode], prefix: &str, out: &mut BTreeMap<String, Request>) {
        for node in nodes {
            match node {
                TreeNode::Folder { name, children, .. } => {
                    walk(dir, children, &format!("{prefix}{name}/"), out)
                }
                TreeNode::Request { path, name, .. } => {
                    let mut request = storage::read_request(dir, path).unwrap();
                    request.seq = 0;
                    let previous = out.insert(format!("{prefix}{name}"), request);
                    assert!(previous.is_none(), "two requests called {prefix}{name}");
                }
                TreeNode::Invalid { path, error } => panic!("{path}: {error}"),
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &storage::scan_tree(dir).unwrap(), "", &mut out);
    out
}

fn without_seq(mut request: Request) -> Request {
    request.seq = 0;
    request
}

#[test]
fn openapi_imports_read_back_exactly_as_planned() {
    for grouping in [OpenApiGrouping::Paths, OpenApiGrouping::Tags] {
        for spec in std::fs::read_dir(testdata("openapi")).unwrap() {
            let spec = spec.unwrap().path();
            let plan = map_openapi(&std::fs::read_to_string(&spec).unwrap(), grouping).unwrap();
            let dir = tempfile::tempdir().unwrap();
            write_plan(dir.path(), &plan).unwrap();

            let mut planned = BTreeMap::new();
            for r in &plan.requests {
                planned.insert(r.name.clone(), without_seq(r.clone()));
            }
            fn folders(list: &[PlannedFolder], prefix: &str, out: &mut BTreeMap<String, Request>) {
                for f in list {
                    let prefix = format!("{prefix}{}/", f.folder.name);
                    for r in &f.requests {
                        out.insert(format!("{prefix}{}", r.name), without_seq(r.clone()));
                    }
                    folders(&f.folders, &prefix, out);
                }
            }
            folders(&plan.folders, "", &mut planned);

            let on_disk = requests_on_disk(dir.path());
            assert_eq!(
                on_disk.keys().collect::<Vec<_>>(),
                planned.keys().collect::<Vec<_>>(),
                "{}: different requests",
                spec.display()
            );
            for (name, request) in &planned {
                assert_eq!(
                    &on_disk[name],
                    request,
                    "{}: {name} changed",
                    spec.display()
                );
            }
            let collection = storage::load_collection(dir.path()).unwrap();
            assert_eq!(
                Collection {
                    id: plan.collection.id.clone(),
                    ..collection
                },
                plan.collection,
                "{}",
                spec.display()
            );
            let env = storage::find_environment(dir.path(), "default")
                .unwrap()
                .unwrap();
            assert_eq!(env.variables, plan.environment.variables);
        }
    }
}

#[test]
fn bruno_imports_read_back_exactly_as_planned() {
    for name in ["shop", "testbench", "yml-collection"] {
        let plan = map_bruno(&testdata("bruno").join(name)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_bruno_plan(dir.path(), &plan).unwrap();

        let mut planned = BTreeMap::new();
        fn items(list: &[PlannedItem], prefix: &str, out: &mut BTreeMap<String, Request>) {
            for item in list {
                match item {
                    PlannedItem::Request(r) => {
                        out.insert(format!("{prefix}{}", r.name), without_seq((**r).clone()));
                    }
                    PlannedItem::Folder(dir) => {
                        let folder = dir.folder.as_ref().unwrap();
                        items(&dir.items, &format!("{prefix}{}/", folder.name), out);
                    }
                }
            }
        }
        items(&plan.root.items, "", &mut planned);

        let on_disk = requests_on_disk(dir.path());
        assert_eq!(
            on_disk.keys().collect::<Vec<_>>(),
            planned.keys().collect::<Vec<_>>(),
            "{name}: different requests"
        );
        for (request_name, request) in &planned {
            assert_eq!(
                &on_disk[request_name], request,
                "{name}: {request_name} changed"
            );
        }
        for env in &plan.environments {
            let read = storage::find_environment(dir.path(), &env.name)
                .unwrap()
                .unwrap_or_else(|| panic!("{name}: environment {} missing", env.name));
            assert_eq!(read.variables, env.variables, "{name}: {}", env.name);
        }
    }
}

/// load → save → load → save: the second text equals the first, the values are equal.
fn idempotent<T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug>(file: &Path) {
    let text = std::fs::read_to_string(file).unwrap();
    let first: T = from_yaml_str(&text, file).unwrap_or_else(|e| panic!("{e}"));
    let saved = to_yaml_string(&first).unwrap();
    let second: T = from_yaml_str(&saved, file).unwrap();
    assert_eq!(
        first,
        second,
        "{}: values changed after saving",
        file.display()
    );
    assert_eq!(
        to_yaml_string(&second).unwrap(),
        saved,
        "{}: saving twice gives different files",
        file.display()
    );
}

#[test]
fn every_frida_file_in_testdata_saves_idempotently() {
    let mut checked = 0;
    for root in ["format", "sample-workspace", "scripting", "six-layers"] {
        let mut stack = vec![testdata(root)];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !name.ends_with(".yaml") {
                    continue;
                }
                let in_environments = path.parent().is_some_and(|p| p.ends_with("environments"));
                match name.as_str() {
                    "frida-workspace.yaml" => idempotent::<Workspace>(&path),
                    "collection.yaml" => idempotent::<Collection>(&path),
                    "folder.yaml" => idempotent::<Folder>(&path),
                    _ if in_environments || name == "environment.yaml" => {
                        idempotent::<Environment>(&path)
                    }
                    _ => idempotent::<Request>(&path),
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 30, "only {checked} files found");
}
