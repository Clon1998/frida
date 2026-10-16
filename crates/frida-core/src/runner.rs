//! The collection runner (PLAN.md M11): sends the saved requests of a collection or folder one
//! after another and reports a result per request.
//!
//! Every request goes through the normal send pipeline ([`http::execute_as`]), in runner mode,
//! with one shared session: variables a login request sets are there for the next request.
//! Scripts steer the run like in Bruno: `fri.runner.skipRequest()`, `fri.setNextRequest(name)`
//! and `fri.runner.stopExecution()` (see [`http::RunnerFlow`]).
//!
//! The app and (later) the CLI use the same code: they only differ in what they do with each
//! [`RunStep`].

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio_util::sync::CancellationToken;

use crate::auth::OAuth2Manager;
use crate::error::{CoreError, Result};
use crate::http::{
    self, CookieJar, ExecuteInput, Executed, ExecutionMode, RunnerFlow, ScriptLog, Session,
};
use crate::model::{Environment, Request};
use crate::storage::{self, TreeNode};

/// A run ends after this many sends per request in it, so `setNextRequest` loops can't run
/// forever.
pub const MAX_VISITS_PER_REQUEST: usize = 10;

/// One request of a run, in run order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunItem {
    /// Collection-relative path of the request file.
    pub path: String,
    pub name: String,
    pub method: String,
}

/// The requests of `folder` (`""` for the whole collection) in run order: the tree order of the
/// sidebar, folders depth-first, sorted by `seq`. Files that can't be read are left out (they
/// show as broken in the sidebar).
pub fn plan_run(collection_dir: &Path, folder: &str) -> Result<Vec<RunItem>> {
    let tree = storage::scan_tree(collection_dir)?;
    let mut items = Vec::new();
    if folder.is_empty() {
        collect(&tree, &mut items);
    } else {
        let Some(children) = find_folder(&tree, folder) else {
            return Err(CoreError::NotFound {
                path: collection_dir.join(folder),
            });
        };
        collect(children, &mut items);
    }
    Ok(items)
}

fn find_folder<'a>(nodes: &'a [TreeNode], path: &str) -> Option<&'a [TreeNode]> {
    for node in nodes {
        if let TreeNode::Folder {
            path: folder_path,
            children,
            ..
        } = node
        {
            if folder_path == path {
                return Some(children);
            }
            if let Some(found) = find_folder(children, path) {
                return Some(found);
            }
        }
    }
    None
}

fn collect(nodes: &[TreeNode], items: &mut Vec<RunItem>) {
    for node in nodes {
        match node {
            TreeNode::Folder { children, .. } => collect(children, items),
            TreeNode::Request {
                path, name, method, ..
            } => items.push(RunItem {
                path: path.clone(),
                name: name.clone(),
                method: method.clone(),
            }),
            TreeNode::Invalid { .. } => {}
        }
    }
}

/// Settings of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    /// Pause between two requests, in milliseconds.
    pub delay_ms: u32,
}

/// How a request of the run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunOutcome {
    /// Sent, and every test passed (or there were none).
    Passed,
    /// Sent, but a test or a post-response script failed.
    Failed,
    /// Not sent: the request couldn't be loaded, prepared or sent, or a pre-request script failed.
    Error,
    /// `fri.runner.skipRequest()` in a pre-request script.
    Skipped,
}

/// The result of one request of the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    /// Counts sends in this run, from 0 (a request sent twice by `setNextRequest` has two).
    pub step: u32,
    pub item: RunItem,
    pub outcome: RunOutcome,
    pub status: Option<u16>,
    pub time_ms: Option<u32>,
    pub tests_passed: u32,
    pub tests_total: u32,
    /// Why it was not sent (`Error`), or the first failing test or script (`Failed`).
    pub message: Option<String>,
}

/// Why a run ended before the last request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunStop {
    /// The user stopped it.
    Cancelled,
    /// `fri.runner.stopExecution()` or `fri.setNextRequest(null)`.
    Script,
    /// `fri.setNextRequest(name)` named a request that isn't in this run.
    UnknownNextRequest { name: String },
    /// Too many sends ([`MAX_VISITS_PER_REQUEST`]): probably a `setNextRequest` loop.
    LoopGuard,
}

/// Totals of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub passed: u32,
    pub failed: u32,
    pub errors: u32,
    pub skipped: u32,
    /// `u32` (not `u64`): TypeScript numbers can't hold every `u64`; 49 days is plenty.
    pub duration_ms: u32,
    /// `None` when the run went through to the end.
    pub stopped: Option<RunStop>,
}

/// Everything a run needs besides its requests: like [`ExecuteInput`], without the request.
pub struct RunInput<'a> {
    pub collection_dir: &'a Path,
    pub collection_env: Option<&'a Environment>,
    pub global_env: Option<&'a Environment>,
    pub session: Session<'a>,
    pub cookies: Option<Arc<CookieJar>>,
    pub oauth: &'a OAuth2Manager,
}

/// One finished request: its result, and what the send produced (for response viewers,
/// consoles and history). `request` is the request as loaded from disk, when it could be.
pub struct RunStep {
    pub result: RunResult,
    pub request: Option<Request>,
    pub executed: std::result::Result<Executed, CoreError>,
}

