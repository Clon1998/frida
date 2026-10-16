//! A single request file, `<slug>.yaml` (PLAN.md §4.3).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::common::{Auth, FORMAT_VERSION, KeyValue, Scripts, Settings, default_true};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub version: u32,
    pub name: String,
    /// Sort order inside its folder. Lower comes first.
    #[serde(default)]
    pub seq: u32,
    /// Any HTTP method, including custom ones such as `PROPFIND`. Stored upper-case.
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub params: Params,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: Body,
    #[serde(default = "inherit")]
    pub auth: Auth,
    #[serde(default)]
    pub vars: Vec<KeyValue>,
    #[serde(default)]
    pub scripts: Scripts,
    #[serde(default)]
    pub tests: String,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub docs: String,
}

fn inherit() -> Auth {
    Auth::Inherit
}

impl Request {
    /// A new, empty `GET` request with the given display name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.into(),
            seq: 0,
            method: "GET".to_string(),
            url: String::new(),
            params: Params::default(),
            headers: Vec::new(),
            body: Body::default(),
            auth: Auth::Inherit,
            vars: Vec::new(),
            scripts: Scripts::default(),
            tests: String::new(),
            settings: Settings::default(),
            docs: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct Params {
    /// Kept in sync with the query string of `url` by the UI.
    pub query: Vec<KeyValue>,
    /// Values for `:name` segments in the URL.
    pub path: Vec<PathParam>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PathParam {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// What the parameter is for (see [`KeyValue::description`](super::KeyValue)).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// The request body. All modes are kept side by side, so switching the mode in the UI
/// doesn't lose what was typed for another mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Body {
    pub mode: BodyMode,
    pub json: String,
    pub xml: String,
    pub text: String,
    pub form_url_encoded: Vec<KeyValue>,
    pub multipart: Vec<MultipartField>,
    /// Path of the file to send for `mode: binary`. Relative to the collection, or absolute.
    pub binary_path: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum BodyMode {
    #[default]
    None,
    Json,
    Xml,
    Text,
    FormUrlencoded,
    Multipart,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MultipartField {
    pub name: String,
    /// `type` is a reserved word in Rust, so the field is called `kind` and renamed for YAML.
    #[serde(rename = "type", default)]
    pub kind: MultipartKind,
    /// The text value, or the file path when `kind` is `file`.
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum MultipartKind {
    #[default]
    Text,
    File,
}
