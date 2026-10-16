//! OAuth2 tokens (PLAN.md §8): fetching, caching and refreshing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio_util::sync::CancellationToken;

use super::jwt::decode_claims;
use super::loopback::Loopback;
use super::pkce;
use crate::model::{OAuth2ClientAuth, OAuth2Config, OAuth2Grant};
use crate::vars::iso_timestamp;

/// Tokens are refreshed this long before they expire (PLAN.md §8).
pub const REFRESH_SKEW: Duration = Duration::from_secs(30);

/// How long to wait for the user to finish signing in in the browser.
const BROWSER_TIMEOUT: Duration = Duration::from_secs(300);

/// Opens a URL in the system browser. Provided by the app (Tauri's opener) or by tests.
pub type BrowserOpener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Tokens are shared between requests with the same token URL, client, scope, grant and user.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TokenKey {
    token_url: String,
    client_id: String,
    scope: String,
    grant: String,
    username: String,
}

impl TokenKey {
    fn of(config: &OAuth2Config) -> Self {
        Self {
            token_url: config.token_url.clone(),
            client_id: config.client_id.clone(),
            scope: config.scope.clone(),
            grant: grant_name(config.grant).to_string(),
            username: config.username.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<SystemTime>,
    pub scope: Option<String>,
}

impl Token {
    /// Expired, or about to expire within [`REFRESH_SKEW`].
    fn needs_refresh(&self) -> bool {
        self.expires_at
            .is_some_and(|at| at <= SystemTime::now() + REFRESH_SKEW)
    }
}

/// A cached token as the token viewer shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub token_url: String,
    pub client_id: String,
    pub scope: String,
    pub grant: String,
    pub username: String,
    pub access_token: String,
    pub token_type: String,
    pub has_refresh_token: bool,
    /// ISO timestamp, or `None` if the provider didn't say.
    pub expires_at: Option<String>,
    pub expired: bool,
    /// Pretty-printed JWT claims, when the access token is a JWT.
    pub claims: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuth2Error {
    #[error("OAuth2 is missing {0}")]
    Missing(&'static str),
    #[error("the token request failed: {0}")]
    Request(String),
    #[error("the token endpoint returned {status}: {body}")]
    Rejected { status: u16, body: String },
    #[error("the token response has no access_token: {0}")]
    BadResponse(String),
    #[error("signing in in the browser failed: {0}")]
    Browser(String),
    #[error("signing in was cancelled")]
    Cancelled,
}

/// Holds OAuth2 tokens in memory and fetches or refreshes them when needed.
pub struct OAuth2Manager {
    tokens: Mutex<HashMap<TokenKey, Token>>,
    /// One lock per token, held while fetching or refreshing it, so requests that need the same
    /// token at the same time wait for one fetch instead of each starting their own (for the
    /// authorization code grant: one browser sign-in, not one per request).
    ///
    /// A `tokio::sync::Mutex`, because it is held across the `.await` of the HTTP call; a
    /// `std::sync::Mutex` must never be (it would block the runtime thread).
    fetching: Mutex<HashMap<TokenKey, Arc<tokio::sync::Mutex<()>>>>,
    open_browser: BrowserOpener,
}

/// The raw token endpoint response (RFC 6749 §5.1).
#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl OAuth2Manager {
    pub fn new(open_browser: BrowserOpener) -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            fetching: Mutex::new(HashMap::new()),
            open_browser,
        }
    }

    /// A valid token for `config`: from the cache, refreshed, or newly fetched.
    /// `client` carries the collection's TLS settings.
    pub async fn token(
        &self,
        config: &OAuth2Config,
        client: &reqwest::Client,
        cancel: &CancellationToken,
    ) -> Result<Token, OAuth2Error> {
        let key = TokenKey::of(config);
        if let Some(token) = self.cached_valid(&key) {
            return Ok(token);
        }
        // Only one fetch per token at a time; whoever waited here finds its result in the cache.
        let lock = Arc::clone(
            self.fetching
                .lock()
                .expect("token fetch locks")
                .entry(key.clone())
                .or_default(),
        );
        let _fetching = lock.lock().await;
        let cached = self
            .tokens
            .lock()
            .expect("token cache lock")
            .get(&key)
            .cloned();
        if let Some(token) = cached {
            if !token.needs_refresh() {
                return Ok(token);
            }
            if let Some(refresh) = &token.refresh_token {
                match self.refresh(config, refresh, client).await {
                    Ok(fresh) => return Ok(self.store(key, fresh)),
                    // A rejected refresh token falls through to a full fetch.
                    Err(err) => tracing::info!(%err, "token refresh failed, fetching a new token"),
                }
            }
        }
        let fresh = self.fetch(config, client, cancel).await?;
        Ok(self.store(key, fresh))
    }

