//! Frida desktop app. This crate contains only Tauri glue: commands live in [`commands`] and
//! forward to the `frida-*` crates, where all the real logic lives.

mod commands;
mod logging;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// Where the generated TypeScript bindings are written, relative to this crate's directory.
///
/// Exporting only happens in debug builds and tests, so release builds compile it out
/// (otherwise the compiler warns about unused code).
#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../ui/src/bindings.ts";

/// Builds the tauri-specta builder that knows every command (and, later, event).
///
/// It's a separate function so both [`run`] and the `export_bindings` test can use it.
/// The `tauri::Wry` type parameter is the concrete webview runtime Tauri uses on desktop.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![commands::get_about_info])
}

/// Writes `app/ui/src/bindings.ts` from the registered commands and their types.
#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &Builder<tauri::Wry>) -> anyhow::Result<()> {
    // `CARGO_MANIFEST_DIR` makes the path independent of the working directory.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS_PATH);
    use anyhow::Context;
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
        .invoke_handler(builder.invoke_handler())
        .setup(|app| {
            let log_dir = app.path().app_data_dir()?.join("logs");
            let guard = logging::init(&log_dir)?;
            // Keep the guard alive for the whole app lifetime (see `logging::init`).
            app.manage(guard);
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
