//! Local, per-machine state that never goes into a repo (PLAN.md §4.6): app settings and
//! per-collection UI state such as open tabs. Stored as JSON in the app data dir.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{CoreError, Result};

/// How many recently opened workspaces are remembered.
pub const MAX_RECENT_WORKSPACES: usize = 10;

/// The automatic update check runs at most this often (PLAN.md M14).
pub const UPDATE_CHECK_INTERVAL_SECS: u32 = 24 * 60 * 60;

/// `settings.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    /// Absolute workspace folders, most recent first.
    pub recent_workspaces: Vec<String>,
    /// Workspaces the user pinned, in the order they were pinned. They stay in the menu and on
    /// the start screen however many others are opened.
    pub pinned_workspaces: Vec<String>,
    /// The collection the user last had selected, per workspace folder (both absolute). Opening
    /// the workspace again selects it instead of the first collection.
    pub active_collections: std::collections::BTreeMap<String, String>,
    /// The active global environment per workspace folder.
    pub active_global_environments: std::collections::BTreeMap<String, String>,
    /// The AWS profile per global environment, keyed by `"<workspace folder>|<environment>"`.
    pub aws_profiles: std::collections::BTreeMap<String, String>,
    /// The local AWS profile per account alias (`account: shared-services` on a secret,
    /// PLAN.md M17). Aliases are shared through the files, profile names are per machine.
    pub aws_accounts: std::collections::BTreeMap<String, String>,
    /// When the app last asked for an update on its own (unix seconds), so it asks at most
    /// once a day (PLAN.md M14). `u32` because TypeScript numbers can't hold every `u64`;
    /// seconds fit until 2106.
    pub last_update_check: Option<u32>,
    /// What the user chooses in the Settings dialog. The fields above are state the app keeps
    /// by itself.
    pub preferences: Preferences,
}

/// The Settings dialog. Every field has a default, so older `settings.json` files load fine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub theme: ThemePreference,
    pub accent: Accent,
    /// Size of the whole UI in percent (100 = normal).
    pub ui_scale: u32,
    /// Font size of the code editors (URL bar, body, scripts, responses) in pixels.
    pub editor_font_size: u32,
    /// How many history entries are kept (PLAN.md §13).
    pub history_size: u32,
    /// The git program. `None` runs `git` from the PATH.
    pub git_path: Option<String>,
    pub aws: AwsPreferences,
    /// Look for a new Frida version on start (at most once a day).
    pub check_for_updates: bool,
    /// The HTTP proxy (PLAN.md M16). Its password is in the keychain, not here.
    pub proxy: crate::network::ProxyPreferences,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            accent: Accent::default(),
            ui_scale: 100,
            editor_font_size: 13,
            history_size: crate::history::DEFAULT_RETENTION as u32,
            git_path: None,
            aws: AwsPreferences::default(),
            check_for_updates: true,
            proxy: crate::network::ProxyPreferences::default(),
        }
    }
}

impl Preferences {
    /// Allowed ranges (the UI offers only these, but the file can be edited by hand).
    pub const UI_SCALE: std::ops::RangeInclusive<u32> = 75..=150;
    pub const EDITOR_FONT_SIZE: std::ops::RangeInclusive<u32> = 10..=24;
    pub const HISTORY_SIZE: std::ops::RangeInclusive<u32> = 0..=100_000;
    pub const AWS_CACHE_MINUTES: std::ops::RangeInclusive<u32> = 1..=24 * 60;

    /// A copy with every number in its allowed range and blank paths turned into `None`.
    pub fn sanitized(&self) -> Self {
        fn clamp(value: u32, range: &std::ops::RangeInclusive<u32>) -> u32 {
            value.clamp(*range.start(), *range.end())
        }
        /// `"  "` → `None`, `" /usr/bin/git "` → `Some("/usr/bin/git")`.
        fn path(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
        }
        Self {
            theme: self.theme,
            accent: self.accent,
            ui_scale: clamp(self.ui_scale, &Self::UI_SCALE),
            editor_font_size: clamp(self.editor_font_size, &Self::EDITOR_FONT_SIZE),
            history_size: clamp(self.history_size, &Self::HISTORY_SIZE),
            git_path: path(&self.git_path),
            aws: AwsPreferences {
                config_file: path(&self.aws.config_file),
                credentials_file: path(&self.aws.credentials_file),
                cache_minutes: clamp(self.aws.cache_minutes, &Self::AWS_CACHE_MINUTES),
            },
            check_for_updates: self.check_for_updates,
            proxy: crate::network::ProxyPreferences {
                mode: self.proxy.mode,
                url: self.proxy.url.trim().to_string(),
                username: self.proxy.username.trim().to_string(),
                bypass: self.proxy.bypass.trim().to_string(),
            },
        }
    }
}

