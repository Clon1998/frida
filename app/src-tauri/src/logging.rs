//! Logging setup: `tracing` events go to a daily-rotating file in `<app data dir>/logs`
//! and, in debug builds, also to stderr.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// How many daily log files to keep before the oldest is deleted.
const MAX_LOG_FILES: usize = 7;

/// Initializes the global `tracing` subscriber.
///
/// The returned [`WorkerGuard`] must be kept alive for as long as the app runs. The file writer
/// runs on a background thread, and dropping the guard flushes and stops it. In Rust a value is
/// "dropped" (destroyed, like a deterministic `close()` in Java) when its owner goes out of scope,
/// so the caller stores the guard in Tauri's managed state, which lives until the app exits.
///
/// The log level comes from the `RUST_LOG` environment variable and defaults to `info`.
pub fn init(log_dir: &Path) -> anyhow::Result<WorkerGuard> {
    // The appender prunes old files on startup and complains if the directory is missing.
    std::fs::create_dir_all(log_dir)?;
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("frida")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)?;
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);

    // `Option<Layer>` is itself a layer: `None` simply does nothing. That is how the stderr
    // layer is switched on only in debug builds without needing two different code paths.
    let stderr_layer = cfg!(debug_assertions).then(tracing_subscriber::fmt::layer);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()?;

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `init` installs a process-wide subscriber, so this must stay the only test that calls it.
    #[test]
    fn init_creates_missing_log_dir() {
        let dir = std::env::temp_dir().join(format!("frida-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let guard = init(&dir.join("logs")).unwrap();
        tracing::info!("hello from test");
        drop(guard); // flushes the background writer

        let files: Vec<_> = std::fs::read_dir(dir.join("logs")).unwrap().collect();
        assert_eq!(files.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
