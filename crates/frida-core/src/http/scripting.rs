//! Pre-request scripts, post-response scripts and tests in the send pipeline (PLAN.md §10).
//!
//! The scripts of the collection, then each folder (outermost first), then the request run one
//! after another, each in a fresh sandbox (`frida-script`). The state (request, variables)
//! flows from one to the next. Pre-request scripts see the request *before* `{{variables}}`
//! are resolved, like in Bruno, so changes they make are resolved afterwards.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use frida_script::{
    BoxFuture, EnvWrite, HttpCall, HttpReply, LogLevel, Script, ScriptError, ScriptHeader,
    ScriptHost, ScriptLimits, ScriptPhase, ScriptRequest, ScriptResponse, ScriptState, ScriptVars,
    TestResult, run_script,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;
use tokio_util::sync::CancellationToken;

use super::cookies::CookieJar;
use super::prepare::{PreparedBody, PreparedRequest, RequestContext, merge_headers};
use super::send::{HttpResponse, send};
use crate::mask::Masker;
use crate::model::{
    Auth, AuthScheme, BodyMode, Collection, EnvVariable, Environment, Folder, KeyValue, Request,
};
use crate::storage;
use crate::vars::{Resolver, ScopeInput, UsageLocation, VarScope, iso_timestamp};

/// Runtime variables (`fri.setVar`) of one collection, kept for the app session.
/// Values are JSON because scripts may store objects, like in Bruno.
pub type RuntimeVars = Map<String, Value>;

/// Session-only changes to environments made by `fri.setEnvVar` (without `persist`), by
/// environment name. `None` means the script deleted the variable.
#[derive(Debug, Default)]
pub struct EnvOverrides(Mutex<HashMap<String, BTreeMap<String, Option<String>>>>);

impl EnvOverrides {
    /// `env` with this session's changes applied.
    pub fn apply(&self, env: &Environment) -> Environment {
        let overrides = self.0.lock().expect("env overrides lock poisoned");
        let mut env = env.clone();
        if let Some(changes) = overrides.get(&env.name) {
            for (name, value) in changes {
                match value {
                    Some(value) => match env.variables.iter_mut().find(|v| &v.name == name) {
                        Some(var) => var.value = Some(value.clone()),
                        None => env.variables.push(EnvVariable {
                            name: name.clone(),
                            value: Some(value.clone()),
                            secret: false,
                            source: None,
                            enabled: true,
                        }),
                    },
                    None => env.variables.retain(|v| &v.name != name),
                }
            }
        }
        env
    }

    fn set(&self, env_name: &str, name: &str, value: Option<String>) {
        let mut overrides = self.0.lock().expect("env overrides lock poisoned");
        overrides
            .entry(env_name.to_string())
            .or_default()
            .insert(name.to_string(), value);
    }

    /// Forgets all session changes (e.g. "Reset runtime variables").
    pub fn clear(&self) {
        self.0.lock().expect("env overrides lock poisoned").clear();
    }
}

/// One `console.log` from a script, for the console panel. Secret values are masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScriptLog {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    /// e.g. `folder "Orders", pre-request script`.
    pub location: String,
    pub request_name: String,
}

/// What the post-response scripts and tests produced, shown with the response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOutcome {
    pub tests: Vec<TestResult>,
    /// Failures of post-response scripts and tests (the response is still shown).
    pub errors: Vec<ScriptError>,
}

/// Where the scripts' side effects go.
pub struct ScriptContext<'a> {
    pub runtime: &'a Mutex<RuntimeVars>,
    pub env_overrides: &'a EnvOverrides,
    pub global_overrides: &'a EnvOverrides,
    /// Where global environments live, for `setGlobalEnvVar(..., { persist: true })`.
    pub workspace_dir: Option<&'a Path>,
    /// Receives every `console.log` (already masked).
    pub on_log: &'a (dyn Fn(ScriptLog) + Send + Sync),
    pub limits: ScriptLimits,
}