/// AWS settings (PLAN.md §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AwsPreferences {
    /// `None`: `AWS_CONFIG_FILE`, else `~/.aws/config`.
    pub config_file: Option<String>,
    /// `None`: `AWS_SHARED_CREDENTIALS_FILE`, else `~/.aws/credentials`.
    pub credentials_file: Option<String>,
    /// How long fetched secret values are reused before asking AWS again.
    pub cache_minutes: u32,
}

impl Default for AwsPreferences {
    fn default() -> Self {
        Self {
            config_file: None,
            credentials_file: None,
            cache_minutes: 15,
        }
    }
}

/// The UI's accent color (buttons, focus rings, selection).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    /// Black on light, white on dark.
    #[default]
    Neutral,
    Blue,
    Green,
    Violet,
    Orange,
    Rose,
}

impl AppSettings {
    /// Moves `workspace` to the top of the recent list.
    pub fn remember_workspace(&mut self, workspace: &str) {
        self.recent_workspaces.retain(|w| w != workspace);
        self.recent_workspaces.insert(0, workspace.to_string());
        self.recent_workspaces.truncate(MAX_RECENT_WORKSPACES);
    }

    /// Takes `workspace` off the recent and pinned lists ("Remove from list"). The folder
    /// itself is not touched.
    pub fn forget_workspace(&mut self, workspace: &str) {
        self.recent_workspaces.retain(|w| w != workspace);
        self.pinned_workspaces.retain(|w| w != workspace);
        self.active_collections.remove(workspace);
    }

    /// Whether the app should look for an update on its own at `now` (unix seconds): switched on
    /// in the settings and not done in the last 24 hours. A clock that went backwards counts as
    /// due, so a wrong timestamp can't switch the check off for good.
    pub fn update_check_due(&self, now: u32) -> bool {
        if !self.preferences.check_for_updates {
            return false;
        }
        match self.last_update_check {
            Some(last) if last <= now => now - last >= UPDATE_CHECK_INTERVAL_SECS,
            _ => true,
        }
    }

