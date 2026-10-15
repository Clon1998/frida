//! Variable resolution (PLAN.md §5).

use std::collections::HashMap;

use frida_core::model::{Collection, EnvVariable, Environment, Folder, KeyValue, Request};
use frida_core::vars::{
    ResolvedVar, Resolver, ScopeInput, UsageLocation, VarScope, VarSource, resolve_context,
};

/// A small builder for test scopes: each field is one layer.
#[derive(Default)]
struct Layers {
    runtime: Vec<(&'static str, &'static str)>,
    request: Vec<KeyValue>,
    folders: Vec<(String, Folder)>,
    collection_env: Option<Environment>,
    collection: Vec<KeyValue>,
    global_env: Option<Environment>,
}

fn kv(name: &str, value: &str) -> KeyValue {
    KeyValue::new(name, value)
}

fn env(name: &str, vars: &[(&str, Option<&str>, bool)]) -> Environment {
    let mut env = Environment::new(name);
    env.variables = vars
        .iter()
        .map(|(n, v, secret)| EnvVariable {
            name: n.to_string(),
            value: v.map(str::to_string),
            secret: *secret,
            source: None,
        })
        .collect();
    env
}

fn folder(path: &str, vars: Vec<KeyValue>) -> (String, Folder) {
    let mut folder = Folder::new(path.rsplit('/').next().unwrap());
    folder.vars = vars;
    (path.to_string(), folder)
}

impl Layers {
    /// Resolves `text` (as the URL) and returns the result and the report's variables.
    fn resolve(&self, text: &str) -> (String, Vec<ResolvedVar>, Vec<String>) {
        let runtime: HashMap<String, String> = self
            .runtime
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut request = Request::new("r");
        request.vars = self.request.clone();
        let mut collection = Collection::new("c");
        collection.vars = self.collection.clone();
        let input = ScopeInput {
            runtime: &runtime,
            request: &request,
            folders: &self.folders,
            collection_env: self.collection_env.as_ref(),
            collection: &collection,
            global_env: self.global_env.as_ref(),
        };
        let scope = VarScope::build(&input);
        let mut resolver = Resolver::new(&scope);
        let out = resolver.resolve(text, UsageLocation::Url);
        let report = resolver.finish();
        (out, report.vars, report.errors)
    }
}

fn var<'a>(vars: &'a [ResolvedVar], name: &str) -> &'a ResolvedVar {
    vars.iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("{name} not in report"))
}

#[test]
fn replaces_placeholders_with_optional_whitespace() {
    let layers = Layers {
        collection: vec![kv("host", "api.dev"), kv("v", "2")],
        ..Default::default()
    };
    let (out, _, _) = layers.resolve("https://{{host}}/v{{ v }}/x?{{  v}}");
    assert_eq!(out, "https://api.dev/v2/x?2");
}

#[test]
fn unresolved_variables_are_left_as_written_and_reported() {
    let (out, vars, errors) = Layers::default().resolve("{{ baseUrl }}/orders");
    assert_eq!(out, "{{ baseUrl }}/orders");
    let base = var(&vars, "baseUrl");
    assert!(!base.resolved);
    assert_eq!(base.source, None);
    assert!(errors.is_empty());
}

#[test]
fn text_without_closing_braces_is_left_alone() {
    let (out, vars, _) = Layers::default().resolve("a {{b c } {{");
    assert_eq!(out, "a {{b c } {{");
    assert!(vars.is_empty());
}

#[test]
fn higher_layers_win_in_the_documented_order() {
    // Each step removes the current winner, and the next layer must take over.
    let mut layers = Layers {
        runtime: vec![("x", "runtime")],
        request: vec![kv("x", "request")],
        folders: vec![
            folder("outer", vec![kv("x", "outer folder")]),
            folder("outer/inner", vec![kv("x", "inner folder")]),
        ],
        collection_env: Some(env("dev", &[("x", Some("collection env"), false)])),
        collection: vec![kv("x", "collection")],
        global_env: Some(env("global", &[("x", Some("global env"), false)])),
    };
    let expected = [
        "runtime",
        "request",
        "inner folder",
        "outer folder",
        "collection env",
        "collection",
        "global env",
    ];
    for value in expected {
        assert_eq!(layers.resolve("{{x}}").0, value);
        match value {
            "runtime" => layers.runtime.clear(),
            "request" => layers.request.clear(),
            "inner folder" => layers.folders[1].1.vars.clear(),
            "outer folder" => layers.folders[0].1.vars.clear(),
            "collection env" => layers.collection_env = None,
            "collection" => layers.collection.clear(),
            _ => {}
        }
    }
}

