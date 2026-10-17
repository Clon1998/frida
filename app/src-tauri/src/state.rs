//! Process-wide app state, managed by Tauri and injected into commands as `State<AppState>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use frida_core::auth::OAuth2Manager;
use frida_core::history::HistoryStore;
use frida_core::http::{CookieJar, EnvOverrides, ResponseCache, Session, SessionStore};
use frida_core::local_state::{LocalStore, Preferences};
use frida_core::storage;
use frida_core::watch::FsWatcher;
use frida_secrets::aws::AwsFiles;
use frida_secrets::{CachedStore, EncryptedFileStore, KeychainStore, SecretStore};
use serde::Deserialize;
use specta::Type;
use tokio_util::sync::CancellationToken;

use crate::error::{ApiError, ApiResult, ErrorCode};

/// Commands run concurrently on Tauri's thread pool, so every mutable field sits behind a
/// `Mutex`. Tauri wraps the whole struct in an `Arc` itself, which is why there is no `Arc` here.
///
/// We use `std::sync::Mutex` (not `tokio::sync::Mutex`) because no lock is ever held across an
/// `.await`. Holding a std mutex across an await point could block a runtime thread and is a
/// classic async pitfall; locking briefly to read or update a value is fine.
pub struct AppState {
    pub local: LocalStore,
    pub workspace: Mutex<Option<OpenWorkspace>>,
    /// Cancel handles of requests in flight, by the request id the UI chose.
    pub in_flight: Mutex<HashMap<String, CancellationToken>>,
    pub responses: Mutex<ResponseCache>,
    pub watcher: Mutex<Option<FsWatcher>>,
    pub secrets: SecretBackend,
    /// What scripts changed (`fri.setVar`, `fri.setEnvVar`), per collection folder. In memory
    /// only. `Arc` so a send can keep using it without holding the map's lock.
    pub sessions: Mutex<HashMap<PathBuf, Arc<SessionStore>>>,
    /// `fri.setGlobalEnvVar` changes, shared by all collections like the global environment.
    pub global_overrides: EnvOverrides,
    /// One cookie jar per collection folder, in memory only.
    pub cookie_jars: Mutex<HashMap<PathBuf, Arc<CookieJar>>>,
    /// OAuth2 tokens, in memory, shared by all collections (tokens are keyed by provider).
    pub oauth: OAuth2Manager,
    /// AWS Secrets Manager values, cached in memory (15 minutes by default, PLAN.md §9.1).
    pub aws: Arc<frida_secrets::aws::AwsSecrets>,
    /// Request history. `Arc` so it can be moved into `spawn_blocking` closures. `None` if the
    /// database couldn't be opened (history is then simply off; sending still works).
    pub history: Option<Arc<HistoryStore>>,
    /// The update found by the last check, ready for "Install and restart" (PLAN.md M14).
    pub update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// Cancels the update download while it runs.
    pub update_cancel: Mutex<Option<CancellationToken>>,
}

/// Where secret values are kept on this machine (PLAN.md §6).
pub enum SecretBackend {
    /// The OS keychain (the normal case), behind a cache so each secret is read once per run.
    Keychain(Arc<CachedStore>),
    /// No OS keychain available: an encrypted file that must be unlocked with a password.
    File(Arc<EncryptedFileStore>),
}

impl SecretBackend {
    /// The store as a trait object. `Arc<dyn SecretStore>` lets commands use either backend
    /// through the same interface and move it into `spawn_blocking` closures.
    pub fn store(&self) -> Arc<dyn SecretStore> {
        match self {
            SecretBackend::Keychain(store) => Arc::clone(store) as Arc<dyn SecretStore>,
            SecretBackend::File(store) => Arc::clone(store) as Arc<dyn SecretStore>,
        }
    }
}

/// Which environments a command is about: the workspace's global ones or a collection's.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EnvScope {
    Global,
    Collection { dir: String },
}

/// The currently open workspace and the folders of its collections.
#[derive(Debug, Clone)]
pub struct OpenWorkspace {
    pub dir: PathBuf,
    pub collection_dirs: Vec<PathBuf>,
}

