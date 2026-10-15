//! Sending requests, cancelling them and saving response bodies.

use std::path::PathBuf;

use frida_core::http::{self, ResponseView};
use frida_core::model::Request;
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;

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
    let cancel = CancellationToken::new();
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .insert(request_id.clone(), cancel.clone());

    // The lock above is released at the end of its statement, *before* this await.
    let runtime = std::collections::HashMap::new();
    let input = http::ExecuteInput {
        collection_dir: &root,
        request_rel: &path,
        request: &request,
        collection_env: None,
        global_env: None,
        runtime: &runtime,
        cookies: None,
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
