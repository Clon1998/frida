//! `frida-script`: the sandbox for pre-request scripts, post-response scripts and tests
//! (PLAN.md §10). Frida's API is `req`/`res`/`fri`; `bru` is an alias for `fri`, so a large
//! subset of Bruno's scripts runs unchanged.
//!
//! How it works:
//! - Every script runs in a **fresh QuickJS runtime** with a memory limit and a deadline.
//!   QuickJS has no file system, network or `require`; the only ways out are the few native
//!   functions registered in [`engine`] (`fri.sendRequest` goes through [`ScriptHost`]).
//! - The API itself (`req`, `res`, `fri`, `test`, `expect`, ...) is plain JavaScript in
//!   `prelude.js`, working on a [`ScriptState`] that is passed in and read back as JSON.
//!   That keeps the Rust side small and the API easy to read and extend.
//!
//! This crate knows nothing about HTTP or variables; `frida-core` implements [`ScriptHost`].

#![doc = "See `docs/scripting.md` for the user-facing API."]

mod engine;
mod model;
mod native;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub use engine::run_script;
pub use model::{
    EnvWrite, ExecutionMode, HttpCall, HttpReply, LogLevel, LogLine, ScriptError, ScriptHeader,
    ScriptPhase, ScriptRequest, ScriptResponse, ScriptState, ScriptVars, TestResult,
};

/// A boxed future that can move between threads. `async fn` in a trait object (`dyn`) is not
/// possible yet, so trait methods return this instead (like returning a `CompletableFuture`).
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// What the sandbox asks of the app.
///
/// `Send + Sync + 'static`: the host is shared with the script thread through an `Arc`, so it
/// must be safe to use from another thread and must not borrow anything short-lived.
pub trait ScriptHost: Send + Sync + 'static {
    /// `fri.sendRequest`: send an HTTP request. `Err` is a message for the script.
    fn send_request(&self, call: HttpCall) -> BoxFuture<Result<HttpReply, String>>;

    /// `fri.interpolate`: replace `{{vars}}` using the script's current variables.
    fn interpolate(&self, text: &str, vars: &ScriptVars) -> String;
}

/// Limits for one script (PLAN.md §10).
#[derive(Debug, Clone, Copy)]
pub struct ScriptLimits {
    /// Wall time for the whole script, including awaited requests and sleeps.
    pub timeout: Duration,
    pub memory_bytes: usize,
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            memory_bytes: 128 * 1024 * 1024,
        }
    }
}

/// A script to run and where it comes from (for error messages).
#[derive(Debug, Clone)]
pub struct Script {
    /// e.g. `folder "Orders"`.
    pub owner: String,
    pub source: String,
}