impl OpenWorkspace {
    /// Every folder the file watcher watches for this workspace.
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.collection_dirs.clone();
        dirs.push(self.dir.join("environments"));
        dirs
    }
}

/// The keychain access group of signed builds: team id + bundle id (`Entitlements.plist`).
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCESS_GROUP: &str = "Q4XB68D4T7.io.github.clon1998.frida";

/// The OS keychain store to use (PLAN.md M13).
///
/// On macOS, a signed build uses the data protection keychain (no password prompts); an
/// unsigned dev build keeps all secrets in one legacy keychain item, the "vault" (one prompt
/// per run). Old per-secret items move over on first use. A signed build also takes over what
/// a dev build put into the vault, so switching to a release loses nothing. (The other way
/// round, secrets entered in a signed build stay there: dev builds can't read that keychain.)
/// Elsewhere: the OS store as is.
fn os_keychain() -> Box<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    {
        use frida_secrets::{MigratingStore, ProtectedStore, VaultStore};
        // The vault, which itself takes over old per-secret items.
        let vault = || -> Box<dyn SecretStore> {
            Box::new(MigratingStore::new(
                Box::new(VaultStore::new(Box::new(KeychainStore))),
                Box::new(KeychainStore),
            ))
        };
        let protected = ProtectedStore::new(KEYCHAIN_ACCESS_GROUP);
        if protected.available() {
            tracing::info!("secrets: data protection keychain");
            Box::new(MigratingStore::new(Box::new(protected), vault()))
        } else {
            tracing::info!("secrets: one vault item in the login keychain (unsigned build)");
            vault()
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(KeychainStore)
    }
}

