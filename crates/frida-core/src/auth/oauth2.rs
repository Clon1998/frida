//! OAuth2 tokens (PLAN.md §8): fetching, caching and refreshing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio_util::sync::CancellationToken;

use super::jwt::decode_claims;
use super::loopback::Loopback;
use super::pkce;
use crate::http::{ConsoleEntry, HttpResponse, PreparedBody, PreparedRequest, header_pairs};
use crate::model::{OAuth2ClientAuth, OAuth2Config, OAuth2Grant};
use crate::vars::iso_timestamp;

/// Tokens are refreshed this long before they expire (PLAN.md §8).
pub const REFRESH_SKEW: Duration = Duration::from_secs(30);

/// How long to wait for the user to finish signing in in the browser.
const BROWSER_TIMEOUT: Duration = Duration::from_secs(300);

/// Opens a URL in the system browser. Provided by the app (Tauri's opener) or by tests.
pub type BrowserOpener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// What fetching a token needs besides its config. Grouped in a struct because every step of
/// the flow (fetch, refresh, the token request) passes it on unchanged.
pub struct TokenCall<'a> {
    /// Carries the collection's TLS and proxy settings.
    pub client: &'a reqwest::Client,
    pub cancel: &'a CancellationToken,
    /// Receives a console entry for every token request (masked), successful or not, so failed
    /// sign-ins can be debugged in the console like any other request.
    pub on_exchange: &'a (dyn Fn(ConsoleEntry) + Send + Sync),
    /// The active environments (`dev · shared`: collection · global), remembered with the
    /// token so the token viewer can show where it came from.
    pub environment: Option<String>,
}

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
    /// See [`TokenCall::environment`].
    pub environment: Option<String>,
    pub fetched_at: SystemTime,
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
    /// The active environments when it was fetched (`dev · shared`), if any.
    pub environment: Option<String>,
    /// ISO timestamp.
    pub fetched_at: String,
    /// Used by the request (or folder, collection) the viewer was opened for, with the active
    /// environments. The viewer shows these first.
    pub current: bool,
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
        call: &TokenCall<'_>,
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
                match self.refresh(config, refresh, call).await {
                    Ok(fresh) => return Ok(self.store(key, fresh)),
                    // A rejected refresh token falls through to a full fetch.
                    Err(err) => tracing::info!(%err, "token refresh failed, fetching a new token"),
                }
            }
        }
        let fresh = self.fetch(config, call).await?;
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
        call: &TokenCall<'_>,
    ) -> Result<Token, OAuth2Error> {
        let fresh = self.fetch(config, call).await?;
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

    /// All cached tokens, for the token viewer. Those for one of `current` (the configs of the
    /// item the viewer is open for) are marked [`TokenInfo::current`] and come first.
    pub fn list(&self, current: &[&OAuth2Config]) -> Vec<TokenInfo> {
        let current: Vec<TokenKey> = current.iter().map(|config| TokenKey::of(config)).collect();
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
                expires_at: token.expires_at.map(iso),
                expired: token.expires_at.is_some_and(|at| at <= SystemTime::now()),
                claims: decode_claims(&token.access_token),
                environment: token.environment.clone(),
                fetched_at: iso(token.fetched_at),
                current: current.contains(key),
            })
            .collect();
        // Current ones first (`!current` is `false` for them, and `false` sorts first), then
        // newest first (`b` before `a`), like a chained Java `Comparator`.
        infos.sort_by(|a, b| (!a.current, &b.fetched_at).cmp(&(!b.current, &a.fetched_at)));
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
        call: &TokenCall<'_>,
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
                let (code, redirect_uri, verifier) = self.authorize(config, call.cancel).await?;
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
        self.token_request(config, form, call).await
    }

    async fn refresh(
        &self,
        config: &OAuth2Config,
        refresh_token: &str,
        call: &TokenCall<'_>,
    ) -> Result<Token, OAuth2Error> {
        let form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
        ];
        let mut token = self.token_request(config, form, call).await?;
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
        call: &TokenCall<'_>,
    ) -> Result<Token, OAuth2Error> {
        let (client, on_exchange) = (call.client, call.on_exchange);
        let grant_type = form
            .iter()
            .find(|(name, _)| *name == "grant_type")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
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

        // Built first (instead of `send()`), so the console can show what went out.
        let request = request
            .form(&form)
            .build()
            .map_err(|e| OAuth2Error::Request(e.to_string()))?;
        let mut log = TokenExchange::new(&request, config, &grant_type);
        let started = Instant::now();
        let response = match client.execute(request).await {
            Ok(response) => response,
            Err(e) => {
                on_exchange(log.entry(Err(e.to_string())));
                return Err(OAuth2Error::Request(e.to_string()));
            }
        };
        let status = response.status();
        let final_url = response.url().to_string();
        // `Version`'s Debug output is already the protocol name, e.g. "HTTP/1.1".
        let http_version = format!("{:?}", response.version());
        let headers = header_pairs(response.headers());
        let body = match response.text().await {
            Ok(body) => body,
            Err(e) => {
                on_exchange(log.entry(Err(e.to_string())));
                return Err(OAuth2Error::Request(e.to_string()));
            }
        };
        log.mask_tokens_in(&body);
        on_exchange(log.entry(Ok(&HttpResponse {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            headers,
            body: body.clone().into_bytes(),
            elapsed: started.elapsed(),
            final_url,
            http_version,
            request_headers: Vec::new(),
        })));
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
            environment: call.environment.clone(),
            fetched_at: SystemTime::now(),
        })
    }
}

fn iso(time: SystemTime) -> String {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    iso_timestamp(millis)
}

/// A token request as the console shows it: what went out, and the values to mask (the
/// client secret, password and Basic credentials, and the tokens in the response).
struct TokenExchange {
    sent: PreparedRequest,
    name: String,
    sensitive: Vec<String>,
}

impl TokenExchange {
    fn new(request: &reqwest::Request, config: &OAuth2Config, grant_type: &str) -> Self {
        let headers = header_pairs(request.headers());
        let mut sensitive = vec![config.client_secret.clone(), config.password.clone()];
        // The whole `Basic …` value: the encoded client id and secret.
        sensitive.extend(
            headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                .map(|(_, value)| value.clone()),
        );
        let body = request
            .body()
            .and_then(|body| body.as_bytes())
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        Self {
            sent: PreparedRequest {
                method: request.method().to_string(),
                url: request.url().to_string(),
                headers,
                body: PreparedBody::Bytes(body),
                // Not used by the console; the client that sends it has the real settings.
                timeout: Duration::ZERO,
                tls_verify: true,
                ca_cert_paths: Vec::new(),
                proxy: Default::default(),
            },
            name: format!("OAuth2 token ({grant_type})"),
            sensitive,
        }
    }

    /// Adds the tokens in a token response to the values to mask.
    fn mask_tokens_in(&mut self, body: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
            return;
        };
        for field in ["access_token", "refresh_token", "id_token"] {
            if let Some(token) = json.get(field).and_then(|v| v.as_str()) {
                self.sensitive.push(token.to_string());
            }
        }
    }

    fn entry(&self, outcome: Result<&HttpResponse, String>) -> ConsoleEntry {
        ConsoleEntry::from_send(
            uuid::Uuid::new_v4().to_string(),
            &self.name,
            "",
            &self.sent,
            outcome,
            &self.sensitive,
        )
    }
}

fn grant_name(grant: OAuth2Grant) -> &'static str {
    match grant {
        OAuth2Grant::ClientCredentials => "client_credentials",
        OAuth2Grant::AuthorizationCode => "authorization_code",
        OAuth2Grant::Password => "password",
    }
}
