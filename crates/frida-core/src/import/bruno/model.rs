//! What both Bruno formats (`.bru` and OpenCollection YAML) are read into, before it is mapped
//! to Frida's model. Keeping one in-between shape means the mapping is written once.

/// A `name: value` row (header, param, variable, form field, assertion).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Kv {
    pub name: String,
    pub value: String,
    pub enabled: bool,
    /// Bruno's `@description('...')` (or `description:` in its YAML format).
    pub description: String,
}

/// One multipart field. File fields hold one or more paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
    pub name: String,
    pub value: String,
    pub files: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Body {
    #[default]
    None,
    Json(String),
    Text(String),
    Xml(String),
    Sparql(String),
    GraphQl {
        query: String,
        variables: String,
    },
    Form(Vec<Kv>),
    Multipart(Vec<Part>),
    /// `body:file`: the selected file, if any.
    File(Option<String>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OAuth2 {
    /// `client_credentials`, `password`, `authorization_code` or `implicit`.
    pub grant: String,
    pub token_url: String,
    pub auth_url: String,
    pub callback_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub pkce: bool,
    /// Client credentials in the token request body (`true`) or a Basic header.
    pub credentials_in_body: bool,
    pub username: String,
    pub password: String,
    /// `None` means a header (with `header_prefix`).
    pub token_query_key: Option<String>,
    pub header_prefix: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Auth {
    #[default]
    None,
    Inherit,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        in_query: bool,
    },
    /// Boxed: much bigger than the other variants.
    OAuth2(Box<OAuth2>),
    /// A mode Frida doesn't support (digest, NTLM, AWS SigV4, ...).
    Unsupported(String),
}

/// Everything collections, folders and requests have in common.
#[derive(Debug, Clone, Default)]
pub(crate) struct Common {
    pub headers: Vec<Kv>,
    pub auth: Auth,
    /// `vars:pre-request`: plain variables.
    pub vars: Vec<Kv>,
    /// `vars:post-response`: variables set from a JS expression after the response.
    pub post_vars: Vec<Kv>,
    pub script_pre: String,
    pub script_post: String,
    pub tests: String,
    pub asserts: Vec<Kv>,
    pub docs: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Request {
    pub name: String,
    pub seq: u32,
    /// `http`, `graphql`, or something Frida skips (`grpc`, `ws`).
    pub kind: String,
    pub method: String,
    pub url: String,
    pub query: Vec<Kv>,
    pub path_params: Vec<Kv>,
    pub body: Body,
    pub timeout_ms: Option<u32>,
    pub common: Common,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Folder {
    pub name: String,
    pub seq: Option<u32>,
    pub common: Common,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Folder(Folder),
    Request(Request),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EnvVar {
    pub name: String,
    pub value: String,
    pub secret: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Env {
    pub name: String,
    pub vars: Vec<EnvVar>,
}

/// A whole Bruno collection, read from disk.
#[derive(Debug, Clone, Default)]
pub(crate) struct Collection {
    pub name: String,
    pub common: Common,
    pub children: Vec<Node>,
    pub environments: Vec<Env>,
    /// Problems found while reading.
    pub warnings: Vec<String>,
}
