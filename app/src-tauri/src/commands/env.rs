//! Environments, their secrets, the active selection, runtime variables and cookies.
//!
//! Keychain access is blocking, so every command touching secrets is `async` and runs that
//! part through [`blocking`] (a `spawn_blocking` wrapper).

use std::path::Path;

use frida_core::http::{self, CookieView};
use frida_core::local_state::CollectionState;
use frida_core::model::Environment;
use frida_core::secrets;
use frida_core::storage;
use frida_secrets::SecretKey;
use frida_secrets::aws::AwsProfile;
use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::{ApiError, ApiResult, ErrorCode, blocking};
use crate::state::{AppState, EnvScope, SecretBackend};

/// An environment plus which of its secrets have a value (the values themselves stay hidden).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    pub environment: Environment,
    pub set_secrets: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn list_environments(
    state: State<'_, AppState>,
    scope: EnvScope,
) -> ApiResult<Vec<EnvironmentInfo>> {
    let (dir, scope_id) = state.env_location(&scope)?;
    let store = state.secrets.store();
    blocking(move || {
        let infos = storage::list_environments(&dir)?
            .into_iter()
            .map(|environment| EnvironmentInfo {
                set_secrets: secrets::set_secret_names(&environment, &scope_id, store.as_ref()),
                environment,
            })
            .collect();
        Ok(infos)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn create_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    name: String,
) -> ApiResult<Environment> {
    let (dir, _) = state.env_location(&scope)?;
    Ok(storage::create_environment(&dir, &name)?)
}

/// Saves an environment that was loaded as `original_name`. Secret values in `environment`
/// are ignored (they never go to disk); use [`set_secret`] for those. Renaming moves secrets.
#[tauri::command]
#[specta::specta]
pub async fn save_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    original_name: String,
    environment: Environment,
) -> ApiResult<()> {
    let (dir, scope_id) = state.env_location(&scope)?;
    storage::save_environment(&dir, &original_name, &environment)?;
    if environment.name != original_name {
        let store = state.secrets.store();
        blocking(move || {
            Ok(secrets::rename_env_secrets(
                &environment,
                &original_name,
                &scope_id,
                store.as_ref(),
            )?)
        })
        .await?;
    }
    Ok(())
}

/// Copies an environment, including its secret values (copied inside Rust, never via the UI).
/// Returns the copy's name.
#[tauri::command]
#[specta::specta]
pub async fn duplicate_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    name: String,
) -> ApiResult<String> {
    let (dir, scope_id) = state.env_location(&scope)?;
    let store = state.secrets.store();
    blocking(move || {
        let copy = storage::duplicate_environment(&dir, &name)?;
        secrets::copy_env_secrets(&copy, &name, &scope_id, store.as_ref())?;
        Ok(copy.name)
    })
    .await
}

/// Saves a new order of the environment list (drag and drop). `names` holds every environment
/// of the scope, in the new order.
#[tauri::command]
#[specta::specta]
pub async fn reorder_environments(
    state: State<'_, AppState>,
    scope: EnvScope,
    names: Vec<String>,
) -> ApiResult<()> {
    let (dir, _) = state.env_location(&scope)?;
    Ok(storage::reorder_environments(&dir, &names)?)
}

/// The environment `name` as YAML, for "Copy to clipboard". Never contains secret values.
#[tauri::command]
#[specta::specta]
pub fn environment_yaml(
    state: State<'_, AppState>,
    scope: EnvScope,
    name: String,
) -> ApiResult<String> {
    let (dir, _) = state.env_location(&scope)?;
    Ok(storage::environment_to_yaml(&dir, &name)?)
}

/// "Paste environment": adds the environment in `yaml` (from the clipboard) to `scope` under a
/// free name. Returns that name. Secret variables arrive without values.
#[tauri::command]
#[specta::specta]
pub fn paste_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    yaml: String,
) -> ApiResult<String> {
    let (dir, _) = state.env_location(&scope)?;
    let env = storage::environment_from_yaml(&yaml)?;
    Ok(storage::add_environment(&dir, &env)?.name)
}

/// Copies or moves the environment `name` from `from` to `to` (another collection, or between a
/// collection and the global environments), secrets included. Returns its name there. Moving
/// the active environment away clears the selection in `from`.
#[tauri::command]
#[specta::specta]
pub async fn transfer_environment(
    state: State<'_, AppState>,
    from: EnvScope,
    name: String,
    to: EnvScope,
    mode: secrets::TransferMode,
) -> ApiResult<String> {
    let (from_dir, from_id) = state.env_location(&from)?;
    let (to_dir, to_id) = state.env_location(&to)?;
    let store = state.secrets.store();
    let moved_name = name.clone();
    let added = blocking(move || {
        Ok(secrets::transfer_environment(
            &name,
            &secrets::EnvLocation {
                dir: &from_dir,
                scope_id: &from_id,
            },
            &secrets::EnvLocation {
                dir: &to_dir,
                scope_id: &to_id,
            },
            mode,
            store.as_ref(),
        )?)
    })
    .await?;
    if mode == secrets::TransferMode::Move {
        let active = match &from {
            EnvScope::Global => {
                let ws = state.workspace_dir()?.to_string_lossy().to_string();
                state
                    .local
                    .load_settings()
                    .active_global_environments
                    .get(&ws)
                    .cloned()
            }
            EnvScope::Collection { dir } => {
                let id = storage::load_collection(&state.collection_dir(dir)?)?.id;
                state.local.load_collection_state(&id)?.active_environment
            }
        };
        if active.as_deref() == Some(moved_name.as_str()) {
            set_active(&state, from, None)?;
        }
    }
    Ok(added.name)
}