/// What the scripts work on: the loaded (not yet resolved) collection, folders and request,
/// and the active environments with session overrides applied.
pub(crate) struct Pipeline<'a> {
    pub collection_dir: &'a Path,
    pub collection: Collection,
    /// `(path, folder)`, outermost first.
    pub folders: Vec<(String, Folder)>,
    pub request: Request,
    pub collection_env: Option<Environment>,
    pub global_env: Option<Environment>,
    pub cookies: Option<Arc<CookieJar>>,
    pub cancel: CancellationToken,
    pub ctx: &'a ScriptContext<'a>,
}

impl Pipeline<'_> {
    /// Every secret value the scripts could print, so logs can be masked.
    fn secrets(&self) -> Vec<String> {
        [&self.collection_env, &self.global_env]
            .into_iter()
            .flatten()
            .flat_map(|env| env.variables.iter())
            .filter(|v| v.secret)
            .filter_map(|v| v.value.clone())
            .collect()
    }

    /// The non-empty scripts for `phase`: collection, folders (outermost first), request.
    fn scripts(&self, phase: ScriptPhase) -> Vec<Script> {
        let source = |scripts: &crate::model::Scripts, tests: &str| -> String {
            match phase {
                ScriptPhase::Pre => scripts.pre.clone(),
                ScriptPhase::Post => scripts.post.clone(),
                ScriptPhase::Tests => tests.to_string(),
            }
        };
        let mut scripts = vec![Script {
            owner: format!("collection {:?}", self.collection.name),
            source: source(&self.collection.scripts, &self.collection.tests),
        }];
        for (_, folder) in &self.folders {
            scripts.push(Script {
                owner: format!("folder {:?}", folder.name),
                source: source(&folder.scripts, &folder.tests),
            });
        }
        scripts.push(Script {
            owner: format!("request {:?}", self.request.name),
            source: source(&self.request.scripts, &self.request.tests),
        });
        scripts.retain(|s| !s.source.trim().is_empty());
        scripts
    }

    fn vars(&self) -> ScriptVars {
        let runtime = self
            .ctx
            .runtime
            .lock()
            .expect("runtime lock poisoned")
            .clone();
        let env_map = |env: &Option<Environment>| -> BTreeMap<String, String> {
            env.iter()
                .flat_map(|e| e.variables.iter())
                .filter_map(|v| v.value.clone().map(|value| (v.name.clone(), value)))
                .collect()
        };
        let mut folder = BTreeMap::new();
        for (_, f) in &self.folders {
            folder.extend(key_values(&f.vars));
        }
        ScriptVars {
            runtime,
            env_name: self.collection_env.as_ref().map(|e| e.name.clone()),
            env: env_map(&self.collection_env),
            global_env_name: self.global_env.as_ref().map(|e| e.name.clone()),
            global_env: env_map(&self.global_env),
            collection: key_values(&self.collection.vars),
            folder,
            request: key_values(&self.request.vars),
        }
    }

    fn host(&self) -> Arc<CoreHost> {
        Arc::new(CoreHost {
            collection: self.collection.clone(),
            folders: self.folders.clone(),
            request: self.request.clone(),
            collection_env: self.collection_env.clone(),
            global_env: self.global_env.clone(),
            collection_dir: self.collection_dir.to_path_buf(),
            cookies: self.cookies.clone(),
            cancel: self.cancel.clone(),
        })
    }

    /// Runs the scripts in order and handles their side effects after each one.
    /// Stops at the first error when `stop_on_error`.
    async fn run_chain(
        &mut self,
        scripts: Vec<Script>,
        mut state: ScriptState,
        stop_on_error: bool,
    ) -> (ScriptState, Vec<TestResult>, Vec<ScriptError>) {
        let masker = Masker::new(&self.secrets());
        let mut tests = Vec::new();
        let mut errors = Vec::new();
        for script in scripts {
            let location = format!("{}, {}", script.owner, state.phase.label());
            let (mut out, error) =
                run_script(script, state.clone(), self.host(), self.ctx.limits).await;
            for log in out.logs.drain(..) {
                (self.ctx.on_log)(ScriptLog {
                    timestamp: now_iso(),
                    level: log.level,
                    message: masker.text(&log.message),
                    location: location.clone(),
                    request_name: self.request.name.clone(),
                });
            }
            tests.append(&mut out.tests);
            *self.ctx.runtime.lock().expect("runtime lock poisoned") = out.vars.runtime.clone();
            for write in out.env_writes.drain(..) {
                self.apply_env_write(&write, &masker);
            }
            state = out;
            if let Some(error) = error {
                let error = ScriptError {
                    message: masker.text(&error.message),
                    ..error
                };
                errors.push(error);
                if stop_on_error {
                    break;
                }
            }
        }
        (state, tests, errors)
    }

    /// `fri.setEnvVar`/`setGlobalEnvVar`: a session override, plus the file with `persist`.
    fn apply_env_write(&mut self, write: &EnvWrite, masker: &Masker) {
        let (env, overrides, dir) = if write.global {
            (
                &mut self.global_env,
                self.ctx.global_overrides,
                self.ctx.workspace_dir.map(Path::to_path_buf),
            )
        } else {
            (
                &mut self.collection_env,
                self.ctx.env_overrides,
                Some(self.collection_dir.to_path_buf()),
            )
        };
        let Some(env) = env else { return };
        overrides.set(&env.name, &write.name, write.value.clone());
        *env = overrides.apply(env);
        if !write.persist {
            return;
        }
        let is_secret = env
            .variables
            .iter()
            .any(|v| v.name == write.name && v.secret);
        let result = if is_secret {
            Err("secret variables are never written to files".to_string())
        } else {
            match dir {
                Some(dir) => persist_env_var(&dir, &env.name, &write.name, write.value.as_deref())
                    .map_err(|e| e.to_string()),
                None => Err("no workspace is open".to_string()),
            }
        };
        if let Err(message) = result {
            (self.ctx.on_log)(ScriptLog {
                timestamp: now_iso(),
                level: LogLevel::Warn,
                message: masker.text(&format!(
                    "could not save {:?} to environment {:?}: {message}",
                    write.name, env.name
                )),
                location: "environment".into(),
                request_name: self.request.name.clone(),
            });
        }
    }

    /// Runs the pre-request scripts and applies their changes to `self.request` (and the
    /// inherited headers). `Err` when a script failed; the request is then not sent.
    pub(crate) async fn run_pre(&mut self) -> Result<PreOutcome, ScriptError> {
        let scripts = self.scripts(ScriptPhase::Pre);
        if scripts.is_empty() {
            return Ok(PreOutcome::default());
        }
        let before = self.script_request();
        let state = new_state(
            ScriptPhase::Pre,
            &self.collection.name,
            before.clone(),
            None,
            self.vars(),
        );
        let (out, _, mut errors) = self.run_chain(scripts, state, true).await;
        if let Some(error) = errors.pop() {
            return Err(error);
        }
        self.apply_request_changes(&before, &out.req);
        Ok(PreOutcome {
            skip: out.skip_request,
        })
    }

    /// Runs post-response scripts, then tests.
    pub(crate) async fn run_post(
        &mut self,
        sent: &PreparedRequest,
        response: &HttpResponse,
    ) -> ScriptOutcome {
        let req = sent_request(&self.request, sent);
        let res = script_response(response);
        let mut outcome = ScriptOutcome::default();

        let post = self.scripts(ScriptPhase::Post);
        if !post.is_empty() {
            let state = new_state(
                ScriptPhase::Post,
                &self.collection.name,
                req.clone(),
                Some(res.clone()),
                self.vars(),
            );
            let (_, mut tests, errors) = self.run_chain(post, state, false).await;
            outcome.tests.append(&mut tests);
            outcome.errors.extend(errors);
        }
        let tests = self.scripts(ScriptPhase::Tests);
        if !tests.is_empty() {
            let state = new_state(
                ScriptPhase::Tests,
                &self.collection.name,
                req,
                Some(res),
                self.vars(),
            );
            let (_, mut results, errors) = self.run_chain(tests, state, false).await;
            outcome.tests.append(&mut results);
            outcome.errors.extend(errors);
        }
        outcome
    }

    /// The request as pre-request scripts see it: unresolved, with inherited headers merged.
    fn script_request(&self) -> ScriptRequest {
        let folders: Vec<Folder> = self.folders.iter().map(|(_, f)| f.clone()).collect();
        let ctx = RequestContext {
            collection_dir: self.collection_dir,
            collection: &self.collection,
            folders: &folders,
        };
        let r = &self.request;
        ScriptRequest {
            name: r.name.clone(),
            method: r.method.clone(),
            url: r.url.clone(),
            headers: merge_headers(ctx, r)
                .into_iter()
                .map(|(name, value)| ScriptHeader { name, value })
                .collect(),
            body: body_value(r),
            body_mode: body_mode_name(r.body.mode).to_string(),
            timeout_ms: r.settings.timeout_ms.map(u64::from),
            auth_mode: auth_mode(&r.auth).to_string(),
        }
    }

    /// Writes what the scripts changed back into the request that will be resolved and sent.
    fn apply_request_changes(&mut self, before: &ScriptRequest, after: &ScriptRequest) {
        let r = &mut self.request;
        if after.url != before.url {
            r.url = after.url.clone();
        }
        if after.method != before.method {
            r.method = after.method.clone();
        }
        if after.timeout_ms != before.timeout_ms {
            r.settings.timeout_ms = after.timeout_ms.and_then(|ms| u32::try_from(ms).ok());
        }
        if after.headers != before.headers {
            // The script's list is the complete, merged set: it replaces the inherited headers.
            r.headers = after
                .headers
                .iter()
                .map(|h| KeyValue::new(h.name.clone(), h.value.clone()))
                .collect();
            self.collection.headers.clear();
            for (_, folder) in &mut self.folders {
                folder.headers.clear();
            }
        }
        if after.body != before.body {
            set_body(r, &after.body);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PreOutcome {
    /// `fri.runner.skipRequest()` was called.
    pub skip: bool,
}

fn new_state(
    phase: ScriptPhase,
    collection_name: &str,
    req: ScriptRequest,
    res: Option<ScriptResponse>,
    vars: ScriptVars,
) -> ScriptState {
    ScriptState {
        phase,
        collection_name: collection_name.to_string(),
        req,
        res,
        vars,
        env_writes: Vec::new(),
        logs: Vec::new(),
        tests: Vec::new(),
        next_request: None,
        skip_request: false,
    }
}

fn key_values(rows: &[KeyValue]) -> BTreeMap<String, String> {
    rows.iter()
        .filter(|row| row.enabled && !row.name.trim().is_empty())
        .map(|row| (row.name.trim().to_string(), row.value.clone()))
        .collect()
}

fn body_mode_name(mode: BodyMode) -> &'static str {
    match mode {
        BodyMode::None => "none",
        BodyMode::Json => "json",
        BodyMode::Xml => "xml",
        BodyMode::Text => "text",
        BodyMode::FormUrlencoded => "formUrlEncoded",
        BodyMode::Multipart => "multipartForm",
        BodyMode::Binary => "file",
    }
}

fn auth_mode(auth: &Auth) -> &'static str {
    match auth {
        Auth::Inherit => "inherit",
        Auth::None => "none",
        Auth::Custom { schemes } => match schemes.first() {
            Some(AuthScheme::Basic { .. }) => "basic",
            Some(AuthScheme::Bearer { .. }) => "bearer",
            Some(AuthScheme::ApiKey { .. }) => "apikey",
            Some(AuthScheme::OAuth2(_)) => "oauth2",
            None => "none",
        },
    }
}

