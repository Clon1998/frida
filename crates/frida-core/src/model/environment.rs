//! Environment files in `environments/*.yaml` (PLAN.md §4.4). Global and collection
//! environments share this format.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::common::{FORMAT_VERSION, default_true, is_false, is_true, is_zero};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Environment {
    pub version: u32,
    pub name: String,
    /// Position in the environment list, starting at 1 (set by drag-and-drop). `0` means "not
    /// ordered yet": those come after the ordered ones, by name. Only written when set, so
    /// existing files don't change.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seq: u32,
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
            seq: 0,
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

/// Where a secret's value comes from, besides the keychain. All sources are in AWS.
///
/// `account` (PLAN.md M17) names an AWS account alias such as `shared-services` instead of
/// using the environment's AWS profile. The alias, not a profile name, is in the file because
/// profile names differ per machine; each user maps the alias to a local profile once.
/// Optional fields are only written when set, so older files stay unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SecretSource {
    /// A Secrets Manager secret, or one key of a JSON secret.
    #[serde(rename_all = "camelCase")]
    AwsSecretsManager {
        secret_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        json_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// The client secret of a Cognito app client (for OAuth2 client credentials).
    #[serde(rename_all = "camelCase")]
    CognitoClientSecret {
        /// e.g. `eu-central-1_AbCdEf`; its prefix is the default region.
        user_pool_id: String,
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// The value of an API Gateway API key (for `x-api-key`).
    #[serde(rename_all = "camelCase")]
    ApiGatewayKey {
        api_key_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
}

impl SecretSource {
    /// The account alias, when the secret doesn't use the environment's profile.
    pub fn account(&self) -> Option<&str> {
        let account = match self {
            Self::AwsSecretsManager { account, .. }
            | Self::CognitoClientSecret { account, .. }
            | Self::ApiGatewayKey { account, .. } => account,
        };
        account.as_deref().map(str::trim).filter(|a| !a.is_empty())
    }

    pub fn region(&self) -> Option<&str> {
        let region = match self {
            Self::AwsSecretsManager { region, .. }
            | Self::CognitoClientSecret { region, .. }
            | Self::ApiGatewayKey { region, .. } => region,
        };
        region.as_deref().map(str::trim).filter(|r| !r.is_empty())
    }
}
