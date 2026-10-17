//! M3 acceptance (PLAN.md §15): OAuth2 against the mock provider in `frida-testserver`
//! (client credentials, password, and authorization code with the browser step simulated),
//! token caching and refresh, and OAuth2 + API key sending both.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use frida_core::auth::{BrowserOpener, OAuth2Manager};
use frida_core::http::{self, ExecuteInput};
use frida_core::model::{
    ApiKeyLocation, Auth, AuthScheme, Collection, OAuth2ClientAuth, OAuth2Config, OAuth2Grant,
    Request, TokenPlacement,
};
use frida_core::storage;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

/// A "browser" that follows the authorization URL. The mock provider approves immediately and
/// redirects to Frida's loopback listener, which completes the flow.
fn simulated_browser() -> BrowserOpener {
    Arc::new(|url: &str| {
        let url = url.to_string();
        tokio::spawn(async move {
            let _ = reqwest::get(url).await;
        });
        Ok(())
    })
}

/// Like [`simulated_browser`], and remembers the authorization URLs it was asked to open.
fn recording_browser() -> (BrowserOpener, Arc<std::sync::Mutex<Vec<String>>>) {
    let opened = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = Arc::clone(&opened);
    let follow = simulated_browser();
    let opener: BrowserOpener = Arc::new(move |url: &str| {
        log.lock().unwrap().push(url.to_string());
        follow(url)
    });
    (opener, opened)
}

fn oauth2(addr: SocketAddr, grant: OAuth2Grant, client_id: &str) -> AuthScheme {
    AuthScheme::OAuth2(Box::new(OAuth2Config {
        grant,
        token_url: format!("http://{addr}/oauth/token"),
        auth_url: format!("http://{addr}/oauth/authorize"),
        client_id: client_id.into(),
        client_secret: "{{clientSecret}}".into(), // resolved from collection vars
        scope: "orders/read".into(),
        username: "alice".into(),
        password: "wonderland".into(),
        ..OAuth2Config::default()
    }))
}

/// A collection on disk with `collection_auth`, plus one request to `/echo`.
fn collection_with(
    collection_auth: Auth,
    addr: SocketAddr,
) -> (tempfile::TempDir, String, Request) {
    let dir = tempfile::tempdir().unwrap();
    let mut collection: Collection = storage::create_collection(dir.path(), "Auth").unwrap();
    collection.auth = collection_auth;
    collection.vars = vec![frida_core::model::KeyValue::new("clientSecret", "secret")];
    storage::save_collection(dir.path(), &collection).unwrap();
    let mut request = Request::new("Echo");
    request.url = format!("http://{addr}/echo");
    let rel = storage::create_request(dir.path(), "", &request).unwrap();
    (dir, rel, request)
}

/// Sends the request and returns the echoed headers.
async fn send(dir: &Path, rel: &str, request: &Request, oauth: &OAuth2Manager) -> Value {
    send_reply(dir, rel, request, oauth).await["headers"].clone()
}

/// Sends the request and returns the whole echo (`method`, `path`, `query`, `headers`, ...).
async fn send_reply(dir: &Path, rel: &str, request: &Request, oauth: &OAuth2Manager) -> Value {
    let session = http::SessionStore::default();
    let executed = http::execute(
        ExecuteInput {
            collection_dir: dir,
            request_rel: rel,
            request,
            collection_env: None,
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth,
        },
        CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap_or_else(|e| panic!("send failed: {e}"));
    serde_json::from_slice(&executed.response.body).unwrap()
}

/// The `Authorization` header the server saw.
fn authorization(headers: &Value) -> String {
    headers["authorization"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn client_credentials_token_is_fetched_and_cached() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "frida");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());

    let first = send(dir.path(), &rel, &request, &oauth).await;
    let auth = first["authorization"].as_str().unwrap().to_string();
    assert!(
        auth.starts_with("Bearer client_credentials-token-"),
        "{auth}"
    );

    // The second send reuses the cached token.
    let second = send(dir.path(), &rel, &request, &oauth).await;
    assert_eq!(second["authorization"], auth);
    assert_eq!(oauth.list().len(), 1);
}

