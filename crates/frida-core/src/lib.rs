//! `frida-core`: the heart of Frida.
//!
//! Domain model, YAML storage, variable resolution, request building, HTTP execution, auth,
//! importers and codegen all live here (see PLAN.md §3). This crate must never depend on Tauri,
//! so it stays testable and reusable for the future CLI.

mod about;
pub mod auth;
pub mod codegen;
pub mod error;
pub mod history;
pub mod http;
pub mod import;
pub mod jsonpath;
pub mod local_state;
mod mask;
pub mod model;
pub mod runner;
pub mod secrets;
pub mod storage;
pub mod vars;
pub mod watch;
pub mod yaml;

pub use about::{AboutInfo, about};
pub use error::{CoreError, Result};