/// The body for `req.getBody()`: parsed JSON, a string, or an object of form fields.
fn body_value(r: &Request) -> Value {
    let fields = |rows: &[KeyValue]| -> Value {
        Value::Object(
            rows.iter()
                .filter(|row| row.enabled)
                .map(|row| (row.name.clone(), Value::String(row.value.clone())))
                .collect(),
        )
    };
    match r.body.mode {
        BodyMode::None | BodyMode::Binary => Value::Null,
        BodyMode::Json => serde_json::from_str(&r.body.json)
            .unwrap_or_else(|_| Value::String(r.body.json.clone())),
        BodyMode::Xml => Value::String(r.body.xml.clone()),
        BodyMode::Text => Value::String(r.body.text.clone()),
        BodyMode::FormUrlencoded => fields(&r.body.form_url_encoded),
        BodyMode::Multipart => Value::Object(
            r.body
                .multipart
                .iter()
                .filter(|f| f.enabled)
                .map(|f| (f.name.clone(), Value::String(f.value.clone())))
                .collect(),
        ),
    }
}

/// `req.setBody(...)`: strings replace the text of the current mode, objects become JSON
/// (or form fields in form mode).
fn set_body(r: &mut Request, body: &Value) {
    let as_text = |value: &Value| match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    match (r.body.mode, body) {
        (_, Value::Null) => r.body.mode = BodyMode::None,
        (BodyMode::FormUrlencoded, Value::Object(map)) => {
            r.body.form_url_encoded = map
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), json_to_plain(v)))
                .collect();
        }
        (BodyMode::Xml, value) => r.body.xml = as_text(value),
        (BodyMode::Text, value) => r.body.text = as_text(value),
        (BodyMode::Json, value) => r.body.json = as_text(value),
        (_, Value::String(text)) => {
            r.body.mode = BodyMode::Text;
            r.body.text = text.clone();
        }
        (_, value) => {
            r.body.mode = BodyMode::Json;
            r.body.json = as_text(value);
        }
    }
}