    /// The cached token for `key` if it doesn't need a refresh yet.
    fn cached_valid(&self, key: &TokenKey) -> Option<Token> {
        self.tokens
            .lock()
            .expect("token cache lock")
            .get(key)
            .filter(|t| !t.needs_refresh())
            .cloned()
    }

    /// Fetches a new token, ignoring the cache ("Get new token" in the UI).
    pub async fn fetch_new(
        &self,
        config: &OAuth2Config,
        client: &reqwest::Client,
        cancel: &CancellationToken,
    ) -> Result<Token, OAuth2Error> {
        let fresh = self.fetch(config, client, cancel).await?;
        Ok(self.store(TokenKey::of(config), fresh))
    }

    /// The cached, unexpired token for `config`, without fetching (used by curl export).
    pub fn cached(&self, config: &OAuth2Config) -> Option<Token> {
        let tokens = self.tokens.lock().expect("token cache lock");
        tokens
            .get(&TokenKey::of(config))
            .filter(|t| t.expires_at.is_none_or(|at| at > SystemTime::now()))
            .cloned()
    }

    /// All cached tokens, for the token viewer.
    pub fn list(&self) -> Vec<TokenInfo> {
        let tokens = self.tokens.lock().expect("token cache lock");
        let mut infos: Vec<TokenInfo> = tokens
            .iter()
            .map(|(key, token)| TokenInfo {
                token_url: key.token_url.clone(),
                client_id: key.client_id.clone(),
                scope: key.scope.clone(),
                grant: key.grant.clone(),
                username: key.username.clone(),
                access_token: token.access_token.clone(),
                token_type: token.token_type.clone(),
                has_refresh_token: token.refresh_token.is_some(),
                expires_at: token.expires_at.map(|at| {
                    let millis = at
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    iso_timestamp(millis)
                }),
                expired: token.expires_at.is_some_and(|at| at <= SystemTime::now()),
                claims: decode_claims(&token.access_token),
            })
            .collect();
        infos.sort_by(|a, b| (&a.token_url, &a.client_id).cmp(&(&b.token_url, &b.client_id)));
        infos
    }

    /// Forgets the token for these identifying fields, or every token when `None`.
    pub fn clear(&self, which: Option<&TokenInfo>) {
        let mut tokens = self.tokens.lock().expect("token cache lock");
        match which {
            None => tokens.clear(),
            Some(info) => tokens.retain(|key, _| {
                !(key.token_url == info.token_url
                    && key.client_id == info.client_id
                    && key.scope == info.scope
                    && key.grant == info.grant
                    && key.username == info.username)
            }),
        }
    }

    fn store(&self, key: TokenKey, token: Token) -> Token {
        self.tokens
            .lock()
            .expect("token cache lock")
            .insert(key, token.clone());
        token
    }

    async fn fetch(
        &self,
        config: &OAuth2Config,
        client: &reqwest::Client,
        cancel: &CancellationToken,
    ) -> Result<Token, OAuth2Error> {
        if config.token_url.trim().is_empty() {
            return Err(OAuth2Error::Missing("a token URL"));
        }
        let mut form = Vec::new();
        match config.grant {
            OAuth2Grant::ClientCredentials => {
                form.push(("grant_type", "client_credentials".to_string()))
            }
            OAuth2Grant::Password => {
                form.push(("grant_type", "password".to_string()));
                form.push(("username", config.username.clone()));
                form.push(("password", config.password.clone()));
            }
            OAuth2Grant::AuthorizationCode => {
                let (code, redirect_uri, verifier) = self.authorize(config, cancel).await?;
                form.push(("grant_type", "authorization_code".to_string()));
                form.push(("code", code));
                form.push(("redirect_uri", redirect_uri));
                if let Some(verifier) = verifier {
                    form.push(("code_verifier", verifier));
                }
            }
        }
        if !config.scope.trim().is_empty() {
            form.push(("scope", config.scope.clone()));
        }
        self.token_request(config, form, client).await
    }

