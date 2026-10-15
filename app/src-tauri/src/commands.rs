//! Tauri commands: thin glue only. Each command forwards to a `frida-*` crate.
//!
//! `#[tauri::command]` makes a function callable from the UI via `invoke(...)`, and
//! `#[specta::specta]` records its signature so tauri-specta can generate a typed TS wrapper
//! (`commands.getAboutInfo()` in `app/ui/src/bindings.ts`).

use frida_core::AboutInfo;

/// Returns the product name and core library version.
#[tauri::command]
#[specta::specta]
pub fn get_about_info() -> AboutInfo {
    frida_core::about()
}
