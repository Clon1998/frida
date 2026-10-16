//! Frida archives (PLAN.md M12): a workspace, collection, folder or single request as a `.zip`.
//!
//! An archive holds the on-disk files exactly as in PLAN.md §4 under `content/`, plus a small
//! manifest (`frida-export.yaml`) saying what it contains. Secret values are never in it: they
//! live in the keychain, and the environment files only carry `secret: true` markers.
//!
//! Importing is defensive, since an archive may come from anyone: every entry name must be a
//! plain relative path (no absolute paths, no `..`), symlinks are refused, and sizes are capped
//! (a "zip bomb" can't fill the disk). Files are extracted into a staging folder first and moved
//! into place at the end, so a failed import leaves nothing half-written behind.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{CoreError, Result};
use crate::model::{CollectionRef, Workspace};
use crate::storage::paths::{
    COLLECTION_FILE, ENVIRONMENTS_DIR, EntryKind, WORKSPACE_FILE, slugify, unique_name,
};
use crate::storage::{self, COLLECTIONS_DIR};
use crate::yaml::to_yaml_string;

/// The manifest at the root of every archive.
pub const MANIFEST_FILE: &str = "frida-export.yaml";
/// The folder inside the archive that holds the exported files.
const CONTENT_DIR: &str = "content";
/// The archive format this Frida writes and reads.
const FORMAT: u32 = 1;

/// Limits for imports, so a malicious archive can't exhaust the disk or memory.
const MAX_ENTRIES: usize = 20_000;
const MAX_ENTRY_BYTES: u64 = 100 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 500 * 1024 * 1024;

/// What an archive contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveKind {
    Workspace,
    Collection,
    Folder,
    Request,
}

/// `frida-export.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub kind: ArchiveKind,
    /// The display name of what was exported.
    pub name: String,
    pub frida_version: String,
    /// ISO 8601, UTC.
    pub exported_at: String,
}

/// What to export. Paths are absolute folders; `rel` is collection-relative (as in the tree).
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportSource {
    #[serde(rename_all = "camelCase")]
    Workspace { workspace_dir: String },
    #[serde(rename_all = "camelCase")]
    Collection { collection_dir: String },
    #[serde(rename_all = "camelCase")]
    Folder { collection_dir: String, rel: String },
    #[serde(rename_all = "camelCase")]
    Request { collection_dir: String, rel: String },
}

/// Where to import an archive. Which one fits depends on the archive's kind.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportTarget {
    /// A workspace archive becomes a new workspace in `dir` (missing or empty).
    #[serde(rename_all = "camelCase")]
    Workspace { dir: String },
    /// A collection archive becomes a new collection of the workspace, in `dir` (missing or
    /// empty) or by default in `<workspace>/collections/<name>`.
    #[serde(rename_all = "camelCase")]
    Collection {
        workspace_dir: String,
        dir: Option<String>,
    },
    /// A folder or request archive goes into the folder `parent` (`""`: root) of a collection.
    #[serde(rename_all = "camelCase")]
    Into {
        collection_dir: String,
        parent: String,
    },
}

/// What an export wrote or an import found, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    pub manifest: Manifest,
    /// Number of files in the archive.
    pub files: u32,
    pub warnings: Vec<String>,
}

/// The result of an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedArchive {
    pub summary: ArchiveSummary,
    /// The new workspace or collection folder, or the collection the folder/request went into.
    pub dir: String,
    /// For a folder or request: its collection-relative path.
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------------------------

/// One file to put into the archive: from disk, or generated (the rewritten workspace file).
enum Source {
    Disk(PathBuf),
    Bytes(Vec<u8>),
}

