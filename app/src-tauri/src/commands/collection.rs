//! Tree and file operations inside a collection. `dir` is always a collection folder of the
//! open workspace (checked by `AppState::collection_dir`), and `path` is relative to it.

use frida_core::model::{Collection, Folder, Request};
use frida_core::storage::{self, TreeNode};
use tauri::State;

use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;

/// The collection's own settings file (`collection.yaml`).
#[tauri::command]
#[specta::specta]
pub fn read_collection(state: State<'_, AppState>, dir: String) -> ApiResult<Collection> {
    Ok(storage::load_collection(&state.collection_dir(&dir)?)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_collection(
    state: State<'_, AppState>,
    dir: String,
    collection: Collection,
) -> ApiResult<()> {
    let root = state.collection_dir(&dir)?;
    // The id keys local state and secrets, so it must never change through the UI.
    let current = storage::load_collection(&root)?;
    let collection = Collection {
        id: current.id,
        ..collection
    };
    Ok(storage::save_collection(&root, &collection)?)
}

#[tauri::command]
#[specta::specta]
pub fn get_tree(state: State<'_, AppState>, dir: String) -> ApiResult<Vec<TreeNode>> {
    Ok(storage::scan_tree(&state.collection_dir(&dir)?)?)
}

#[tauri::command]
#[specta::specta]
pub fn read_request(state: State<'_, AppState>, dir: String, path: String) -> ApiResult<Request> {
    Ok(storage::read_request(&state.collection_dir(&dir)?, &path)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_request(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    request: Request,
) -> ApiResult<()> {
    Ok(storage::save_request(
        &state.collection_dir(&dir)?,
        &path,
        &request,
    )?)
}

#[tauri::command]
#[specta::specta]
pub fn read_folder(state: State<'_, AppState>, dir: String, path: String) -> ApiResult<Folder> {
    Ok(storage::read_folder(&state.collection_dir(&dir)?, &path)?)
}

/// Creates an empty GET request named `name` in folder `parent` (`""` = root).
/// Returns the new request's path.
#[tauri::command]
#[specta::specta]
pub fn create_request(
    state: State<'_, AppState>,
    dir: String,
    parent: String,
    name: String,
) -> ApiResult<String> {
    let root = state.collection_dir(&dir)?;
    Ok(storage::create_request(
        &root,
        &parent,
        &Request::new(name),
    )?)
}

/// Creates a request from a full request object (used by "Save as new request").
#[tauri::command]
#[specta::specta]
pub fn create_request_from(
    state: State<'_, AppState>,
    dir: String,
    parent: String,
    request: Request,
) -> ApiResult<String> {
    let root = state.collection_dir(&dir)?;
    Ok(storage::create_request(&root, &parent, &request)?)
}

#[tauri::command]
#[specta::specta]
pub fn create_folder(
    state: State<'_, AppState>,
    dir: String,
    parent: String,
    name: String,
) -> ApiResult<String> {
    Ok(storage::create_folder(
        &state.collection_dir(&dir)?,
        &parent,
        &name,
    )?)
}

/// Renames a request or folder. Returns its new path.
#[tauri::command]
#[specta::specta]
pub fn rename_item(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    name: String,
) -> ApiResult<String> {
    Ok(storage::rename_item(
        &state.collection_dir(&dir)?,
        &path,
        &name,
    )?)
}

#[tauri::command]
#[specta::specta]
pub fn duplicate_request(
    state: State<'_, AppState>,
    dir: String,
    path: String,
) -> ApiResult<String> {
    Ok(storage::duplicate_request(
        &state.collection_dir(&dir)?,
        &path,
    )?)
}

#[tauri::command]
#[specta::specta]
pub fn delete_item(state: State<'_, AppState>, dir: String, path: String) -> ApiResult<()> {
    Ok(storage::delete_item(&state.collection_dir(&dir)?, &path)?)
}

/// Moves a request or folder into `parent` at position `index`. Returns its new path.
#[tauri::command]
#[specta::specta]
pub fn move_item(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    parent: String,
    index: u32,
) -> ApiResult<String> {
    let root = state.collection_dir(&dir)?;
    Ok(storage::move_item(&root, &path, &parent, index as usize)?)
}

/// Shows a request file or folder in Finder / Explorer / the file manager.
#[tauri::command]
#[specta::specta]
pub fn reveal_item(state: State<'_, AppState>, dir: String, path: String) -> ApiResult<()> {
    let root = state.collection_dir(&dir)?;
    let target = if path.is_empty() {
        root
    } else {
        root.join(&path)
    };
    tauri_plugin_opener::reveal_item_in_dir(&target).map_err(|e| {
        ApiError::new(
            ErrorCode::Io,
            format!("could not open the file manager: {e}"),
        )
    })
}
