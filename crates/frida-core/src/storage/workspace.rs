//! Workspaces: `frida-workspace.yaml` plus the list of collections it contains.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::collection::{create_collection, load_collection};
use super::paths::{COLLECTION_FILE, ENVIRONMENTS_DIR, EntryKind, WORKSPACE_FILE, unique_name};
use crate::error::{CoreError, Result};
use crate::model::{Collection, CollectionRef, Workspace};
use crate::yaml::{read_yaml, write_yaml};

/// Creates a new workspace in `dir`, including its `environments/` folder for global envs.
pub fn create_workspace(dir: &Path, name: &str) -> Result<Workspace> {
    let file = dir.join(WORKSPACE_FILE);
    if file.exists() {
        return Err(CoreError::AlreadyExists { path: file });
    }
    let workspace = Workspace::new(name);
    write_yaml(&file, &workspace)?;
    let envs = dir.join(ENVIRONMENTS_DIR);
    std::fs::create_dir_all(&envs).map_err(|e| CoreError::io(&envs, e))?;
    Ok(workspace)
}

/// A workspace as the UI shows it: its collections with names, or an error per collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOverview {
    pub dir: String,
    pub name: String,
    pub collections: Vec<CollectionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    /// Absolute folder of the collection.
    pub dir: String,
    /// `None` when `collection.yaml` couldn't be read (see `error`).
    pub id: Option<String>,
    pub name: String,
    /// Why the collection couldn't be loaded, e.g. the folder was moved or deleted.
    pub error: Option<String>,
}

/// Opens a workspace and loads the name and id of each collection. A broken or missing
/// collection is reported in its summary instead of failing the whole workspace.
pub fn load_overview(dir: &Path) -> Result<WorkspaceOverview> {
    let workspace = open_workspace(dir)?;
    let collections = workspace
        .collections
        .iter()
        .map(|entry| {
            let col_dir = collection_dir(dir, entry);
            let shown_dir = col_dir.canonicalize().unwrap_or_else(|_| col_dir.clone());
            match load_collection(&col_dir) {
                Ok(collection) => CollectionSummary {
                    dir: shown_dir.to_string_lossy().to_string(),
                    id: Some(collection.id),
                    name: collection.name,
                    error: None,
                },
                Err(err) => CollectionSummary {
                    dir: shown_dir.to_string_lossy().to_string(),
                    id: None,
                    name: entry.path.clone(),
                    error: Some(err.to_string()),
                },
            }
        })
        .collect();
    Ok(WorkspaceOverview {
        dir: dir.to_string_lossy().to_string(),
        name: workspace.name,
        collections,
    })
}

pub fn open_workspace(dir: &Path) -> Result<Workspace> {
    read_yaml(&dir.join(WORKSPACE_FILE))
}

pub fn save_workspace(dir: &Path, workspace: &Workspace) -> Result<()> {
    write_yaml(&dir.join(WORKSPACE_FILE), workspace)
}

/// The absolute folder of a collection entry (entries may be relative to the workspace).
pub fn collection_dir(workspace_dir: &Path, entry: &CollectionRef) -> PathBuf {
    let path = Path::new(&entry.path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_dir.join(path)
    }
}

/// Adds an existing collection folder to the workspace. Returns the loaded collection.
pub fn add_collection(workspace_dir: &Path, collection_dir: &Path) -> Result<Collection> {
    if !collection_dir.join(COLLECTION_FILE).exists() {
        return Err(CoreError::InvalidInput(format!(
            "{} is not a Frida collection (no {COLLECTION_FILE} found)",
            collection_dir.display()
        )));
    }
    let collection = load_collection(collection_dir)?;
    register(workspace_dir, collection_dir)?;
    Ok(collection)
}

/// Creates a new collection in `collection_dir` and adds it to the workspace.
pub fn create_collection_in_workspace(
    workspace_dir: &Path,
    collection_dir: &Path,
    name: &str,
) -> Result<Collection> {
    let collection = create_collection(collection_dir, name)?;
    register(workspace_dir, collection_dir)?;
    Ok(collection)
}

/// Folder inside the workspace where collections created by name are put.
pub const COLLECTIONS_DIR: &str = "collections";

/// Creates a new collection at `<workspace>/collections/<slug>` (with a unique slug) and adds it
/// to the workspace. Returns the new collection's folder.
pub fn create_named_collection(workspace_dir: &Path, name: &str) -> Result<PathBuf> {
    let dir = new_collection_dir(workspace_dir, name)?;
    create_collection_in_workspace(workspace_dir, &dir, name)?;
    Ok(dir)
}

/// A free folder for a new collection called `name`: `<workspace>/collections/<unique slug>`.
pub fn new_collection_dir(workspace_dir: &Path, name: &str) -> Result<PathBuf> {
    let parent = workspace_dir.join(COLLECTIONS_DIR);
    std::fs::create_dir_all(&parent).map_err(|e| CoreError::io(&parent, e))?;
    Ok(parent.join(unique_name(&parent, name, EntryKind::Folder, None)))
}

/// Removes a collection from the workspace list. The files on disk are left untouched.
pub fn remove_collection(workspace_dir: &Path, collection_dir: &Path) -> Result<()> {
    let mut workspace = open_workspace(workspace_dir)?;
    let before = workspace.collections.len();
    workspace
        .collections
        .retain(|entry| !same_dir(&self::collection_dir(workspace_dir, entry), collection_dir));
    if workspace.collections.len() == before {
        return Err(CoreError::NotFound {
            path: collection_dir.to_path_buf(),
        });
    }
    save_workspace(workspace_dir, &workspace)
}

fn register(workspace_dir: &Path, collection_dir: &Path) -> Result<()> {
    let mut workspace = open_workspace(workspace_dir)?;
    let already_listed = workspace
        .collections
        .iter()
        .any(|entry| same_dir(&self::collection_dir(workspace_dir, entry), collection_dir));
    if already_listed {
        return Ok(());
    }
    workspace.collections.push(CollectionRef {
        path: workspace_relative(workspace_dir, collection_dir),
    });
    save_workspace(workspace_dir, &workspace)
}

/// `./collections/orders` when the collection lives inside the workspace folder, otherwise the
/// absolute path. Relative paths keep a workspace that is shared through git portable.
fn workspace_relative(workspace_dir: &Path, collection_dir: &Path) -> String {
    match collection_dir.strip_prefix(workspace_dir) {
        Ok(rel) => {
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            format!("./{}", parts.join("/"))
        }
        Err(_) => collection_dir.to_string_lossy().to_string(),
    }
}

/// Compares two folders, ignoring differences such as `a/./b` vs `a/b` or symlinks.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