/// Writes `source` as a Frida archive to `out` (replaced if it exists).
pub fn export_archive(source: &ExportSource, out: &Path) -> Result<ArchiveSummary> {
    let mut entries: Vec<(String, Source)> = Vec::new();
    let mut warnings = Vec::new();
    let (kind, name) = match source {
        ExportSource::Workspace { workspace_dir } => {
            let ws = Path::new(workspace_dir);
            let workspace = storage::open_workspace(ws)?;
            collect_files(&ws.join(ENVIRONMENTS_DIR), ENVIRONMENTS_DIR, &mut entries)?;
            // Collections may live anywhere; in the archive they all go below `collections/`.
            let mut refs = Vec::new();
            let mut used = Vec::<String>::new();
            for entry in &workspace.collections {
                let dir = storage::collection_dir(ws, entry);
                let collection = match storage::load_collection(&dir) {
                    Ok(c) => c,
                    Err(err) => {
                        warnings.push(format!("Skipped collection {}: {err}", entry.path));
                        continue;
                    }
                };
                let slug = unique_slug(&collection.name, &mut used);
                let prefix = format!("{COLLECTIONS_DIR}/{slug}");
                collect_files(&dir, &prefix, &mut entries)?;
                ca_cert_warnings(&collection.settings.tls.ca_cert_paths, &mut warnings);
                refs.push(CollectionRef {
                    path: format!("./{prefix}"),
                });
            }
            let rewritten = Workspace {
                collections: refs,
                ..workspace.clone()
            };
            entries.push((
                WORKSPACE_FILE.to_string(),
                Source::Bytes(to_yaml_string(&rewritten)?.into_bytes()),
            ));
            (ArchiveKind::Workspace, workspace.name)
        }
        ExportSource::Collection { collection_dir } => {
            let dir = Path::new(collection_dir);
            let collection = storage::load_collection(dir)?;
            collect_files(dir, "", &mut entries)?;
            ca_cert_warnings(&collection.settings.tls.ca_cert_paths, &mut warnings);
            (ArchiveKind::Collection, collection.name)
        }
        ExportSource::Folder {
            collection_dir,
            rel,
        } => {
            let root = Path::new(collection_dir);
            let folder = storage::read_folder(root, rel)?;
            let dir = storage::paths::resolve(root, rel)?;
            collect_files(&dir, last_segment(rel), &mut entries)?;
            (ArchiveKind::Folder, folder.name)
        }
        ExportSource::Request {
            collection_dir,
            rel,
        } => {
            let root = Path::new(collection_dir);
            let request = storage::read_request(root, rel)?;
            let file = storage::paths::resolve(root, rel)?;
            entries.push((last_segment(rel).to_string(), Source::Disk(file)));
            (ArchiveKind::Request, request.name)
        }
    };

    let manifest = Manifest {
        format: FORMAT,
        kind,
        name,
        frida_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: crate::vars::iso_timestamp(now_millis()),
    };
    write_zip(out, &manifest, &entries)?;
    tracing::info!(kind = ?manifest.kind, files = entries.len(), "archive exported");
    Ok(ArchiveSummary {
        manifest,
        files: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        warnings,
    })
}

/// Adds every non-hidden file below `dir` as `<prefix>/<relative path>`. Hidden entries
/// (`.git`, `.gitignore`, Frida's `.tmp…` files, `.DS_Store`) are left out.
fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<(String, Source)>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut children: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| CoreError::io(dir, e))?
        .filter_map(|e| e.ok())
        .collect();
    // Sorted, so the same files always give the same archive.
    children.sort_by_key(|e| e.file_name());
    for child in children {
        let name = child.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = child.path();
        let zip_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        // `file_type` doesn't follow symlinks: a link to somewhere else is not exported.
        let file_type = child.file_type().map_err(|e| CoreError::io(&path, e))?;
        if file_type.is_dir() {
            collect_files(&path, &zip_path, out)?;
        } else if file_type.is_file() {
            out.push((zip_path, Source::Disk(path)));
        }
    }
    Ok(())
}

/// Writes the zip to a temporary file next to `out` and renames it, so a failed export never
/// leaves a broken archive behind.
fn write_zip(out: &Path, manifest: &Manifest, entries: &[(String, Source)]) -> Result<()> {
    let dir = out.parent().unwrap_or(Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| CoreError::io(dir, e))?;
    let zip_err = |e: zip::result::ZipError| {
        CoreError::InvalidInput(format!("could not write the archive: {e}"))
    };
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // `reopen` gives a second handle to the same temp file; the zip writer owns that one, and
    // `temp` keeps the file alive until it is persisted below.
    let file = temp.reopen().map_err(|e| CoreError::io(out, e))?;
    let mut zip = ZipWriter::new(file);
    zip.start_file(MANIFEST_FILE, options).map_err(zip_err)?;
    zip.write_all(to_yaml_string(manifest)?.as_bytes())
        .map_err(|e| CoreError::io(out, e))?;
    for (name, source) in entries {
        zip.start_file(format!("{CONTENT_DIR}/{name}"), options)
            .map_err(zip_err)?;
        match source {
            Source::Disk(path) => {
                let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
                zip.write_all(&bytes).map_err(|e| CoreError::io(out, e))?;
            }
            Source::Bytes(bytes) => zip.write_all(bytes).map_err(|e| CoreError::io(out, e))?,
        }
    }
    zip.finish().map_err(zip_err)?;
    temp.persist(out).map_err(|e| CoreError::io(out, e.error))?;
    Ok(())
}

