//! Turning a saved [`Request`] plus its folder and collection into a concrete HTTP request
//! (PLAN.md §7, steps 1 and 6). Variables, auth and scripts are added in later milestones.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{CoreError, Result};
use crate::model::{BodyMode, Collection, Folder, KeyValue, MultipartKind, Request};

/// Used when neither the request, its folders nor the collection set a timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A request with everything inherited and merged, ready for [`super::send`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    /// In sending order. Names are unique (case-insensitively) after merging.
    pub headers: Vec<(String, String)>,
    pub body: PreparedBody,
    pub timeout: Duration,
    pub tls_verify: bool,
    /// Absolute paths of extra CA certificates (PEM).
    pub ca_cert_paths: Vec<PathBuf>,
    /// The proxy settings it is sent with (PLAN.md M16). [`prepare`] leaves the default
    /// (system settings); the send pipeline sets the session's.
    pub proxy: crate::network::ProxySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedBody {
    None,
    Bytes(Vec<u8>),
    Form(Vec<(String, String)>),
    Multipart(Vec<PreparedPart>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedPart {
    Text { name: String, value: String },
    File { name: String, path: PathBuf },
}

/// Everything a request inherits from: its collection and its folders, outermost first.
///
/// This struct *borrows* its data (`&'a ...`) instead of owning copies. The `'a` is a
/// *lifetime*: it tells the compiler "this struct must not outlive the values it points to".
/// Think of it as a view object that holds references, where the compiler checks at compile
/// time that the referenced objects are still alive, so there are no dangling pointers.
#[derive(Debug, Clone, Copy)]
pub struct RequestContext<'a> {
    /// The collection folder, used to resolve relative file paths (CA certs, body files).
    pub collection_dir: &'a Path,
    pub collection: &'a Collection,
    pub folders: &'a [Folder],
}

/// Builds the concrete request. Reads the body file for `binary` mode.
pub fn prepare(ctx: RequestContext<'_>, request: &Request) -> Result<PreparedRequest> {
    let method = request.method.trim().to_uppercase();
    if method.is_empty()
        || !method
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(CoreError::InvalidInput(format!(
            "{:?} is not a valid HTTP method",
            request.method
        )));
    }

    let url = apply_path_params(&normalize_url(&request.url), request);
    let body = prepare_body(ctx.collection_dir, request)?;
    let headers = merge_headers(ctx, request);

    Ok(PreparedRequest {
        method,
        url,
        headers,
        body,
        timeout: effective_timeout(ctx, request),
        tls_verify: ctx.collection.settings.tls.verify,
        ca_cert_paths: ctx
            .collection
            .settings
            .tls
            .ca_cert_paths
            .iter()
            .map(|p| ctx.collection_dir.join(p))
            .collect(),
        proxy: crate::network::ProxySettings::default(),
    })
}

/// Adds `http://` when the user typed `localhost:3000/x` without a scheme, like most clients do.
pub(crate) fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.contains("://") {
        url.to_string()
    } else {
        format!("http://{url}")
    }
}

/// Replaces `:name` path segments with the request's path param values.
/// Only the path is touched, never the host (`localhost:8080`) or the query string.
pub(crate) fn apply_path_params(url: &str, request: &Request) -> String {
    if request.params.path.is_empty() {
        return url.to_string();
    }
    // Split off `scheme://host[:port]` so a port is never mistaken for a `:param`.
    let after_scheme = url.find("://").map(|i| i + 3).unwrap_or(0);
    let path_start = url[after_scheme..]
        .find('/')
        .map(|i| after_scheme + i)
        .unwrap_or(url.len());
    let (origin, rest) = url.split_at(path_start);
    let (path, query) = match rest.find(['?', '#']) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };

    let replaced: Vec<String> = path
        .split('/')
        .map(|segment| {
            let value = segment.strip_prefix(':').and_then(|name| {
                request
                    .params
                    .path
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.value.clone())
            });
            value.unwrap_or_else(|| segment.to_string())
        })
        .collect();
    format!("{origin}{}{query}", replaced.join("/"))
}

