//! The data a script sees and changes. It is passed into the sandbox as JSON, the JS prelude
//! (`prelude.js`) builds the Bruno-style `req`, `res` and `bru` objects on top of it, and the
//! changed state comes back out as JSON after the script ran.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;

/// How the request is being sent, for `req.getExecutionMode()`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    /// One request, sent from its tab.
    #[default]
    Standalone,
    /// Part of a collection or folder run (PLAN.md M11).
    Runner,
}

/// When a script runs. Decides which objects exist (`res` only after the response).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ScriptPhase {
    /// Pre-request script: may change the request.
    Pre,
    /// Post-response script.
    Post,
    /// Tests, after the post-response scripts.
    Tests,
}

impl ScriptPhase {
    pub fn label(self) -> &'static str {
        match self {
            ScriptPhase::Pre => "pre-request script",
            ScriptPhase::Post => "post-response script",
            ScriptPhase::Tests => "tests",
        }
    }
}

/// One header of [`ScriptRequest`]. Kept as a list (not a map) to keep the order and casing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptHeader {
    pub name: String,
    pub value: String,
}

/// The request as scripts see it. In pre-request scripts it is not interpolated yet
/// (`{{vars}}` are still in it), like in Bruno.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    /// The effective headers, including inherited ones.
    pub headers: Vec<ScriptHeader>,
    /// JSON bodies are parsed (when valid), form bodies are objects, text bodies are strings.
    /// `null` when there is no body.
    pub body: Value,
    /// Body mode as in the request file (`json`, `text`, `formUrlEncoded`, ...).
    pub body_mode: String,
    pub timeout_ms: Option<u64>,
    pub auth_mode: String,
}

/// The response as post-response scripts and tests see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<ScriptHeader>,
    /// The body as text; the prelude parses JSON for `res.body`.
    pub body: String,
    pub response_time_ms: u64,
    pub url: String,
    pub size_bytes: u64,
}

/// The variables scripts can read and write.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptVars {
    /// `fri.getVar/setVar`. Any JSON value (Bruno allows objects).
    pub runtime: Map<String, Value>,
    pub env_name: Option<String>,
    /// The active collection environment, after earlier `setEnvVar` calls.
    pub env: BTreeMap<String, String>,
    pub global_env_name: Option<String>,
    pub global_env: BTreeMap<String, String>,
    pub collection: BTreeMap<String, String>,
    /// All folders' variables merged, nearest folder winning.
    pub folder: BTreeMap<String, String>,
    pub request: BTreeMap<String, String>,
}

/// An environment change from `fri.setEnvVar`, `setGlobalEnvVar` or their delete versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvWrite {
    /// `true` for the global environment.
    pub global: bool,
    pub name: String,
    /// `None` deletes the variable.
    pub value: Option<String>,
    /// Also write it to the environment file (`{ persist: true }`), not only for this session.
    pub persist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

/// One `console.log` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub level: LogLevel,
    pub message: String,
}

/// One `test(...)` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    /// Why it failed (the assertion message).
    pub error: Option<String>,
}

/// Everything shared with the sandbox. Inputs and outputs live together so the JS side can
/// simply mutate one object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptState {
    pub phase: ScriptPhase,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    pub collection_name: String,
    pub req: ScriptRequest,
    pub res: Option<ScriptResponse>,
    pub vars: ScriptVars,
    // Outputs, appended to by the script:
    #[serde(default)]
    pub env_writes: Vec<EnvWrite>,
    #[serde(default)]
    pub logs: Vec<LogLine>,
    #[serde(default)]
    pub tests: Vec<TestResult>,
    /// `fri.setNextRequest(name)`: in a run, continue with the request called `name`.
    #[serde(default)]
    pub next_request: Option<String>,
    /// `fri.runner.skipRequest()` in a pre-request script: don't send.
    #[serde(default)]
    pub skip_request: bool,
    /// `fri.runner.stopExecution()` or `fri.setNextRequest(null)`: end the run after this request.
    #[serde(default)]
    pub stop_execution: bool,
}

/// A request made by `fri.sendRequest` (axios-style options, like Bruno).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpCall {
    pub method: String,
    pub url: String,
    pub headers: Vec<ScriptHeader>,
    /// Text body; objects were JSON-encoded by the prelude.
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// The answer to an [`HttpCall`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpReply {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<ScriptHeader>,
    pub body: String,
    pub response_time_ms: u64,
}

/// A script failed: threw, timed out or ran out of memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScriptError {
    /// Where the script lives, e.g. `request "Login", pre-request script`.
    pub location: String,
    pub message: String,
    /// 1-based line in the script, when known.
    pub line: Option<u32>,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} (line {line}): {}", self.location, self.message),
            None => write!(f, "{}: {}", self.location, self.message),
        }
    }
}
