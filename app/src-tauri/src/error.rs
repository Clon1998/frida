//! The error type every command returns to the UI: `{ code, message, hint? }` (CLAUDE.md).

use frida_core::CoreError;
use frida_core::auth::OAuth2Error;
use frida_core::http::SendError;
use frida_secrets::SecretError;
use serde::Serialize;
use specta::Type;

/// A structured, user-actionable error. tauri-specta turns commands returning
/// `Result<T, ApiError>` into TS functions returning `{ status: "ok", data } | { status: "error", error }`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    /// What the user can do about it, when there is something useful to say.
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Io,
    InvalidFile,
    UnsupportedVersion,
    AlreadyExists,
    NotFound,
    InvalidInput,
    InvalidUrl,
    Timeout,
    Cancelled,
    Connect,
    Tls,
    Http,
    NoWorkspace,
    SecretLocked,
    WrongPassword,
    Secret,
    Auth,
    Script,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Lets commands use `?` on `frida_core` results: the `CoreError` is converted automatically.
impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        let message = error.to_string();
        match error {
            CoreError::Io { .. } => ApiError::new(ErrorCode::Io, message),
            CoreError::InvalidFile { .. } => ApiError::new(ErrorCode::InvalidFile, message)
                .with_hint("Fix the file in an editor (it may contain a git merge conflict)."),
            CoreError::UnsupportedVersion { .. } => {
                ApiError::new(ErrorCode::UnsupportedVersion, message)
            }
            CoreError::AlreadyExists { .. } => ApiError::new(ErrorCode::AlreadyExists, message),
            CoreError::NotFound { .. } => ApiError::new(ErrorCode::NotFound, message)
                .with_hint("It may have been moved or deleted outside Frida."),
            CoreError::InvalidInput(_) => ApiError::new(ErrorCode::InvalidInput, message),
            CoreError::Send(send) => send.into(),
            CoreError::Script(_) => ApiError::new(ErrorCode::Script, message).with_hint(
                "Fix the script at the line shown; its console.log output is in the console.",
            ),
            CoreError::Secret(secret) => secret.into(),
            CoreError::Auth(auth) => match auth {
                OAuth2Error::Cancelled => ApiError::new(ErrorCode::Cancelled, message),
                OAuth2Error::Rejected { .. } => ApiError::new(ErrorCode::Auth, message).with_hint(
                    "Check the client id and secret, the scope and the token URL in the Auth tab.",
                ),
                OAuth2Error::Browser(_) => ApiError::new(ErrorCode::Auth, message)
                    .with_hint("Check the authorization URL and the redirect URI."),
                _ => ApiError::new(ErrorCode::Auth, message),
            },
        }
    }
}

impl From<SendError> for ApiError {
    fn from(error: SendError) -> Self {
        let message = error.to_string();
        match error {
            SendError::InvalidUrl { .. } => ApiError::new(ErrorCode::InvalidUrl, message),
            SendError::Timeout(_) => ApiError::new(ErrorCode::Timeout, message)
                .with_hint("Increase the timeout in the request, folder or collection settings."),
            SendError::Cancelled => ApiError::new(ErrorCode::Cancelled, message),
            SendError::Connect(_) => ApiError::new(ErrorCode::Connect, message)
                .with_hint("Check that the server is running and the host and port are correct."),
            SendError::Tls(_) => ApiError::new(ErrorCode::Tls, message).with_hint(
                "Add the server's CA certificate in the collection settings, or turn off TLS verification there.",
            ),
            SendError::File { .. } => ApiError::new(ErrorCode::Io, message),
            SendError::Other(_) => ApiError::new(ErrorCode::Http, message),
        }
    }
}

impl From<SecretError> for ApiError {
    fn from(error: SecretError) -> Self {
        let message = error.to_string();
        match error {
            SecretError::Locked => ApiError::new(ErrorCode::SecretLocked, message).with_hint(
                "Unlock the secret file with your master password in the environment editor.",
            ),
            SecretError::WrongPassword => ApiError::new(ErrorCode::WrongPassword, message),
            SecretError::KeychainUnavailable(_) => ApiError::new(ErrorCode::Secret, message)
                .with_hint("Restart Frida; it will use an encrypted secret file instead."),
            SecretError::Keychain(_) | SecretError::File { .. } => {
                ApiError::new(ErrorCode::Secret, message)
            }
        }
    }
}

/// Runs blocking work (keychain, file I/O) off the async runtime (CLAUDE.md) and flattens the
/// join error. `spawn_blocking` moves the closure to a thread pool meant for blocking calls,
/// like submitting a task to a separate `ExecutorService` in Java.
pub async fn blocking<T, F>(work: F) -> ApiResult<T>
where
    F: FnOnce() -> ApiResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| ApiError::new(ErrorCode::Io, format!("background task failed: {e}")))?
}

/// Shorthand used by every command.
pub type ApiResult<T> = Result<T, ApiError>;
