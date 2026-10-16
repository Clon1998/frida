//! Sending requests, inspecting their variables, cancelling and saving response bodies.

use std::path::{Path, PathBuf};

use frida_core::auth::TokenInfo;
use frida_core::history::NewEntry;
use frida_core::http::{self, ConsoleEntry, ExecuteInput, ResponseView};
use tauri_specta::Event;

use crate::{ConsoleLogged, ScriptLogged};
use frida_core::model::{Environment, Request};
use frida_core::secrets;
use frida_core::storage;
use frida_core::vars::ResolveReport;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::env::{active_environments, aws_profile};
use crate::error::{ApiError, ApiResult, ErrorCode, blocking};
use crate::state::AppState;
use crate::state::EnvScope;
use frida_core::secrets::AwsFetch;

/// The active global and collection environment of a collection, with secret values filled in
/// from the keychain (blocking, so off the async runtime) and from AWS Secrets Manager
/// (`aws`: fetch when sending, cache only when inspecting).
pub(crate) async fn load_active_envs(
    state: &AppState,
    collection_dir: &Path,
    aws: AwsFetch,
) -> ApiResult<(Option<Environment>, Option<Environment>)> {
    let (mut global, mut collection) = load_keychain_envs(state, collection_dir).await?;
    let dir_text = collection_dir.to_string_lossy().to_string();
    if let Some(env) = &mut global {
        let profile = aws_profile(state, &EnvScope::Global, &env.name)?;
        secrets::fill_aws_secrets(env, true, profile.as_deref(), &state.aws, aws).await?;
    }
    if let Some(env) = &mut collection {
        let scope = EnvScope::Collection { dir: dir_text };
        let profile = aws_profile(state, &scope, &env.name)?;
        secrets::fill_aws_secrets(env, false, profile.as_deref(), &state.aws, aws).await?;
    }
    Ok((global, collection))
}

async fn load_keychain_envs(
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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    dir: String,
    path: String,
    request: Request,
) -> ApiResult<ResponseView> {
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root, AwsFetch::Fetch).await?;
    let store = state.session_store(&root);
    let workspace_dir = state.workspace_dir().ok();
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
        session: state.session(&store, workspace_dir.as_deref()),
        cookies: Some(state.cookie_jar(&root)),
        oauth: &state.oauth,
    };
    // Script output goes to the console as it happens (already masked).
    let on_log = |log: http::ScriptLog| {
        if let Err(err) = (ScriptLogged { log }).emit(&app) {
            tracing::warn!(%err, "could not emit script log");
        }
    };
    let result = http::execute(input, cancel, &on_log).await;
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .remove(&request_id);

    record_history(&state, &root, &path, &request, &result);
    log_to_console(&app, &request_id, &path, &request, &result);
    let executed = result?;
    let response = executed.response;
    let id = uuid::Uuid::new_v4().to_string();
    let mut view = ResponseView::new(id.clone(), &response);
    view.scripts = executed.scripts;
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

/// Sends the raw request and response (or the error) to the console panel.
fn log_to_console(
    app: &tauri::AppHandle,
    request_id: &str,
    path: &str,
    request: &Request,
    result: &Result<http::Executed, frida_core::CoreError>,
) {
    let entry = match result {
        Ok(executed) => ConsoleEntry::from_send(
            request_id.to_string(),
            &request.name,
            path,
            &executed.sent,
            Ok(&executed.response),
            &executed.sensitive,
        ),
        // Not sent (invalid URL, auth failed, cancelled, ...): show it as typed, which contains
        // `{{placeholders}}` rather than secret values.
        Err(err) => ConsoleEntry::not_sent(
            request_id.to_string(),
            &request.name,
            path,
            &request.method,
            &request.url,
            err.to_string(),
        ),
    };
    if let Err(err) = (ConsoleLogged { entry }).emit(app) {
        tracing::warn!(%err, "could not emit console entry");
    }
}