/// Merges headers collection → folders (outermost first) → request. A later definition of the
/// same name (case-insensitive) replaces the earlier one. Disabled rows are skipped.
pub(crate) fn merge_headers(ctx: RequestContext<'_>, request: &Request) -> Vec<(String, String)> {
    let layers = std::iter::once(&ctx.collection.headers)
        .chain(ctx.folders.iter().map(|f| &f.headers))
        .chain(std::iter::once(&request.headers));

    let mut merged: Vec<(String, String)> = Vec::new();
    for row in layers
        .flatten()
        .filter(|row| row.enabled && !row.name.trim().is_empty())
    {
        let name = row.name.trim();
        match merged
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            Some(existing) => *existing = (name.to_string(), row.value.clone()),
            None => merged.push((name.to_string(), row.value.clone())),
        }
    }

    // Add a sensible Content-Type for the body mode, unless the user set one.
    let has_content_type = merged
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("content-type"));
    let default_type = match request.body.mode {
        BodyMode::Json => Some("application/json"),
        BodyMode::Xml => Some("application/xml"),
        BodyMode::Text => Some("text/plain"),
        BodyMode::Binary => Some("application/octet-stream"),
        // reqwest sets these itself, including the multipart boundary.
        BodyMode::FormUrlencoded | BodyMode::Multipart | BodyMode::None => None,
    };
    if let (false, Some(value)) = (has_content_type, default_type) {
        merged.push(("Content-Type".to_string(), value.to_string()));
    }
    merged
}

fn prepare_body(collection_dir: &Path, request: &Request) -> Result<PreparedBody> {
    let body = &request.body;
    let enabled = |rows: &[KeyValue]| -> Vec<(String, String)> {
        rows.iter()
            .filter(|r| r.enabled && !r.name.is_empty())
            .map(|r| (r.name.clone(), r.value.clone()))
            .collect()
    };

    Ok(match body.mode {
        BodyMode::None => PreparedBody::None,
        BodyMode::Json => PreparedBody::Bytes(body.json.clone().into_bytes()),
        BodyMode::Xml => PreparedBody::Bytes(body.xml.clone().into_bytes()),
        BodyMode::Text => PreparedBody::Bytes(body.text.clone().into_bytes()),
        BodyMode::FormUrlencoded => PreparedBody::Form(enabled(&body.form_url_encoded)),
        BodyMode::Multipart => PreparedBody::Multipart(
            body.multipart
                .iter()
                .filter(|f| f.enabled && !f.name.is_empty())
                .map(|f| match f.kind {
                    MultipartKind::Text => PreparedPart::Text {
                        name: f.name.clone(),
                        value: f.value.clone(),
                    },
                    MultipartKind::File => PreparedPart::File {
                        name: f.name.clone(),
                        path: collection_dir.join(&f.value),
                    },
                })
                .collect(),
        ),
        BodyMode::Binary => {
            if body.binary_path.trim().is_empty() {
                return Err(CoreError::InvalidInput(
                    "choose a file to send as the binary body".to_string(),
                ));
            }
            let path = collection_dir.join(&body.binary_path);
            let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            PreparedBody::Bytes(bytes)
        }
    })
}

