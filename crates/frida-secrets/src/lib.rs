//! `frida-secrets`: where secret variable values live (PLAN.md §6).
//!
//! Secret values never go into YAML, logs or history. They are stored in a [`SecretStore`]:
//! - [`KeychainStore`]: the OS credential store (macOS Keychain, Windows Credential Manager,
//!   Linux Secret Service). The default.
//! - [`EncryptedFileStore`]: an encrypted file in the app data dir, protected by a master
//!   password. Used when no OS keychain is available (e.g. headless Linux).
//! - [`MemoryStore`]: for tests.
//! - On macOS (PLAN.md M13): [`ProtectedStore`], the data protection keychain of a signed app
//!   (no prompts), or in unsigned dev builds a [`VaultStore`] (all secrets in one legacy item);
//!   both behind a [`MigratingStore`] that takes over the old per-secret items.
//! - [`CachedStore`]: an in-memory cache in front of another store; the app wraps the keychain
//!   in it so each secret is read (and macOS asks for access) at most once per run.
//!
//! AWS Secrets Manager (M8) is a separate, async provider: see [`aws`].
//!
//! All stores do blocking I/O. Callers in async code must use `tokio::task::spawn_blocking`.

pub mod aws;
mod cached;
mod encrypted_file;
mod keychain;
mod memory;
mod migrate;
#[cfg(target_os = "macos")]
mod protected;
mod vault;

pub use cached::CachedStore;
pub use encrypted_file::EncryptedFileStore;
pub use keychain::{KeychainStore, keychain_available};
pub use memory::MemoryStore;
pub use migrate::MigratingStore;
#[cfg(target_os = "macos")]
pub use protected::ProtectedStore;
pub use vault::VaultStore;

/// Identifies one secret: the variable `var` of environment `env` in scope `scope_id`.
///
/// `scope_id` is the collection id for collection environments, or a hash of the workspace
/// folder for global environments (see `frida_core::secrets::workspace_scope_id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecretKey {
    pub scope_id: String,
    pub env: String,
    pub var: String,
}

impl SecretKey {
    pub fn new(
        scope_id: impl Into<String>,
        env: impl Into<String>,
        var: impl Into<String>,
    ) -> Self {
        Self {
            scope_id: scope_id.into(),
            env: env.into(),
            var: var.into(),
        }
    }

    /// `frida/{scopeId}/{envName}/{varName}` (PLAN.md §4.4).
    pub fn qualified(&self) -> String {
        format!("frida/{}/{}/{}", self.scope_id, self.env, self.var)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SecretError {
    #[error("the OS keychain is not available: {0}")]
    KeychainUnavailable(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("the secret file is locked; enter the master password to unlock it")]
    Locked,

    #[error("wrong master password")]
    WrongPassword,

    #[error("could not access the secret file {path}: {message}")]
    File { path: String, message: String },
}

/// A place to keep secret values.
///
/// This is a *trait*, Rust's version of an interface. Callers hold it as `Box<dyn SecretStore>`
/// or `Arc<dyn SecretStore>` (a "trait object"): the concrete type is chosen at runtime, just
/// like holding a Java interface reference. `Send + Sync` means it may be used from several
/// threads, which Tauri commands do.
pub trait SecretStore: Send + Sync {
    /// The value, or `None` when no value has been set yet.
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
}
