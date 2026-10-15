//! Workspaces: `frida-workspace.yaml` plus the list of collections it contains.

use std::path::{Path, PathBuf};

use super::collection::{create_collection, load_collection};
use super::paths::{COLLECTION_FILE, ENVIRONMENTS_DIR, WORKSPACE_FILE};
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
