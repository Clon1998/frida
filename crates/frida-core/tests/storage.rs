//! Storage operations on real (temporary) folders.

use std::path::Path;

use frida_core::model::{BodyMode, Request};
use frida_core::storage::{self, TreeNode};

/// A fresh collection in a temp dir. The `TempDir` must be kept alive: when it is dropped,
/// the folder is deleted (that's why it is returned, even if a test doesn't use it directly).
fn new_collection() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    storage::create_collection(dir.path(), "Orders API").unwrap();
    dir
}

/// A compact picture of the tree: `["get-health.yaml", "orders/", "  orders/get-order.yaml"]`.
fn outline(root: &Path) -> Vec<String> {
    fn walk(nodes: &[TreeNode], depth: usize, out: &mut Vec<String>) {
        for node in nodes {
            let indent = "  ".repeat(depth);
            match node {
                TreeNode::Folder { path, children, .. } => {
                    out.push(format!("{indent}{path}/"));
                    walk(children, depth + 1, out);
                }
                TreeNode::Request { path, .. } => out.push(format!("{indent}{path}")),
                TreeNode::Invalid { path, .. } => out.push(format!("{indent}{path} (invalid)")),
            }
        }
    }
    let mut out = Vec::new();
    walk(&storage::scan_tree(root).unwrap(), 0, &mut out);
    out
}

fn create(root: &Path, parent: &str, name: &str) -> String {
    storage::create_request(root, parent, &Request::new(name)).unwrap()
}

#[test]
fn new_collection_has_collection_file_and_environments_folder() {
    let dir = new_collection();
    assert!(dir.path().join("collection.yaml").is_file());
    assert!(dir.path().join("environments").is_dir());
    let collection = storage::load_collection(dir.path()).unwrap();
    assert_eq!(collection.name, "Orders API");
    assert_eq!(
        collection.id.len(),
        36,
        "expected a UUID, got {}",
        collection.id
    );
    assert!(storage::create_collection(dir.path(), "Again").is_err());
}

#[test]
fn creating_requests_and_folders_matches_the_on_disk_layout() {
    let dir = new_collection();
    let root = dir.path();

    assert_eq!(create(root, "", "Get health"), "get-health.yaml");
    let orders = storage::create_folder(root, "", "Orders").unwrap();
    assert_eq!(orders, "orders");
    assert_eq!(create(root, &orders, "Get order"), "orders/get-order.yaml");
    assert_eq!(
        create(root, &orders, "Get order"),
        "orders/get-order-2.yaml"
    );

    assert!(root.join("orders/folder.yaml").is_file());
    assert_eq!(
        outline(root),
        [
            "get-health.yaml",
            "orders/",
            "  orders/get-order.yaml",
            "  orders/get-order-2.yaml"
        ]
    );
}

#[test]
fn save_and_read_request_round_trips_and_normalizes_method() {
    let dir = new_collection();
    let rel = create(dir.path(), "", "Create order");

    let mut request = storage::read_request(dir.path(), &rel).unwrap();
    request.method = " post ".to_string();
    request.url = "{{baseUrl}}/orders".to_string();
    request.body.mode = BodyMode::Json;
    request.body.json = "{\n  \"id\": 1\n}".to_string();
    storage::save_request(dir.path(), &rel, &request).unwrap();

    let back = storage::read_request(dir.path(), &rel).unwrap();
    assert_eq!(back.method, "POST");
    assert_eq!(back.body.json, request.body.json);
}

#[test]
fn environments_and_hidden_folders_are_not_part_of_the_tree() {
    let dir = new_collection();
    let root = dir.path();
    create(root, "", "Ping");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".git/folder.yaml"), "version: 1\nname: Git\n").unwrap();
    std::fs::write(
        root.join("environments/dev.yaml"),
        "version: 1\nname: dev\n",
    )
    .unwrap();
    // A plain directory without folder.yaml is ignored too.
    std::fs::create_dir_all(root.join("scratch")).unwrap();

    assert_eq!(outline(root), ["ping.yaml"]);
}

