//! Reading and changing a collection on disk: the tree, requests, folders and their order.
//!
//! Every function takes the collection root folder plus collection-relative paths with `/`
//! (e.g. `"orders/get-order.yaml"`). That is also how the UI refers to items.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::paths::{
    COLLECTION_FILE, ENVIRONMENTS_DIR, EntryKind, FOLDER_FILE, YAML_EXT, rel_join, rel_parent,
    resolve, to_rel, unique_name,
};
use crate::error::{CoreError, Result};
use crate::model::{Collection, Folder, Request};
use crate::yaml::{read_yaml, write_yaml};

/// One entry in the sidebar tree.
///
/// `#[serde(tag = "kind")]` adds a `kind: "folder" | "request" | "invalid"` field, which becomes a
/// TypeScript discriminated union the UI can `switch` on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TreeNode {
    Folder {
        path: String,
        name: String,
        seq: u32,
        children: Vec<TreeNode>,
    },
    Request {
        path: String,
        name: String,
        seq: u32,
        method: String,
    },
    /// A file that couldn't be read. Shown with its error instead of hiding it or failing the
    /// whole tree, so one broken file (e.g. a git merge conflict) doesn't lock the user out.
    Invalid { path: String, error: String },
}

impl TreeNode {
    pub fn path(&self) -> &str {
        match self {
            TreeNode::Folder { path, .. }
            | TreeNode::Request { path, .. }
            | TreeNode::Invalid { path, .. } => path,
        }
    }

    /// Sort key: `seq` first, then name. Invalid files go last.
    fn sort_key(&self) -> (u32, String) {
        match self {
            TreeNode::Folder { seq, name, .. } | TreeNode::Request { seq, name, .. } => {
                (*seq, name.to_lowercase())
            }
            TreeNode::Invalid { path, .. } => (u32::MAX, path.clone()),
        }
    }
}

/// Creates a new collection in `dir` (which may already exist, e.g. an existing git repo).
pub fn create_collection(dir: &Path, name: &str) -> Result<Collection> {
    let file = dir.join(COLLECTION_FILE);
    if file.exists() {
        return Err(CoreError::AlreadyExists { path: file });
    }
    let collection = Collection::new(name);
    write_yaml(&file, &collection)?;
    let envs = dir.join(ENVIRONMENTS_DIR);
    std::fs::create_dir_all(&envs).map_err(|e| CoreError::io(&envs, e))?;
    Ok(collection)
}

pub fn load_collection(dir: &Path) -> Result<Collection> {
    read_yaml(&dir.join(COLLECTION_FILE))
}

pub fn save_collection(dir: &Path, collection: &Collection) -> Result<()> {
    write_yaml(&dir.join(COLLECTION_FILE), collection)
}

/// Reads the whole request/folder tree of a collection, sorted by `seq`.
pub fn scan_tree(root: &Path) -> Result<Vec<TreeNode>> {
    scan_dir(root, root)
}

fn scan_dir(root: &Path, dir: &Path) -> Result<Vec<TreeNode>> {
    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut nodes = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(dir, e))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden entries such as `.git` and editor temp files.
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            if dir == root && file_name == ENVIRONMENTS_DIR {
                continue;
            }
            // Only directories with a folder.yaml belong to the collection.
            let folder_file = path.join(FOLDER_FILE);
            if !folder_file.exists() {
                continue;
            }
            let rel = to_rel(root, &path);
            match read_yaml::<Folder>(&folder_file) {
                Ok(folder) => nodes.push(TreeNode::Folder {
                    path: rel,
                    name: folder.name,
                    seq: folder.seq,
                    children: scan_dir(root, &path)?,
                }),
                Err(err) => nodes.push(TreeNode::Invalid {
                    path: rel,
                    error: err.to_string(),
                }),
            }
        } else if is_request_file(&path, &file_name) {
            let rel = to_rel(root, &path);
            match read_yaml::<Request>(&path) {
                Ok(request) => nodes.push(TreeNode::Request {
                    path: rel,
                    name: request.name,
                    seq: request.seq,
                    method: request.method,
                }),
                Err(err) => nodes.push(TreeNode::Invalid {
                    path: rel,
                    error: err.to_string(),
                }),
            }
        }
    }

    nodes.sort_by_key(TreeNode::sort_key);
    Ok(nodes)
}

