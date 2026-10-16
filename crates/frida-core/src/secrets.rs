//! Glue between environments and the secret stores of `frida-secrets` (PLAN.md §6).

use std::path::Path;

use frida_secrets::aws::{AwsSecretRef, AwsSecrets};
use frida_secrets::{SecretKey, SecretStore};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};
use crate::model::{Environment, SecretSource};
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
    // AWS-backed secrets are filled by `fill_aws_secrets`, not from the keychain.
    for var in env
        .variables
        .iter_mut()
        .filter(|v| v.secret && v.source.is_none())
    {
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

/// Whether to ask AWS for secrets that aren't cached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsFetch {
    /// Sending: fetch what isn't cached.
    Fetch,
    /// Inspecting while typing: cached values only, never a network call.
    CacheOnly,
}

/// Fills the secret variables of `env` whose source is AWS Secrets Manager (PLAN.md §9.1).
/// `profile` is the AWS profile chosen for this environment; without one, and if the
/// environment has AWS secrets, sending fails with [`CoreError::AwsProfileRequired`] so the UI
/// can ask for it. With [`AwsFetch::CacheOnly`] missing values just stay unset.
pub async fn fill_aws_secrets(
    env: &mut Environment,
    global: bool,
    profile: Option<&str>,
    aws: &AwsSecrets,
    mode: AwsFetch,
) -> Result<()> {
    let has_aws = env
        .variables
        .iter()
        .any(|v| v.secret && matches!(v.source, Some(SecretSource::AwsSecretsManager { .. })));
    if !has_aws {
        return Ok(());
    }
    let Some(profile) = profile else {
        return match mode {
            AwsFetch::CacheOnly => Ok(()),
            AwsFetch::Fetch => Err(CoreError::AwsProfileRequired {
                environment: env.name.clone(),
                global,
            }),
        };
    };
    let env_name = env.name.clone();
    for var in env.variables.iter_mut().filter(|v| v.secret && v.enabled) {
        let Some(SecretSource::AwsSecretsManager {
            secret_id,
            json_key,
            region,
        }) = &var.source
        else {
            continue;
        };
        let secret = AwsSecretRef {
            profile: profile.to_string(),
            region: region.clone(),
            secret_id: secret_id.clone(),
            json_key: json_key.clone(),
        };
        let value = match mode {
            AwsFetch::Fetch => Some(aws.get(&secret).await),
            AwsFetch::CacheOnly => aws.cached(&secret),
        };
        match value {
            Some(Ok(value)) => var.value = Some(value),
            Some(Err(source)) => {
                return Err(CoreError::Aws {
                    environment: env_name,
                    variable: var.name.clone(),
                    source,
                });
            }
            None => var.value = None,
        }
    }
    Ok(())
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

/// Copies the secrets of the environment `from` to `to` (used when duplicating it). `env` is
/// the copy; its secret variables decide what is copied.
pub fn copy_env_secrets(
    env: &Environment,
    from: &str,
    scope_id: &str,
    store: &dyn SecretStore,
) -> Result<()> {
    for var in env.variables.iter().filter(|v| v.secret) {
        if let Some(value) = store.get(&SecretKey::new(scope_id, from, &var.name))? {
            store.set(&SecretKey::new(scope_id, &env.name, &var.name), &value)?;
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
                enabled: true,
            },
            EnvVariable {
                name: "plain".into(),
                value: Some("x".into()),
                secret: false,
                source: None,
                enabled: true,
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

    #[test]
    fn duplicates_copy_secret_values_and_keep_the_original() {
        let store = MemoryStore::default();
        store
            .set(&SecretKey::new("col", "dev", "token"), "s3cr3t")
            .unwrap();
        let mut copy = secret_env();
        copy.name = "dev copy".into();
        copy_env_secrets(&copy, "dev", "col", &store).unwrap();
        for env in ["dev", "dev copy"] {
            assert_eq!(
                store
                    .get(&SecretKey::new("col", env, "token"))
                    .unwrap()
                    .as_deref(),
                Some("s3cr3t")
            );
        }
    }
}