/// Strings as they are, anything else as JSON (for variables and form fields).
pub(crate) fn json_to_plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Runtime variables as the resolver needs them (text).
pub(crate) fn runtime_strings(runtime: &RuntimeVars) -> HashMap<String, String> {
    runtime
        .iter()
        .map(|(k, v)| (k.clone(), json_to_plain(v)))
        .collect()
}

/// The request as post-response scripts see it: as sent.
fn sent_request(request: &Request, sent: &PreparedRequest) -> ScriptRequest {
    let body = match &sent.body {
        PreparedBody::None => Value::Null,
        PreparedBody::Bytes(bytes) => {
            let text = String::from_utf8_lossy(bytes).to_string();
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        }
        PreparedBody::Form(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        ),
        PreparedBody::Multipart(_) => Value::Null,
    };
    ScriptRequest {
        name: request.name.clone(),
        method: sent.method.clone(),
        url: sent.url.clone(),
        headers: sent
            .headers
            .iter()
            .map(|(name, value)| ScriptHeader {
                name: name.clone(),
                value: value.clone(),
            })
            .collect(),
        body,
        body_mode: body_mode_name(request.body.mode).to_string(),
        timeout_ms: u64::try_from(sent.timeout.as_millis()).ok(),
        auth_mode: auth_mode(&request.auth).to_string(),
    }
}