fn is_request_file(path: &Path, file_name: &str) -> bool {
    path.extension().is_some_and(|ext| ext == YAML_EXT)
        && file_name != COLLECTION_FILE
        && file_name != FOLDER_FILE
}

pub fn read_request(root: &Path, rel: &str) -> Result<Request> {
    let path = existing(root, rel)?;
    read_yaml(&path)
}

/// Saves a request to its existing file. Renaming is a separate operation ([`rename_item`]),
/// so saving never moves files around under the user's feet.
pub fn save_request(root: &Path, rel: &str, request: &Request) -> Result<()> {
    let path = resolve(root, rel)?;
    let mut request = request.clone();
    request.method = request.method.trim().to_uppercase();
    write_yaml(&path, &request)
}

/// The folders a request (or folder) at `rel` inherits from, outermost first.
/// `orders/eu/get.yaml` → `[orders, orders/eu]`.
pub fn load_folder_chain(root: &Path, rel: &str) -> Result<Vec<Folder>> {
    let mut chain = Vec::new();
    let mut current = String::new();
    let parent = rel_parent(rel);
    if parent.is_empty() {
        return Ok(chain);
    }
    for part in parent.split('/') {
        current = rel_join(&current, part);
        chain.push(read_folder(root, &current)?);
    }
    Ok(chain)
}

pub fn read_folder(root: &Path, rel: &str) -> Result<Folder> {
    read_yaml(&existing(root, rel)?.join(FOLDER_FILE))
}

pub fn save_folder(root: &Path, rel: &str, folder: &Folder) -> Result<()> {
    write_yaml(&resolve(root, rel)?.join(FOLDER_FILE), folder)
}

/// Creates a new request in the folder `parent` (`""` = collection root), placed last.
/// Returns the new request's relative path.
pub fn create_request(root: &Path, parent: &str, request: &Request) -> Result<String> {
    let dir = existing(root, parent)?;
    let file_name = unique_name(&dir, &request.name, EntryKind::Request, None);
    let mut request = request.clone();
    request.seq = next_seq(root, parent)?;
    let rel = rel_join(parent, &file_name);
    save_request(root, &rel, &request)?;
    Ok(rel)
}

/// Creates a new folder in `parent`, placed last. Returns its relative path.
pub fn create_folder(root: &Path, parent: &str, name: &str) -> Result<String> {
    let dir = existing(root, parent)?;
    let dir_name = unique_name(&dir, name, EntryKind::Folder, None);
    let mut folder = Folder::new(name);
    folder.seq = next_seq(root, parent)?;
    let rel = rel_join(parent, &dir_name);
    save_folder(root, &rel, &folder)?;
    Ok(rel)
}

/// Renames a request or folder: updates its display name and renames the file or directory
/// to the matching slug. Returns the new relative path.
pub fn rename_item(root: &Path, rel: &str, new_name: &str) -> Result<String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(CoreError::InvalidInput(
            "the name must not be empty".to_string(),
        ));
    }
    let path = existing(root, rel)?;
    let parent = rel_parent(rel);
    let parent_dir = resolve(root, parent)?;

    let new_rel = if path.is_dir() {
        let mut folder = read_folder(root, rel)?;
        folder.name = new_name.to_string();
        save_folder(root, rel, &folder)?;
        let dir_name = unique_name(&parent_dir, new_name, EntryKind::Folder, Some(&path));
        rel_join(parent, &dir_name)
    } else {
        let mut request = read_request(root, rel)?;
        request.name = new_name.to_string();
        save_request(root, rel, &request)?;
        let file_name = unique_name(&parent_dir, new_name, EntryKind::Request, Some(&path));
        rel_join(parent, &file_name)
    };

    if new_rel != rel {
        let new_path = resolve(root, &new_rel)?;
        std::fs::rename(&path, &new_path).map_err(|e| CoreError::io(&path, e))?;
    }
    Ok(new_rel)
}

/// Copies a request next to the original as "<name> (copy)", placed directly after it.
pub fn duplicate_request(root: &Path, rel: &str) -> Result<String> {
    let mut request = read_request(root, rel)?;
    request.name = format!("{} (copy)", request.name);
    let parent = rel_parent(rel);
    let new_rel = create_request(root, parent, &request)?;

    // Put the copy right after the original instead of at the end.
    let mut order: Vec<String> = child_paths(root, parent)?
        .into_iter()
        .filter(|p| *p != new_rel)
        .collect();
    let original_index = order
        .iter()
        .position(|p| p == rel)
        .unwrap_or(order.len() - 1);
    order.insert(original_index + 1, new_rel.clone());
    reorder(root, parent, &order)?;
    Ok(new_rel)
}

