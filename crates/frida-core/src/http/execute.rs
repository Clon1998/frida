//! The whole send pipeline for one request (PLAN.md §7). Steps: load the inherited context,
//! run pre-request scripts, build the variable scope, resolve variables, prepare, apply auth,
//! send, then run post-response scripts and tests.

use std::path::Path;
use std::sync::{Arc, Mutex};

use frida_script::ExecutionMode;
use tokio_util::sync::CancellationToken;

use super::cookies::CookieJar;
use super::prepare::{PreparedRequest, RequestContext, prepare};
use super::scripting::{
    EnvOverrides, Pipeline, RunnerFlow, RuntimeVars, ScriptContext, ScriptLog, ScriptOutcome,
    runtime_strings,
};
use super::send::{HttpResponse, SendError, build_client, send};
use crate::auth::{OAuth2Manager, TokenInfo, apply_auth, effective_auth};
use crate::codegen::{CurlOptions, curl_request, render_curl};
use crate::error::{CoreError, Result};
use crate::model::AuthScheme;
use crate::model::{Environment, Request};
use crate::storage;
use crate::vars::{ResolveReport, ResolvedContext, ScopeInput, resolve_context};

/// Everything needed to send a request.
pub struct ExecuteInput<'a> {
    pub collection_dir: &'a Path,
    /// Where the request lives in the collection; decides which folders it inherits from.
    pub request_rel: &'a str,
    /// The editor's current state, saved or not.
    pub request: &'a Request,
    /// Active environments, with secret values already filled in.
    pub collection_env: Option<&'a Environment>,
    pub global_env: Option<&'a Environment>,
    /// What scripts changed earlier in this session.
    pub session: Session<'a>,
    pub cookies: Option<Arc<CookieJar>>,
    /// Holds OAuth2 tokens across sends.
    pub oauth: &'a OAuth2Manager,
}

/// The session state scripts read and change: runtime variables and environment overrides.
/// Held by the app (one per collection, plus the global overrides) and shared by all sends.
#[derive(Clone, Copy)]
pub struct Session<'a> {
    pub runtime: &'a Mutex<RuntimeVars>,
    pub env_overrides: &'a EnvOverrides,
    pub global_overrides: &'a EnvOverrides,
    /// Where global environments live (for `setGlobalEnvVar(..., { persist: true })`).
    pub workspace_dir: Option<&'a Path>,
    /// Don't verify TLS certificates for any request of this session, whatever the
    /// collection says (`frida-run --insecure`, like `curl -k`).
    pub skip_tls_verify: bool,
}

/// Owns the session state for one collection (the app keeps one per collection; tests and a
/// future CLI can simply create one).
#[derive(Debug, Default)]
pub struct SessionStore {
    pub runtime: Mutex<RuntimeVars>,
    pub env_overrides: EnvOverrides,
    pub global_overrides: EnvOverrides,
    /// See [`Session::skip_tls_verify`].
    pub skip_tls_verify: bool,
}

impl SessionStore {
    /// With these runtime variables already set.
    pub fn with_runtime(vars: &[(&str, &str)]) -> Self {
        let runtime = vars
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect();
        Self {
            runtime: Mutex::new(runtime),
            ..Self::default()
        }
    }

    pub fn session<'a>(&'a self, workspace_dir: Option<&'a Path>) -> Session<'a> {
        Session {
            runtime: &self.runtime,
            env_overrides: &self.env_overrides,
            global_overrides: &self.global_overrides,
            workspace_dir,
            skip_tls_verify: self.skip_tls_verify,
        }
    }
}

/// A response plus what produced it.
pub struct Executed {
    pub response: HttpResponse,
    pub report: ResolveReport,
    /// The request exactly as sent (variables resolved, auth applied), for history.
    pub sent: PreparedRequest,
    /// Every secret value in the sent request (resolved secrets and auth values), to mask.
    pub sensitive: Vec<String>,
    /// Test results and post-response script errors.
    pub scripts: ScriptOutcome,
    /// What the scripts asked of a collection run (next request, stop).
    pub flow: RunnerFlow,
}

