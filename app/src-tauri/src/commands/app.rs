//! App-level commands: about info and local settings.

use frida_core::AboutInfo;
use frida_core::local_state::{AppSettings, CollectionState, ThemePreference};
use tauri::State;

use crate::error::ApiResult;
use crate::state::AppState;

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

#[tauri::command]
#[specta::specta]
pub fn set_theme(state: State<'_, AppState>, theme: ThemePreference) -> ApiResult<()> {
    let mut settings = state.local.load_settings();
    settings.theme = theme;
    Ok(state.local.save_settings(&settings)?)
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
    Ok(state
        .local
        .save_collection_state(&collection_id, &collection_state)?)
}
