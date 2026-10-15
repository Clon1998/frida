//! The whole send pipeline for one request (PLAN.md §7). Steps: load the inherited context,
//! build the variable scope, resolve variables, prepare and send. Scripts (M5), auth (M3) and
//! history (M4) are added here in later milestones.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::cookies::CookieJar;
use super::prepare::{RequestContext, prepare};
use super::send::{HttpResponse, build_client, send};
use crate::auth::{OAuth2Manager, TokenInfo, apply_auth};
use crate::error::Result;
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
    pub runtime: &'a HashMap<String, String>,
    pub cookies: Option<Arc<CookieJar>>,
    /// Holds OAuth2 tokens across sends.
    pub oauth: &'a OAuth2Manager,
}

/// A response plus the variable report of the request that produced it.
pub struct Executed {
    pub response: HttpResponse,
    pub report: ResolveReport,
}

/// Resolves variables in the request without sending it, for the inspector.
pub fn inspect(input: &ExecuteInput<'_>) -> Result<ResolveReport> {
    Ok(load_and_resolve(input)?.report)
}

pub async fn execute(input: ExecuteInput<'_>, cancel: CancellationToken) -> Result<Executed> {
    let resolved = load_and_resolve(&input)?;
    for name in resolved.report.unresolved() {
        tracing::debug!(variable = name, "unresolved variable");
    }
    let ctx = RequestContext {
        collection_dir: input.collection_dir,
        collection: &resolved.collection,
        folders: &resolved.folders,
    };
    let mut prepared = prepare(ctx, &resolved.request)?;
    if !resolved.auth.is_empty() {
        // The token endpoint is reached with the collection's TLS settings, without cookies.
        let token_client = build_client(&prepared, None).await?;
        apply_auth(
            &mut prepared,
            &resolved.auth,
            input.oauth,
            &token_client,
            &cancel,
        )
        .await?;
    }
    // The URL may contain secrets, so it is only logged at debug level (CLAUDE.md).
    tracing::debug!(method = %prepared.method, "sending request");
    let response = send(&prepared, input.cookies.clone(), cancel).await?;
    Ok(Executed {
        response,
        report: resolved.report,
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
    let client = build_client(&prepared, None).await?;
    for scheme in &resolved.auth {
        if let AuthScheme::OAuth2(config) = scheme {
            input.oauth.fetch_new(config, &client, &cancel).await?;
        }
    }
    Ok(input.oauth.list())
}

fn load_and_resolve(input: &ExecuteInput<'_>) -> Result<ResolvedContext> {
    let collection = storage::load_collection(input.collection_dir)?;
    let folders = storage::load_folder_chain(input.collection_dir, input.request_rel)?;
    Ok(resolve_context(&ScopeInput {
        runtime: input.runtime,
        request: input.request,
        folders: &folders,
        collection_env: input.collection_env,
        collection: &collection,
        global_env: input.global_env,
    }))
}
