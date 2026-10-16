//! Request history and curl import/export.

use frida_core::codegen::CurlOptions;
use frida_core::history::{HistoryEntry, HistorySummary};
use frida_core::http::{self, ExecuteInput};
use frida_core::import::{CurlImport, import_curl as parse_curl};
use frida_core::model::Request;
use frida_core::storage;
use tauri::State;

use super::http::load_active_envs;
use crate::error::{ApiError, ApiResult, ErrorCode, blocking};
use crate::state::AppState;
use frida_core::secrets::AwsFetch;

/// How many entries the history list shows at most.
const LIST_LIMIT: usize = 500;

fn store(state: &AppState) -> ApiResult<std::sync::Arc<frida_core::history::HistoryStore>> {
    state.history.clone().ok_or_else(|| {
        ApiError::new(ErrorCode::Io, "history is not available")
            .with_hint("See the log file for why the history database could not be opened.")
    })
}

/// History of the collection `dir` (or of all collections when `None`), newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_history(
    state: State<'_, AppState>,
    dir: Option<String>,
    search: String,
) -> ApiResult<Vec<HistorySummary>> {
    let collection_id = match dir {
        Some(dir) => Some(storage::load_collection(&state.collection_dir(&dir)?)?.id),
        None => None,
    };
    let history = store(&state)?;
    blocking(move || Ok(history.list(collection_id.as_deref(), &search, LIST_LIMIT)?)).await
}

/// One entry with its request and response. The response body is also put in the response
/// cache, so "Save to file" works for it like for a live response.
#[tauri::command]
#[specta::specta]
pub async fn get_history_entry(state: State<'_, AppState>, id: u32) -> ApiResult<HistoryEntry> {
    let history = store(&state)?;
    let (entry, body) =
        blocking(move || Ok((history.get(id)?, history.response_body(id)?))).await?;
    if let Some(response) = &entry.response {
        state
            .responses
            .lock()
            .expect("responses lock poisoned")
            .insert(response.id.clone(), body);
    }
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_history(state: State<'_, AppState>) -> ApiResult<()> {
    let history = store(&state)?;
    blocking(move || Ok(history.clear()?)).await
}

/// "Copy as curl" for the editor's current request at `path` in collection `dir`.
#[tauri::command]
#[specta::specta]
pub async fn export_curl(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    request: Request,
    options: CurlOptions,
) -> ApiResult<String> {
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root, AwsFetch::CacheOnly).await?;
    let store = state.session_store(&root);
    let workspace_dir = state.workspace_dir().ok();
    let input = ExecuteInput {
        collection_dir: &root,
        request_rel: &path,
        request: &request,
        collection_env: collection_env.as_ref(),
        global_env: global_env.as_ref(),
        session: state.session(&store, workspace_dir.as_deref()),
        cookies: None,
        oauth: &state.oauth,
    };
    if options.include_secrets {
        tracing::info!("curl exported with secrets included (explicitly confirmed)");
    }
    Ok(http::export_curl(&input, options)?)
}

/// Parses a curl command into a request (not saved yet).
#[tauri::command]
#[specta::specta]
pub fn import_curl(command: String) -> ApiResult<CurlImport> {
    parse_curl(&command).map_err(|message| {
        ApiError::new(ErrorCode::InvalidInput, message)
            .with_hint("Paste a complete curl command, e.g. copied from the browser's dev tools.")
    })
}

/// "Restore as new request": creates a request in the collection root from a history entry and
/// returns its path.
#[tauri::command]
#[specta::specta]
pub async fn restore_history_entry(
    state: State<'_, AppState>,
    dir: String,
    id: u32,
) -> ApiResult<String> {
    let root = state.collection_dir(&dir)?;
    let history = store(&state)?;
    let entry = blocking(move || Ok(history.get(id)?)).await?;
    Ok(storage::create_request(&root, "", &entry.to_request())?)
}
