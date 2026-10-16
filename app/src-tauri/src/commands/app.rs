//! App-level commands: about info and local settings.

use frida_core::AboutInfo;
use frida_core::local_state::{AppSettings, CollectionState, Preferences};
use tauri::State;

use crate::error::{ApiResult, blocking};
use crate::state::AppState;

/// The script and test examples for the editors' "Examples" menu (`frida_script::snippets`;
/// each one is run against the test server by `crates/frida-core/tests/snippets.rs`).
#[tauri::command]
#[specta::specta]
pub fn list_snippets() -> Vec<frida_script::snippets::Snippet> {
    frida_script::snippets::snippets()
}

/// Returns the product name and core library version.
#[tauri::command]
#[specta::specta]
pub fn get_about_info() -> AboutInfo {
    frida_core::about()
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.local.load_settings()
}

/// Saves the Settings dialog and applies it right away (git program, AWS). Values out of range
/// are corrected; returns what was saved.
#[tauri::command]
#[specta::specta]
pub fn save_preferences(
    state: State<'_, AppState>,
    preferences: Preferences,
) -> ApiResult<Preferences> {
    let preferences = preferences.sanitized();
    let mut settings = state.local.load_settings();
    settings.preferences = preferences.clone();
    state.local.save_settings(&settings)?;
    state.apply_preferences(&preferences);
    Ok(preferences)
}

/// The version of the git program at `path` (`None`: `git` from the PATH), for "Test" in the
/// settings. Doesn't change the setting.
#[tauri::command]
#[specta::specta]
pub async fn check_git(path: Option<String>) -> ApiResult<String> {
    let program = std::path::PathBuf::from(
        path.as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .unwrap_or("git"),
    );
    blocking(move || Ok(frida_git::version_of(&program)?)).await
}

/// Forgets all fetched AWS secret values, so the next send asks AWS again.
#[tauri::command]
#[specta::specta]
pub fn clear_aws_cache(state: State<'_, AppState>) {
    state.aws.clear(None);
}

/// Open tabs and expanded folders of a collection, from local state.
#[tauri::command]
#[specta::specta]
pub fn get_collection_state(
    state: State<'_, AppState>,
    collection_id: String,
) -> ApiResult<CollectionState> {
    Ok(state.local.load_collection_state(&collection_id)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_collection_state(
    state: State<'_, AppState>,
    collection_id: String,
    collection_state: CollectionState,
) -> ApiResult<()> {
    // Only the UI's view state (tabs, folders); the active environment is kept as it is.
    Ok(state
        .local
        .save_view_state(&collection_id, &collection_state)?)
}