    async fn refresh(
        &self,
        config: &OAuth2Config,
        refresh_token: &str,
        client: &reqwest::Client,
    ) -> Result<Token, OAuth2Error> {
        let form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
        ];
        let mut token = self.token_request(config, form, client).await?;
        // Providers may omit a new refresh token; keep using the old one then.
        if token.refresh_token.is_none() {
            token.refresh_token = Some(refresh_token.to_string());
        }
        Ok(token)
    }

    /// The browser part of the authorization code flow. Returns the code, the redirect URI
    /// that was used, and the PKCE verifier.
    async fn authorize(
        &self,
        config: &OAuth2Config,
        cancel: &CancellationToken,
    ) -> Result<(String, String, Option<String>), OAuth2Error> {
        if config.auth_url.trim().is_empty() {
            return Err(OAuth2Error::Missing("an authorization URL"));
        }
        let loopback = Loopback::bind(&config.redirect_uri)
            .await
            .map_err(OAuth2Error::Browser)?;
        let redirect_uri = loopback.redirect_uri.clone();
        let state = pkce::random_string(32);
        let verifier = config.use_pkce.then(pkce::code_verifier);

        let mut url = reqwest::Url::parse(&config.auth_url)
            .map_err(|e| OAuth2Error::Browser(format!("invalid authorization URL: {e}")))?;
        {
            // `query_pairs_mut` borrows `url` mutably until this block ends, then `url` is usable again.
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &config.client_id)
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("state", &state);
            if !config.scope.trim().is_empty() {
                query.append_pair("scope", &config.scope);
            }
            if let Some(verifier) = &verifier {
                query
                    .append_pair("code_challenge", &pkce::code_challenge(verifier))
                    .append_pair("code_challenge_method", "S256");
            }
        }

        (self.open_browser)(url.as_str()).map_err(OAuth2Error::Browser)?;
        let callback = tokio::select! {
            result = tokio::time::timeout(BROWSER_TIMEOUT, loopback.wait()) => match result {
                Ok(callback) => callback.map_err(OAuth2Error::Browser)?,
                Err(_) => return Err(OAuth2Error::Browser("timed out waiting for the browser".into())),
            },
            () = cancel.cancelled() => return Err(OAuth2Error::Cancelled),
        };

        if let Some(error) = callback.error {
            return Err(OAuth2Error::Browser(format!(
                "the provider returned {error:?}"
            )));
        }
        if callback.state.as_deref() != Some(state.as_str()) {
            return Err(OAuth2Error::Browser(
                "the state parameter doesn't match (possible CSRF)".into(),
            ));
        }
        let code = callback
            .code
            .ok_or_else(|| OAuth2Error::Browser("the redirect has no code".into()))?;
        Ok((code, redirect_uri, verifier))
    }

    async fn token_request(
        &self,
        config: &OAuth2Config,
        mut form: Vec<(&'static str, String)>,
        client: &reqwest::Client,
    ) -> Result<Token, OAuth2Error> {
        let mut request = client
            .post(&config.token_url)
            .header("Accept", "application/json");
        match config.client_auth {
            OAuth2ClientAuth::BasicHeader => {
                request = request.basic_auth(&config.client_id, Some(&config.client_secret));
            }
            OAuth2ClientAuth::Body => {
                form.push(("client_id", config.client_id.clone()));
                if !config.client_secret.is_empty() {
                    form.push(("client_secret", config.client_secret.clone()));
                }
            }
        }
        // Public clients (no secret) still identify themselves in the body.
        if matches!(config.client_auth, OAuth2ClientAuth::BasicHeader)
            && config.client_secret.is_empty()
        {
            form.push(("client_id", config.client_id.clone()));
        }

        let response = request
            .form(&form)
            .send()
            .await
            .map_err(|e| OAuth2Error::Request(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| OAuth2Error::Request(e.to_string()))?;
        if !status.is_success() {
            // The error body (e.g. `{"error":"invalid_client"}`) never contains our secrets.
            return Err(OAuth2Error::Rejected {
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }
        let parsed: TokenResponse =
            serde_json::from_str(&body).map_err(|e| OAuth2Error::BadResponse(e.to_string()))?;
        let access_token = parsed
            .access_token
            .ok_or_else(|| OAuth2Error::BadResponse("no access_token field".into()))?;
        Ok(Token {
            access_token,
            token_type: parsed.token_type.unwrap_or_else(|| "Bearer".to_string()),
            refresh_token: parsed.refresh_token,
            expires_at: parsed
                .expires_in
                .map(|secs| SystemTime::now() + Duration::from_secs(secs)),
            scope: parsed.scope,
        })
    }
}

fn grant_name(grant: OAuth2Grant) -> &'static str {
    match grant {
        OAuth2Grant::ClientCredentials => "client_credentials",
        OAuth2Grant::AuthorizationCode => "authorization_code",
        OAuth2Grant::Password => "password",
    }
}
