//! Environment files in `environments/*.yaml` (PLAN.md §4.4). Global and collection
//! environments share this format.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::common::{FORMAT_VERSION, default_true, is_false, is_true};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Environment {
    pub version: u32,
    pub name: String,
    /// Optional highlight color (e.g. `red` for production), shown on the environment picker so
    /// it is obvious which environment requests go to. One of the UI's named colors; unknown
    /// names are ignored by the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub variables: Vec<EnvVariable>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.into(),
            color: None,
            variables: Vec::new(),
        }
    }

    /// Returns a copy that is safe to write to disk: secret variables lose their value.
    ///
    /// Secret values live in the OS keychain (M2) and must never end up in YAML (PLAN.md §6).
    /// Storage calls this before every write, so a value set by mistake is never persisted.
    pub fn without_secret_values(&self) -> Self {
        let mut copy = self.clone();
        for variable in &mut copy.variables {
            if variable.secret {
                variable.value = None;
            }
        }
        copy
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EnvVariable {
    pub name: String,
    /// `None` for secrets, whose value is stored outside the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
    /// Where a secret value comes from when it isn't in the keychain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SecretSource>,
    /// Disabled variables stay in the file but are ignored, like disabled rows elsewhere.
    /// Only `enabled: false` is written, so existing files don't change.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SecretSource {
    #[serde(rename_all = "camelCase")]
    AwsSecretsManager {
        secret_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        json_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
    },
}