#[test]
fn broken_files_show_up_as_invalid_instead_of_failing_the_tree() {
    let dir = new_collection();
    create(dir.path(), "", "Ping");
    std::fs::write(dir.path().join("broken.yaml"), "<<<<<<< HEAD\nversion: 1\n").unwrap();

    assert_eq!(outline(dir.path()), ["ping.yaml", "broken.yaml (invalid)"]);
}

#[test]
fn rename_updates_name_and_file_name() {
    let dir = new_collection();
    let root = dir.path();
    let rel = create(root, "", "Get order");

    let new_rel = storage::rename_item(root, &rel, "Fetch order").unwrap();
    assert_eq!(new_rel, "fetch-order.yaml");
    assert!(!root.join("get-order.yaml").exists());
    assert_eq!(
        storage::read_request(root, &new_rel).unwrap().name,
        "Fetch order"
    );

    // Renaming to a name with the same slug keeps the file.
    assert_eq!(
        storage::rename_item(root, &new_rel, "Fetch Order").unwrap(),
        new_rel
    );
    assert!(storage::rename_item(root, &new_rel, "   ").is_err());
}

#[test]
fn rename_folder_moves_its_contents() {
    let dir = new_collection();
    let root = dir.path();
    let folder = storage::create_folder(root, "", "Orders").unwrap();
    create(root, &folder, "Get order");

    let new_folder = storage::rename_item(root, &folder, "Purchase orders").unwrap();
    assert_eq!(new_folder, "purchase-orders");
    assert_eq!(
        outline(root),
        ["purchase-orders/", "  purchase-orders/get-order.yaml"]
    );
}

#[test]
fn duplicate_places_the_copy_right_after_the_original() {
    let dir = new_collection();
    let root = dir.path();
    let first = create(root, "", "First");
    create(root, "", "Second");

    let copy = storage::duplicate_request(root, &first).unwrap();
    assert_eq!(copy, "first-copy.yaml");
    assert_eq!(
        outline(root),
        ["first.yaml", "first-copy.yaml", "second.yaml"]
    );
    assert_eq!(
        storage::read_request(root, &copy).unwrap().name,
        "First (copy)"
    );
}

#[test]
fn move_reorders_within_a_folder() {
    let dir = new_collection();
    let root = dir.path();
    let a = create(root, "", "A");
    create(root, "", "B");
    create(root, "", "C");

    storage::move_item(root, &a, "", 2).unwrap();
    assert_eq!(outline(root), ["b.yaml", "c.yaml", "a.yaml"]);

    // Out-of-range positions mean "last".
    storage::move_item(root, "b.yaml", "", 99).unwrap();
    assert_eq!(outline(root), ["c.yaml", "a.yaml", "b.yaml"]);
}

#[test]
fn move_between_folders_moves_the_file_and_avoids_name_clashes() {
    let dir = new_collection();
    let root = dir.path();
    let orders = storage::create_folder(root, "", "Orders").unwrap();
    create(root, &orders, "Get order");
    let loose = create(root, "", "Get order");

    let moved = storage::move_item(root, &loose, &orders, 0).unwrap();
    assert_eq!(moved, "orders/get-order-2.yaml");
    assert_eq!(
        outline(root),
        [
            "orders/",
            "  orders/get-order-2.yaml",
            "  orders/get-order.yaml"
        ]
    );
}

#[test]
fn a_folder_cannot_be_moved_into_itself() {
    let dir = new_collection();
    let root = dir.path();
    let outer = storage::create_folder(root, "", "Outer").unwrap();
    let inner = storage::create_folder(root, &outer, "Inner").unwrap();

    assert!(storage::move_item(root, &outer, &outer, 0).is_err());
    assert!(storage::move_item(root, &outer, &inner, 0).is_err());
}

#[test]
fn reorder_only_rewrites_files_whose_seq_changes() {
    let dir = new_collection();
    let root = dir.path();
    let a = create(root, "", "A");
    let b = create(root, "", "B");
    let before = std::fs::read_to_string(root.join(&a)).unwrap();

    storage::reorder(root, "", &[a.clone(), b]).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(&a)).unwrap(), before);
}