#[tokio::test]
async fn client_secret_in_the_body_works_too() {
    let addr = server().await;
    let AuthScheme::OAuth2(mut config) = oauth2(addr, OAuth2Grant::ClientCredentials, "frida")
    else {
        unreachable!()
    };
    config.client_auth = OAuth2ClientAuth::Body;
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::OAuth2(config)],
        },
        addr,
    );
    let headers = send(
        dir.path(),
        &rel,
        &request,
        &OAuth2Manager::new(simulated_browser()),
    )
    .await;
    assert!(
        headers["authorization"]
            .as_str()
            .unwrap()
            .starts_with("Bearer ")
    );
}

#[tokio::test]
async fn password_grant_works() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::Password, "frida");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let headers = send(
        dir.path(),
        &rel,
        &request,
        &OAuth2Manager::new(simulated_browser()),
    )
    .await;
    assert!(
        headers["authorization"]
            .as_str()
            .unwrap()
            .starts_with("Bearer password-token-")
    );
}

#[tokio::test]
async fn authorization_code_with_pkce_works_with_a_simulated_browser() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::AuthorizationCode, "frida");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let (browser, opened) = recording_browser();
    let headers = send(dir.path(), &rel, &request, &OAuth2Manager::new(browser)).await;
    assert!(
        headers["authorization"]
            .as_str()
            .unwrap()
            .starts_with("Bearer authorization_code-token-"),
        "{headers}"
    );
    // The mock checked the verifier against this challenge.
    let url = opened.lock().unwrap()[0].clone();
    assert!(
        url.contains("code_challenge=") && url.contains("code_challenge_method=S256"),
        "{url}"
    );
}

#[tokio::test]
async fn expiring_tokens_are_refreshed_with_the_refresh_token() {
    let addr = server().await;
    // `short-lived` tokens expire after 1 s, which is inside the 30 s refresh skew.
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "short-lived");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());

    let first = send(dir.path(), &rel, &request, &oauth).await;
    assert!(
        first["authorization"]
            .as_str()
            .unwrap()
            .contains("client_credentials-token-")
    );
    let second = send(dir.path(), &rel, &request, &oauth).await;
    assert!(
        second["authorization"]
            .as_str()
            .unwrap()
            .contains("refresh_token-token-"),
        "{second}"
    );
}

#[tokio::test]
async fn oauth2_plus_api_key_sends_both() {
    let addr = server().await;
    let schemes = vec![
        oauth2(addr, OAuth2Grant::ClientCredentials, "frida"),
        AuthScheme::ApiKey {
            location: ApiKeyLocation::Header,
            name: "x-api-key".into(),
            value: "key-123".into(),
        },
    ];
    let (dir, rel, request) = collection_with(Auth::Custom { schemes }, addr);
    let headers = send(
        dir.path(),
        &rel,
        &request,
        &OAuth2Manager::new(simulated_browser()),
    )
    .await;
    assert!(
        headers["authorization"]
            .as_str()
            .unwrap()
            .starts_with("Bearer ")
    );
    assert_eq!(headers["x-api-key"], "key-123");
}

#[tokio::test]
async fn requests_and_folders_inherit_or_override_auth() {
    let addr = server().await;
    let (dir, rel, mut request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::Bearer {
                token: "from-collection".into(),
            }],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    assert_eq!(
        send(dir.path(), &rel, &request, &oauth).await["authorization"],
        "Bearer from-collection"
    );

    request.auth = Auth::Custom {
        schemes: vec![AuthScheme::Basic {
            username: "u".into(),
            password: "p".into(),
        }],
    };
    assert_eq!(
        send(dir.path(), &rel, &request, &oauth).await["authorization"],
        "Basic dTpw"
    );

    request.auth = Auth::None;
    assert!(
        send(dir.path(), &rel, &request, &oauth)
            .await
            .get("authorization")
            .is_none()
    );
}