/// Deletes an environment file and its secrets.
#[tauri::command]
#[specta::specta]
pub async fn delete_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    name: String,
) -> ApiResult<()> {
    let (dir, scope_id) = state.env_location(&scope)?;
    let env = storage::find_environment(&dir, &name)?.ok_or_else(|| {
        ApiError::new(
            ErrorCode::NotFound,
            format!("environment {name:?} not found"),
        )
    })?;
    storage::delete_environment(&dir, &name)?;
    let store = state.secrets.store();
    blocking(move || {
        Ok(secrets::delete_env_secrets(
            &env,
            &scope_id,
            store.as_ref(),
        )?)
    })
    .await
}

/// Stores a secret value in the keychain (never in YAML).
#[tauri::command]
#[specta::specta]
pub async fn set_secret(
    state: State<'_, AppState>,
    scope: EnvScope,
    environment: String,
    variable: String,
    value: String,
) -> ApiResult<()> {
    let (_, scope_id) = state.env_location(&scope)?;
    let store = state.secrets.store();
    let key = SecretKey::new(scope_id, environment, variable);
    blocking(move || Ok(store.set(&key, &value)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_secret(
    state: State<'_, AppState>,
    scope: EnvScope,
    environment: String,
    variable: String,
) -> ApiResult<()> {
    let (_, scope_id) = state.env_location(&scope)?;
    let store = state.secrets.store();
    let key = SecretKey::new(scope_id, environment, variable);
    blocking(move || Ok(store.delete(&key)?)).await
}

/// Returns a secret's value. Only called when the user clicks "reveal" (PLAN.md §3).
#[tauri::command]
#[specta::specta]
pub async fn reveal_secret(
    state: State<'_, AppState>,
    scope: EnvScope,
    environment: String,
    variable: String,
) -> ApiResult<Option<String>> {
    let (_, scope_id) = state.env_location(&scope)?;
    let store = state.secrets.store();
    let key = SecretKey::new(scope_id, environment, variable);
    tracing::info!(variable = %key.var, "secret revealed in the UI");
    blocking(move || Ok(store.get(&key)?)).await
}

/// The selected global and collection environment.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActiveEnvironments {
    pub global: Option<String>,
    pub collection: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn get_active_environments(
    state: State<'_, AppState>,
    dir: String,
) -> ApiResult<ActiveEnvironments> {
    let collection_dir = state.collection_dir(&dir)?;
    active_environments(&state, &collection_dir)
}

/// Selects an environment (`None` = no environment).
#[tauri::command]
#[specta::specta]
pub fn set_active_environment(
    state: State<'_, AppState>,
    scope: EnvScope,
    name: Option<String>,
) -> ApiResult<()> {
    set_active(&state, scope, name)
}

fn set_active(state: &AppState, scope: EnvScope, name: Option<String>) -> ApiResult<()> {
    match scope {
        EnvScope::Global => {
            let ws = state.workspace_dir()?.to_string_lossy().to_string();
            let mut settings = state.local.load_settings();
            match name {
                Some(name) => settings.active_global_environments.insert(ws, name),
                None => settings.active_global_environments.remove(&ws),
            };
            Ok(state.local.save_settings(&settings)?)
        }
        EnvScope::Collection { dir } => {
            let collection_dir = state.collection_dir(&dir)?;
            let id = storage::load_collection(&collection_dir)?.id;
            let collection_state = CollectionState {
                active_environment: name,
                ..state.local.load_collection_state(&id)?
            };
            Ok(state.local.save_collection_state(&id, &collection_state)?)
        }
    }
}

/// Looks up the active environment names for a collection (used here and when sending).
pub fn active_environments(
    state: &AppState,
    collection_dir: &Path,
) -> ApiResult<ActiveEnvironments> {
    let ws = state.workspace_dir()?.to_string_lossy().to_string();
    let global = state
        .local
        .load_settings()
        .active_global_environments
        .get(&ws)
        .cloned();
    let id = storage::load_collection(collection_dir)?.id;
    let collection = state.local.load_collection_state(&id)?.active_environment;
    Ok(ActiveEnvironments { global, collection })
}

/// How secrets are stored on this machine, for the env editor.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretBackendStatus {
    /// `true`: the OS keychain. `false`: the encrypted file fallback.
    pub keychain: bool,
    /// For the file fallback: whether it is unlocked.
    pub unlocked: bool,
    /// For the file fallback: whether it was created already (unlocking checks the password).
    pub file_exists: bool,
}

#[tauri::command]
#[specta::specta]
pub fn secret_backend_status(state: State<'_, AppState>) -> SecretBackendStatus {
    match &state.secrets {
        SecretBackend::Keychain(_) => SecretBackendStatus {
            keychain: true,
            unlocked: true,
            file_exists: false,
        },
        SecretBackend::File(file) => SecretBackendStatus {
            keychain: false,
            unlocked: file.is_unlocked(),
            file_exists: file.exists(),
        },
    }
}

/// Unlocks (or, the first time, creates) the encrypted secret file.
#[tauri::command]
#[specta::specta]
pub async fn unlock_secret_file(state: State<'_, AppState>, password: String) -> ApiResult<()> {
    match &state.secrets {
        SecretBackend::Keychain(_) => Ok(()),
        SecretBackend::File(file) => {
            let file = std::sync::Arc::clone(file);
            // Argon2 key derivation is deliberately slow, so it runs off the async runtime too.
            blocking(move || Ok(file.unlock(&password)?)).await
        }
    }
}

/// Clears what scripts set in this session: the collection's runtime variables (`fri.setVar`)
/// and the in-memory environment changes (`fri.setEnvVar`, `fri.setGlobalEnvVar`).
#[tauri::command]
#[specta::specta]
pub fn clear_runtime_vars(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    let collection_dir = state.collection_dir(&dir)?;
    state
        .sessions
        .lock()
        .expect("sessions lock poisoned")
        .remove(&collection_dir);
    state.global_overrides.clear();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn list_cookies(state: State<'_, AppState>, dir: String) -> ApiResult<Vec<CookieView>> {
    let collection_dir = state.collection_dir(&dir)?;
    Ok(http::list_cookies(&state.cookie_jar(&collection_dir)))
}

/// Removes the cookies of `domain`, or all of the collection's cookies when `domain` is `None`.
#[tauri::command]
#[specta::specta]
pub fn clear_cookies(
    state: State<'_, AppState>,
    dir: String,
    domain: Option<String>,
) -> ApiResult<()> {
    let collection_dir = state.collection_dir(&dir)?;
    http::clear_cookies(&state.cookie_jar(&collection_dir), domain.as_deref());
    Ok(())
}

/// The AWS profile chosen for environment `name` (PLAN.md §9.1), from local state.
pub fn aws_profile(state: &AppState, scope: &EnvScope, name: &str) -> ApiResult<Option<String>> {
    match scope {
        EnvScope::Global => {
            let key = global_aws_key(&state.workspace_dir()?, name);
            Ok(state.local.load_settings().aws_profiles.get(&key).cloned())
        }
        EnvScope::Collection { dir } => {
            let id = storage::load_collection(&state.collection_dir(dir)?)?.id;
            Ok(state
                .local
                .load_collection_state(&id)?
                .aws_profiles
                .get(name)
                .cloned())
        }
    }
}

fn global_aws_key(workspace_dir: &Path, env: &str) -> String {
    format!("{}|{env}", workspace_dir.to_string_lossy())
}

/// The AWS profiles in `~/.aws/config` and `~/.aws/credentials` (or the files from the settings).
#[tauri::command]
#[specta::specta]
pub async fn list_aws_profiles(state: State<'_, AppState>) -> ApiResult<Vec<AwsProfile>> {
    let files = state.aws.files();
    blocking(move || Ok(frida_secrets::aws::list_profiles(&files))).await
}

#[tauri::command]
#[specta::specta]
pub fn get_aws_profile(
    state: State<'_, AppState>,
    scope: EnvScope,
    environment: String,
) -> ApiResult<Option<String>> {
    aws_profile(&state, &scope, &environment)
}

/// Chooses (or with `None` forgets) the AWS profile of an environment. Kept in local state:
/// profile names are personal, so they don't go into the shared environment file.
#[tauri::command]
#[specta::specta]
pub fn set_aws_profile(
    state: State<'_, AppState>,
    scope: EnvScope,
    environment: String,
    profile: Option<String>,
) -> ApiResult<()> {
    match scope {
        EnvScope::Global => {
            let key = global_aws_key(&state.workspace_dir()?, &environment);
            let mut settings = state.local.load_settings();
            match profile {
                Some(p) => settings.aws_profiles.insert(key, p),
                None => settings.aws_profiles.remove(&key),
            };
            Ok(state.local.save_settings(&settings)?)
        }
        EnvScope::Collection { dir } => {
            let id = storage::load_collection(&state.collection_dir(&dir)?)?.id;
            let mut collection_state = state.local.load_collection_state(&id)?;
            match profile {
                Some(p) => collection_state.aws_profiles.insert(environment, p),
                None => collection_state.aws_profiles.remove(&environment),
            };
            Ok(state.local.save_collection_state(&id, &collection_state)?)
        }
    }
}

/// "Refresh secrets": forgets the cached AWS values, so the next send fetches them again.
#[tauri::command]
#[specta::specta]
pub fn refresh_aws_secrets(state: State<'_, AppState>) {
    state.aws.clear(None);
}