fn ca_cert_warnings(paths: &[String], warnings: &mut Vec<String>) {
    for path in paths {
        if Path::new(path).is_absolute() || path.starts_with("..") {
            warnings.push(format!(
                "The CA certificate {path} is outside the collection and was not included."
            ));
        }
    }
}

/// A slug for `name` that isn't in `used` yet (`orders`, `orders-2`, ...).
fn unique_slug(name: &str, used: &mut Vec<String>) -> String {
    let base = match slugify(name) {
        s if s.is_empty() => "collection".to_string(),
        s => s,
    };
    let mut slug = base.clone();
    let mut n = 2;
    while used.contains(&slug) {
        slug = format!("{base}-{n}");
        n += 1;
    }
    used.push(slug.clone());
    slug
}

fn last_segment(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------------------------

/// The checked contents of an archive: its manifest and its files (paths below `content/`).
struct Contents {
    manifest: Manifest,
    files: Vec<(PathBuf, Vec<u8>)>,
}

/// Reads and checks `archive` without writing anything (the import dialog's preview).
pub fn preview_archive(archive: &Path) -> Result<ArchiveSummary> {
    let contents = read_archive(archive)?;
    Ok(ArchiveSummary {
        files: u32::try_from(contents.files.len()).unwrap_or(u32::MAX),
        manifest: contents.manifest,
        warnings: Vec::new(),
    })
}

/// Imports `archive` into `target`. The archive's kind must fit the target (a collection
/// archive needs a `Collection` target, and so on).
pub fn import_archive(archive: &Path, target: &ImportTarget) -> Result<ImportedArchive> {
    let contents = read_archive(archive)?;
    let manifest = contents.manifest.clone();
    let files = u32::try_from(contents.files.len()).unwrap_or(u32::MAX);
    let mut warnings = Vec::new();
    let mismatch = || {
        CoreError::InvalidInput(format!(
            "this archive contains a {}, which can't be imported here",
            kind_name(manifest.kind)
        ))
    };

    let (dir, path) = match (manifest.kind, target) {
        (ArchiveKind::Workspace, ImportTarget::Workspace { dir }) => {
            require(&contents, WORKSPACE_FILE)?;
            let dest = PathBuf::from(dir);
            extract_to_new_dir(&contents, &dest)?;
            (dest, None)
        }
        (ArchiveKind::Collection, ImportTarget::Collection { workspace_dir, dir }) => {
            require(&contents, COLLECTION_FILE)?;
            let ws = Path::new(workspace_dir);
            let dest = match dir {
                Some(dir) => PathBuf::from(dir),
                None => storage::new_collection_dir(ws, &manifest.name)?,
            };
            extract_to_new_dir(&contents, &dest)?;
            // Two collections with the same id would share local state and keychain secrets.
            let mut collection = storage::load_collection(&dest)?;
            let taken = storage::load_overview(ws)?
                .collections
                .iter()
                .any(|c| c.id.as_deref() == Some(collection.id.as_str()));
            if taken {
                collection.id = uuid::Uuid::new_v4().to_string();
                storage::save_collection(&dest, &collection)?;
                warnings.push(
                    "The workspace already has this collection, so the copy got a new id (its secrets need to be entered again).".into(),
                );
            }
            storage::add_collection(ws, &dest)?;
            (dest, None)
        }
        (
            ArchiveKind::Folder | ArchiveKind::Request,
            ImportTarget::Into {
                collection_dir,
                parent,
            },
        ) => {
            let root = Path::new(collection_dir);
            let parent_dir = storage::paths::resolve(root, parent)?;
            let rel = extract_item(&contents, &parent_dir)?;
            let path = storage::paths::rel_join(parent, &rel);
            (root.to_path_buf(), Some(path))
        }
        _ => return Err(mismatch()),
    };
    tracing::info!(kind = ?manifest.kind, files, "archive imported");
    Ok(ImportedArchive {
        summary: ArchiveSummary {
            manifest,
            files,
            warnings,
        },
        dir: dir.to_string_lossy().to_string(),
        path,
    })
}

fn kind_name(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::Workspace => "workspace",
        ArchiveKind::Collection => "collection",
        ArchiveKind::Folder => "folder",
        ArchiveKind::Request => "request",
    }
}

