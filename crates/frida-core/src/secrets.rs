//! Glue between environments and the secret stores of `frida-secrets` (PLAN.md §6).

use std::collections::BTreeMap;
use std::path::Path;

use frida_secrets::aws::{AwsSecretRef, AwsSecrets, AwsTarget};
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

/// Which AWS profile reads which secret (PLAN.md §9.1, M17).
#[derive(Debug, Clone, Copy)]
pub struct AwsProfiles<'a> {
    /// The profile chosen for the environment, for secrets without an account alias.
    pub environment: Option<&'a str>,
    /// Account alias → local profile, for secrets with `account: <alias>`.
    pub accounts: &'a BTreeMap<String, String>,
}

/// Fills the secret variables of `env` whose value comes from AWS: Secrets Manager, a Cognito
/// app client secret or an API Gateway key (PLAN.md §9.1, M17).
///
/// A secret with an account alias uses the profile mapped to that alias; the others use the
/// environment's profile. A missing profile fails the send with
/// [`CoreError::AwsProfileRequired`] or [`CoreError::AwsAccountRequired`], so the UI can ask
/// for it. With [`AwsFetch::CacheOnly`] missing values just stay unset.
pub async fn fill_aws_secrets(
    env: &mut Environment,
    global: bool,
    profiles: AwsProfiles<'_>,
    aws: &AwsSecrets,
    mode: AwsFetch,
) -> Result<()> {
    let env_name = env.name.clone();
    for var in env.variables.iter_mut().filter(|v| v.secret && v.enabled) {
        let Some(source) = &var.source else {
            continue;
        };
        let profile = match source.account() {
            Some(alias) => match profiles.accounts.get(alias) {
                Some(profile) => profile.as_str(),
                None if mode == AwsFetch::CacheOnly => continue,
                None => {
                    return Err(CoreError::AwsAccountRequired {
                        account: alias.to_string(),
                        environment: env_name,
                    });
                }
            },
            None => match profiles.environment {
                Some(profile) => profile,
                None if mode == AwsFetch::CacheOnly => continue,
                None => {
                    return Err(CoreError::AwsProfileRequired {
                        environment: env_name,
                        global,
                    });
                }
            },
        };
        let secret = AwsSecretRef {
            profile: profile.to_string(),
            region: source.region().map(str::to_string),
            target: aws_target(source),
            json_key: match source {
                SecretSource::AwsSecretsManager { json_key, .. } => {
                    json_key.clone().filter(|k| !k.trim().is_empty())
                }
                _ => None,
            },
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

/// What `source` reads from AWS.
fn aws_target(source: &SecretSource) -> AwsTarget {
    match source {
        SecretSource::AwsSecretsManager { secret_id, .. } => AwsTarget::Secret {
            secret_id: secret_id.trim().to_string(),
        },
        SecretSource::CognitoClientSecret {
            user_pool_id,
            client_id,
            ..
        } => AwsTarget::CognitoClientSecret {
            user_pool_id: user_pool_id.trim().to_string(),
            client_id: client_id.trim().to_string(),
        },
        SecretSource::ApiGatewayKey { api_key_id, .. } => AwsTarget::ApiGatewayKey {
            api_key_id: api_key_id.trim().to_string(),
        },
    }
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

/// Copy or move, for [`transfer_environment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TransferMode {
    Copy,
    /// Copy, then delete the original (file and secrets).
    Move,
}

/// Where an environment lives: its folder (workspace or collection) and keychain scope.
pub struct EnvLocation<'a> {
    pub dir: &'a Path,
    pub scope_id: &'a str,
}

/// Copies or moves the environment `name` to another scope, e.g. from a collection to the
/// workspace's global environments or to another collection. It gets a free name there
/// (`dev 2` if `dev` exists). Secret values go along through the keychain (never through a
/// file). Returns the new environment.
///
/// Blocking (keychain), see [`fill_secret_values`].
pub fn transfer_environment(
    name: &str,
    from: &EnvLocation,
    to: &EnvLocation,
    mode: TransferMode,
    store: &dyn SecretStore,
) -> Result<Environment> {
    if from.dir == to.dir {
        return Err(CoreError::InvalidInput(
            "the environment is already there".into(),
        ));
    }
    let original =
        storage::find_environment(from.dir, name)?.ok_or_else(|| CoreError::NotFound {
            path: from.dir.join("environments").join(name),
        })?;
    let added = storage::add_environment(to.dir, &original)?;
    for var in original
        .variables
        .iter()
        .filter(|v| v.secret && v.source.is_none())
    {
        let source = SecretKey::new(from.scope_id, &original.name, &var.name);
        if let Some(value) = store.get(&source)? {
            store.set(&SecretKey::new(to.scope_id, &added.name, &var.name), &value)?;
        }
    }
    if mode == TransferMode::Move {
        storage::delete_environment(from.dir, &original.name)?;
        delete_env_secrets(&original, from.scope_id, store)?;
    }
    Ok(added)
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

    /// Two scopes on disk: a collection and the workspace (global environments).
    fn two_scopes() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let col = tmp.path().join("col");
        let ws = tmp.path().join("ws");
        std::fs::create_dir_all(col.join("environments")).unwrap();
        std::fs::create_dir_all(ws.join("environments")).unwrap();
        (tmp, col, ws)
    }

    #[test]
    fn copying_and_moving_environments_takes_their_secrets_along() {
        let (_tmp, col, ws) = two_scopes();
        let store = MemoryStore::default();
        storage::add_environment(&col, &secret_env()).unwrap();
        store
            .set(&SecretKey::new("col", "dev", "token"), "s3cr3t")
            .unwrap();
        // A "dev" already exists globally, so the copy becomes "dev 2".
        storage::add_environment(&ws, &Environment::new("dev")).unwrap();
        let from = EnvLocation {
            dir: &col,
            scope_id: "col",
        };
        let to = EnvLocation {
            dir: &ws,
            scope_id: "ws",
        };

        let copy = transfer_environment("dev", &from, &to, TransferMode::Copy, &store).unwrap();
        assert_eq!(copy.name, "dev 2");
        assert_eq!(copy.variables[1].value.as_deref(), Some("x"));
        assert_eq!(
            store
                .get(&SecretKey::new("ws", "dev 2", "token"))
                .unwrap()
                .as_deref(),
            Some("s3cr3t")
        );
        // The secret value is in the keychain only, not in the new file.
        let on_disk = std::fs::read_to_string(ws.join("environments/dev-2.yaml")).unwrap();
        assert!(!on_disk.contains("s3cr3t"));
        assert!(storage::find_environment(&col, "dev").unwrap().is_some());

        let moved = transfer_environment("dev", &from, &to, TransferMode::Move, &store).unwrap();
        assert_eq!(moved.name, "dev 3");
        assert!(storage::find_environment(&col, "dev").unwrap().is_none());
        assert_eq!(
            store.get(&SecretKey::new("col", "dev", "token")).unwrap(),
            None
        );
        assert!(transfer_environment("x", &to, &to, TransferMode::Copy, &store).is_err());
    }

    #[test]
    fn clipboard_yaml_never_contains_secret_values() {
        let (_tmp, col, ws) = two_scopes();
        let mut env = secret_env();
        env.variables[0].value = Some("typed-in".into());
        storage::add_environment(&col, &env).unwrap();

        let yaml = storage::environment_to_yaml(&col, "dev").unwrap();
        assert!(yaml.contains("token") && !yaml.contains("typed-in"));
        // Even if someone pastes a value for a secret, it is dropped.
        let pasted = storage::environment_from_yaml(
            &yaml.replace("secret: true", "secret: true\n  value: sneaky"),
        )
        .unwrap();
        assert!(
            pasted
                .variables
                .iter()
                .all(|v| !v.secret || v.value.is_none())
        );
        assert_eq!(storage::add_environment(&ws, &pasted).unwrap().name, "dev");
        assert!(storage::environment_from_yaml("hello: world").is_err());
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