fn script_response(response: &HttpResponse) -> ScriptResponse {
    ScriptResponse {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response
            .headers
            .iter()
            .map(|(name, value)| ScriptHeader {
                name: name.clone(),
                value: value.clone(),
            })
            .collect(),
        body: String::from_utf8_lossy(&response.body).to_string(),
        response_time_ms: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        url: response.final_url.clone(),
        size_bytes: response.body.len() as u64,
    }
}

/// Writes one variable to an environment file (`{ persist: true }`).
fn persist_env_var(
    dir: &Path,
    env_name: &str,
    name: &str,
    value: Option<&str>,
) -> crate::Result<()> {
    let Some(mut env) = storage::find_environment(dir, env_name)? else {
        return Ok(());
    };
    match value {
        Some(value) => match env.variables.iter_mut().find(|v| v.name == name) {
            Some(var) => var.value = Some(value.to_string()),
            None => env.variables.push(EnvVariable {
                name: name.to_string(),
                value: Some(value.to_string()),
                secret: false,
                source: None,
                enabled: true,
            }),
        },
        None => env.variables.retain(|v| v.name != name || v.secret),
    }
    storage::save_environment(dir, env_name, &env)
}

fn now_iso() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    iso_timestamp(millis)
}

/// The app side of the sandbox: real HTTP for `fri.sendRequest` and the variable resolver
/// for `fri.interpolate`. Holds owned copies (`'static`), because it is shared with the
/// script's thread.
struct CoreHost {
    collection: Collection,
    folders: Vec<(String, Folder)>,
    request: Request,
    collection_env: Option<Environment>,
    global_env: Option<Environment>,
    collection_dir: PathBuf,
    cookies: Option<Arc<CookieJar>>,
    cancel: CancellationToken,
}

