//! M4 acceptance (PLAN.md §15): a snapshot corpus of curl commands (`testdata/curl/*.curl`)
//! imports as expected, and exporting requests to curl and importing them again round-trips.

use std::path::Path;
use std::sync::Arc;

use frida_core::auth::OAuth2Manager;
use frida_core::codegen::{Shell, curl_request, render_curl};
use frida_core::http::RequestContext;
use frida_core::import::import_curl;
use frida_core::model::{
    Auth, AuthScheme, BodyMode, Collection, KeyValue, MultipartField, MultipartKind, Request,
};
use frida_core::yaml::to_yaml_string;

#[test]
fn corpus_imports_match_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/curl");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "curl"))
        .collect();
    files.sort();
    assert!(files.len() >= 10);

    for file in files {
        let command = std::fs::read_to_string(&file).unwrap();
        let imported = import_curl(&command).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        let snapshot = format!(
            "{}\nwarnings: {:#?}",
            to_yaml_string(&imported.request).unwrap(),
            imported.warnings
        );
        insta::assert_snapshot!(name, snapshot);
    }
}

fn base(method: &str, url: &str) -> Request {
    let mut r = Request::new("r");
    r.method = method.into();
    r.url = url.into();
    r
}

/// Requests that should survive export → import unchanged in the parts curl can express.
fn corpus() -> Vec<Request> {
    let mut json = base("POST", "https://api.example.com/orders?expand=items");
    // The default Content-Type of each body mode is added on export and dropped on import.
    json.headers = vec![KeyValue::new("X-Id", "it's 1")];
    json.body.mode = BodyMode::Json;
    json.body.json = "{\n  \"item\": \"book\"\n}".into();

    let mut form = base("POST", "https://example.com/login");
    form.body.mode = BodyMode::FormUrlencoded;
    form.body.form_url_encoded = vec![
        KeyValue::new("user", "alice"),
        KeyValue::new("pass", "a&b c"),
    ];

    let mut multipart = base("POST", "https://example.com/upload");
    multipart.body.mode = BodyMode::Multipart;
    multipart.body.multipart = vec![
        MultipartField {
            name: "note".into(),
            kind: MultipartKind::Text,
            value: "hi".into(),
            enabled: true,
        },
        MultipartField {
            name: "file".into(),
            kind: MultipartKind::File,
            value: "a.pdf".into(),
            enabled: true,
        },
    ];

    let mut binary = base("PUT", "https://example.com/files/x");
    binary.body.mode = BodyMode::Binary;
    binary.body.binary_path = "x.bin".into();

    let mut basic = base("DELETE", "https://example.com/orders/1");
    basic.auth = Auth::Custom {
        schemes: vec![AuthScheme::Basic {
            username: "alice".into(),
            password: "pw".into(),
        }],
    };

    let mut text = base("PATCH", "https://example.com/notes/1");
    text.headers = vec![KeyValue::new("Content-Type", "text/markdown")];
    text.body.mode = BodyMode::Text;
    text.body.text = "plain text".into();

    vec![
        base("GET", "https://example.com/a?x=1&y=2"),
        base("HEAD", "https://example.com/"),
        json,
        form,
        multipart,
        binary,
        basic,
        text,
    ]
}

#[test]
fn export_then_import_round_trips() {
    let collection = Collection::new("c");
    let ctx = RequestContext {
        collection_dir: Path::new("."),
        collection: &collection,
        folders: &[],
    };
    let oauth = OAuth2Manager::new(Arc::new(|_| Ok(())));

    for original in corpus() {
        let auth = match &original.auth {
            Auth::Custom { schemes } => schemes.clone(),
            _ => vec![],
        };
        let (curl, _) = curl_request(ctx, &original, &auth, &oauth);
        let command = render_curl(&curl, Shell::Bash, &[]);
        let back = import_curl(&command)
            .unwrap_or_else(|e| panic!("{command}\n{e}"))
            .request;

        let label = format!("{} {}\n{command}", original.method, original.url);
        assert_eq!(back.method, original.method, "{label}");
        assert_eq!(back.url, original.url, "{label}");
        assert_eq!(back.headers, original.headers, "{label}");
        assert_eq!(back.body.mode, original.body.mode, "{label}");
        assert_eq!(back.body.json, original.body.json, "{label}");
        assert_eq!(back.body.text, original.body.text, "{label}");
        assert_eq!(
            back.body.form_url_encoded, original.body.form_url_encoded,
            "{label}"
        );
        assert_eq!(back.body.multipart, original.body.multipart, "{label}");
        assert_eq!(back.body.binary_path, original.body.binary_path, "{label}");
        assert_eq!(back.auth, original.auth, "{label}");
    }
}

#[test]
fn powershell_exports_import_too() {
    let mut request = base("PUT", "https://example.com/x");
    request.headers = vec![KeyValue::new("X-A", "b")];
    let collection = Collection::new("c");
    let ctx = RequestContext {
        collection_dir: Path::new("."),
        collection: &collection,
        folders: &[],
    };
    let (curl, _) = curl_request(
        ctx,
        &request,
        &[],
        &OAuth2Manager::new(Arc::new(|_| Ok(()))),
    );
    let command = render_curl(&curl, Shell::PowerShell, &[]);
    let back = import_curl(&command).unwrap().request;
    assert_eq!(
        (back.method.as_str(), back.url.as_str()),
        ("PUT", "https://example.com/x")
    );
    assert_eq!(back.headers, request.headers);
}
