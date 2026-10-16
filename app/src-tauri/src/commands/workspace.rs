//! Opening workspaces and managing their collection list.

use std::path::{Path, PathBuf};

use frida_core::import::{self, ImportSummary};
use frida_core::storage::{self, WorkspaceOverview};
use tauri::State;

use crate::error::{ApiResult, blocking};
use crate::state::AppState;

/// Creates a new workspace in `dir` and opens it.
#[tauri::command]
#[specta::specta]
pub fn create_workspace(
    state: State<'_, AppState>,
    dir: String,
    name: String,
) -> ApiResult<WorkspaceOverview> {
    storage::create_workspace(Path::new(&dir), &name)?;
    open_workspace(state, dir)
}

/// Opens the workspace in `dir` and remembers it in the recent list.
#[tauri::command]
#[specta::specta]
pub fn open_workspace(state: State<'_, AppState>, dir: String) -> ApiResult<WorkspaceOverview> {
    let path = PathBuf::from(&dir);
    let overview = state.refresh_workspace(&path)?;
    let mut settings = state.local.load_settings();
    settings.remember_workspace(&dir);
    state.local.save_settings(&settings)?;
    tracing::info!(workspace = %dir, "workspace opened");
    Ok(overview)
}

/// Reloads the open workspace, e.g. after `frida-workspace.yaml` changed on disk.
#[tauri::command]
#[specta::specta]
pub fn reload_workspace(state: State<'_, AppState>) -> ApiResult<WorkspaceOverview> {
    let dir = state.workspace_dir()?;
    state.refresh_workspace(&dir)
}

#[tauri::command]
#[specta::specta]
pub fn close_workspace(state: State<'_, AppState>) {
    let previous = state
        .workspace
        .lock()
        .expect("workspace lock poisoned")
        .take();
    if let (Some(previous), Some(watcher)) = (
        previous,
        state
            .watcher
            .lock()
            .expect("watcher lock poisoned")
            .as_mut(),
    ) {
        for dir in previous.watched_dirs() {
            watcher.unwatch(&dir);
        }
    }
}

/// Creates a new collection at `<workspace>/collections/<name>` and adds it to the workspace.
/// Returns the updated overview and the new collection's folder.
#[tauri::command]
#[specta::specta]
pub fn create_collection(
    state: State<'_, AppState>,
    name: String,
) -> ApiResult<(WorkspaceOverview, String)> {
    let ws = state.workspace_dir()?;
    let dir = storage::create_named_collection(&ws, &name)?;
    let overview = state.refresh_workspace(&ws)?;
    // The overview lists canonical folders, so return the same form for the UI to select it.
    let dir = dir.canonicalize().unwrap_or(dir);
    Ok((overview, dir.to_string_lossy().to_string()))
}

/// Adds an existing collection folder to the open workspace.
#[tauri::command]
#[specta::specta]
pub fn add_collection(state: State<'_, AppState>, dir: String) -> ApiResult<WorkspaceOverview> {
    let ws = state.workspace_dir()?;
    storage::add_collection(&ws, Path::new(&dir))?;
    state.refresh_workspace(&ws)
}

/// Removes a collection from the workspace list. Its files are not deleted.
#[tauri::command]
#[specta::specta]
pub fn remove_collection(state: State<'_, AppState>, dir: String) -> ApiResult<WorkspaceOverview> {
    let ws = state.workspace_dir()?;
    storage::remove_collection(&ws, Path::new(&dir))?;
    state.refresh_workspace(&ws)
}

/// What importing the OpenAPI spec at `file` would create (counts, base URL, warnings).
#[tauri::command]
#[specta::specta]
pub async fn preview_openapi(file: String) -> ApiResult<ImportSummary> {
    blocking(move || Ok(import::preview_openapi_file(Path::new(&file))?)).await
}

/// The result of an import: the updated workspace, the new collection's folder and the summary.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImported {
    pub overview: WorkspaceOverview,
    pub dir: String,
    pub summary: ImportSummary,
}

/// Imports the OpenAPI spec at `file` as a new collection, in `target` or by default in
/// `<workspace>/collections/<title>`, and adds it to the workspace.
#[tauri::command]
#[specta::specta]
pub async fn import_openapi(
    state: State<'_, AppState>,
    file: String,
    target: Option<String>,
) -> ApiResult<OpenApiImported> {
    let ws = state.workspace_dir()?;
    let ws_for_import = ws.clone();
    let (dir, summary) = blocking(move || {
        let target = target.map(PathBuf::from);
        Ok(import::import_openapi_file(
            &ws_for_import,
            Path::new(&file),
            target.as_deref(),
        )?)
    })
    .await?;
    let overview = state.refresh_workspace(&ws)?;
    // The overview lists canonical folders, so return the same form for the UI to select it.
    let dir = dir.canonicalize().unwrap_or(dir);
    Ok(OpenApiImported {
        overview,
        dir: dir.to_string_lossy().to_string(),
        summary,
    })
}