/// Runs `items` in order and calls `on_step` after each request. Returns the totals.
///
/// `on_step` is `FnMut` (it may change things, e.g. collect results) and `Send`, because the
/// future of this async fn moves between threads and holds the closure across `.await`s.
pub async fn run(
    input: RunInput<'_>,
    items: &[RunItem],
    options: &RunOptions,
    cancel: CancellationToken,
    on_log: &(dyn Fn(ScriptLog) + Send + Sync),
    on_step: &mut (dyn FnMut(RunStep) + Send),
) -> RunSummary {
    let started = Instant::now();
    let mut summary = RunSummary::default();
    let max_steps = items.len() * MAX_VISITS_PER_REQUEST;
    let mut index = 0;
    let mut step = 0;

    while index < items.len() {
        if cancel.is_cancelled() {
            summary.stopped = Some(RunStop::Cancelled);
            break;
        }
        if step >= max_steps {
            summary.stopped = Some(RunStop::LoopGuard);
            break;
        }
        if step > 0 && options.delay_ms > 0 {
            // Wait, but stop waiting at once when the run is cancelled.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(options.delay_ms.into())) => {}
                _ = cancel.cancelled() => {
                    summary.stopped = Some(RunStop::Cancelled);
                    break;
                }
            }
        }

        let item = &items[index];
        let (request, executed) = send_one(&input, item, &cancel, on_log).await;
        let (result, flow) = to_result(step as u32, item, &executed);
        match result.outcome {
            RunOutcome::Passed => summary.passed += 1,
            RunOutcome::Failed => summary.failed += 1,
            RunOutcome::Error => summary.errors += 1,
            RunOutcome::Skipped => summary.skipped += 1,
        }
        on_step(RunStep {
            result,
            request,
            executed,
        });
        step += 1;

        if cancel.is_cancelled() {
            summary.stopped = Some(RunStop::Cancelled);
            break;
        }
        if flow.stop {
            summary.stopped = Some(RunStop::Script);
            break;
        }
        index = match flow.next_request {
            None => index + 1,
            Some(name) => match items.iter().position(|i| i.name == name) {
                Some(next) => next,
                None => {
                    summary.stopped = Some(RunStop::UnknownNextRequest { name });
                    break;
                }
            },
        };
    }
    summary.duration_ms = millis(started.elapsed());
    summary
}

/// A duration in whole milliseconds, capped at `u32::MAX`.
fn millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

/// Loads the saved request and sends it in runner mode.
async fn send_one(
    input: &RunInput<'_>,
    item: &RunItem,
    cancel: &CancellationToken,
    on_log: &(dyn Fn(ScriptLog) + Send + Sync),
) -> (Option<Request>, std::result::Result<Executed, CoreError>) {
    let request = match storage::read_request(input.collection_dir, &item.path) {
        Ok(request) => request,
        Err(err) => return (None, Err(err)),
    };
    let execute_input = ExecuteInput {
        collection_dir: input.collection_dir,
        request_rel: &item.path,
        request: &request,
        collection_env: input.collection_env,
        global_env: input.global_env,
        session: input.session,
        cookies: input.cookies.clone(),
        oauth: input.oauth,
    };
    // A child token: cancelling the run cancels the request in flight too.
    let executed = http::execute_as(
        execute_input,
        ExecutionMode::Runner,
        cancel.child_token(),
        on_log,
    )
    .await;
    (Some(request), executed)
}

/// Turns what a send produced into a result, plus what scripts asked of the run.
fn to_result(
    step: u32,
    item: &RunItem,
    executed: &std::result::Result<Executed, CoreError>,
) -> (RunResult, RunnerFlow) {
    let mut result = RunResult {
        step,
        item: item.clone(),
        outcome: RunOutcome::Passed,
        status: None,
        time_ms: None,
        tests_passed: 0,
        tests_total: 0,
        message: None,
    };
    let flow = match executed {
        Ok(executed) => {
            let tests = &executed.scripts.tests;
            result.status = Some(executed.response.status);
            result.time_ms = Some(millis(executed.response.elapsed));
            result.tests_total = tests.len() as u32;
            result.tests_passed = tests.iter().filter(|t| t.passed).count() as u32;
            let failed_test = tests.iter().find(|t| !t.passed);
            let script_error = executed.scripts.errors.first();
            if failed_test.is_some() || script_error.is_some() {
                result.outcome = RunOutcome::Failed;
                result.message = match (failed_test, script_error) {
                    (Some(test), _) => Some(match &test.error {
                        Some(error) => format!("{}: {error}", test.name),
                        None => test.name.clone(),
                    }),
                    (None, Some(error)) => Some(error.to_string()),
                    (None, None) => None,
                };
            }
            executed.flow.clone()
        }
        Err(CoreError::Skipped { stop }) => {
            result.outcome = RunOutcome::Skipped;
            RunnerFlow {
                next_request: None,
                stop: *stop,
            }
        }
        Err(err) => {
            result.outcome = RunOutcome::Error;
            result.message = Some(err.to_string());
            RunnerFlow::default()
        }
    };
    (result, flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, yaml: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, yaml).unwrap();
    }

    fn request(name: &str, seq: u32) -> String {
        format!("version: 1\nname: {name}\nseq: {seq}\nmethod: GET\nurl: http://x\n")
    }

    #[test]
    fn plans_follow_the_tree_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "collection.yaml", "version: 1\nid: c\nname: C\n");
        write(root, "b.yaml", &request("B", 2));
        write(root, "a.yaml", &request("A", 1));
        write(
            root,
            "orders/folder.yaml",
            "version: 1\nname: Orders\nseq: 3\n",
        );
        write(root, "orders/list.yaml", &request("List", 2));
        write(root, "orders/create.yaml", &request("Create", 1));
        write(root, "broken.yaml", "not: [valid");

        let names = |folder: &str| -> Vec<String> {
            plan_run(root, folder)
                .unwrap()
                .into_iter()
                .map(|i| i.name)
                .collect()
        };
        assert_eq!(names(""), ["A", "B", "Create", "List"]);
        assert_eq!(names("orders"), ["Create", "List"]);
        assert!(plan_run(root, "nope").is_err());
    }
}
