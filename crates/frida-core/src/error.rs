//! Errors returned by `frida-core`.

use std::path::PathBuf;

/// Everything that can go wrong in this crate.
///
/// `thiserror` generates the `Display` and `std::error::Error` impls from the `#[error]`
/// attributes, so each variant is just data plus a message template (like a small exception
/// hierarchy in Java, but as one enum that callers can `match` on).
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("could not access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not a valid Frida file: {message}")]
    InvalidFile { path: PathBuf, message: String },

    #[error(
        "{path} uses format version {found}, but this Frida only understands version {supported}. \
         Update Frida to open it."
    )]
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("{path} already exists")]
    AlreadyExists { path: PathBuf },

    #[error("{path} was not found")]
    NotFound { path: PathBuf },

    #[error("{0}")]
    InvalidInput(String),

    /// `#[from]` generates `impl From<SendError> for CoreError`, which lets the `?` operator
    /// convert a send error into a `CoreError` automatically (Java would need a manual wrap).
    #[error(transparent)]
    Send(#[from] crate::http::SendError),

    #[error(transparent)]
    Secret(#[from] frida_secrets::SecretError),

    #[error(transparent)]
    Auth(#[from] crate::auth::OAuth2Error),

    /// An environment reads secrets from AWS but no AWS profile was chosen for it yet.
    #[error("choose an AWS profile for environment {environment:?} to read its AWS secrets")]
    AwsProfileRequired { environment: String, global: bool },

    #[error("environment {environment:?}, variable {variable:?}: {source}")]
    Aws {
        environment: String,
        variable: String,
        #[source]
        source: frida_secrets::aws::AwsError,
    },

    /// A pre-request script failed, so the request was not sent.
    #[error("{0}")]
    Script(frida_script::ScriptError),
}

impl CoreError {
    /// Shorthand for wrapping an I/O error together with the path it happened on.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// `Result` with [`CoreError`] as the error type. A common Rust convention that saves typing.
pub type Result<T> = std::result::Result<T, CoreError>;