#[tokio::test]
async fn rejected_credentials_give_a_clear_error() {
    let addr = server().await;
    let AuthScheme::OAuth2(mut config) = oauth2(addr, OAuth2Grant::ClientCredentials, "frida")
    else {
        unreachable!()
    };
    config.client_secret = "wrong".into();
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::OAuth2(config)],
        },
        addr,
    );
    let session = http::SessionStore::default();
    let oauth = OAuth2Manager::new(simulated_browser());
    let err = http::execute(
        ExecuteInput {
            collection_dir: dir.path(),
            request_rel: &rel,
            request: &request,
            collection_env: None,
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &oauth,
        },
        CancellationToken::new(),
        &|_| {},
    )
    .await
    .err()
    .unwrap();
    let message = err.to_string();
    assert!(
        message.contains("401") && message.contains("invalid_client"),
        "{message}"
    );
}

/// Requests sent at the same time share one token fetch (for the authorization code grant,
/// one fetch means one browser sign-in, not one per request).
#[tokio::test]
async fn concurrent_requests_share_one_token_fetch() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "frida");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    let (a, b, c, d) = tokio::join!(
        send(dir.path(), &rel, &request, &oauth),
        send(dir.path(), &rel, &request, &oauth),
        send(dir.path(), &rel, &request, &oauth),
        send(dir.path(), &rel, &request, &oauth),
    );
    let tokens: std::collections::BTreeSet<_> = [a, b, c, d]
        .iter()
        .map(|r| r["authorization"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tokens.len(), 1, "one token for all four: {tokens:?}");
}

#[tokio::test]
async fn basic_bearer_and_query_api_keys_reach_the_server() {
    let addr = server().await;
    let (dir, rel, mut request) = collection_with(Auth::None, addr);
    let oauth = OAuth2Manager::new(simulated_browser());

    // Values may be variables, like any other field (`clientSecret` is a collection var).
    request.auth = Auth::Custom {
        schemes: vec![
            AuthScheme::Basic {
                username: "alice".into(),
                password: "{{clientSecret}}".into(),
            },
            AuthScheme::ApiKey {
                location: ApiKeyLocation::Query,
                name: "api_key".into(),
                value: "k&1".into(),
            },
        ],
    };
    let reply = send_reply(dir.path(), &rel, &request, &oauth).await;
    // base64("alice:secret")
    assert_eq!(authorization(&reply["headers"]), "Basic YWxpY2U6c2VjcmV0");
    assert_eq!(reply["query"], "api_key=k%261");

    request.auth = Auth::Custom {
        schemes: vec![AuthScheme::Bearer {
            token: "{{clientSecret}}".into(),
        }],
    };
    let headers = send(dir.path(), &rel, &request, &oauth).await;
    assert_eq!(authorization(&headers), "Bearer secret");

    // "No auth" on the request beats auth on the collection.
    request.auth = Auth::None;
    let headers = send(dir.path(), &rel, &request, &oauth).await;
    assert_eq!(headers["authorization"], Value::Null);
}

#[tokio::test]
async fn oauth2_tokens_can_go_into_another_header_or_the_query() {
    let addr = server().await;
    let AuthScheme::OAuth2(mut config) = oauth2(addr, OAuth2Grant::ClientCredentials, "frida")
    else {
        unreachable!()
    };
    config.token_placement = TokenPlacement {
        location: ApiKeyLocation::Header,
        name: "X-Access-Token".into(),
        prefix: "".into(),
    };
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::OAuth2(config.clone())],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    let reply = send_reply(dir.path(), &rel, &request, &oauth).await;
    let token = reply["headers"]["x-access-token"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(token.starts_with("client_credentials-token-"), "{token}");
    assert_eq!(reply["headers"]["authorization"], Value::Null);

    config.token_placement = TokenPlacement {
        location: ApiKeyLocation::Query,
        name: "access_token".into(),
        prefix: "".into(),
    };
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::OAuth2(config)],
        },
        addr,
    );
    let reply = send_reply(dir.path(), &rel, &request, &oauth).await;
    // Same provider, client and scope: the cached token is reused.
    assert_eq!(reply["query"], format!("access_token={token}"));
}