/// Timeout lookup: request → nearest folder → ... → collection → [`DEFAULT_TIMEOUT`].
fn effective_timeout(ctx: RequestContext<'_>, request: &Request) -> Duration {
    let from_folders = ctx.folders.iter().rev().find_map(|f| f.settings.timeout_ms);
    request
        .settings
        .timeout_ms
        .or(from_folders)
        .or(ctx.collection.settings.timeout_ms)
        .map(|ms| Duration::from_millis(u64::from(ms)))
        .unwrap_or(DEFAULT_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyValue, MultipartField, PathParam};

    fn ctx<'a>(collection: &'a Collection, folders: &'a [Folder]) -> RequestContext<'a> {
        RequestContext {
            collection_dir: Path::new("/col"),
            collection,
            folders,
        }
    }

    fn header<'a>(prepared: &'a PreparedRequest, name: &str) -> Option<&'a str> {
        prepared
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn headers_merge_with_later_layers_winning() {
        let mut collection = Collection::new("C");
        collection.headers = vec![
            KeyValue::new("Accept", "text/plain"),
            KeyValue::new("X-Col", "1"),
        ];
        let mut folder = Folder::new("F");
        folder.headers = vec![KeyValue::new("accept", "application/xml")];
        let mut request = Request::new("R");
        request.headers = vec![
            KeyValue::new("ACCEPT", "application/json"),
            KeyValue {
                enabled: false,
                ..KeyValue::new("X-Off", "nope")
            },
        ];

        let folders = [folder];
        let prepared = prepare(ctx(&collection, &folders), &request).unwrap();
        assert_eq!(header(&prepared, "accept"), Some("application/json"));
        assert_eq!(header(&prepared, "x-col"), Some("1"));
        assert_eq!(header(&prepared, "x-off"), None);
        assert_eq!(prepared.headers.len(), 2);
    }

    #[test]
    fn json_body_gets_content_type_unless_user_sets_one() {
        let collection = Collection::new("C");
        let mut request = Request::new("R");
        request.body.mode = BodyMode::Json;
        request.body.json = "{}".to_string();

        let prepared = prepare(ctx(&collection, &[]), &request).unwrap();
        assert_eq!(header(&prepared, "content-type"), Some("application/json"));
        assert_eq!(prepared.body, PreparedBody::Bytes(b"{}".to_vec()));

        request.headers = vec![KeyValue::new("Content-Type", "application/vnd.api+json")];
        let prepared = prepare(ctx(&collection, &[]), &request).unwrap();
        assert_eq!(
            header(&prepared, "content-type"),
            Some("application/vnd.api+json")
        );
    }

    #[test]
    fn path_params_replace_only_path_segments() {
        let collection = Collection::new("C");
        let mut request = Request::new("R");
        request.url = "localhost:8080/orders/:orderId/items/:itemId?expand=:orderId".to_string();
        request.params.path = vec![
            PathParam {
                name: "orderId".to_string(),
                value: "42".to_string(),
                description: String::new(),
            },
            PathParam {
                name: "itemId".to_string(),
                value: "7".to_string(),
                description: String::new(),
            },
        ];

        let prepared = prepare(ctx(&collection, &[]), &request).unwrap();
        assert_eq!(
            prepared.url,
            "http://localhost:8080/orders/42/items/7?expand=:orderId"
        );
    }

    #[test]
    fn timeout_is_inherited_from_nearest_level() {
        let mut collection = Collection::new("C");
        collection.settings.timeout_ms = Some(5_000);
        let mut outer = Folder::new("Outer");
        outer.settings.timeout_ms = Some(4_000);
        let inner = Folder::new("Inner");
        let mut request = Request::new("R");

        let folders = [outer, inner];
        let t = |r: &Request, f: &[Folder], c: &Collection| prepare(ctx(c, f), r).unwrap().timeout;
        assert_eq!(
            t(&request, &folders, &collection),
            Duration::from_millis(4_000)
        );
        assert_eq!(t(&request, &[], &collection), Duration::from_millis(5_000));
        request.settings.timeout_ms = Some(100);
        assert_eq!(
            t(&request, &folders, &collection),
            Duration::from_millis(100)
        );

        collection.settings.timeout_ms = None;
        assert_eq!(t(&Request::new("R"), &[], &collection), DEFAULT_TIMEOUT);
    }

    #[test]
    fn form_and_multipart_skip_disabled_rows_and_resolve_file_paths() {
        let collection = Collection::new("C");
        let mut request = Request::new("R");
        request.body.mode = BodyMode::Multipart;
        request.body.multipart = vec![
            MultipartField {
                name: "a".into(),
                kind: MultipartKind::Text,
                value: "1".into(),
                enabled: true,
            },
            MultipartField {
                name: "f".into(),
                kind: MultipartKind::File,
                value: "files/x.png".into(),
                enabled: true,
            },
            MultipartField {
                name: "off".into(),
                kind: MultipartKind::Text,
                value: "".into(),
                enabled: false,
            },
        ];
        let prepared = prepare(ctx(&collection, &[]), &request).unwrap();
        assert_eq!(
            prepared.body,
            PreparedBody::Multipart(vec![
                PreparedPart::Text {
                    name: "a".into(),
                    value: "1".into()
                },
                PreparedPart::File {
                    name: "f".into(),
                    path: PathBuf::from("/col/files/x.png")
                },
            ])
        );
    }

    #[test]
    fn custom_methods_are_allowed_but_garbage_is_not() {
        let collection = Collection::new("C");
        let mut request = Request::new("R");
        request.method = "propfind".to_string();
        assert_eq!(
            prepare(ctx(&collection, &[]), &request).unwrap().method,
            "PROPFIND"
        );
        request.method = "GET /x".to_string();
        assert!(prepare(ctx(&collection, &[]), &request).is_err());
    }
}
