//! `frida-workspace.yaml` (PLAN.md §4.1).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::common::FORMAT_VERSION;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Workspace {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub collections: Vec<CollectionRef>,
}

impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.into(),
            collections: Vec::new(),
        }
    }
}

/// Points at a collection folder: relative to the workspace folder, or absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionRef {
    pub path: String,
}