#[test]
fn disabled_rows_are_ignored() {
    let layers = Layers {
        request: vec![KeyValue {
            enabled: false,
            ..kv("x", "disabled")
        }],
        collection: vec![kv("x", "collection")],
        ..Default::default()
    };
    let (out, vars, _) = layers.resolve("{{x}}");
    assert_eq!(out, "collection");
    assert!(var(&vars, "x").shadowed.is_empty());
}

#[test]
fn values_resolve_recursively() {
    let layers = Layers {
        collection: vec![
            kv("baseUrl", "{{scheme}}://{{host}}/api"),
            kv("scheme", "https"),
        ],
        collection_env: Some(env("dev", &[("host", Some("dev.example.com"), false)])),
        ..Default::default()
    };
    let (out, vars, _) = layers.resolve("{{baseUrl}}/orders");
    assert_eq!(out, "https://dev.example.com/api/orders");
    // Variables used inside another variable are reported with that location.
    assert_eq!(
        var(&vars, "host").used_in,
        [UsageLocation::Variable {
            name: "baseUrl".into()
        }]
    );
}

#[test]
fn cycles_are_detected_and_named() {
    let layers = Layers {
        collection: vec![kv("a", "x{{b}}"), kv("b", "y{{c}}"), kv("c", "{{a}}")],
        ..Default::default()
    };
    let (out, _, errors) = layers.resolve("{{a}}");
    assert_eq!(out, "xy{{a}}");
    assert_eq!(errors, ["variable cycle: a → b → c → a"]);
}

#[test]
fn self_reference_is_a_cycle() {
    let layers = Layers {
        collection: vec![kv("a", "{{a}}")],
        ..Default::default()
    };
    let (_, _, errors) = layers.resolve("{{a}}");
    assert_eq!(errors, ["variable cycle: a → a"]);
}

#[test]
fn nesting_deeper_than_ten_levels_is_an_error() {
    // v0 -> v1 -> ... -> v11
    let collection: Vec<KeyValue> = (0..12)
        .map(|i| kv(&format!("v{i}"), &format!("{{{{v{}}}}}", i + 1)))
        .collect();
    let layers = Layers {
        collection,
        ..Default::default()
    };
    let (_, _, errors) = layers.resolve("{{v0}}");
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("nested more than 10 levels"),
        "{errors:?}"
    );

    // Ten levels are fine.
    let collection: Vec<KeyValue> = (0..10)
        .map(|i| kv(&format!("v{i}"), &format!("{{{{v{}}}}}", i + 1)))
        .chain([kv("v10", "end")])
        .collect();
    let layers = Layers {
        collection,
        ..Default::default()
    };
    let (out, _, errors) = layers.resolve("{{v0}}");
    assert_eq!(out, "end");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn dynamic_variables_are_fresh_per_occurrence() {
    let (out, vars, _) = Layers::default().resolve("{{$uuid}} {{$uuid}} {{$timestamp}}");
    let parts: Vec<&str> = out.split(' ').collect();
    assert_eq!(parts[0].len(), 36);
    assert_ne!(parts[0], parts[1]);
    assert!(parts[2].parse::<u64>().is_ok());
    assert_eq!(var(&vars, "$uuid").source, Some(VarSource::Dynamic));

    let (out, vars, _) = Layers::default().resolve("{{$nope}}");
    assert_eq!(out, "{{$nope}}");
    assert!(!var(&vars, "$nope").resolved);
}

#[test]
fn process_env_is_a_separate_namespace() {
    // PATH is set in every test environment we run in.
    let path = std::env::var("PATH").unwrap();
    let layers = Layers {
        collection: vec![kv("process.env.PATH", "shadow attempt")],
        ..Default::default()
    };
    let (out, vars, _) =
        layers.resolve("{{process.env.PATH}}|{{process.env.FRIDA_SURELY_UNSET_VAR}}");
    assert_eq!(
        out,
        format!("{path}|{{{{process.env.FRIDA_SURELY_UNSET_VAR}}}}")
    );
    assert_eq!(
        var(&vars, "process.env.PATH").source,
        Some(VarSource::ProcessEnv)
    );
}

