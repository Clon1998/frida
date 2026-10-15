//! Reading and writing Frida's YAML files.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{CoreError, Result};
use crate::model::FORMAT_VERSION;

/// Only the `version` field, parsed before the full file (see [`from_yaml_str`]).
#[derive(serde::Deserialize)]
struct VersionProbe {
    version: Option<u32>,
}

/// Parses YAML text into `T`, checking the format version first.
///
/// The version is checked *before* parsing the whole file, so a file written by a newer Frida
/// gets a clear "update Frida" error instead of a confusing "unknown field" message.
///
/// `T: DeserializeOwned` means "any type serde can build from data without borrowing from the
/// input string". It's a generic bound, comparable to `<T extends Deserializable>` in Java.
pub fn from_yaml_str<T: DeserializeOwned>(text: &str, path: &Path) -> Result<T> {
    let invalid = |message: String| CoreError::InvalidFile {
        path: path.to_path_buf(),
        message,
    };

    let probe: VersionProbe = serde_saphyr::from_str(text).map_err(|e| invalid(e.to_string()))?;
    match probe.version {
        Some(FORMAT_VERSION) => {}
        Some(found) => {
            return Err(CoreError::UnsupportedVersion {
                path: path.to_path_buf(),
                found,
                supported: FORMAT_VERSION,
            });
        }
        None => return Err(invalid("missing `version` field".to_string())),
    }

    serde_saphyr::from_str(text).map_err(|e| invalid(e.to_string()))
}

/// Serializes `value` to YAML text.
pub fn to_yaml_string<T: Serialize>(value: &T) -> Result<String> {
    serde_saphyr::to_string(value).map_err(|e| CoreError::InvalidInput(e.to_string()))
}

/// Reads and parses a YAML file.
pub fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
    from_yaml_str(&text, path)
}

/// Writes `value` as YAML, atomically.
///
/// The text goes to a temporary file in the same directory, which is then renamed over the
/// target. A rename within one directory is atomic, so readers (the file watcher, git, an
/// editor) see either the old or the new file, never a half-written one.
pub fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = to_yaml_string(value)?;
    let dir = path.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("{} has no parent folder", path.display()))
    })?;
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;

    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| CoreError::io(dir, e))?;
    std::io::Write::write_all(&mut temp, text.as_bytes()).map_err(|e| CoreError::io(path, e))?;
    temp.persist(path)
        .map_err(|e| CoreError::io(path, e.error))?;
    Ok(())
}
