//! Resolving every place a request can contain `{{variables}}` (PLAN.md §5.2).

use super::resolver::Resolver;
use super::scope::{ScopeInput, VarScope};
use super::{AvailableVar, DYNAMIC_VARIABLES, ResolveReport, UsageLocation, VarSource};
use crate::auth::effective_auth;
use crate::model::{AuthScheme, Collection, Folder, KeyValue, Request};

/// A request and its inherited data with all variables resolved, plus the report.
#[derive(Debug, Clone)]
pub struct ResolvedContext {
    pub request: Request,
    pub collection: Collection,
    /// Same order as the input: outermost first.
    pub folders: Vec<Folder>,
    /// The auth schemes that apply (after inheritance), with variables resolved.
    pub auth: Vec<AuthScheme>,
    pub report: ResolveReport,
}

/// Resolves the request and the headers it inherits from its folders and collection.
pub fn resolve_context(input: &ScopeInput<'_>) -> ResolvedContext {
    let scope = VarScope::build(input);
    let mut resolver = Resolver::new(&scope);

    let mut collection = input.collection.clone();
    collection.headers = resolve_headers(&mut resolver, &collection.headers);
    let folders = input
        .folders
        .iter()
        .map(|(_, folder)| {
            let mut folder = folder.clone();
            folder.headers = resolve_headers(&mut resolver, &folder.headers);
            folder
        })
        .collect();
    let request = resolve_request(&mut resolver, input.request);
    let auth = effective_auth(input.request, input.folders, input.collection)
        .iter()
        .map(|scheme| resolve_auth(&mut resolver, scheme))
        .collect();

    let mut report = resolver.finish();
    report.available = available(&scope);
    ResolvedContext {
        request,
        collection,
        folders,
        auth,
        report,
    }
}

/// The winning definition of every name in scope (layers are ordered by priority), then the
/// dynamic variables.
fn available(scope: &VarScope) -> Vec<AvailableVar> {
    let mut out: Vec<AvailableVar> = Vec::new();
    for layer in &scope.layers {
        for def in &layer.vars {
            if out.iter().any(|v| v.name == def.name) {
                continue;
            }
            out.push(AvailableVar {
                name: def.name.clone(),
                source: layer.source.clone(),
                value: def.value.clone(),
                is_secret: def.is_secret,
                has_value: def.value.is_some(),
            });
        }
    }
    for name in DYNAMIC_VARIABLES {
        out.push(AvailableVar {
            name: format!("${name}"),
            source: VarSource::Dynamic,
            value: None,
            is_secret: false,
            has_value: true,
        });
    }
    out
}

/// A copy of `request` with variables resolved in the URL, path params, headers and every
/// body mode (including form fields, multipart values and file paths).
pub fn resolve_request(resolver: &mut Resolver<'_>, request: &Request) -> Request {
    let mut out = request.clone();

    out.url = resolver.resolve(&request.url, UsageLocation::Url);
    for param in &mut out.params.path {
        param.value = resolver.resolve(
            &param.value,
            UsageLocation::PathParam {
                name: param.name.clone(),
            },
        );
    }
    // Query params live in the URL when sending; the table is resolved too, for the inspector.
    for param in out.params.query.iter_mut().filter(|p| p.enabled) {
        param.value = resolver.resolve(&param.value, UsageLocation::Url);
    }
    out.headers = resolve_headers(resolver, &request.headers);

    let body = &mut out.body;
    body.json = resolver.resolve(&body.json, UsageLocation::Body);
    body.xml = resolver.resolve(&body.xml, UsageLocation::Body);
    body.text = resolver.resolve(&body.text, UsageLocation::Body);
    body.binary_path = resolver.resolve(&body.binary_path, UsageLocation::Body);
    for field in body.form_url_encoded.iter_mut().filter(|f| f.enabled) {
        field.name = resolver.resolve(&field.name, UsageLocation::Body);
        field.value = resolver.resolve(&field.value, UsageLocation::Body);
    }
    for field in body.multipart.iter_mut().filter(|f| f.enabled) {
        field.name = resolver.resolve(&field.name, UsageLocation::Body);
        field.value = resolver.resolve(&field.value, UsageLocation::Body);
    }
    out
}

/// A copy of an auth scheme with variables resolved in every text field.
fn resolve_auth(resolver: &mut Resolver<'_>, scheme: &AuthScheme) -> AuthScheme {
    let mut r = |text: &str| resolver.resolve(text, UsageLocation::Auth);
    match scheme {
        AuthScheme::Basic { username, password } => AuthScheme::Basic {
            username: r(username),
            password: r(password),
        },
        AuthScheme::Bearer { token } => AuthScheme::Bearer { token: r(token) },
        AuthScheme::ApiKey {
            location,
            name,
            value,
        } => AuthScheme::ApiKey {
            location: *location,
            name: r(name),
            value: r(value),
        },
        AuthScheme::OAuth2(config) => {
            let mut c = (**config).clone();
            c.token_url = r(&c.token_url);
            c.auth_url = r(&c.auth_url);
            c.redirect_uri = r(&c.redirect_uri);
            c.client_id = r(&c.client_id);
            c.client_secret = r(&c.client_secret);
            c.scope = r(&c.scope);
            c.username = r(&c.username);
            c.password = r(&c.password);
            AuthScheme::OAuth2(Box::new(c))
        }
    }
}

/// Enabled headers with resolved names and values. Disabled rows are kept unchanged.
fn resolve_headers(resolver: &mut Resolver<'_>, headers: &[KeyValue]) -> Vec<KeyValue> {
    headers
        .iter()
        .map(|header| {
            if !header.enabled {
                return header.clone();
            }
            let location = UsageLocation::Header {
                name: header.name.clone(),
            };
            KeyValue {
                name: resolver.resolve(&header.name, location.clone()),
                value: resolver.resolve(&header.value, location),
                enabled: true,
            }
        })
        .collect()
}