impl AppState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let secrets = if frida_secrets::keychain_available() {
            SecretBackend::Keychain(Arc::new(CachedStore::new(os_keychain())))
        } else {
            tracing::warn!("no OS keychain available, using the encrypted secret file");
            SecretBackend::File(Arc::new(EncryptedFileStore::new(
                app_data_dir.join("secrets.enc"),
            )))
        };
        let history = match HistoryStore::open(&app_data_dir.join("history.sqlite")) {
            Ok(store) => Some(Arc::new(store)),
            Err(err) => {
                tracing::warn!(%err, "history disabled");
                None
            }
        };
        let state = Self {
            aws: Arc::new(frida_secrets::aws::AwsSecrets::with_sdk()),
            history,
            local: LocalStore::new(app_data_dir),
            workspace: Mutex::new(None),
            in_flight: Mutex::new(HashMap::new()),
            update: Mutex::new(None),
            update_cancel: Mutex::new(None),
            responses: Mutex::new(ResponseCache::default()),
            watcher: Mutex::new(None),
            secrets,
            sessions: Mutex::new(HashMap::new()),
            global_overrides: EnvOverrides::default(),
            cookie_jars: Mutex::new(HashMap::new()),
            // The authorization code flow opens the system browser for the user to sign in.
            oauth: OAuth2Manager::new(Arc::new(|url: &str| {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
            })),
        };
        state.apply_preferences(&state.local.load_settings().preferences);
        state
    }

    /// Makes the parts of the settings that live outside the UI take effect: which git program
    /// runs, and the AWS files and cache time. Called at startup and after every change.
    pub fn apply_preferences(&self, preferences: &Preferences) {
        frida_git::set_executable(preferences.git_path.as_ref().map(PathBuf::from));
        self.aws.configure(
            Duration::from_secs(u64::from(preferences.aws.cache_minutes) * 60),
            AwsFiles {
                config: preferences.aws.config_file.as_ref().map(PathBuf::from),
                credentials: preferences.aws.credentials_file.as_ref().map(PathBuf::from),
            },
        );
    }

    /// The folder holding the environments of `scope`, and the keychain scope id of its secrets.
    pub fn env_location(&self, scope: &EnvScope) -> ApiResult<(PathBuf, String)> {
        match scope {
            EnvScope::Global => {
                let dir = self.workspace_dir()?;
                let scope_id = frida_core::secrets::workspace_scope_id(&dir);
                Ok((dir, scope_id))
            }
            EnvScope::Collection { dir } => {
                let dir = self.collection_dir(dir)?;
                let scope_id = storage::load_collection(&dir)?.id;
                Ok((dir, scope_id))
            }
        }
    }

    /// The cookie jar of a collection, created on first use.
    pub fn cookie_jar(&self, collection_dir: &Path) -> Arc<CookieJar> {
        let mut jars = self.cookie_jars.lock().expect("cookie jars lock poisoned");
        Arc::clone(jars.entry(collection_dir.to_path_buf()).or_default())
    }

    /// The script session of a collection, created on first use.
    pub fn session_store(&self, collection_dir: &Path) -> Arc<SessionStore> {
        let mut sessions = self.sessions.lock().expect("sessions lock poisoned");
        Arc::clone(sessions.entry(collection_dir.to_path_buf()).or_default())
    }

    /// What a send needs from the session. The references borrow `store`, `self` and
    /// `workspace_dir`, so the returned value can't outlive them (that is what `'a` says).
    pub fn session<'a>(
        &'a self,
        store: &'a SessionStore,
        workspace_dir: Option<&'a Path>,
    ) -> Session<'a> {
        Session {
            runtime: &store.runtime,
            env_overrides: &store.env_overrides,
            global_overrides: &self.global_overrides,
            workspace_dir,
            // The app always follows the collection's TLS settings.
            skip_tls_verify: false,
        }
    }

    /// The open workspace's folder, or a `no_workspace` error.
    pub fn workspace_dir(&self) -> ApiResult<PathBuf> {
        self.workspace
            .lock()
            .expect("workspace lock poisoned")
            .as_ref()
            .map(|ws| ws.dir.clone())
            .ok_or_else(|| ApiError::new(ErrorCode::NoWorkspace, "no workspace is open"))
    }

    /// Checks that `dir` is a collection of the open workspace before touching any files in
    /// it. The UI passes folders as strings; this keeps commands from operating on arbitrary
    /// folders on disk.
    pub fn collection_dir(&self, dir: &str) -> ApiResult<PathBuf> {
        let guard = self.workspace.lock().expect("workspace lock poisoned");
        let workspace = guard
            .as_ref()
            .ok_or_else(|| ApiError::new(ErrorCode::NoWorkspace, "no workspace is open"))?;
        let wanted = canonical(Path::new(dir));
        workspace
            .collection_dirs
            .iter()
            .find(|known| canonical(known) == wanted)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFound,
                    format!("{dir} is not a collection of the open workspace"),
                )
            })
    }

    /// Re-reads the workspace file and refreshes the known collection folders and the watcher.
    pub fn refresh_workspace(&self, dir: &Path) -> ApiResult<storage::WorkspaceOverview> {
        let overview = storage::load_overview(dir)?;
        let collection_dirs: Vec<PathBuf> = overview
            .collections
            .iter()
            .map(|c| PathBuf::from(&c.dir))
            .collect();

        if let Some(watcher) = self.watcher.lock().expect("watcher lock poisoned").as_mut() {
            let previous = self
                .workspace
                .lock()
                .expect("workspace lock poisoned")
                .clone();
            if let Some(previous) = previous {
                for old in previous.watched_dirs() {
                    watcher.unwatch(&old);
                }
            }
            // Global environments live in the workspace's own `environments/` folder.
            let global_envs = dir.join("environments");
            let watched = collection_dirs
                .iter()
                .cloned()
                .chain(std::iter::once(global_envs));
            for new in &watched.collect::<Vec<_>>() {
                // Let-chains (`if a && let Ok(..) = ..`) combine a condition and a pattern match.
                if new.exists()
                    && let Err(err) = watcher.watch(new)
                {
                    tracing::warn!(%err, "could not watch collection");
                }
            }
        }

        *self.workspace.lock().expect("workspace lock poisoned") = Some(OpenWorkspace {
            dir: dir.to_path_buf(),
            collection_dirs,
        });
        Ok(overview)
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
