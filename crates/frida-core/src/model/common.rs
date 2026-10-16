//! Building blocks shared by collections, folders and requests (PLAN.md §4).

use serde::{Deserialize, Serialize};
use specta::Type;

/// The only on-disk format version this build understands. Every file carries `version: 1`.
pub const FORMAT_VERSION: u32 = 1;

/// serde calls this for fields marked `#[serde(default = "default_true")]`. serde's `default`
/// attribute needs a *function path*, not a literal value, which is why this tiny function exists.
pub(crate) fn default_true() -> bool {
    true
}

/// serde's `skip_serializing_if` also needs a function. `&bool` because serde passes a reference.
/// For `skip_serializing_if`: fields that default to `true` are only written when `false`.
pub(crate) fn is_true(value: &bool) -> bool {
    *value
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// A `name: value` row that can be switched off, used for headers, query params, vars and form fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct KeyValue {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Disabled rows stay in the file but are ignored when sending (PLAN.md §5.2).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            enabled: true,
        }
    }
}

/// Pre- and post-request scripts. Empty strings mean "no script".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct Scripts {
    pub pre: String,
    pub post: String,
}

/// Request-level and folder-level settings. `None` means "inherit from the parent".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub timeout_ms: Option<u32>,
}

/// TLS options, set on the collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct TlsSettings {
    /// `false` accepts invalid or self-signed certificates.
    pub verify: bool,
    /// Extra PEM files trusted *in addition to* the system roots. Relative to the collection
    /// folder, or absolute.
    pub ca_cert_paths: Vec<String>,
}

impl Default for TlsSettings {
    fn default() -> Self {
        Self {
            verify: true,
            ca_cert_paths: Vec::new(),
        }
    }
}

/// Collection-level settings: the root of the settings inheritance chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct CollectionSettings {
    pub timeout_ms: Option<u32>,
    pub tls: TlsSettings,
}

impl Default for CollectionSettings {
    fn default() -> Self {
        Self {
            timeout_ms: Some(30_000),
            tls: TlsSettings::default(),
        }
    }
}

/// How a request, folder or collection authenticates (PLAN.md §4.5).
///
/// `#[serde(tag = "mode")]` stores the variant name in a `mode` field, so YAML looks like
/// `auth: { mode: custom, schemes: [...] }`. This is an *enum with data*: unlike a Java enum,
/// each Rust variant can carry its own fields (closer to a sealed interface with records).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Auth {
    /// Use the nearest parent's auth. On a collection this behaves like `None`.
    Inherit,
    None,
    /// One or more schemes, applied in order (e.g. OAuth2 + API key).
    Custom {
        schemes: Vec<AuthScheme>,
    },
}

/// A single auth scheme. Stored in M1 so files round-trip; applied when sending from M3 on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthScheme {
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    Bearer {
        #[serde(default)]
        token: String,
    },
    #[serde(rename_all = "camelCase")]
    ApiKey {
        #[serde(rename = "in", default)]
        location: ApiKeyLocation,
        name: String,
        #[serde(default)]
        value: String,
    },
    /// `Box` puts the (large) config on the heap. An enum value is as big as its largest
    /// variant, so without the box every `Basic` or `Bearer` would reserve room for all the
    /// OAuth2 fields too. serde and specta treat `Box<T>` exactly like `T`.
    OAuth2(Box<OAuth2Config>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct OAuth2Config {
    pub grant: OAuth2Grant,
    pub token_url: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub use_pkce: bool,
    pub username: String,
    pub password: String,
    pub client_auth: OAuth2ClientAuth,
    pub token_placement: TokenPlacement,
    pub preset: Option<String>,
}

impl Default for OAuth2Config {
    fn default() -> Self {
        Self {
            grant: OAuth2Grant::ClientCredentials,
            token_url: String::new(),
            auth_url: String::new(),
            redirect_uri: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scope: String::new(),
            use_pkce: true,
            username: String::new(),
            password: String::new(),
            client_auth: OAuth2ClientAuth::BasicHeader,
            token_placement: TokenPlacement::default(),
            preset: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    ClientCredentials,
    AuthorizationCode,
    Password,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2ClientAuth {
    BasicHeader,
    Body,
}

/// Where the OAuth2 access token is put on the outgoing request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct TokenPlacement {
    #[serde(rename = "in")]
    pub location: ApiKeyLocation,
    pub name: String,
    pub prefix: String,
}

impl Default for TokenPlacement {
    fn default() -> Self {
        Self {
            location: ApiKeyLocation::Header,
            name: "Authorization".to_string(),
            prefix: "Bearer".to_string(),
        }
    }
}
