//! The whole send pipeline for one request (PLAN.md §7). In M1: load the inherited context,
//! prepare and send. Later milestones add variables, scripts, auth, tests and history here.

use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::prepare::{RequestContext, prepare};
use super::send::{HttpResponse, send};
use crate::error::Result;
use crate::model::Request;
use crate::storage;

/// Sends `request` as if it lived at `request_rel` in the collection at `collection_dir`.
///
/// The request is passed in (instead of read from disk) because the UI sends the current,
/// possibly unsaved editor state. Its location still decides which folders it inherits from.
pub async fn execute(
    collection_dir: &Path,
    request_rel: &str,
    request: &Request,
    cancel: CancellationToken,
) -> Result<HttpResponse> {
    let collection = storage::load_collection(collection_dir)?;
    let folders = storage::load_folder_chain(collection_dir, request_rel)?;
    let ctx = RequestContext {
        collection_dir,
        collection: &collection,
        folders: &folders,
    };
    let prepared = prepare(ctx, request)?;
    tracing::debug!(method = %prepared.method, url = %prepared.url, "sending request");
    Ok(send(&prepared, cancel).await?)
}
