//! `collection.yaml` and `folder.yaml` (PLAN.md §4.2).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::common::{Auth, CollectionSettings, FORMAT_VERSION, KeyValue, Scripts, Settings};

/// The root of a collection: the folder that contains `collection.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub version: u32,
    /// Stable UUID. Keys local state (tabs, AWS profile mapping, secrets), so it must never change.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vars: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default = "no_auth")]
    pub auth: Auth,
    #[serde(default)]
    pub scripts: Scripts,
    #[serde(default)]
    pub tests: String,
    #[serde(default)]
    pub settings: CollectionSettings,
    #[serde(default)]
    pub docs: String,
}

fn no_auth() -> Auth {
    Auth::None
}

impl Collection {
    /// A new collection with a fresh random id.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            vars: Vec::new(),
            headers: Vec::new(),
            auth: Auth::None,
            scripts: Scripts::default(),
            tests: String::new(),
            settings: CollectionSettings::default(),
            docs: String::new(),
        }
    }
}

/// Folder metadata. A directory inside a collection is a folder when it contains `folder.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub seq: u32,
    #[serde(default)]
    pub vars: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default = "inherit")]
    pub auth: Auth,
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

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.into(),
            seq: 0,
            vars: Vec::new(),
            headers: Vec::new(),
            auth: Auth::Inherit,
            scripts: Scripts::default(),
            tests: String::new(),
            settings: Settings::default(),
            docs: String::new(),
        }
    }
}
