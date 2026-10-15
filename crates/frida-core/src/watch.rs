//! Watching collection folders for changes made outside Frida (git pull, an editor).

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::error::{CoreError, Result};

/// Changes arriving within this window are reported together, so a `git pull` that touches
/// 50 files triggers one reload instead of 50.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Watches folders recursively and calls back with the changed paths.
///
/// The callback runs on the debouncer's own background thread, which is why it must be
/// `Send + 'static`: it gets moved to another thread (`Send`) and may live as long as the
/// watcher (`'static`, i.e. it can't borrow short-lived local variables). Java lambdas have
/// similar rules (captured variables must be effectively final), just checked differently.
pub struct FsWatcher {
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    /// The watched folders, shared with the callback thread.
    ///
    /// `Arc<Mutex<T>>` is Rust's standard way to share mutable data between threads:
    /// `Arc` is a thread-safe reference-counted pointer (several owners of one value), and
    /// `Mutex` makes sure only one thread at a time can access the value inside, similar to
    /// a `synchronized` block in Java, except the compiler won't let you touch the data
    /// without holding the lock.
    roots: Arc<Mutex<Vec<PathBuf>>>,
}

impl FsWatcher {
    pub fn new(on_change: impl Fn(Vec<PathBuf>) + Send + 'static) -> Result<Self> {
        let roots: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
        // A second handle to the same list, moved into the callback closure.
        let callback_roots = Arc::clone(&roots);
        let debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let Ok(events) = result else {
                return;
            };
            let roots = callback_roots
                .lock()
                .expect("watcher roots lock poisoned")
                .clone();
            let mut paths: Vec<PathBuf> = events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    )
                })
                .flat_map(|event| event.paths.iter().cloned())
                .filter(|path| is_relevant(&roots, path))
                .collect();
            paths.sort();
            paths.dedup();
            if !paths.is_empty() {
                on_change(paths);
            }
        })
        .map_err(|e| CoreError::InvalidInput(format!("could not start the file watcher: {e}")))?;
        Ok(Self { debouncer, roots })
    }

    pub fn watch(&mut self, dir: &Path) -> Result<()> {
        self.debouncer
            .watch(dir, RecursiveMode::Recursive)
            .map_err(|e| {
                CoreError::InvalidInput(format!("could not watch {}: {e}", dir.display()))
            })?;
        // The OS reports real paths, so store the canonical form (macOS: /var -> /private/var).
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.roots
            .lock()
            .expect("watcher roots lock poisoned")
            .push(canonical);
        Ok(())
    }

    pub fn unwatch(&mut self, dir: &Path) {
        // Unwatching a folder that is already gone is fine.
        let _ = self.debouncer.unwatch(dir);
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.roots
            .lock()
            .expect("watcher roots lock poisoned")
            .retain(|root| *root != canonical && root != dir);
    }
}

/// Ignores hidden entries *inside* a watched folder (`.git/...`, our own `.tmpXXXX` files from
/// atomic writes). The watched folder itself may live under a hidden folder such as `~/.config`.
fn is_relevant(roots: &[PathBuf], path: &Path) -> bool {
    let relative = roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);
    !relative.components().any(|c| match c {
        Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn reports_changed_files_and_ignores_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize: on macOS the temp dir is a symlink (/var -> /private/var), and the
        // watcher reports the real path.
        let root = dir.path().canonicalize().unwrap();

        // A channel connects the watcher thread to this test thread, like a BlockingQueue.
        let (tx, rx) = mpsc::channel();
        let mut watcher = FsWatcher::new(move |paths| {
            let _ = tx.send(paths);
        })
        .unwrap();
        watcher.watch(&root).unwrap();
        // Give the OS watcher a moment to start before writing.
        std::thread::sleep(Duration::from_millis(200));

        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/index"), "x").unwrap();
        std::fs::write(root.join("get-order.yaml"), "version: 1").unwrap();

        let paths = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("no change reported");
        assert!(paths.contains(&root.join("get-order.yaml")), "{paths:?}");
        assert!(
            paths.iter().all(|p| !p.starts_with(root.join(".git"))),
            "{paths:?}"
        );
    }

    #[test]
    fn only_hidden_entries_below_the_root_are_ignored() {
        let roots = [PathBuf::from("/home/me/.config/col")];
        assert!(is_relevant(
            &roots,
            Path::new("/home/me/.config/col/orders/get.yaml")
        ));
        assert!(!is_relevant(
            &roots,
            Path::new("/home/me/.config/col/.git/HEAD")
        ));
        assert!(!is_relevant(
            &roots,
            Path::new("/home/me/.config/col/orders/.tmpAbC123")
        ));
    }
}