impl ScriptHost for CoreHost {
    fn send_request(&self, call: HttpCall) -> BoxFuture<Result<HttpReply, String>> {
        let settings = &self.collection.settings;
        let prepared = PreparedRequest {
            method: call.method,
            url: super::prepare::normalize_url(&call.url),
            headers: call
                .headers
                .into_iter()
                .map(|h| (h.name, h.value))
                .collect(),
            body: match call.body {
                Some(body) => PreparedBody::Bytes(body.into_bytes()),
                None => PreparedBody::None,
            },
            timeout: Duration::from_millis(
                call.timeout_ms
                    .or(settings.timeout_ms.map(u64::from))
                    .unwrap_or(30_000),
            ),
            tls_verify: settings.tls.verify,
            ca_cert_paths: settings
                .tls
                .ca_cert_paths
                .iter()
                .map(|p| self.collection_dir.join(p))
                .collect(),
        };
        let cookies = self.cookies.clone();
        let cancel = self.cancel.clone();
        Box::pin(async move {
            let response = send(&prepared, cookies, cancel)
                .await
                .map_err(|e| e.to_string())?;
            Ok(HttpReply {
                status: response.status,
                status_text: response.status_text,
                headers: response
                    .headers
                    .into_iter()
                    .map(|(name, value)| ScriptHeader { name, value })
                    .collect(),
                body: String::from_utf8_lossy(&response.body).to_string(),
                response_time_ms: u64::try_from(response.elapsed.as_millis()).unwrap_or(0),
            })
        })
    }

    fn interpolate(&self, text: &str, vars: &ScriptVars) -> String {
        // The environments as the script currently sees them (after its own setEnvVar calls).
        let with_values = |env: &Option<Environment>, values: &BTreeMap<String, String>| {
            env.as_ref().map(|env| {
                let mut env = env.clone();
                env.variables
                    .retain(|v| values.contains_key(&v.name) || v.value.is_none());
                for (name, value) in values {
                    match env.variables.iter_mut().find(|v| &v.name == name) {
                        Some(var) => var.value = Some(value.clone()),
                        None => env.variables.push(EnvVariable {
                            name: name.clone(),
                            value: Some(value.clone()),
                            secret: false,
                            source: None,
                            enabled: true,
                        }),
                    }
                }
                env
            })
        };
        let collection_env = with_values(&self.collection_env, &vars.env);
        let global_env = with_values(&self.global_env, &vars.global_env);
        let runtime = runtime_strings(&vars.runtime);
        let scope = VarScope::build(&ScopeInput {
            runtime: &runtime,
            request: &self.request,
            folders: &self.folders,
            collection_env: collection_env.as_ref(),
            collection: &self.collection,
            global_env: global_env.as_ref(),
        });
        Resolver::new(&scope).resolve(text, UsageLocation::Body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_body_follows_the_body_mode() {
        let mut r = Request::new("a");
        r.body.mode = BodyMode::Json;
        set_body(&mut r, &serde_json::json!({ "a": 1 }));
        assert_eq!(r.body.json, "{\n  \"a\": 1\n}");

        r.body.mode = BodyMode::FormUrlencoded;
        set_body(&mut r, &serde_json::json!({ "x": "1", "n": 2 }));
        assert_eq!(r.body.form_url_encoded.len(), 2);
        let n = r
            .body
            .form_url_encoded
            .iter()
            .find(|f| f.name == "n")
            .unwrap();
        assert_eq!(n.value, "2");

        r.body.mode = BodyMode::None;
        set_body(&mut r, &Value::String("hi".into()));
        assert_eq!((r.body.mode, r.body.text.as_str()), (BodyMode::Text, "hi"));
    }

    #[test]
    fn overrides_change_set_and_remove_variables() {
        let mut env = Environment::new("dev");
        env.variables.push(EnvVariable {
            name: "a".into(),
            value: Some("1".into()),
            secret: false,
            source: None,
            enabled: true,
        });
        let overrides = EnvOverrides::default();
        overrides.set("dev", "a", None);
        overrides.set("dev", "b", Some("2".into()));
        overrides.set("prod", "c", Some("3".into()));
        let applied = overrides.apply(&env);
        let names: Vec<_> = applied.variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["b"]);
    }
}