/// Resolves variables in the request without sending it, for the inspector.
pub fn inspect(input: &ExecuteInput<'_>) -> Result<ResolveReport> {
    Ok(load_and_resolve(input)?.report)
}

/// Sends the request. Script `console.log` output goes to `on_log` as it happens, so it is
/// shown even when the send fails later.
pub async fn execute(
    input: ExecuteInput<'_>,
    cancel: CancellationToken,
    on_log: &(dyn Fn(ScriptLog) + Send + Sync),
) -> Result<Executed> {
    execute_as(input, ExecutionMode::Standalone, cancel, on_log).await
}

/// Like [`execute`], with the execution mode scripts see (`req.getExecutionMode()`); the
/// collection runner uses [`ExecutionMode::Runner`].
pub async fn execute_as(
    input: ExecuteInput<'_>,
    mode: ExecutionMode,
    cancel: CancellationToken,
    on_log: &(dyn Fn(ScriptLog) + Send + Sync),
) -> Result<Executed> {
    let session = input.session;
    let script_ctx = ScriptContext {
        runtime: session.runtime,
        env_overrides: session.env_overrides,
        global_overrides: session.global_overrides,
        workspace_dir: session.workspace_dir,
        on_log,
        limits: frida_script::ScriptLimits::default(),
        mode,
    };
    let mut collection = storage::load_collection(input.collection_dir)?;
    // Everything below reads the TLS settings from here: the request, OAuth2 token calls and
    // `fri.sendRequest`.
    if session.skip_tls_verify {
        collection.settings.tls.verify = false;
    }
    let mut pipeline = Pipeline {
        collection_dir: input.collection_dir,
        collection,
        folders: storage::load_folder_chain(input.collection_dir, input.request_rel)?,
        request: input.request.clone(),
        collection_env: input.collection_env.map(|e| session.env_overrides.apply(e)),
        global_env: input.global_env.map(|e| session.global_overrides.apply(e)),
        cookies: input.cookies.clone(),
        cancel: cancel.clone(),
        ctx: &script_ctx,
    };

    let pre = pipeline.run_pre().await.map_err(CoreError::Script)?;
    if pre.skip {
        return Err(CoreError::Skipped {
            stop: pre.flow.stop,
        });
    }

    let runtime = runtime_strings(&session.runtime.lock().expect("runtime lock poisoned"));
    let resolved = resolve_context(&ScopeInput {
        runtime: &runtime,
        request: &pipeline.request,
        folders: &pipeline.folders,
        collection_env: pipeline.collection_env.as_ref(),
        collection: &pipeline.collection,
        global_env: pipeline.global_env.as_ref(),
    });
    for name in resolved.report.unresolved() {
        tracing::debug!(variable = name, "unresolved variable");
    }
    let ctx = RequestContext {
        collection_dir: input.collection_dir,
        collection: &resolved.collection,
        folders: &resolved.folders,
    };
    let mut prepared = prepare(ctx, &resolved.request)?;
    let mut sensitive = resolved.report.secret_values.clone();
    if !resolved.auth.is_empty() {
        // The token endpoint is reached with the collection's TLS settings, without cookies.
        let token_client = build_client(&prepared, None)
            .await?
            .build()
            .map_err(|e| SendError::Tls(e.to_string()))?;
        let auth_values = apply_auth(
            &mut prepared,
            &resolved.auth,
            input.oauth,
            &token_client,
            &cancel,
        )
        .await?;
        sensitive.extend(auth_values);
    }
    // The URL may contain secrets, so it is only logged at debug level (CLAUDE.md).
    tracing::debug!(method = %prepared.method, "sending request");
    let response = send(&prepared, input.cookies.clone(), cancel).await?;
    let (scripts, post_flow) = pipeline.run_post(&prepared, &response).await;
    // Pre-request scripts may have asked too; the post-response scripts have the last word.
    let mut flow = pre.flow;
    if post_flow.next_request.is_some() {
        flow.next_request = post_flow.next_request;
    }
    flow.stop |= post_flow.stop;
    Ok(Executed {
        response,
        report: resolved.report,
        sent: prepared,
        sensitive,
        scripts,
        flow,
    })
}