/// Writes a history entry in the background, so the response isn't delayed. Cancelled requests
/// aren't recorded. Secret values are masked inside `HistoryStore::add`.
fn record_history(
    state: &AppState,
    root: &Path,
    path: &str,
    request: &Request,
    result: &Result<http::Executed, frida_core::CoreError>,
) {
    let Some(history) = state.history.clone() else {
        return;
    };
    let collection_id = storage::load_collection(root)
        .map(|c| c.id)
        .unwrap_or_default();
    let (entry, sensitive) = match result {
        Ok(executed) => (
            NewEntry::from_send(
                &collection_id,
                path,
                &request.name,
                &executed.sent,
                Ok(&executed.response),
            ),
            executed.sensitive.clone(),
        ),
        Err(frida_core::CoreError::Send(http::SendError::Cancelled)) => return,
        // The request never got as far as being sent: record it as typed (with
        // `{{placeholders}}`, so no secret values) together with the error.
        Err(err) => (
            NewEntry {
                collection_id,
                request_path: path.to_string(),
                request_name: request.name.clone(),
                method: request.method.clone(),
                url: request.url.clone(),
                request_headers: Vec::new(),
                request_body: None,
                response: None,
                error: Some(err.to_string()),
            },
            Vec::new(),
        ),
    };
    let retention = state.local.load_settings().preferences.history_size as usize;
    // Fire and forget: SQLite is blocking I/O, so it runs on the blocking thread pool.
    tokio::task::spawn_blocking(move || {
        if let Err(err) = history.add(&entry, &sensitive, retention) {
            tracing::warn!(%err, "could not write history");
        }
    });
}

/// Resolves the variables of `request` without sending it, for the inspector panel and the
/// editor tooltips (highlighting, `{{` autocompletion). Secret values are masked.
///
/// Like [`fetch_oauth2_tokens`], `path` may also be a folder or `""` (the collection) with
/// `request: None`: then it reports the variables available inside that folder, for the
/// folder and collection settings tabs.
#[tauri::command]
#[specta::specta]
pub async fn inspect_request(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    request: Option<Request>,
) -> ApiResult<ResolveReport> {
    let (request, path) = request_or_stand_in(request, path);
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root, AwsFetch::CacheOnly).await?;
    let store = state.session_store(&root);
    let workspace_dir = state.workspace_dir().ok();
    let report = http::inspect(&ExecuteInput {
        collection_dir: &root,
        request_rel: &path,
        request: &request,
        collection_env: collection_env.as_ref(),
        global_env: global_env.as_ref(),
        session: state.session(&store, workspace_dir.as_deref()),
        cookies: None,
        oauth: &state.oauth,
    })?;
    Ok(report.masked())
}

/// For folders and the collection (`request: None`, `path` = the folder or `""`): a stand-in
/// request inside them, which inherits their auth, headers and vars. Returns the request and
/// its collection-relative path.
fn request_or_stand_in(request: Option<Request>, path: String) -> (Request, String) {
    match request {
        Some(request) => (request, path),
        None => {
            let rel = if path.is_empty() {
                "_.yaml".to_string()
            } else {
                format!("{path}/_.yaml")
            };
            (Request::new("stand-in"), rel)
        }
    }
}

/// Fetches new OAuth2 tokens for the item at `path` ("Get new token"). `path` may be a request,
/// a folder or `""` (the collection); `request` is the editor's state for request tabs. For the
/// authorization code flow this opens the browser; cancel it with `request_id`.
#[tauri::command]
#[specta::specta]
pub async fn fetch_oauth2_tokens(
    state: State<'_, AppState>,
    request_id: String,
    dir: String,
    path: String,
    request: Option<Request>,
) -> ApiResult<Vec<TokenInfo>> {
    let root = state.collection_dir(&dir)?;
    let (global_env, collection_env) = load_active_envs(&state, &root, AwsFetch::Fetch).await?;
    let store = state.session_store(&root);
    let workspace_dir = state.workspace_dir().ok();
    let (request, request_rel) = request_or_stand_in(request, path);
    let cancel = CancellationToken::new();
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .insert(request_id.clone(), cancel.clone());
    let result = http::fetch_tokens(
        ExecuteInput {
            collection_dir: &root,
            request_rel: &request_rel,
            request: &request,
            collection_env: collection_env.as_ref(),
            global_env: global_env.as_ref(),
            session: state.session(&store, workspace_dir.as_deref()),
            cookies: None,
            oauth: &state.oauth,
        },
        cancel,
    )
    .await;
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .remove(&request_id);
    Ok(result?)
}

/// All cached OAuth2 tokens, for the token viewer. Shown only when the user opens the viewer.
#[tauri::command]
#[specta::specta]
pub fn list_oauth2_tokens(state: State<'_, AppState>) -> Vec<TokenInfo> {
    state.oauth.list()
}

/// Forgets one cached token, or all of them when `token` is `None`.
#[tauri::command]
#[specta::specta]
pub fn clear_oauth2_tokens(state: State<'_, AppState>, token: Option<TokenInfo>) {
    state.oauth.clear(token.as_ref());
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
