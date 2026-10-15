//! Sending requests, inspecting their variables, cancelling and saving response bodies.

use std::path::{Path, PathBuf};

use frida_core::http::{self, ExecuteInput, ResponseView};
use frida_core::model::{Environment, Request};
use frida_core::secrets;
use frida_core::storage;
use frida_core::vars::ResolveReport;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::env::active_environments;
use crate::error::{ApiError, ApiResult, ErrorCode, blocking};
use crate::state::AppState;

/// The active global and collection environment of a collection, with secret values filled in
/// from the keychain. Keychain access is blocking, so it runs off the async runtime.
async fn load_active_envs(
    state: &AppState,
    collection_dir: &Path,
) -> ApiResult<(Option<Environment>, Option<Environment>)> {
    let active = active_environments(state, collection_dir)?;
    let workspace_dir = state.workspace_dir()?;
    let workspace_scope = secrets::workspace_scope_id(&workspace_dir);
    let collection_dir: PathBuf = collection_dir.to_path_buf();
    let collection_scope = storage::load_collection(&collection_dir)?.id;
    let store = state.secrets.store();

    blocking(move || {
        let global = match &active.global {
            Some(name) => secrets::load_environment_with_secrets(
                &workspace_dir,
                name,
                &workspace_scope,
                store.as_ref(),
            )?,
            None => None,
        };
        let collection = match &active.collection {
            Some(name) => secrets::load_environment_with_secrets(
                &collection_dir,
                name,
                &collection_scope,
                store.as_ref(),
            )?,
            None => None,
        };
        Ok((global, collection))
    })
    .await
}

/// Sends `request` (the editor's current state, saved or not) as if it lived at `path` in the
/// collection `dir`. `request_id` is chosen by the UI and used by [`cancel_request`].
///
/// This is an `async` command, so Tauri runs it on its tokio runtime and the UI stays
/// responsive while waiting for the server.
#[tauri::command]
#[specta::specta]
pub async fn send_request(
    state: State<'_, AppState>,
    request_id: String,
    dir: String,
    path: String,
    request: Request,
) -> ApiResult<ResponseView> {
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root).await?;
    let runtime = state.runtime_vars(&root);
    let cancel = CancellationToken::new();
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .insert(request_id.clone(), cancel.clone());

    // The lock above is released at the end of its statement, *before* this await.
    let input = ExecuteInput {
        collection_dir: &root,
        request_rel: &path,
        request: &request,
        collection_env: collection_env.as_ref(),
        global_env: global_env.as_ref(),
        runtime: &runtime,
        cookies: Some(state.cookie_jar(&root)),
    };
    let result = http::execute(input, cancel).await;
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .remove(&request_id);

    let response = result?.response;
    let id = uuid::Uuid::new_v4().to_string();
    let view = ResponseView::new(id.clone(), &response);
    state
        .responses
        .lock()
        .expect("responses lock poisoned")
        .insert(id, response.body);
    tracing::info!(
        status = view.status,
        time_ms = view.time_ms,
        "response received"
    );
    Ok(view)
}

/// Resolves the variables of `request` without sending it, for the inspector panel and the
/// editor tooltips. Secret values are masked.
#[tauri::command]
#[specta::specta]
pub async fn inspect_request(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    request: Request,
) -> ApiResult<ResolveReport> {
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root).await?;
    let runtime = state.runtime_vars(&root);
    let report = http::inspect(&ExecuteInput {
        collection_dir: &root,
        request_rel: &path,
        request: &request,
        collection_env: collection_env.as_ref(),
        global_env: global_env.as_ref(),
        runtime: &runtime,
        cookies: None,
    })?;
    Ok(report.masked())
}

/// Cancels an in-flight request. Unknown ids (already finished) are ignored.
#[tauri::command]
#[specta::specta]
pub fn cancel_request(state: State<'_, AppState>, request_id: String) {
    if let Some(token) = state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .get(&request_id)
    {
        token.cancel();
    }
}

/// Writes the full body of a recent response to `target` (chosen in a save dialog).
#[tauri::command]
#[specta::specta]
pub fn save_response_body(
    state: State<'_, AppState>,
    response_id: String,
    target: String,
) -> ApiResult<()> {
    let body = state
        .responses
        .lock()
        .expect("responses lock poisoned")
        .get(&response_id)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            ApiError::new(ErrorCode::NotFound, "this response is no longer available")
                .with_hint("Send the request again, then save.")
        })?;
    let target = PathBuf::from(target);
    std::fs::write(&target, body).map_err(|e| {
        ApiError::new(
            ErrorCode::Io,
            format!("could not write {}: {e}", target.display()),
        )
    })
}