#[test]
fn delete_removes_files_and_folders() {
    let dir = new_collection();
    let root = dir.path();
    let folder = storage::create_folder(root, "", "Orders").unwrap();
    create(root, &folder, "Get order");
    let ping = create(root, "", "Ping");

    storage::delete_item(root, &ping).unwrap();
    storage::delete_item(root, &folder).unwrap();
    assert!(outline(root).is_empty());
    assert!(storage::delete_item(root, "").is_err());
}

#[test]
fn paths_outside_the_collection_are_rejected() {
    let dir = new_collection();
    let root = dir.path();
    assert!(storage::read_request(root, "../outside.yaml").is_err());
    assert!(storage::delete_item(root, "../").is_err());
    assert!(storage::create_folder(root, "/tmp", "x").is_err());
}

#[test]
fn workspace_add_create_and_remove_collections() {
    let ws = tempfile::tempdir().unwrap();
    let ws_dir = ws.path();
    storage::create_workspace(ws_dir, "EIP").unwrap();
    assert!(ws_dir.join("environments").is_dir());

    // A new collection inside the workspace folder is stored with a relative path.
    let inner = ws_dir.join("collections/orders-api");
    storage::create_collection_in_workspace(ws_dir, &inner, "Orders API").unwrap();

    // An existing collection elsewhere is stored with an absolute path.
    let outside = new_collection();
    storage::add_collection(ws_dir, outside.path()).unwrap();
    // Adding it twice doesn't duplicate the entry.
    storage::add_collection(ws_dir, outside.path()).unwrap();

    let workspace = storage::open_workspace(ws_dir).unwrap();
    let paths: Vec<&str> = workspace
        .collections
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    assert_eq!(paths[0], "./collections/orders-api");
    assert_eq!(paths.len(), 2);
    assert_eq!(
        storage::collection_dir(ws_dir, &workspace.collections[0]),
        ws_dir.join("./collections/orders-api")
    );

    // Folders without collection.yaml are refused.
    assert!(storage::add_collection(ws_dir, ws_dir).is_err());

    storage::remove_collection(ws_dir, outside.path()).unwrap();
    assert_eq!(
        storage::open_workspace(ws_dir).unwrap().collections.len(),
        1
    );
    // Removing only edits the workspace file; the collection itself stays.
    assert!(outside.path().join("collection.yaml").exists());
}

#[test]
fn overview_reports_broken_collections_without_failing() {
    let ws = tempfile::tempdir().unwrap();
    storage::create_workspace(ws.path(), "EIP").unwrap();
    let good = ws.path().join("orders");
    storage::create_collection_in_workspace(ws.path(), &good, "Orders API").unwrap();
    let gone = ws.path().join("gone");
    storage::create_collection_in_workspace(ws.path(), &gone, "Gone").unwrap();
    std::fs::remove_dir_all(&gone).unwrap();

    let overview = storage::load_overview(ws.path()).unwrap();
    assert_eq!(overview.name, "EIP");
    assert_eq!(overview.collections[0].name, "Orders API");
    assert!(overview.collections[0].error.is_none());
    assert!(overview.collections[1].id.is_none());
    assert!(overview.collections[1].error.is_some());
}

#[test]
fn named_collections_go_into_the_collections_folder() {
    let ws = tempfile::tempdir().unwrap();
    storage::create_workspace(ws.path(), "EIP").unwrap();

    let first = storage::create_named_collection(ws.path(), "Orders API").unwrap();
    let second = storage::create_named_collection(ws.path(), "Orders API").unwrap();
    assert_eq!(first, ws.path().join("collections/orders-api"));
    assert_eq!(second, ws.path().join("collections/orders-api-2"));

    let workspace = storage::open_workspace(ws.path()).unwrap();
    let paths: Vec<&str> = workspace
        .collections
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["./collections/orders-api", "./collections/orders-api-2"]
    );
}
