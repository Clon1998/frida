//! The git panel (PLAN.md §12): thin commands over `frida-git`, one repository per collection.
//! Git runs as a child process, so every command waits for it on a blocking thread.

use std::path::PathBuf;

use frida_git::{Branch, Repo, Status};
use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::{ApiResult, blocking};
use crate::state::AppState;

/// The git state of a collection folder, for the panel and the status bar.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GitInfo {
    /// `git` isn't installed.
    NoGit,
    /// The collection isn't in a repository (the panel offers `git init`).
    NoRepo,
    Repo {
        status: Status,
    },
}

/// Runs `work` with the repository of collection `dir`, off the async runtime.
async fn with_repo<T, F>(state: &AppState, dir: &str, work: F) -> ApiResult<T>
where
    F: FnOnce(&Repo) -> frida_git::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let dir: PathBuf = state.collection_dir(dir)?;
    blocking(move || {
        let repo = Repo::open(&dir)?;
        Ok(work(&repo)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn git_info(state: State<'_, AppState>, dir: String) -> ApiResult<GitInfo> {
    let dir = state.collection_dir(&dir)?;
    blocking(move || {
        if frida_git::version().is_err() {
            return Ok(GitInfo::NoGit);
        }
        match Repo::open(&dir) {
            Ok(repo) => Ok(GitInfo::Repo {
                status: repo.status()?,
            }),
            Err(frida_git::GitError::NotARepo(_)) => Ok(GitInfo::NoRepo),
            Err(err) => Err(err.into()),
        }
    })
    .await
}

/// `git init` in the collection folder.
#[tauri::command]
#[specta::specta]
pub async fn git_init(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    let dir = state.collection_dir(&dir)?;
    blocking(move || Ok(frida_git::init(&dir).map(drop)?)).await
}

/// Stages `paths` (relative to the collection), or everything when `paths` is empty.
#[tauri::command]
#[specta::specta]
pub async fn git_stage(
    state: State<'_, AppState>,
    dir: String,
    paths: Vec<String>,
) -> ApiResult<()> {
    with_repo(&state, &dir, move |repo| {
        if paths.is_empty() {
            repo.stage_all()
        } else {
            repo.stage(&paths)
        }
    })
    .await
}

/// Unstages `paths`, or everything when `paths` is empty. Files on disk are not touched.
#[tauri::command]
#[specta::specta]
pub async fn git_unstage(
    state: State<'_, AppState>,
    dir: String,
    paths: Vec<String>,
) -> ApiResult<()> {
    with_repo(&state, &dir, move |repo| {
        if paths.is_empty() {
            repo.unstage_all()
        } else {
            repo.unstage(&paths)
        }
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn git_diff(
    state: State<'_, AppState>,
    dir: String,
    path: String,
    staged: bool,
) -> ApiResult<String> {
    with_repo(&state, &dir, move |repo| repo.diff(&path, staged)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_commit(state: State<'_, AppState>, dir: String, message: String) -> ApiResult<()> {
    with_repo(&state, &dir, move |repo| repo.commit(&message)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_fetch(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    with_repo(&state, &dir, |repo| repo.fetch()).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_pull(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    with_repo(&state, &dir, |repo| repo.pull()).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_push(state: State<'_, AppState>, dir: String) -> ApiResult<()> {
    with_repo(&state, &dir, |repo| repo.push()).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_branches(state: State<'_, AppState>, dir: String) -> ApiResult<Vec<Branch>> {
    with_repo(&state, &dir, |repo| repo.branches()).await
}

/// Switches branches; refused while there are uncommitted changes.
#[tauri::command]
#[specta::specta]
pub async fn git_switch(state: State<'_, AppState>, dir: String, branch: String) -> ApiResult<()> {
    with_repo(&state, &dir, move |repo| repo.switch(&branch)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_create_branch(
    state: State<'_, AppState>,
    dir: String,
    name: String,
) -> ApiResult<()> {
    with_repo(&state, &dir, move |repo| repo.create_branch(&name)).await
}
