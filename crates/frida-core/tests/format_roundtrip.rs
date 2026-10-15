//! Round-trip tests for the on-disk format (PLAN.md §4).
//!
//! Each fixture in `testdata/format/` is a verbatim example from PLAN.md. It must parse, and
//! writing it back and parsing again must give an identical value. The snapshots show exactly
//! what Frida writes, so format changes are visible in code review.

use std::path::{Path, PathBuf};

use frida_core::CoreError;
use frida_core::model::{
    Auth, AuthScheme, BodyMode, Collection, Environment, Folder, Request, Workspace,
};
use frida_core::yaml::{from_yaml_str, read_yaml, to_yaml_string, write_yaml};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/format")
        .join(name)
}

/// Parses a fixture, writes it to YAML, parses that again, and checks nothing changed.
/// Returns the parsed value and the YAML Frida wrote.
fn round_trip<T>(name: &str) -> (T, String)
where
    T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug,
{
    let parsed: T = read_yaml(&fixture(name)).unwrap();
    let written = to_yaml_string(&parsed).unwrap();
    let reparsed: T = from_yaml_str(&written, Path::new(name)).unwrap();
    assert_eq!(parsed, reparsed, "{name} changed after a round trip");
    (parsed, written)
}

#[test]
fn workspace_round_trips() {
    let (ws, yaml) = round_trip::<Workspace>("frida-workspace.yaml");
    assert_eq!(ws.name, "EIP");
    assert_eq!(ws.collections.len(), 2);
    insta::assert_snapshot!(yaml);
}

#[test]
fn collection_round_trips() {
    let (col, yaml) = round_trip::<Collection>("collection.yaml");
    assert_eq!(col.settings.timeout_ms, Some(30_000));
    let Auth::Custom { schemes } = &col.auth else {
        panic!("expected custom auth, got {:?}", col.auth);
    };
    assert!(matches!(schemes[0], AuthScheme::OAuth2(_)));
    assert!(matches!(schemes[1], AuthScheme::ApiKey { .. }));
    insta::assert_snapshot!(yaml);
}

#[test]
fn folder_round_trips() {
    let (folder, yaml) = round_trip::<Folder>("folder.yaml");
    assert_eq!(folder.auth, Auth::Inherit);
    insta::assert_snapshot!(yaml);
}

#[test]
fn request_round_trips() {
    let (req, yaml) = round_trip::<Request>("request.yaml");
    assert_eq!(req.method, "GET");
    assert_eq!(req.body.mode, BodyMode::None);
    assert_eq!(req.params.path[0].value, "{{lastOrderId}}");
    assert_eq!(req.settings.timeout_ms, None);
    insta::assert_snapshot!(yaml);
}

#[test]
fn environment_round_trips() {
    let (env, yaml) = round_trip::<Environment>("environment.yaml");
    let secret = env
        .variables
        .iter()
        .find(|v| v.name == "clientSecret")
        .unwrap();
    assert!(secret.secret);
    assert_eq!(secret.value, None);
    insta::assert_snapshot!(yaml);
}

#[test]
fn minimal_request_gets_defaults() {
    let yaml = "version: 1\nname: Ping\nmethod: GET\n";
    let req: Request = from_yaml_str(yaml, Path::new("ping.yaml")).unwrap();
    assert_eq!(req.auth, Auth::Inherit);
    assert_eq!(req.body.mode, BodyMode::None);
    assert!(req.headers.is_empty());
}

#[test]
fn unknown_fields_are_ignored() {
    let yaml = "version: 1\nname: Ping\nmethod: GET\nsomethingNew: 42\n";
    let req: Request = from_yaml_str(yaml, Path::new("ping.yaml")).unwrap();
    assert_eq!(req.name, "Ping");
}

#[test]
fn newer_format_version_is_rejected_clearly() {
    let yaml = "version: 2\nname: Ping\nmethod: GET\nbody: [completely, different]\n";
    let err = from_yaml_str::<Request>(yaml, Path::new("ping.yaml")).unwrap_err();
    assert!(
        matches!(err, CoreError::UnsupportedVersion { found: 2, .. }),
        "{err}"
    );
}

#[test]
fn missing_version_is_an_error() {
    let err =
        from_yaml_str::<Request>("name: Ping\nmethod: GET\n", Path::new("p.yaml")).unwrap_err();
    assert!(err.to_string().contains("version"), "{err}");
}

#[test]
fn write_yaml_creates_folders_and_overwrites_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/get-order.yaml");

    write_yaml(&path, &Request::new("First")).unwrap();
    write_yaml(&path, &Request::new("Second")).unwrap();

    let back: Request = read_yaml(&path).unwrap();
    assert_eq!(back.name, "Second");
    // Only the target file is left behind, no temp files.
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn secret_values_never_reach_yaml() {
    let mut env: Environment = read_yaml(&fixture("environment.yaml")).unwrap();
    let secret = env
        .variables
        .iter_mut()
        .find(|v| v.name == "clientSecret")
        .unwrap();
    secret.value = Some("super-secret-value".to_string());

    let yaml = to_yaml_string(&env.without_secret_values()).unwrap();
    assert!(!yaml.contains("super-secret-value"));
}
