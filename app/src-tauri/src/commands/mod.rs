//! Tauri commands: thin glue only. Each command validates its input, forwards to a `frida-*`
//! crate and converts errors to [`crate::error::ApiError`].
//!
//! `#[tauri::command]` makes a function callable from the UI, and `#[specta::specta]` records
//! its signature so tauri-specta can generate a typed TS wrapper in `app/ui/src/bindings.ts`
//! (`get_tree` becomes `commands.getTree(...)`).

pub mod app;
pub mod collection;
pub mod env;
pub mod git;
pub mod history;
pub mod http;
pub mod runner;
pub mod workspace;
