//! Process-wide app state, managed by Tauri and injected into commands as `State<AppState>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use frida_core::http::ResponseCache;
use frida_core::local_state::LocalStore;
use frida_core::storage;
use frida_core::watch::FsWatcher;
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
}

/// The currently open workspace and the folders of its collections.
#[derive(Debug, Clone)]
pub struct OpenWorkspace {
    pub dir: PathBuf,
    pub collection_dirs: Vec<PathBuf>,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            local: LocalStore::new(app_data_dir),
            workspace: Mutex::new(None),
            in_flight: Mutex::new(HashMap::new()),
            responses: Mutex::new(ResponseCache::default()),
            watcher: Mutex::new(None),
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
                for old in &previous.collection_dirs {
                    watcher.unwatch(old);
                }
            }
            for new in &collection_dirs {
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
