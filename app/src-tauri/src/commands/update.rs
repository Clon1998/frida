//! In-app updates (PLAN.md M14): thin glue around `tauri-plugin-updater`.
//!
//! The plugin reads `latest.json` from the newest **published** GitHub release, compares the
//! version and verifies the download with the update public key in `tauri.conf.json`
//! (`requireSignedVersion` also binds the signature to the announced version, so an altered
//! manifest can't offer an old release as a new one). Frida's own logic is small: check at most
//! once a day, never while a request or run is in flight, and let the download be cancelled.
//!
//! This lives in the app crate, not in `frida-core`, because the updater is a Tauri plugin;
//! the "once a day" rule itself is in `frida_core::local_state`.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;
use tokio_util::sync::CancellationToken;

use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;

/// A newer version that can be installed (or, see `install_blocker`, only downloaded by hand).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// The release notes (Markdown), when the release has any.
    pub notes: Option<String>,
    /// When the release was published, RFC 3339.
    pub date: Option<String>,
    /// Why this copy of Frida can't update itself (a development build, or a bare binary
    /// without an installer). `None`: "Install and restart" works.
    pub install_blocker: Option<String>,
}

/// Emitted while an update downloads. `total` is `None` when the server doesn't say.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// Bytes. `u32` (up to 4 GB) because TypeScript numbers can't hold every `u64`.
    pub downloaded: u32,
    pub total: Option<u32>,
}

/// Download progress is reported in steps of this many bytes, not for every chunk.
const PROGRESS_STEP: u64 = 256 * 1024;

/// Where the releases are, for "download it yourself" hints.
const RELEASES_URL: &str = "https://github.com/Clon1998/frida/releases/latest";

/// Looks for a newer version. `manual` is "Check for updates" in the settings; otherwise this
/// is the check on start, which does nothing unless it's switched on and due (once a day) and
/// never runs in development builds. Returns `None` when Frida is up to date (or the automatic
/// check was skipped).
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
    manual: bool,
) -> ApiResult<Option<UpdateInfo>> {
    if !manual {
        if cfg!(debug_assertions) {
            return Ok(None);
        }
        let mut settings = state.local.load_settings();
        let now = unix_now();
        if !settings.update_check_due(now) {
            return Ok(None);
        }
        // Remember the attempt before asking, so a network problem doesn't retry on every
        // start; the next try is tomorrow (or now, by hand).
        settings.last_update_check = Some(now);
        state.local.save_settings(&settings)?;
    }

    let updater = app.updater().map_err(update_error)?;
    let found = match updater.check().await {
        Ok(found) => found,
        // The automatic check is quiet: being offline is no reason for an error toast.
        Err(err) if !manual => {
            tracing::info!(%err, "update check failed");
            return Ok(None);
        }
        Err(err) => return Err(update_error(err)),
    };
    let info = found.as_ref().map(|u| update_info(&app, u));
    if let Some(info) = &info {
        tracing::info!(version = %info.version, "update available");
    }
    *state.update.lock().expect("update lock poisoned") = found;
    Ok(info)
}

/// Downloads and installs the update found by the last [`check_for_update`], then restarts
/// Frida. Progress arrives as [`UpdateProgress`] events; [`cancel_update`] stops the download.
/// Refused while requests or runs are in flight. On Windows the installer closes the app
/// itself.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> ApiResult<()> {
    let update = state
        .update
        .lock()
        .expect("update lock poisoned")
        .clone()
        .ok_or_else(|| {
            ApiError::new(ErrorCode::Update, "there is no update to install")
                .with_hint("Check for updates first.")
        })?;
    if let Some(blocker) = install_blocker(&app) {
        return Err(ApiError::new(ErrorCode::Update, blocker)
            .with_hint(format!("Download the new version from {RELEASES_URL}.")));
    }
    if !state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .is_empty()
    {
        return Err(ApiError::new(
            ErrorCode::Busy,
            "requests or a collection run are still in progress",
        )
        .with_hint("Wait until they finish (or cancel them), then install the update."));
    }

    let cancel = CancellationToken::new();
    *state.update_cancel.lock().expect("update lock poisoned") = Some(cancel.clone());
    let result = download(&app, &update, &cancel).await;
    *state.update_cancel.lock().expect("update lock poisoned") = None;
    let bytes = result?;

    // Installing replaces the app on disk (blocking file work), so off the async runtime. The
    // signature is checked again inside `install` before anything is touched.
    let installing = update.clone();
    tokio::task::spawn_blocking(move || installing.install(bytes))
        .await
        .map_err(|e| ApiError::new(ErrorCode::Update, format!("the installer stopped: {e}")))?
        .map_err(update_error)?;
    tracing::info!(version = %update.version, "update installed, restarting");
    app.restart()
}