/// Fetches new tokens for every OAuth2 scheme that applies to the request ("Get new token"),
/// ignoring the cache. Returns all cached tokens afterwards, for the token viewer.
pub async fn fetch_tokens(
    input: ExecuteInput<'_>,
    cancel: CancellationToken,
) -> Result<Vec<TokenInfo>> {
    let resolved = load_and_resolve(&input)?;
    let ctx = RequestContext {
        collection_dir: input.collection_dir,
        collection: &resolved.collection,
        folders: &resolved.folders,
    };
    // Only for the TLS settings and timeout; nothing is sent to the request's URL.
    let prepared = prepare(ctx, &resolved.request)?;
    let client = build_client(&prepared, None)
        .await?
        .build()
        .map_err(|e| SendError::Tls(e.to_string()))?;
    for scheme in &resolved.auth {
        if let AuthScheme::OAuth2(config) = scheme {
            input.oauth.fetch_new(config, &client, &cancel).await?;
        }
    }
    Ok(input.oauth.list())
}

/// "Copy as curl" (PLAN.md §11.3) for the request, with variables resolved or kept as
/// `{{placeholders}}`. Secrets are masked as `***` unless `options.include_secrets`.
pub fn export_curl(input: &ExecuteInput<'_>, options: CurlOptions) -> Result<String> {
    if options.resolve_variables {
        let resolved = load_and_resolve(input)?;
        let ctx = RequestContext {
            collection_dir: input.collection_dir,
            collection: &resolved.collection,
            folders: &resolved.folders,
        };
        let (curl, auth_values) = curl_request(ctx, &resolved.request, &resolved.auth, input.oauth);
        let mut masked = resolved.report.secret_values;
        masked.extend(auth_values);
        if options.include_secrets {
            masked.clear();
        }
        Ok(render_curl(&curl, options.shell, &masked))
    } else {
        let collection = storage::load_collection(input.collection_dir)?;
        let folders = storage::load_folder_chain(input.collection_dir, input.request_rel)?;
        let folder_docs: Vec<_> = folders.iter().map(|(_, f)| f.clone()).collect();
        let ctx = RequestContext {
            collection_dir: input.collection_dir,
            collection: &collection,
            folders: &folder_docs,
        };
        let auth = effective_auth(input.request, &folders, &collection);
        let (curl, literal_values) = curl_request(ctx, input.request, &auth, input.oauth);
        // Placeholders hide secrets already; mask literal values typed into auth fields.
        let masked = if options.include_secrets {
            Vec::new()
        } else {
            literal_values
        };
        Ok(render_curl(&curl, options.shell, &masked))
    }
}

/// Resolves without running scripts (inspector, curl export), using what scripts set earlier.
fn load_and_resolve(input: &ExecuteInput<'_>) -> Result<ResolvedContext> {
    let collection = storage::load_collection(input.collection_dir)?;
    let folders = storage::load_folder_chain(input.collection_dir, input.request_rel)?;
    let session = input.session;
    let runtime = runtime_strings(&session.runtime.lock().expect("runtime lock poisoned"));
    let collection_env = input.collection_env.map(|e| session.env_overrides.apply(e));
    let global_env = input.global_env.map(|e| session.global_overrides.apply(e));
    Ok(resolve_context(&ScopeInput {
        runtime: &runtime,
        request: input.request,
        folders: &folders,
        collection_env: collection_env.as_ref(),
        collection: &collection,
        global_env: global_env.as_ref(),
    }))
}
