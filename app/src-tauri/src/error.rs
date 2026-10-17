//! The error type every command returns to the UI: `{ code, message, hint? }` (CLAUDE.md).

use frida_core::CoreError;
use frida_core::auth::OAuth2Error;
use frida_core::http::SendError;
use frida_secrets::SecretError;
use frida_secrets::aws::AwsError;
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
    /// For AWS problems: which environment, and a command that fixes it (for a copy button).
    /// Boxed because it is rare: every command returns `Result<_, ApiError>`, and keeping the
    /// error small keeps those results small (clippy's `result_large_err`).
    pub aws: Option<Box<AwsErrorInfo>>,
}

/// Extra details of AWS errors, so the UI can ask for a profile or offer the login command.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AwsErrorInfo {
    pub environment: String,
    /// The global environment (otherwise the collection's).
    pub global: Option<bool>,
    /// e.g. `aws sso login --profile dev`.
    pub fix_command: Option<String>,
    /// The account alias that needs a profile (`aws_account_required`, PLAN.md M17).
    pub account: Option<String>,
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
    Git,
    /// An environment with AWS secrets has no AWS profile yet: the UI asks for one.
    AwsProfileRequired,
    /// A secret's account alias has no AWS profile on this machine yet: the UI asks for one.
    AwsAccountRequired,
    Aws,
    /// Checking for, downloading or installing an update failed (PLAN.md M14).
    Update,
    /// Something is still running that the action would interrupt.
    Busy,
    /// The proxy couldn't be reached or refused the login (PLAN.md M16).
    Proxy,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            aws: None,
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
            CoreError::AwsProfileRequired {
                environment,
                global,
            } => ApiError {
                aws: Some(Box::new(AwsErrorInfo {
                    environment,
                    global: Some(global),
                    fix_command: None,
                    account: None,
                })),
                ..ApiError::new(ErrorCode::AwsProfileRequired, message)
            },
            CoreError::AwsAccountRequired {
                account,
                environment,
            } => ApiError {
                aws: Some(Box::new(AwsErrorInfo {
                    environment,
                    global: None,
                    fix_command: None,
                    account: Some(account),
                })),
                ..ApiError::new(ErrorCode::AwsAccountRequired, message)
            },
            CoreError::Aws {
                environment,
                source,
                ..
            } => {
                let fix_command = source.fix_command();
                let hint = match &source {
                    AwsError::SsoLogin { .. } => {
                        "Log in with the command below in a terminal, then send again."
                    }
                    AwsError::NoCredentials { .. } => {
                        "Check the profile in ~/.aws/config, or choose another one in the environment editor."
                    }
                    AwsError::NoClientSecret { .. } => {
                        "This Cognito app client was created without a secret. The client credentials grant needs a client with one."
                    }
                    AwsError::NotFound { .. } | AwsError::MissingKey { .. } => {
                        "Check the secret id, JSON key and region of the variable in the environment editor."
                    }
                    AwsError::AccessDenied { .. } => {
                        "Use a profile that may read this secret (environment editor → AWS profile)."
                    }
                    _ => "See the AWS message above.",
                };
                ApiError {
                    aws: Some(Box::new(AwsErrorInfo {
                        environment,
                        global: None,
                        fix_command,
                        account: None,
                    })),
                    ..ApiError::new(ErrorCode::Aws, message).with_hint(hint)
                }
            }
            CoreError::Script(_) => ApiError::new(ErrorCode::Script, message).with_hint(
                "Fix the script at the line shown; its console.log output is in the console.",
            ),
            CoreError::Skipped { .. } => ApiError::new(ErrorCode::Script, message)
                .with_hint("Only skipped in a collection run? Check the script's conditions."),
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
            SendError::Proxy { .. } => ApiError::new(ErrorCode::Proxy, message).with_hint(
                "Check the proxy address in Settings → Network, or turn off the proxy for this collection in its settings.",
            ),
            SendError::ProxyAuth { .. } => ApiError::new(ErrorCode::Proxy, message)
                .with_hint("Check the proxy's username and password in Settings → Network."),
            SendError::Other(_) => ApiError::new(ErrorCode::Http, message),
        }
    }
}

impl From<frida_git::GitError> for ApiError {
    fn from(error: frida_git::GitError) -> Self {
        use frida_git::GitError;
        let message = error.to_string();
        match &error {
            GitError::NotInstalled => ApiError::new(ErrorCode::Git, message)
                .with_hint("Install git (https://git-scm.com) and restart Frida."),
            GitError::NotARepo(_) => ApiError::new(ErrorCode::Git, message)
                .with_hint("Initialize a repository from the Git panel."),
            GitError::Command { message: text, .. }
                if text.contains("fast-forward") || text.contains("diverg") =>
            {
                ApiError::new(ErrorCode::Git, message).with_hint(
                    "Your branch and the remote have both changed. Merge or rebase in a terminal or your git tool, then refresh.",
                )
            }
            GitError::Command { message: text, .. }
                if text.contains("Please tell me who you are") =>
            {
                ApiError::new(ErrorCode::Git, message).with_hint(
                    "Set your name and email once with git config --global user.name / user.email.",
                )
            }
            GitError::Command { message: text, .. }
                if text.contains("Authentication failed")
                    || text.contains("Permission denied")
                    || text.contains("could not read Username") =>
            {
                ApiError::new(ErrorCode::Git, message).with_hint(
                    "Git couldn't log in to the remote. Check your SSH key or credential helper in a terminal.",
                )
            }
            GitError::Timeout(_) => ApiError::new(ErrorCode::Git, message)
                .with_hint("Check your network connection and the remote URL."),
            _ => ApiError::new(ErrorCode::Git, message),
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