#[test]
fn unset_secrets_are_unresolved_and_do_not_fall_back() {
    let layers = Layers {
        collection_env: Some(env("dev", &[("token", None, true)])),
        global_env: Some(env("global", &[("token", Some("global-token"), false)])),
        ..Default::default()
    };
    let (out, vars, _) = layers.resolve("{{token}}");
    assert_eq!(out, "{{token}}");
    let token = var(&vars, "token");
    assert!(!token.resolved);
    assert!(token.is_secret);
    assert_eq!(
        token.source,
        Some(VarSource::CollectionEnv { name: "dev".into() })
    );
}

#[test]
fn secrets_are_masked_in_the_report_and_tracked_for_redaction() {
    let mut request = Request::new("r");
    request.url = "{{auth}}".into();
    request.headers = vec![kv("Authorization", "Bearer {{token}}")];
    let mut collection = Collection::new("c");
    collection.vars = vec![kv("auth", "token={{token}}")];
    let env = env("dev", &[("token", Some("s3cr3t"), true)]);
    let runtime = HashMap::new();

    let ctx = resolve_context(&ScopeInput {
        runtime: &runtime,
        request: &request,
        folders: &[],
        collection_env: Some(&env),
        collection: &collection,
        global_env: None,
    });
    assert_eq!(ctx.request.headers[0].value, "Bearer s3cr3t");
    // `auth` contains the secret, so it counts as secret too.
    assert!(var(&ctx.report.vars, "auth").is_secret);
    assert!(ctx.report.secret_values.contains(&"s3cr3t".to_string()));

    let masked = ctx.report.masked();
    let shown = serde_json::to_string(&masked).unwrap();
    assert!(!shown.contains("s3cr3t"), "{shown}");
    assert!(var(&masked.vars, "token").masked);
}

#[test]
fn resolves_every_part_of_a_request_and_inherited_headers() {
    let mut request = Request::new("r");
    request.url = "{{base}}/orders/:id".into();
    request.params.path = vec![frida_core::model::PathParam {
        name: "id".into(),
        value: "{{id}}".into(),
    }];
    request.headers = vec![kv("X-{{h}}", "{{v}}")];
    request.body.json = r#"{"id": "{{id}}"}"#.into();
    request.body.form_url_encoded = vec![kv("{{h}}", "{{v}}")];
    let mut collection = Collection::new("c");
    collection.vars = vec![
        kv("base", "http://x"),
        kv("id", "42"),
        kv("h", "Team"),
        kv("v", "orders"),
    ];
    collection.headers = vec![kv("X-Collection", "{{v}}")];
    let folders = [folder("orders", vec![])];
    let mut folders = folders.to_vec();
    folders[0].1.headers = vec![kv("X-Folder", "{{id}}")];
    let runtime = HashMap::new();

    let ctx = resolve_context(&ScopeInput {
        runtime: &runtime,
        request: &request,
        folders: &folders,
        collection_env: None,
        collection: &collection,
        global_env: None,
    });
    assert_eq!(ctx.request.url, "http://x/orders/:id");
    assert_eq!(ctx.request.params.path[0].value, "42");
    assert_eq!(ctx.request.headers[0], kv("X-Team", "orders"));
    assert_eq!(ctx.request.body.json, r#"{"id": "42"}"#);
    assert_eq!(ctx.request.body.form_url_encoded[0], kv("Team", "orders"));
    assert_eq!(ctx.collection.headers[0].value, "orders");
    assert_eq!(ctx.folders[0].headers[0].value, "42");

    let id = var(&ctx.report.vars, "id");
    assert!(
        id.used_in
            .contains(&UsageLocation::PathParam { name: "id".into() })
    );
    assert!(id.used_in.contains(&UsageLocation::Body));
    assert!(id.used_in.contains(&UsageLocation::Header {
        name: "X-Folder".into()
    }));
}
