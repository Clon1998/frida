//! `frida-core`: the heart of Frida.
//!
//! Domain model, YAML storage, variable resolution, request building, HTTP execution, auth,
//! importers and codegen all live here (see PLAN.md §3). This crate must never depend on Tauri,
//! so it stays testable and reusable for the future CLI.

mod about;
pub mod error;
pub mod http;
pub mod local_state;
pub mod model;
pub mod storage;
pub mod watch;
pub mod yaml;

pub use about::{AboutInfo, about};
pub use error::{CoreError, Result};
