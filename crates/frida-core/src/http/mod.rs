//! HTTP execution (PLAN.md §7): prepare a request, send it, and present the response.

mod console;
mod cookies;
mod decode;
mod execute;
mod prepare;
mod scripting;
mod send;
mod view;

pub use console::{ConsoleBody, ConsoleEntry, ConsoleResponse, MAX_CONSOLE_BODY};
pub use cookies::{CookieJar, CookieView, clear_cookies, list_cookies};
pub use execute::{
    ExecuteInput, Executed, Session, SessionStore, execute, export_curl, fetch_tokens, inspect,
};
pub use prepare::{
    DEFAULT_TIMEOUT, PreparedBody, PreparedPart, PreparedRequest, RequestContext, prepare,
};
pub(crate) use prepare::{apply_path_params, merge_headers, normalize_url};
pub use scripting::{EnvOverrides, RuntimeVars, ScriptLog, ScriptOutcome};
pub use send::{HttpResponse, SendError, send};
pub use view::{MAX_DISPLAY_BYTES, ResponseCache, ResponseView};
