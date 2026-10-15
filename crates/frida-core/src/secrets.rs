//! Glue between environments and the secret stores of `frida-secrets` (PLAN.md §6).

use std::path::Path;

use frida_secrets::{SecretKey, SecretStore};
use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::model::Environment;
use crate::storage;

/// The keychain scope of a workspace's global environments: a short hash of the workspace
/// folder, so two workspaces with an environment of the same name don't share secrets.
/// (Collection environments use the collection id instead.)
pub fn workspace_scope_id(workspace_dir: &Path) -> String {
    let canonical = workspace_dir
        .canonicalize()
        .unwrap_or_else(|_| workspace_dir.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("ws-{}", &hex[..16])
}

/// Fills the values of `env`'s secret variables from `store`, in memory only.
///
/// This does blocking I/O (keychain access), so async callers must run it in
/// `tokio::task::spawn_blocking`. Returns one message per secret that couldn't be read; those
/// variables simply stay unset.
pub fn fill_secret_values(
    env: &mut Environment,
    scope_id: &str,
    store: &dyn SecretStore,
) -> Vec<String> {
    let mut errors = Vec::new();
    for var in env.variables.iter_mut().filter(|v| v.secret) {
        let key = SecretKey::new(scope_id, &env.name, &var.name);
        match store.get(&key) {
            Ok(value) => var.value = value,
            Err(err) => {
                var.value = None;
                errors.push(format!("{}: {err}", var.name));
            }
        }
    }
    errors
}

/// Loads the environment `name` from `dir` with its secret values filled in from `store`.
/// Blocking, see [`fill_secret_values`]. Secrets that can't be read stay unset (logged).
pub fn load_environment_with_secrets(
    dir: &Path,
    name: &str,
    scope_id: &str,
    store: &dyn SecretStore,
) -> Result<Option<Environment>> {
    let Some(mut env) = storage::find_environment(dir, name)? else {
        return Ok(None);
    };
    for error in fill_secret_values(&mut env, scope_id, store) {
        // Only the variable name and the store's error, never a value.
        tracing::warn!(environment = %env.name, %error, "could not read secret");
    }
    Ok(Some(env))
}

/// Names of `env`'s secret variables that have a value in `store`. Blocking.
pub fn set_secret_names(env: &Environment, scope_id: &str, store: &dyn SecretStore) -> Vec<String> {
    env.variables
        .iter()
        .filter(|v| v.secret)
        .filter(|v| {
            store
                .get(&SecretKey::new(scope_id, &env.name, &v.name))
                .is_ok_and(|value| value.is_some())
        })
        .map(|v| v.name.clone())
        .collect()
}

/// Moves the secrets of `env` when the environment is renamed, so they aren't orphaned.
pub fn rename_env_secrets(
    env: &Environment,
    old_name: &str,
    scope_id: &str,
    store: &dyn SecretStore,
) -> Result<()> {
    for var in env.variables.iter().filter(|v| v.secret) {
        let old = SecretKey::new(scope_id, old_name, &var.name);
        if let Some(value) = store.get(&old)? {
            store.set(&SecretKey::new(scope_id, &env.name, &var.name), &value)?;
            store.delete(&old)?;
        }
    }
    Ok(())
}

/// Deletes all secrets of `env` (used when the environment is deleted).
pub fn delete_env_secrets(
    env: &Environment,
    scope_id: &str,
    store: &dyn SecretStore,
) -> Result<()> {
    for var in env.variables.iter().filter(|v| v.secret) {
        store.delete(&SecretKey::new(scope_id, &env.name, &var.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use frida_secrets::MemoryStore;

    use super::*;
    use crate::model::EnvVariable;

    fn secret_env() -> Environment {
        let mut env = Environment::new("dev");
        env.variables = vec![
            EnvVariable {
                name: "token".into(),
                value: None,
                secret: true,
                source: None,
            },
            EnvVariable {
                name: "plain".into(),
                value: Some("x".into()),
                secret: false,
                source: None,
            },
        ];
        env
    }

    #[test]
    fn fills_only_secret_values() {
        let store = MemoryStore::default();
        store
            .set(&SecretKey::new("col", "dev", "token"), "s3cr3t")
            .unwrap();
        let mut env = secret_env();
        assert!(fill_secret_values(&mut env, "col", &store).is_empty());
        assert_eq!(env.variables[0].value.as_deref(), Some("s3cr3t"));
        assert_eq!(env.variables[1].value.as_deref(), Some("x"));
    }

    #[test]
    fn renaming_moves_secrets() {
        let store = MemoryStore::default();
        store
            .set(&SecretKey::new("col", "dev", "token"), "s3cr3t")
            .unwrap();
        let mut env = secret_env();
        env.name = "development".into();
        rename_env_secrets(&env, "dev", "col", &store).unwrap();
        assert_eq!(
            store.get(&SecretKey::new("col", "dev", "token")).unwrap(),
            None
        );
        assert_eq!(
            store
                .get(&SecretKey::new("col", "development", "token"))
                .unwrap()
                .as_deref(),
            Some("s3cr3t")
        );
    }

    #[test]
    fn workspace_scope_ids_are_stable_and_distinct() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert_eq!(workspace_scope_id(a.path()), workspace_scope_id(a.path()));
        assert_ne!(workspace_scope_id(a.path()), workspace_scope_id(b.path()));
        assert!(workspace_scope_id(a.path()).starts_with("ws-"));
    }
}