    /// Pins or unpins `workspace`.
    pub fn pin_workspace(&mut self, workspace: &str, pinned: bool) {
        self.pinned_workspaces.retain(|w| w != workspace);
        if pinned {
            self.pinned_workspaces.push(workspace.to_string());
        }
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
    /// The AWS profile per collection environment (PLAN.md §9.1), for AWS-backed secrets.
    pub aws_profiles: std::collections::BTreeMap<String, String>,
    /// "Don't use the proxy" for this collection on this computer (PLAN.md M16), e.g. for
    /// APIs on the local network.
    pub bypass_proxy: bool,
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

    /// Saves what the UI owns (open tabs, expanded folders) and keeps the rest, such as the
    /// active environment, which is set separately. Overwriting it here used to "forget" the
    /// selected environment whenever a tab was opened, switched or closed.
    pub fn save_view_state(&self, collection_id: &str, view: &CollectionState) -> Result<()> {
        let current = self.load_collection_state(collection_id)?;
        self.save_collection_state(
            collection_id,
            &CollectionState {
                active_environment: current.active_environment,
                aws_profiles: current.aws_profiles,
                bypass_proxy: current.bypass_proxy,
                ..view.clone()
            },
        )
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
    fn preferences_are_kept_in_range() {
        let prefs = Preferences {
            ui_scale: 500,
            editor_font_size: 2,
            git_path: Some("   ".into()),
            aws: AwsPreferences {
                config_file: Some(" /x/config ".into()),
                credentials_file: None,
                cache_minutes: 0,
            },
            ..Preferences::default()
        }
        .sanitized();
        assert_eq!(prefs.ui_scale, 150);
        assert_eq!(prefs.editor_font_size, 10);
        assert_eq!(prefs.git_path, None);
        assert_eq!(prefs.aws.config_file.as_deref(), Some("/x/config"));
        assert_eq!(prefs.aws.cache_minutes, 1);
        assert_eq!(Preferences::default().sanitized(), Preferences::default());
    }

    #[test]
    fn update_check_runs_at_most_once_a_day() {
        let day = UPDATE_CHECK_INTERVAL_SECS;
        let mut settings = AppSettings::default();
        assert!(settings.update_check_due(1_000), "never checked");

        settings.last_update_check = Some(1_000);
        assert!(!settings.update_check_due(1_000 + day - 1));
        assert!(settings.update_check_due(1_000 + day));
        assert!(settings.update_check_due(10), "clock went backwards");

        settings.preferences.check_for_updates = false;
        assert!(!settings.update_check_due(1_000 + 2 * day), "switched off");
    }

    #[test]
    fn older_settings_files_still_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"recentWorkspaces":["/w"],"theme":"dark","historySize":50}"#,
        )
        .unwrap();
        let settings = LocalStore::new(dir.path()).load_settings();
        assert_eq!(settings.recent_workspaces, ["/w"]);
        assert_eq!(settings.preferences, Preferences::default());
    }

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
        settings.preferences.theme = ThemePreference::Dark;
        settings.preferences.git_path = Some("/opt/git".into());
        store.save_settings(&settings).unwrap();
        assert_eq!(store.load_settings(), settings);

        let id = "7f1c2e4a-0b5e-4a8e-9f1a-2c3d4e5f6a7b";
        let state = CollectionState {
            open_tabs: vec!["a.yaml".into(), "orders/b.yaml".into()],
            active_tab: Some("a.yaml".into()),
            expanded_folders: vec!["orders".into()],
            active_environment: Some("dev".into()),
            aws_profiles: [("prod".to_string(), "work-sso".to_string())].into(),
            bypass_proxy: true,
        };
        store.save_collection_state(id, &state).unwrap();
        assert_eq!(store.load_collection_state(id).unwrap(), state);
    }

    #[test]
    fn pinned_workspaces_can_be_pinned_unpinned_and_forgotten() {
        let mut settings = AppSettings::default();
        settings.remember_workspace("/a");
        settings.remember_workspace("/b");
        settings.pin_workspace("/a", true);
        settings.pin_workspace("/a", true);
        settings.pin_workspace("/c", true);
        assert_eq!(settings.pinned_workspaces, ["/a", "/c"]);
        settings.pin_workspace("/c", false);
        assert_eq!(settings.pinned_workspaces, ["/a"]);
        settings
            .active_collections
            .insert("/a".into(), "/a/collections/x".into());
        settings.forget_workspace("/a");
        assert!(settings.pinned_workspaces.is_empty());
        assert!(settings.active_collections.is_empty());
        assert_eq!(settings.recent_workspaces, ["/b"]);
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

    #[test]
    fn saving_tabs_keeps_the_active_environment() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(dir.path());
        let id = "0d6c2a4e-8f1b-4c3d-9e5a-7b8c9d0e1f2a";
        store
            .save_collection_state(
                id,
                &CollectionState {
                    active_environment: Some("dev".into()),
                    aws_profiles: [("prod".to_string(), "work".to_string())].into(),
                    bypass_proxy: true,
                    ..CollectionState::default()
                },
            )
            .unwrap();
        // The UI sends its tab state without the environment.
        store
            .save_view_state(
                id,
                &CollectionState {
                    open_tabs: vec!["a.yaml".into()],
                    ..CollectionState::default()
                },
            )
            .unwrap();
        let loaded = store.load_collection_state(id).unwrap();
        assert_eq!(loaded.active_environment.as_deref(), Some("dev"));
        assert!(loaded.bypass_proxy, "the proxy switch survives tab changes");
        assert_eq!(
            loaded.aws_profiles.get("prod").map(String::as_str),
            Some("work")
        );
        assert_eq!(loaded.open_tabs, vec!["a.yaml"]);
    }
}