/// Deletes a request file, or a folder with everything in it.
pub fn delete_item(root: &Path, rel: &str) -> Result<()> {
    if rel.is_empty() {
        return Err(CoreError::InvalidInput(
            "the collection root can't be deleted from here".to_string(),
        ));
    }
    let path = existing(root, rel)?;
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| CoreError::io(&path, e))
    } else {
        std::fs::remove_file(&path).map_err(|e| CoreError::io(&path, e))
    }
}

/// Moves a request or folder into `new_parent` at position `index` (0 = first) and rewrites
/// the `seq` of all siblings there. Also used to reorder within the same folder.
/// Returns the item's new relative path.
pub fn move_item(root: &Path, rel: &str, new_parent: &str, index: usize) -> Result<String> {
    let path = existing(root, rel)?;
    let target_dir = existing(root, new_parent)?;
    if !target_dir.is_dir() || (!new_parent.is_empty() && !target_dir.join(FOLDER_FILE).exists()) {
        return Err(CoreError::InvalidInput(format!(
            "{new_parent:?} is not a folder"
        )));
    }
    // A folder can't be moved into itself or one of its own subfolders.
    if path.is_dir() && (new_parent == rel || new_parent.starts_with(&format!("{rel}/"))) {
        return Err(CoreError::InvalidInput(
            "a folder can't be moved into itself".to_string(),
        ));
    }

    let mut new_rel = rel.to_string();
    if rel_parent(rel) != new_parent {
        let (kind, name) = if path.is_dir() {
            (EntryKind::Folder, read_folder(root, rel)?.name)
        } else {
            (EntryKind::Request, read_request(root, rel)?.name)
        };
        new_rel = rel_join(new_parent, &unique_name(&target_dir, &name, kind, None));
        let new_path = resolve(root, &new_rel)?;
        std::fs::rename(&path, &new_path).map_err(|e| CoreError::io(&path, e))?;
    }

    let mut order: Vec<String> = child_paths(root, new_parent)?
        .into_iter()
        .filter(|p| *p != new_rel)
        .collect();
    order.insert(index.min(order.len()), new_rel.clone());
    reorder(root, new_parent, &order)?;
    Ok(new_rel)
}

/// Rewrites `seq` (1, 2, 3, ...) for the items of folder `parent` in the given order.
/// Only files whose `seq` actually changes are written, to keep git diffs small.
pub fn reorder(root: &Path, parent: &str, ordered: &[String]) -> Result<()> {
    for (i, rel) in ordered.iter().enumerate() {
        if rel_parent(rel) != parent {
            return Err(CoreError::InvalidInput(format!(
                "{rel} is not inside {parent:?}"
            )));
        }
        let seq = (i + 1) as u32;
        let path = existing(root, rel)?;
        if path.is_dir() {
            let mut folder = read_folder(root, rel)?;
            if folder.seq != seq {
                folder.seq = seq;
                save_folder(root, rel, &folder)?;
            }
        } else {
            let mut request = read_request(root, rel)?;
            if request.seq != seq {
                request.seq = seq;
                save_request(root, rel, &request)?;
            }
        }
    }
    Ok(())
}

/// Relative paths of the valid children of `parent`, in display order.
fn child_paths(root: &Path, parent: &str) -> Result<Vec<String>> {
    let dir = existing(root, parent)?;
    Ok(scan_dir(root, &dir)?
        .into_iter()
        .filter(|node| !matches!(node, TreeNode::Invalid { .. }))
        .map(|node| node.path().to_string())
        .collect())
}

fn next_seq(root: &Path, parent: &str) -> Result<u32> {
    let dir = existing(root, parent)?;
    let max = scan_dir(root, &dir)?
        .iter()
        .filter_map(|node| match node {
            TreeNode::Folder { seq, .. } | TreeNode::Request { seq, .. } => Some(*seq),
            TreeNode::Invalid { .. } => None,
        })
        .max()
        .unwrap_or(0);
    Ok(max + 1)
}

/// Resolves `rel` and checks that it exists.
fn existing(root: &Path, rel: &str) -> Result<PathBuf> {
    let path = resolve(root, rel)?;
    if path.exists() {
        Ok(path)
    } else {
        Err(CoreError::NotFound { path })
    }
}
