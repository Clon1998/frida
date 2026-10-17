//! A mock OAuth2 provider for the auth tests (PLAN.md §15, M3).
//!
//! - `GET  /oauth/authorize` approves immediately and redirects to `redirect_uri` with a code
//!   (like a user who is already logged in and clicks "Allow").
//! - `POST /oauth/token` supports `client_credentials`, `password`, `authorization_code` (with
//!   PKCE S256 verification) and `refresh_token`.
//!
//! Clients: see [`CLIENTS`] (all with the secret `secret`). User: `alice` / `wonderland`.
//! Client credentials may be sent as an HTTP Basic header or in the form body.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Default)]
struct OAuthState {
    issued: u32,
    /// code → (PKCE challenge, redirect_uri)
    codes: HashMap<String, (Option<String>, String)>,
    refresh_tokens: HashMap<String, String>, // refresh token → client id
}

type Shared = Arc<Mutex<OAuthState>>;

pub fn router() -> Router {
    Router::new()
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token))
        .with_state(Shared::default())
}

/// The clients the provider knows (all with the secret `secret`):
/// - `frida`: tokens live an hour;
/// - `short-lived`: tokens expire after 1 s and come with a refresh token;
/// - `no-refresh`: short-lived, without a refresh token (the client must fetch a new one);
/// - `refresh-fails`: short-lived, and its refresh tokens are always rejected.
const CLIENTS: [&str; 4] = ["frida", "short-lived", "no-refresh", "refresh-fails"];

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: Option<String>,
}

async fn authorize(State(state): State<Shared>, Query(q): Query<AuthorizeQuery>) -> Response {
    if !CLIENTS.contains(&q.client_id.as_str()) {
        return (StatusCode::BAD_REQUEST, "unknown client").into_response();
    }
    let mut s = state.lock().unwrap();
    s.issued += 1;
    let code = format!("code-{}", s.issued);
    s.codes
        .insert(code.clone(), (q.code_challenge, q.redirect_uri.clone()));
    let sep = if q.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    let state_param = q.state.map(|st| format!("&state={st}")).unwrap_or_default();
    Redirect::to(&format!("{}{sep}code={code}{state_param}", q.redirect_uri)).into_response()
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    username: Option<String>,
    password: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, axum::Json(json!({ "error": code }))).into_response()
}

/// Client id from the Basic header, or from the form body.
fn client(headers: &HeaderMap, form: &TokenForm) -> Option<String> {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b64| STANDARD.decode(b64).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let (id, secret) = match basic {
        Some(pair) => {
            let (id, secret) = pair.split_once(':')?;
            (id.to_string(), secret.to_string())
        }
        None => (
            form.client_id.clone()?,
            form.client_secret.clone().unwrap_or_default(),
        ),
    };
    (CLIENTS.contains(&id.as_str()) && secret == "secret").then_some(id)
}

async fn token(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(client_id) = client(&headers, &form) else {
        return error(StatusCode::UNAUTHORIZED, "invalid_client");
    };
    let mut s = state.lock().unwrap();
    match form.grant_type.as_str() {
        "client_credentials" => {}
        "password" => {
            if form.username.as_deref() != Some("alice")
                || form.password.as_deref() != Some("wonderland")
            {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
        }
        "authorization_code" => {
            let Some((challenge, redirect_uri)) =
                form.code.as_ref().and_then(|c| s.codes.remove(c))
            else {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            if form.redirect_uri.as_deref() != Some(redirect_uri.as_str()) {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            if let Some(challenge) = challenge {
                let verifier = form.code_verifier.clone().unwrap_or_default();
                let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                if computed != challenge {
                    return error(StatusCode::BAD_REQUEST, "invalid_grant");
                }
            }
        }
        "refresh_token" => {
            let known = form
                .refresh_token
                .as_ref()
                .and_then(|r| s.refresh_tokens.remove(r));
            if known.as_deref() != Some(client_id.as_str()) || client_id == "refresh-fails" {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
        }
        _ => return error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    }

    s.issued += 1;
    let n = s.issued;
    let expires_in = if client_id == "frida" { 3600 } else { 1 };
    let mut body = json!({
        "access_token": format!("{}-token-{n}", form.grant_type),
        "token_type": "Bearer",
        "expires_in": expires_in,
        "scope": form.scope.unwrap_or_default(),
    });
    if client_id != "no-refresh" {
        let refresh = format!("refresh-{n}");
        s.refresh_tokens.insert(refresh.clone(), client_id.clone());
        body["refresh_token"] = json!(refresh);
    }
    axum::Json(body).into_response()
}
