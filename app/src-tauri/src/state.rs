//! Process-wide app state, managed by Tauri and injected into commands as `State<AppState>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use frida_core::auth::OAuth2Manager;
use frida_core::history::HistoryStore;
use frida_core::http::{CookieJar, ResponseCache};
use frida_core::local_state::LocalStore;
use frida_core::storage;
use frida_core::watch::FsWatcher;
use frida_secrets::{EncryptedFileStore, KeychainStore, SecretStore};
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
    /// Variables set by scripts (`bru.setVar`), per collection folder. In memory only.
    pub runtime_vars: Mutex<HashMap<PathBuf, HashMap<String, String>>>,
    /// One cookie jar per collection folder, in memory only.
    pub cookie_jars: Mutex<HashMap<PathBuf, Arc<CookieJar>>>,
    /// OAuth2 tokens, in memory, shared by all collections (tokens are keyed by provider).
    pub oauth: OAuth2Manager,
    /// Request history. `Arc` so it can be moved into `spawn_blocking` closures. `None` if the
    /// database couldn't be opened (history is then simply off; sending still works).
    pub history: Option<Arc<HistoryStore>>,
}

/// Where secret values are kept on this machine (PLAN.md §6).
pub enum SecretBackend {
    /// The OS keychain (the normal case).
    Keychain(Arc<KeychainStore>),
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

impl AppState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let secrets = if frida_secrets::keychain_available() {
            SecretBackend::Keychain(Arc::new(KeychainStore))
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
        Self {
            history,
            local: LocalStore::new(app_data_dir),
            workspace: Mutex::new(None),
            in_flight: Mutex::new(HashMap::new()),
            responses: Mutex::new(ResponseCache::default()),
            watcher: Mutex::new(None),
            secrets,
            runtime_vars: Mutex::new(HashMap::new()),
            cookie_jars: Mutex::new(HashMap::new()),
            // The authorization code flow opens the system browser for the user to sign in.
            oauth: OAuth2Manager::new(Arc::new(|url: &str| {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
            })),
        }
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

    /// A copy of a collection's runtime variables.
    pub fn runtime_vars(&self, collection_dir: &Path) -> HashMap<String, String> {
        self.runtime_vars
            .lock()
            .expect("runtime vars lock poisoned")
            .get(collection_dir)
            .cloned()
            .unwrap_or_default()
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