fn require(contents: &Contents, file: &str) -> Result<()> {
    if contents.files.iter().any(|(p, _)| p == Path::new(file)) {
        Ok(())
    } else {
        Err(CoreError::InvalidInput(format!(
            "the archive is damaged: {file} is missing"
        )))
    }
}

/// Opens the zip, checks the manifest and every entry, and reads the files into memory (they
/// are capped at [`MAX_TOTAL_BYTES`] in total).
fn read_archive(path: &Path) -> Result<Contents> {
    let invalid = |message: String| CoreError::InvalidInput(message);
    let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|e| invalid(format!("{} is not a valid archive: {e}", path.display())))?;
    if zip.len() > MAX_ENTRIES {
        return Err(invalid("the archive has too many files".into()));
    }

    let mut manifest = None;
    let mut files = Vec::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| invalid(format!("could not read the archive: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        let name = safe_name(&raw, entry.enclosed_name())
            .ok_or_else(|| invalid(format!("the archive contains an unsafe file name: {raw}")))?;
        if entry.is_symlink() {
            return Err(invalid(format!(
                "the archive contains a symbolic link ({}), which Frida doesn't import",
                name.display()
            )));
        }
        // The declared size can lie, so the read below is limited too (`take`).
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(invalid(format!("{} is too large", name.display())));
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| invalid(format!("could not read {}: {e}", name.display())))?;
        total += bytes.len() as u64;
        if bytes.len() as u64 > MAX_ENTRY_BYTES || total > MAX_TOTAL_BYTES {
            return Err(invalid("the archive is too large".into()));
        }

        if name == Path::new(MANIFEST_FILE) {
            let text = String::from_utf8(bytes)
                .map_err(|_| invalid("the archive's manifest is not text".into()))?;
            // Not `from_yaml_str`: the manifest has `format`, not the `version` of Frida's files.
            let parsed: Manifest = serde_saphyr::from_str(&text)
                .map_err(|e| invalid(format!("the archive's manifest is invalid: {e}")))?;
            manifest = Some(parsed);
        } else if let Ok(rel) = name.strip_prefix(CONTENT_DIR) {
            files.push((rel.to_path_buf(), bytes));
        }
        // Anything else (e.g. `__MACOSX/` from Finder's zip) is ignored.
    }

    let manifest = manifest.ok_or_else(|| {
        invalid(format!(
            "{} is not a Frida archive ({MANIFEST_FILE} is missing)",
            path.display()
        ))
    })?;
    if manifest.format != FORMAT {
        return Err(CoreError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: manifest.format,
            supported: FORMAT,
        });
    }
    Ok(Contents { manifest, files })
}

/// The entry's path if it is a plain relative path. The raw name is checked first, so
/// `a/../../b`, `/etc/x`, `C:\x` and `\\server\x` are refused outright (`enclosed_name`
/// alone would quietly turn some of them into other paths); then every component of the
/// cleaned-up path must be a normal name.
fn safe_name(raw: &str, enclosed: Option<PathBuf>) -> Option<PathBuf> {
    let suspicious = raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.contains(':')
        || raw.split(['/', '\\']).any(|part| part == "..");
    if suspicious {
        return None;
    }
    let name = enclosed?;
    let plain = name.components().all(|c| matches!(c, Component::Normal(_)));
    (plain && name.components().next().is_some()).then_some(name)
}

/// Writes the files below `root`.
fn write_files<'a>(
    root: &Path,
    files: impl Iterator<Item = (&'a PathBuf, &'a Vec<u8>)>,
) -> Result<()> {
    for (rel, bytes) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::write(&path, bytes).map_err(|e| CoreError::io(&path, e))?;
    }
    Ok(())
}

