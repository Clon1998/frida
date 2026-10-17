//! The collection runner (PLAN.md M11): thin glue around `frida_core::runner`.

use frida_core::http::{self, ResponseView};
use frida_core::runner::{self, RunInput, RunItem, RunOptions, RunResult, RunSummary};
use frida_core::secrets::AwsFetch;
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_specta::Event;
use tokio_util::sync::CancellationToken;

use super::http::{load_active_envs, log_to_console, proxy_for};
use crate::ScriptLogged;
use crate::error::ApiResult;
use crate::state::AppState;

/// Emitted after each request of a run, while the run goes on.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    /// The id the UI chose when it started the run.
    pub run_id: String,
    pub result: RunResult,
    /// The response, when the request was sent (its body is in the response cache too, for
    /// saving and filtering, until newer responses push it out).
    pub response: Option<ResponseView>,
}

/// The requests a run of `folder` (`""`: the whole collection) would send, in order.
#[tauri::command]
#[specta::specta]
pub fn plan_run(
    state: State<'_, AppState>,
    dir: String,
    folder: String,
) -> ApiResult<Vec<RunItem>> {
    let root = state.collection_dir(&dir)?;
    Ok(runner::plan_run(&root, &folder)?)
}

/// Runs the saved requests of `folder` with the active environments. Progress arrives as
/// [`RunProgress`] events; stop the run with `cancel_request(run_id)`. Returns the totals.
#[tauri::command]
#[specta::specta]
pub async fn run_collection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    dir: String,
    folder: String,
    options: RunOptions,
) -> ApiResult<RunSummary> {
    let root = state.collection_dir(&dir)?;
    let items = runner::plan_run(&root, &folder)?;
    let (global_env, collection_env) = load_active_envs(&state, &root, AwsFetch::Fetch).await?;
    let store = state.session_store(&root);
    let proxy = proxy_for(&state, &root).await?;
    let workspace_dir = state.workspace_dir().ok();
    let cancel = CancellationToken::new();
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .insert(run_id.clone(), cancel.clone());

    let on_log = |log: http::ScriptLog| {
        if let Err(err) = (ScriptLogged { log }).emit(&app) {
            tracing::warn!(%err, "could not emit script log");
        }
    };
    // Called after each request: console entry, response cache, progress event. Runs don't go
    // into the history (a run of 50 requests would push everything else out).
    let mut on_step = |step: runner::RunStep| {
        let console_id = uuid::Uuid::new_v4().to_string();
        if let Some(request) = &step.request {
            log_to_console(
                &app,
                &console_id,
                &step.result.item.path,
                request,
                &step.executed,
            );
        }
        let response = step.executed.ok().map(|executed| {
            let id = uuid::Uuid::new_v4().to_string();
            let mut view = ResponseView::new(id.clone(), &executed.response);
            view.scripts = executed.scripts;
            state
                .responses
                .lock()
                .expect("responses lock poisoned")
                .insert(id, executed.response.body);
            view
        });
        let progress = RunProgress {
            run_id: run_id.clone(),
            result: step.result,
            response,
        };
        if let Err(err) = progress.emit(&app) {
            tracing::warn!(%err, "could not emit run progress");
        }
    };

    let summary = runner::run(
        RunInput {
            collection_dir: &root,
            collection_env: collection_env.as_ref(),
            global_env: global_env.as_ref(),
            session: state.session(&store, workspace_dir.as_deref(), &proxy),
            cookies: Some(state.cookie_jar(&root)),
            oauth: &state.oauth,
        },
        &items,
        &options,
        cancel,
        &on_log,
        &mut on_step,
    )
    .await;
    state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .remove(&run_id);
    tracing::info!(
        passed = summary.passed,
        failed = summary.failed,
        errors = summary.errors,
        "run finished"
    );
    Ok(summary)
}
