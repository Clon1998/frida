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
    Request,
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
    let reply: Value = serde_json::from_slice(&executed.response.body).unwrap();
    reply["headers"].clone()
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
            .starts_with("Bearer authorization_code-token-"),
        "{headers}"
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