#[tokio::test]
async fn authorization_code_works_without_pkce_too() {
    let addr = server().await;
    let AuthScheme::OAuth2(mut config) = oauth2(addr, OAuth2Grant::AuthorizationCode, "frida")
    else {
        unreachable!()
    };
    config.use_pkce = false;
    config.redirect_uri = "http://127.0.0.1:{port}/callback".into();
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![AuthScheme::OAuth2(config)],
        },
        addr,
    );
    let (browser, opened) = recording_browser();
    let oauth = OAuth2Manager::new(browser);
    let headers = send(dir.path(), &rel, &request, &oauth).await;
    assert!(
        authorization(&headers).starts_with("Bearer authorization_code-token-"),
        "{headers}"
    );
    let url = opened.lock().unwrap()[0].clone();
    assert!(!url.contains("code_challenge"), "no PKCE: {url}");
}

#[tokio::test]
async fn expired_tokens_without_refresh_token_are_fetched_again() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "no-refresh");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    let first = authorization(&send(dir.path(), &rel, &request, &oauth).await);
    let second = authorization(&send(dir.path(), &rel, &request, &oauth).await);
    assert!(
        first.starts_with("Bearer client_credentials-token-"),
        "{first}"
    );
    assert!(
        second.starts_with("Bearer client_credentials-token-"),
        "{second}"
    );
    assert_ne!(first, second, "a new token, since the first one expired");
}

#[tokio::test]
async fn a_rejected_refresh_falls_back_to_a_new_token() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::Password, "refresh-fails");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    let first = authorization(&send(dir.path(), &rel, &request, &oauth).await);
    let second = authorization(&send(dir.path(), &rel, &request, &oauth).await);
    assert!(first.starts_with("Bearer password-token-"), "{first}");
    // Not `refresh_token-token-`: the refresh was rejected, so the password grant ran again.
    assert!(second.starts_with("Bearer password-token-"), "{second}");
    assert_ne!(first, second);
}

#[tokio::test]
async fn get_new_token_and_clear_tokens() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "frida");
    let (dir, rel, request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    let oauth = OAuth2Manager::new(simulated_browser());
    let first = authorization(&send(dir.path(), &rel, &request, &oauth).await);

    // "Get new token" ignores the valid cached token.
    let session = http::SessionStore::default();
    let tokens = http::fetch_tokens(
        ExecuteInput {
            collection_dir: dir.path(),
            request_rel: &rel,
            request: &request,
            collection_env: None,
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &oauth,
        },
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(tokens.len(), 1);
    let fresh = format!("Bearer {}", tokens[0].access_token);
    assert_ne!(fresh, first);
    assert_eq!(tokens[0].client_id, "frida");
    assert!(tokens[0].has_refresh_token && !tokens[0].expired);
    assert_eq!(
        authorization(&send(dir.path(), &rel, &request, &oauth).await),
        fresh,
        "the next send uses the new token"
    );

    // "Clear": the token is gone, and the next send fetches another one.
    oauth.clear(None);
    assert!(oauth.list().is_empty());
    let third = authorization(&send(dir.path(), &rel, &request, &oauth).await);
    assert!(third != first && third != fresh, "{third}");
}

/// Script output goes to the app's console and `frida-run -v`: a script that prints the
/// token (or a Basic password from auth) must show `***`.
#[tokio::test]
async fn auth_values_printed_by_scripts_are_masked() {
    let addr = server().await;
    let scheme = oauth2(addr, OAuth2Grant::ClientCredentials, "frida");
    let (dir, rel, mut request) = collection_with(
        Auth::Custom {
            schemes: vec![scheme],
        },
        addr,
    );
    request.scripts.post = "console.log(res.body.headers.authorization);".into();
    let oauth = OAuth2Manager::new(simulated_browser());
    let logs = std::sync::Mutex::new(Vec::new());
    let session = http::SessionStore::default();
    http::execute(
        ExecuteInput {
            collection_dir: dir.path(),
            request_rel: &rel,
            request: &request,
            collection_env: None,
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &oauth,
        },
        CancellationToken::new(),
        &|log| logs.lock().unwrap().push(log.message),
    )
    .await
    .unwrap();
    assert_eq!(*logs.lock().unwrap(), ["Bearer ***"]);
}
