//! Frida archives (PLAN.md M12): export and import a workspace, collection, folder or request as
//! a `.zip`. Folders named by the UI are checked against the open workspace before use.

use std::path::{Path, PathBuf};

use frida_core::archive::{self, ArchiveSummary, ExportSource, ImportTarget, ImportedArchive};
use frida_core::storage::WorkspaceOverview;
use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::{ApiResult, blocking};
use crate::state::AppState;

/// `source` with its folders replaced by the ones the app knows: the open workspace, or one of
/// its collections. A UI bug (or a compromised page) can't export arbitrary folders this way.
fn checked_source(state: &AppState, source: ExportSource) -> ApiResult<ExportSource> {
    let known = |dir: &str| -> ApiResult<String> {
        Ok(state.collection_dir(dir)?.to_string_lossy().to_string())
    };
    Ok(match source {
        ExportSource::Workspace { .. } => ExportSource::Workspace {
            workspace_dir: state.workspace_dir()?.to_string_lossy().to_string(),
        },
        ExportSource::Collection { collection_dir } => ExportSource::Collection {
            collection_dir: known(&collection_dir)?,
        },
        ExportSource::Folder {
            collection_dir,
            rel,
        } => ExportSource::Folder {
            collection_dir: known(&collection_dir)?,
            rel,
        },
        ExportSource::Request {
            collection_dir,
            rel,
        } => ExportSource::Request {
            collection_dir: known(&collection_dir)?,
            rel,
        },
    })
}

/// Writes `source` as a Frida archive to `target` (chosen in a save dialog).
#[tauri::command]
#[specta::specta]
pub async fn export_archive(
    state: State<'_, AppState>,
    source: ExportSource,
    target: String,
) -> ApiResult<ArchiveSummary> {
    let source = checked_source(&state, source)?;
    blocking(move || Ok(archive::export_archive(&source, Path::new(&target))?)).await
}

/// What the archive at `file` contains, without importing it.
#[tauri::command]
#[specta::specta]
pub async fn preview_archive(file: String) -> ApiResult<ArchiveSummary> {
    blocking(move || Ok(archive::preview_archive(Path::new(&file))?)).await
}

/// The result of an import, plus the refreshed workspace when one is open.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveImported {
    pub imported: ImportedArchive,
    pub overview: Option<WorkspaceOverview>,
}

/// Imports the archive at `file` into `target`. A new workspace (`target.kind == "workspace"`)
/// is not opened here; the UI opens it afterwards.
#[tauri::command]
#[specta::specta]
pub async fn import_archive(
    state: State<'_, AppState>,
    file: String,
    target: ImportTarget,
) -> ApiResult<ArchiveImported> {
    let target = match target {
        ImportTarget::Workspace { dir } => ImportTarget::Workspace { dir },
        ImportTarget::Collection { dir, .. } => ImportTarget::Collection {
            workspace_dir: state.workspace_dir()?.to_string_lossy().to_string(),
            dir,
        },
        ImportTarget::Into {
            collection_dir,
            parent,
        } => ImportTarget::Into {
            collection_dir: state
                .collection_dir(&collection_dir)?
                .to_string_lossy()
                .to_string(),
            parent,
        },
    };
    let refresh = matches!(target, ImportTarget::Collection { .. });
    let mut imported =
        blocking(move || Ok(archive::import_archive(Path::new(&file), &target)?)).await?;
    let overview = if refresh {
        let ws = state.workspace_dir()?;
        // The overview lists canonical folders; return the same form so the UI can select it.
        let dir = PathBuf::from(&imported.dir);
        imported.dir = dir
            .canonicalize()
            .unwrap_or(dir)
            .to_string_lossy()
            .to_string();
        Some(state.refresh_workspace(&ws)?)
    } else {
        None
    };
    Ok(ArchiveImported { imported, overview })
}
