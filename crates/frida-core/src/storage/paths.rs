//! Path helpers: slugs, safe relative paths and unique file names.

use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, Result};

pub const WORKSPACE_FILE: &str = "frida-workspace.yaml";
pub const COLLECTION_FILE: &str = "collection.yaml";
pub const FOLDER_FILE: &str = "folder.yaml";
pub const ENVIRONMENTS_DIR: &str = "environments";
pub const YAML_EXT: &str = "yaml";

/// Turns a display name into a file-name-friendly slug: `"Get Order (v2)"` → `"get-order-v2"`.
///
/// German umlauts are transliterated (`ä` → `ae`) so names stay readable. Anything that isn't
/// an ASCII letter or digit becomes a `-`, and runs of dashes are collapsed.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        match c {
            'a'..='z' | '0'..='9' => slug.push(c),
            'ä' => slug.push_str("ae"),
            'ö' => slug.push_str("oe"),
            'ü' => slug.push_str("ue"),
            'ß' => slug.push_str("ss"),
            _ => {
                if !slug.is_empty() && !slug.ends_with('-') {
                    slug.push('-');
                }
            }
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Joins a collection-relative path (as sent by the UI, always with `/`) onto `root`.
///
/// Rejects absolute paths and `..`, so a path coming from the UI can never point outside the
/// collection folder. An empty string means the collection root itself.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    // `components()` splits a path into parts. Only plain names ("Normal") and "." are allowed.
    let is_safe = rel_path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !is_safe {
        return Err(CoreError::InvalidInput(format!(
            "path {rel:?} must stay inside the collection"
        )));
    }
    Ok(root.join(rel_path))
}

/// The inverse of [`resolve`]: `root/orders/get.yaml` → `"orders/get.yaml"`, always with `/`
/// (also on Windows), so the UI sees the same paths on every OS.
pub fn to_rel(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Parent of a relative path: `"orders/get.yaml"` → `"orders"`, `"get.yaml"` → `""`.
pub fn rel_parent(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("")
}

/// Joins a parent and a child name into a relative path, handling the empty root.
pub fn rel_join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// What kind of entry [`unique_name`] creates. Folders have no extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Request,
    Folder,
}

/// Picks a file or folder name in `dir` based on `display_name` that isn't taken yet:
/// `get-order.yaml`, then `get-order-2.yaml`, `get-order-3.yaml`, ...
///
/// Names Frida itself uses (`collection.yaml`, `folder.yaml`, `environments/`) count as taken.
/// `ignore` is the entry being renamed, which may keep its own name.
pub fn unique_name(
    dir: &Path,
    display_name: &str,
    kind: EntryKind,
    ignore: Option<&Path>,
) -> String {
    let mut base = slugify(display_name);
    if base.is_empty() {
        base = match kind {
            EntryKind::Request => "request".to_string(),
            EntryKind::Folder => "folder".to_string(),
        };
    }

    let file_name = |n: u32| -> String {
        let stem = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        match kind {
            EntryKind::Request => format!("{stem}.{YAML_EXT}"),
            EntryKind::Folder => stem,
        }
    };

    let is_reserved = |name: &str| {
        name == COLLECTION_FILE
            || name == FOLDER_FILE
            || name == ENVIRONMENTS_DIR
            || name == WORKSPACE_FILE
    };

    // Counts 1, 2, 3, ... until a free name is found. `1..` is an endless range, and `find`
    // stops at the first match, so this loop always terminates.
    (1..)
        .map(file_name)
        .find(|name| {
            let candidate = dir.join(name);
            let taken = candidate.exists() && Some(candidate.as_path()) != ignore;
            !taken && !is_reserved(name)
        })
        .expect("an endless range always yields a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_spaces_punctuation_and_umlauts() {
        assert_eq!(slugify("Get Order (v2)"), "get-order-v2");
        assert_eq!(slugify("  Größe ändern!  "), "groesse-aendern");
        assert_eq!(slugify("a__b--c"), "a-b-c");
        assert_eq!(slugify("🚀"), "");
    }

    #[test]
    fn resolve_rejects_escaping_paths() {
        let root = Path::new("/col");
        assert!(resolve(root, "orders/get.yaml").is_ok());
        assert!(resolve(root, "").is_ok());
        assert!(resolve(root, "../secret.yaml").is_err());
        assert!(resolve(root, "orders/../../x").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
    }

    #[test]
    fn rel_helpers() {
        let root = Path::new("/col");
        assert_eq!(
            to_rel(root, Path::new("/col/orders/get.yaml")),
            "orders/get.yaml"
        );
        assert_eq!(rel_parent("orders/get.yaml"), "orders");
        assert_eq!(rel_parent("get.yaml"), "");
        assert_eq!(rel_join("", "get.yaml"), "get.yaml");
        assert_eq!(rel_join("orders", "get.yaml"), "orders/get.yaml");
    }

    #[test]
    fn unique_name_skips_taken_and_reserved_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("get-order.yaml"), "").unwrap();

        assert_eq!(
            unique_name(dir.path(), "Get order", EntryKind::Request, None),
            "get-order-2.yaml"
        );
        assert_eq!(
            unique_name(dir.path(), "Collection", EntryKind::Request, None),
            "collection-2.yaml"
        );
        assert_eq!(
            unique_name(dir.path(), "Environments", EntryKind::Folder, None),
            "environments-2"
        );
        assert_eq!(
            unique_name(dir.path(), "!!!", EntryKind::Folder, None),
            "folder"
        );
        // A file being renamed may keep its own name.
        let own = dir.path().join("get-order.yaml");
        assert_eq!(
            unique_name(dir.path(), "Get order", EntryKind::Request, Some(&own)),
            "get-order.yaml"
        );
    }
}