/// Stops a running update download. Installing (after the download) can't be stopped.
#[tauri::command]
#[specta::specta]
pub fn cancel_update(state: State<'_, AppState>) {
    if let Some(cancel) = state
        .update_cancel
        .lock()
        .expect("update lock poisoned")
        .as_ref()
    {
        cancel.cancel();
    }
}

/// Downloads the update, reporting progress. `tokio::select!` waits for whichever finishes
/// first: the download or the cancel token. When the token wins, the download future is simply
/// dropped, which stops it (Rust futures do nothing unless they are polled).
async fn download(
    app: &AppHandle,
    update: &Update,
    cancel: &CancellationToken,
) -> ApiResult<Vec<u8>> {
    let mut downloaded: u64 = 0;
    let mut reported: u64 = 0;
    let on_chunk = |chunk: usize, total: Option<u64>| {
        downloaded += chunk as u64;
        if downloaded - reported >= PROGRESS_STEP || Some(downloaded) == total {
            reported = downloaded;
            let progress = UpdateProgress {
                downloaded: clamp_u32(downloaded),
                total: total.map(clamp_u32),
            };
            if let Err(err) = progress.emit(app) {
                tracing::debug!(%err, "could not emit update progress");
            }
        }
    };
    tokio::select! {
        result = update.download(on_chunk, || {}) => result.map_err(update_error),
        _ = cancel.cancelled() => Err(ApiError::new(ErrorCode::Cancelled, "the download was cancelled")),
    }
}

fn update_info(app: &AppHandle, update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        date: update.date.and_then(|d| {
            d.format(&time::format_description::well_known::Rfc3339)
                .ok()
        }),
        install_blocker: install_blocker(app),
    }
}

/// Why this copy can't replace itself, if it can't. The updater needs to know which installer
/// it came from (`.app`, NSIS, MSI, AppImage, `.deb`, `.rpm`); Tauri records that in bundled
/// builds only. And without the update public key nothing could be verified.
fn install_blocker(app: &AppHandle) -> Option<String> {
    if !has_update_key(app) {
        return Some("this build has no update key, so updates can't be verified".into());
    }
    if cfg!(debug_assertions) {
        return Some("this is a development build, which can't update itself".into());
    }
    if tauri::utils::platform::bundle_type().is_none() {
        return Some("this copy of Frida wasn't installed from an installer".into());
    }
    None
}

/// Whether `plugins.updater.pubkey` in `tauri.conf.json` is filled in.
fn has_update_key(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty())
}

/// Turns a plugin error into a message the user can act on.
fn update_error(err: tauri_plugin_updater::Error) -> ApiError {
    use tauri_plugin_updater::Error as E;
    let message = format!("update failed: {err}");
    let hint = match &err {
        E::Reqwest(_) | E::Network(_) | E::ReleaseNotFound => {
            "Check your internet connection and try again later."
        }
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => {
            "The download's signature didn't match, so nothing was installed. Try again later, or download the new version from GitHub."
        }
        E::AuthenticationFailed => {
            "Installing needs administrator rights. Try again and confirm the prompt."
        }
        E::TargetNotFound(_) | E::TargetsNotFound(_) => {
            "There is no update for this platform yet. Download the new version from GitHub."
        }
        _ => "Download the new version from GitHub instead.",
    };
    tracing::warn!(%err, "update failed");
    ApiError::new(ErrorCode::Update, message).with_hint(hint)
}

fn unix_now() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    clamp_u32(secs)
}

fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    /// The updater settings that keep updates safe, read from the real `tauri.conf.json` so a
    /// config edit can't silently weaken them.
    #[test]
    fn updater_config_is_strict() {
        let text = include_str!("../../tauri.conf.json");
        let config: serde_json::Value = serde_json::from_str(text).unwrap();
        let updater = &config["plugins"]["updater"];
        // An altered latest.json can't pair a new version number with an old (validly
        // signed) release.
        assert_eq!(updater["requireSignedVersion"], true);
        assert_ne!(updater["allowDowngrades"], true);
        assert_ne!(updater["dangerousInsecureTransportProtocol"], true);
        // Only published releases: GitHub's "latest" never points at a draft.
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/Clon1998/frida/releases/latest/download/latest.json"
            ])
        );
        // Without a public key nothing could be verified (`install_blocker` would refuse).
        let pubkey = updater["pubkey"].as_str().unwrap_or_default();
        assert!(!pubkey.trim().is_empty(), "plugins.updater.pubkey is empty");
        // Updater artifacts are only built in the release workflow, which has the private key.
        assert_ne!(config["bundle"]["createUpdaterArtifacts"], true);
    }
}