/// Extracts everything into `dest`, which must be missing or empty: first into a staging folder
/// next to it, then renamed into place.
fn extract_to_new_dir(contents: &Contents, dest: &Path) -> Result<()> {
    if dest.exists() {
        let empty = std::fs::read_dir(dest)
            .map_err(|e| CoreError::io(dest, e))?
            .next()
            .is_none();
        if !empty {
            return Err(CoreError::InvalidInput(format!(
                "{} is not empty; choose an empty or new folder",
                dest.display()
            )));
        }
    }
    let parent = dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    let staging = tempfile::Builder::new()
        .prefix(".frida-import-")
        .tempdir_in(parent)
        .map_err(|e| CoreError::io(parent, e))?;
    write_files(staging.path(), contents.files.iter().map(|(p, b)| (p, b)))?;
    if dest.exists() {
        std::fs::remove_dir(dest).map_err(|e| CoreError::io(dest, e))?;
    }
    // `keep` stops the TempDir from deleting the folder when it goes out of scope (RAII), since
    // it now *is* the result.
    let staged = staging.keep();
    std::fs::rename(&staged, dest).map_err(|e| CoreError::io(dest, e))?;
    Ok(())
}

/// Puts a folder or request archive into `parent_dir` under a free name. Returns the name used.
fn extract_item(contents: &Contents, parent_dir: &Path) -> Result<String> {
    let damaged = || CoreError::InvalidInput("the archive is damaged: unexpected contents".into());
    match contents.manifest.kind {
        ArchiveKind::Request => {
            let [(file, bytes)] = contents.files.as_slice() else {
                return Err(damaged());
            };
            if file.components().count() != 1 {
                return Err(damaged());
            }
            let name = unique_name(
                parent_dir,
                &contents.manifest.name,
                EntryKind::Request,
                None,
            );
            let path = parent_dir.join(&name);
            std::fs::write(&path, bytes).map_err(|e| CoreError::io(&path, e))?;
            Ok(name)
        }
        _ => {
            // Everything sits below one top folder, which may be renamed to a free name.
            let top = contents
                .files
                .first()
                .and_then(|(p, _)| p.components().next())
                .ok_or_else(damaged)?;
            let all_below = contents
                .files
                .iter()
                .all(|(p, _)| p.components().next() == Some(top) && p.components().count() > 1);
            if !all_below {
                return Err(damaged());
            }
            let name = unique_name(parent_dir, &contents.manifest.name, EntryKind::Folder, None);
            let staging = tempfile::Builder::new()
                .prefix(".frida-import-")
                .tempdir_in(parent_dir)
                .map_err(|e| CoreError::io(parent_dir, e))?;
            let below_top = contents.files.iter().map(|(p, b)| {
                (
                    p.strip_prefix(top.as_os_str())
                        .expect("checked above")
                        .to_path_buf(),
                    b,
                )
            });
            // Collected first: `write_files` wants references to owned paths.
            let below_top: Vec<(PathBuf, &Vec<u8>)> = below_top.collect();
            write_files(staging.path(), below_top.iter().map(|(p, b)| (p, *b)))?;
            let dest = parent_dir.join(&name);
            std::fs::rename(staging.keep(), &dest).map_err(|e| CoreError::io(&dest, e))?;
            Ok(name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_relative_entry_names_are_safe() {
        let ok = |raw: &str| safe_name(raw, Some(PathBuf::from(raw)));
        assert_eq!(
            ok("content/orders/get.yaml"),
            Some(PathBuf::from("content/orders/get.yaml"))
        );
        for bad in [
            "content/../../evil.yaml",
            "content/a/../b.yaml",
            "/etc/passwd",
            "\\\\server\\share\\x",
            "C:\\evil.yaml",
            "content\\..\\evil.yaml",
        ] {
            assert_eq!(ok(bad), None, "{bad}");
        }
        assert_eq!(safe_name("x", None), None);
    }

    #[test]
    fn collection_slugs_are_unique_in_a_workspace_export() {
        let mut used = Vec::new();
        assert_eq!(unique_slug("Orders API", &mut used), "orders-api");
        assert_eq!(unique_slug("Orders API", &mut used), "orders-api-2");
        assert_eq!(unique_slug("???", &mut used), "collection");
    }
}
