//! M12 acceptance (PLAN.md §15): Frida archives. A collection exported and imported into
//! another workspace loads unchanged; no secret values end up in an archive; unsafe archives
//! (path traversal, absolute paths, symlinks) are rejected without writing anything.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use frida_core::archive::{
    ArchiveKind, ExportSource, ImportTarget, MANIFEST_FILE, export_archive, import_archive,
    preview_archive,
};
use frida_core::model::{EnvVariable, Environment};
use frida_core::storage;
use zip::write::SimpleFileOptions;

fn sample_collection() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/sample-workspace/collections/echo-api")
}

/// Every non-hidden file below `dir`, relative path → content.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn new_workspace(dir: &Path) -> PathBuf {
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    storage::create_workspace(&ws, "Target").unwrap();
    ws
}

#[test]
fn a_collection_round_trips_into_another_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("echo.zip");
    let source = sample_collection();
    let summary = export_archive(
        &ExportSource::Collection {
            collection_dir: source.to_string_lossy().into(),
        },
        &zip,
    )
    .unwrap();
    assert_eq!(summary.manifest.kind, ArchiveKind::Collection);
    assert_eq!(preview_archive(&zip).unwrap().files, summary.files);

    let ws = new_workspace(tmp.path());
    let imported = import_archive(
        &zip,
        &ImportTarget::Collection {
            workspace_dir: ws.to_string_lossy().into(),
            dir: None,
        },
    )
    .unwrap();
    let dest = PathBuf::from(&imported.dir);
    assert!(dest.starts_with(ws.join("collections")));
    assert_eq!(files(&dest), files(&source), "same files, byte for byte");
    // Hidden files (here `.DS_Store`) are not exported.
    assert!(!dest.join(".DS_Store").exists());
    // It is in the workspace and loads like the original.
    let overview = storage::load_overview(&ws).unwrap();
    assert_eq!(overview.collections.len(), 1);
    assert_eq!(
        storage::scan_tree(&dest).unwrap(),
        storage::scan_tree(&source).unwrap()
    );

    // Importing the same archive again into the same workspace: a second copy with a new id.
    let again = import_archive(
        &zip,
        &ImportTarget::Collection {
            workspace_dir: ws.to_string_lossy().into(),
            dir: None,
        },
    )
    .unwrap();
    assert_ne!(again.dir, imported.dir);
    let ids: Vec<_> = storage::load_overview(&ws)
        .unwrap()
        .collections
        .into_iter()
        .map(|c| c.id.unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert!(!again.summary.warnings.is_empty());
}

