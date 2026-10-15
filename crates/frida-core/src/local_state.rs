//! Local, per-machine state that never goes into a repo (PLAN.md §4.6): app settings and
//! per-collection UI state such as open tabs. Stored as JSON in the app data dir.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{CoreError, Result};

/// How many recently opened workspaces are remembered.
pub const MAX_RECENT_WORKSPACES: usize = 10;

/// `settings.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    /// Absolute workspace folders, most recent first.
    pub recent_workspaces: Vec<String>,
    pub theme: ThemePreference,
    /// The active global environment per workspace folder.
    pub active_global_environments: std::collections::BTreeMap<String, String>,
}

impl AppSettings {
    /// Moves `workspace` to the top of the recent list.
    pub fn remember_workspace(&mut self, workspace: &str) {
        self.recent_workspaces.retain(|w| w != workspace);
        self.recent_workspaces.insert(0, workspace.to_string());
        self.recent_workspaces.truncate(MAX_RECENT_WORKSPACES);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// `state/{collectionId}.json`: what the user had open in a collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct CollectionState {
    /// Collection-relative paths of the open request tabs, in tab order.
    pub open_tabs: Vec<String>,
    pub active_tab: Option<String>,
    /// Folders expanded in the sidebar tree.
    pub expanded_folders: Vec<String>,
    /// The active collection environment.
    pub active_environment: Option<String>,
}

/// Reads and writes the local state files below one base directory (the app data dir).
#[derive(Debug, Clone)]
pub struct LocalStore {
    dir: PathBuf,
}

impl LocalStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load_settings(&self) -> AppSettings {
        load_or_default(&self.dir.join("settings.json"))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        save_json(&self.dir.join("settings.json"), settings)
    }

    pub fn load_collection_state(&self, collection_id: &str) -> Result<CollectionState> {
        Ok(load_or_default(&self.collection_state_path(collection_id)?))
    }

    pub fn save_collection_state(
        &self,
        collection_id: &str,
        state: &CollectionState,
    ) -> Result<()> {
        save_json(&self.collection_state_path(collection_id)?, state)
    }

    fn collection_state_path(&self, collection_id: &str) -> Result<PathBuf> {
        // The id comes from collection.yaml, which anyone can edit. Only allow UUID-like ids
        // so it can't be used to write outside the state folder.
        let is_safe = !collection_id.is_empty()
            && collection_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !is_safe {
            return Err(CoreError::InvalidInput(format!(
                "invalid collection id {collection_id:?}"
            )));
        }
        Ok(self.dir.join("state").join(format!("{collection_id}.json")))
    }
}

/// Missing or unreadable state is not an error: the app starts with defaults instead.
/// Losing open tabs is better than refusing to start.
fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };
    serde_json::from_str(&text).unwrap_or_else(|err| {
        tracing::warn!(path = %path.display(), %err, "ignoring unreadable local state file");
        T::default()
    })
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text =
        serde_json::to_string_pretty(value).map_err(|e| CoreError::InvalidInput(e.to_string()))?;
    let dir = path
        .parent()
        .expect("state files always have a parent folder");
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| CoreError::io(dir, e))?;
    std::io::Write::write_all(&mut temp, text.as_bytes()).map_err(|e| CoreError::io(path, e))?;
    temp.persist(path)
        .map_err(|e| CoreError::io(path, e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_corrupt_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(dir.path());
        assert_eq!(store.load_settings(), AppSettings::default());

        std::fs::write(dir.path().join("settings.json"), "{ not json").unwrap();
        assert_eq!(store.load_settings(), AppSettings::default());
    }

    #[test]
    fn settings_and_collection_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(dir.path());

        let mut settings = AppSettings::default();
        settings.remember_workspace("/a");
        settings.theme = ThemePreference::Dark;
        store.save_settings(&settings).unwrap();
        assert_eq!(store.load_settings(), settings);

        let id = "7f1c2e4a-0b5e-4a8e-9f1a-2c3d4e5f6a7b";
        let state = CollectionState {
            open_tabs: vec!["a.yaml".into(), "orders/b.yaml".into()],
            active_tab: Some("a.yaml".into()),
            expanded_folders: vec!["orders".into()],
            active_environment: Some("dev".into()),
        };
        store.save_collection_state(id, &state).unwrap();
        assert_eq!(store.load_collection_state(id).unwrap(), state);
    }

    #[test]
    fn recent_workspaces_are_deduplicated_and_capped() {
        let mut settings = AppSettings::default();
        for i in 0..15 {
            settings.remember_workspace(&format!("/ws{i}"));
        }
        settings.remember_workspace("/ws10");
        assert_eq!(settings.recent_workspaces.len(), MAX_RECENT_WORKSPACES);
        assert_eq!(settings.recent_workspaces[0], "/ws10");
        assert_eq!(
            settings
                .recent_workspaces
                .iter()
                .filter(|w| *w == "/ws10")
                .count(),
            1
        );
    }

    #[test]
    fn collection_ids_cannot_escape_the_state_folder() {
        let store = LocalStore::new("/data");
        assert!(store.load_collection_state("../../etc/passwd").is_err());
        assert!(
            store
                .save_collection_state("", &CollectionState::default())
                .is_err()
        );
    }
}
