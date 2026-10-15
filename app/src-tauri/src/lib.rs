//! Frida desktop app. This crate contains only Tauri glue: commands live in [`commands`] and
//! forward to the `frida-*` crates, where all the real logic lives.

mod commands;
mod error;
mod logging;
mod state;

use frida_core::watch::FsWatcher;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_specta::{Builder, Event, collect_commands, collect_events};

use crate::state::AppState;

/// Where the generated TypeScript bindings are written, relative to this crate's directory.
///
/// Exporting only happens in debug builds and tests, so release builds compile it out
/// (otherwise the compiler warns about unused code).
#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../ui/src/bindings.ts";

/// Emitted when files in a watched collection change on disk (git pull, an editor, or Frida
/// itself). The UI reloads the tree and checks open tabs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct FsChanged {
    /// Absolute paths of the changed files and folders.
    pub paths: Vec<String>,
}

/// Builds the tauri-specta builder that knows every command and event.
///
/// It's a separate function so both [`run`] and the `export_bindings` test can use it.
/// The `tauri::Wry` type parameter is the concrete webview runtime Tauri uses on desktop.
fn specta_builder() -> Builder<tauri::Wry> {
    use commands::{app, collection, env, history, http, workspace};

    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app::get_about_info,
            app::get_settings,
            app::set_theme,
            app::get_collection_state,
            app::save_collection_state,
            workspace::create_workspace,
            workspace::open_workspace,
            workspace::reload_workspace,
            workspace::close_workspace,
            workspace::create_collection,
            workspace::add_collection,
            workspace::remove_collection,
            collection::read_collection,
            collection::save_collection,
            collection::get_tree,
            collection::read_request,
            collection::save_request,
            collection::read_folder,
            collection::save_folder,
            collection::create_request,
            collection::create_request_from,
            collection::create_folder,
            collection::rename_item,
            collection::duplicate_request,
            collection::delete_item,
            collection::move_item,
            collection::reveal_item,
            env::list_environments,
            env::create_environment,
            env::save_environment,
            env::delete_environment,
            env::set_secret,
            env::delete_secret,
            env::reveal_secret,
            env::get_active_environments,
            env::set_active_environment,
            env::secret_backend_status,
            env::unlock_secret_file,
            env::clear_runtime_vars,
            env::list_cookies,
            env::clear_cookies,
            history::list_history,
            history::get_history_entry,
            history::clear_history,
            history::restore_history_entry,
            history::export_curl,
            history::import_curl,
            http::send_request,
            http::inspect_request,
            http::fetch_oauth2_tokens,
            http::list_oauth2_tokens,
            http::clear_oauth2_tokens,
            http::cancel_request,
            http::save_response_body,
        ])
        .events(collect_events![FsChanged])
}

/// Writes `app/ui/src/bindings.ts` from the registered commands and their types.
#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &Builder<tauri::Wry>) -> anyhow::Result<()> {
    use anyhow::Context;
    // `CARGO_MANIFEST_DIR` makes the path independent of the working directory.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS_PATH);
    builder
        .export(specta_typescript::Typescript::default(), &path)
        .with_context(|| format!("failed to export TypeScript bindings to {}", path.display()))
}

/// Starts the desktop app. Called from `main.rs`.
pub fn run() {
    let builder = specta_builder();

    // Regenerate bindings on every debug start so the UI types never drift from Rust.
    #[cfg(debug_assertions)]
    export_bindings(&builder).expect("failed to export TypeScript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let data_dir = app.path().app_data_dir()?;
            let guard = logging::init(&data_dir.join("logs"))?;
            // Keep the guard alive for the whole app lifetime (see `logging::init`).
            app.manage(guard);

            let state = AppState::new(data_dir);
            // The watcher calls back on its own thread. `AppHandle` is cheap to clone and
            // safe to use from any thread, so the closure owns its own copy.
            let handle = app.handle().clone();
            let watcher = FsWatcher::new(move |paths| {
                let paths = paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                if let Err(err) = (FsChanged { paths }).emit(&handle) {
                    tracing::warn!(%err, "could not emit fs-changed event");
                }
            });
            match watcher {
                Ok(watcher) => {
                    *state.watcher.lock().expect("watcher lock poisoned") = Some(watcher);
                }
                Err(err) => tracing::warn!(%err, "file watcher disabled"),
            }
            app.manage(state);

            tracing::info!(version = env!("CARGO_PKG_VERSION"), "Frida started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Frida application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Running `cargo test` regenerates the bindings. CI then fails if the committed
    /// `bindings.ts` differs, which catches forgotten regenerations.
    #[test]
    fn export_bindings_succeeds() {
        export_bindings(&specta_builder()).unwrap();
    }
}