#[test]
fn folders_and_requests_go_into_another_collection_under_free_names() {
    let tmp = tempfile::tempdir().unwrap();
    let source = sample_collection();
    let target = tmp.path().join("target");
    storage::create_collection(&target, "Target").unwrap();
    std::fs::create_dir(target.join("bodies")).unwrap(); // taken: the folder gets another name

    let folder_zip = tmp.path().join("bodies.zip");
    export_archive(
        &ExportSource::Folder {
            collection_dir: source.to_string_lossy().into(),
            rel: "bodies".into(),
        },
        &folder_zip,
    )
    .unwrap();
    let into = ImportTarget::Into {
        collection_dir: target.to_string_lossy().into(),
        parent: String::new(),
    };
    let folder = import_archive(&folder_zip, &into).unwrap();
    let folder_path = folder.path.unwrap();
    assert_ne!(folder_path, "bodies");
    assert_eq!(
        files(&target.join(&folder_path)),
        files(&source.join("bodies"))
    );

    let request_zip = tmp.path().join("slow.zip");
    export_archive(
        &ExportSource::Request {
            collection_dir: source.to_string_lossy().into(),
            rel: "slow-request.yaml".into(),
        },
        &request_zip,
    )
    .unwrap();
    let request = import_archive(
        &request_zip,
        &ImportTarget::Into {
            collection_dir: target.to_string_lossy().into(),
            parent: folder_path.clone(),
        },
    )
    .unwrap();
    let rel = request.path.unwrap();
    assert!(rel.starts_with(&format!("{folder_path}/")));
    assert_eq!(
        storage::read_request(&target, &rel).unwrap(),
        storage::read_request(&source, "slow-request.yaml").unwrap()
    );

    // A request archive can't become a collection.
    let err = import_archive(
        &request_zip,
        &ImportTarget::Collection {
            workspace_dir: tmp.path().to_string_lossy().into(),
            dir: None,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("request"));
}

#[test]
fn a_workspace_round_trips_with_its_collections_moved_inside() {
    let tmp = tempfile::tempdir().unwrap();
    // A workspace whose collection lives outside its folder.
    let ws = new_workspace(tmp.path());
    let outside = tmp.path().join("elsewhere").join("orders");
    storage::create_collection(&outside, "Orders API").unwrap();
    storage::add_collection(&ws, &outside).unwrap();
    storage::create_environment(&ws, "shared").unwrap();

    let zip = tmp.path().join("ws.zip");
    export_archive(
        &ExportSource::Workspace {
            workspace_dir: ws.to_string_lossy().into(),
        },
        &zip,
    )
    .unwrap();
    let dest = tmp.path().join("imported");
    import_archive(
        &zip,
        &ImportTarget::Workspace {
            dir: dest.to_string_lossy().into(),
        },
    )
    .unwrap();
    let overview = storage::load_overview(&dest).unwrap();
    assert_eq!(overview.collections.len(), 1);
    assert_eq!(overview.collections[0].name, "Orders API");
    assert!(overview.collections[0].error.is_none());
    assert!(PathBuf::from(&overview.collections[0].dir).starts_with(dest.canonicalize().unwrap()));
    assert!(
        storage::find_environment(&dest, "shared")
            .unwrap()
            .is_some()
    );
}

#[test]
fn archives_never_contain_secret_values() {
    let tmp = tempfile::tempdir().unwrap();
    let col = tmp.path().join("col");
    storage::create_collection(&col, "Col").unwrap();
    storage::create_environment(&col, "prod").unwrap();
    let mut env = Environment::new("prod");
    env.variables.push(EnvVariable {
        name: "token".into(),
        value: Some("s3cr3t-value".into()),
        secret: true,
        source: None,
        enabled: true,
    });
    storage::save_environment(&col, "prod", &env).unwrap();

    let zip = tmp.path().join("col.zip");
    export_archive(
        &ExportSource::Collection {
            collection_dir: col.to_string_lossy().into(),
        },
        &zip,
    )
    .unwrap();
    // Look at the uncompressed contents, not the (compressed) zip bytes.
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().unwrap().to_string();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text).unwrap();
        assert!(!text.contains("s3cr3t-value"), "{} leaks the secret", name);
        if name.ends_with("environments/prod.yaml") {
            assert!(text.contains("secret: true"));
        }
    }
}

/// A zip with a valid manifest plus the given extra entries.
fn crafted(path: &Path, kind: &str, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file(MANIFEST_FILE, options).unwrap();
    write!(
        zip,
        "format: 1\nkind: {kind}\nname: Evil\nfridaVersion: 0.1.0\nexportedAt: 2026-10-16T00:00:00Z\n"
    )
    .unwrap();
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn unsafe_archives_are_rejected_without_writing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    storage::create_collection(&target, "Target").unwrap();
    let before = files(&target);
    let into = ImportTarget::Into {
        collection_dir: target.to_string_lossy().into(),
        parent: String::new(),
    };

    for (name, entry) in [
        ("traversal", "content/../../evil.yaml"),
        ("dotdot", "content/a/../../evil.yaml"),
        ("absolute", "/tmp/frida-evil.yaml"),
    ] {
        let zip = tmp.path().join(format!("{name}.zip"));
        crafted(&zip, "request", &[(entry, b"name: Evil\n")]);
        let err = import_archive(&zip, &into).unwrap_err();
        assert!(err.to_string().contains("unsafe"), "{name}: {err}");
    }

    // A symlink entry (unix mode `S_IFLNK`).
    let zip_path = tmp.path().join("symlink.zip");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file(MANIFEST_FILE, options).unwrap();
        zip.write_all(b"format: 1\nkind: request\nname: Evil\nfridaVersion: x\nexportedAt: x\n")
            .unwrap();
        zip.add_symlink("content/link.yaml", "/etc/passwd", options)
            .unwrap();
        zip.finish().unwrap();
    }
    let err = import_archive(&zip_path, &into).unwrap_err();
    assert!(err.to_string().contains("symbolic link"), "{err}");

    // Not a Frida archive at all.
    let plain = tmp.path().join("plain.zip");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&plain).unwrap());
        zip.start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
    }
    assert!(
        import_archive(&plain, &into)
            .unwrap_err()
            .to_string()
            .contains("not a Frida archive")
    );

    assert_eq!(files(&target), before, "nothing was written");
    assert!(!tmp.path().join("evil.yaml").exists());
    assert!(!Path::new("/tmp/frida-evil.yaml").exists());
}
