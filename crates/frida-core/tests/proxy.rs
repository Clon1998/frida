//! M16 acceptance: sends, OAuth2 token calls and `fri.sendRequest` go through a manual proxy
//! (with and without a login), the bypass list skips it, and proxy problems are explained.

use std::net::SocketAddr;
use std::sync::Arc;

use frida_core::CoreError;
use frida_core::auth::OAuth2Manager;
use frida_core::http::{self, ConsoleEntry, ExecuteInput, Executed, SendError, SessionStore};
use frida_core::model::{Auth, AuthScheme, Collection, OAuth2Config, OAuth2Grant, Request};
use frida_core::network::{ManualProxy, ProxySettings};
use frida_core::storage;
use tokio_util::sync::CancellationToken;

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

/// A manual proxy at `url`; `bypass` as typed in the settings.
fn manual(url: &str, login: Option<(&str, &str)>, bypass: &str) -> ProxySettings {
    let mut proxy = ManualProxy::parse(url, bypass).unwrap();
    if let Some((user, password)) = login {
        proxy.username = Some(user.into());
        proxy.password = Some(password.into());
    }
    ProxySettings::Manual(Arc::new(proxy))
}

/// Sends `request` from a fresh collection (with `auth`) with the given proxy settings.
async fn send(request: Request, auth: Auth, proxy: ProxySettings) -> Result<Executed, CoreError> {
    let dir = tempfile::tempdir().unwrap();
    let mut collection: Collection = storage::create_collection(dir.path(), "Proxy").unwrap();
    collection.auth = auth;
    collection.settings.tls.verify = false;
    storage::save_collection(dir.path(), &collection).unwrap();
    let rel = storage::create_request(dir.path(), "", &request).unwrap();
    let session = SessionStore {
        proxy,
        ..SessionStore::default()
    };
    let oauth = OAuth2Manager::new(Arc::new(|_| Ok(())));
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
        &|_| {},
    )
    .await
}

fn get(url: String) -> Request {
    let mut request = Request::new("Echo");
    request.url = url;
    request
}

#[tokio::test(flavor = "multi_thread")]
async fn plain_http_goes_through_the_proxy_and_the_console_says_so() {
    let addr = server().await;
    let proxy = frida_testserver::spawn_proxy(None).await.unwrap();
    let url = format!("http://{addr}/echo?x=1");
    let executed = send(get(url.clone()), Auth::None, manual(&proxy.url(), None, ""))
        .await
        .unwrap();
    assert_eq!(executed.response.status, 200);
    assert_eq!(proxy.requests(), [format!("GET {url}")]);

    let entry = ConsoleEntry::from_send(
        "1".into(),
        "Echo",
        "echo.yaml",
        &executed.sent,
        Ok(&executed.response),
        &[],
    );
    assert_eq!(entry.proxy, Some(format!("via proxy {}", proxy.url())));
}

#[tokio::test(flavor = "multi_thread")]
async fn https_is_tunnelled_with_connect() {
    let tls = frida_testserver::spawn_tls().await.unwrap();
    let proxy = frida_testserver::spawn_proxy(None).await.unwrap();
    let url = format!("https://localhost:{}/echo", tls.addr.port());
    let executed = send(get(url), Auth::None, manual(&proxy.url(), None, ""))
        .await
        .unwrap();
    assert_eq!(executed.response.status, 200);
    assert_eq!(
        proxy.requests(),
        [format!("CONNECT localhost:{}", tls.addr.port())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_is_sent_and_a_wrong_one_gives_a_407_error() {
    let addr = server().await;
    let tls = frida_testserver::spawn_tls().await.unwrap();
    let proxy = frida_testserver::spawn_proxy(Some(("ann", "s3cret")))
        .await
        .unwrap();
    let http_url = format!("http://{addr}/echo");
    let https_url = format!("https://localhost:{}/echo", tls.addr.port());

    let right = manual(&proxy.url(), Some(("ann", "s3cret")), "");
    for url in [&http_url, &https_url] {
        let executed = send(get(url.clone()), Auth::None, right.clone()).await;
        assert_eq!(executed.unwrap().response.status, 200, "{url}");
    }

    let wrong = manual(&proxy.url(), Some(("ann", "nope")), "");
    let none = manual(&proxy.url(), None, "");
    for (settings, url) in [
        (&wrong, &http_url),
        (&wrong, &https_url),
        (&none, &http_url),
    ] {
        let err = send(get(url.clone()), Auth::None, settings.clone())
            .await
            .err()
            .unwrap_or_else(|| panic!("{url} should fail"));
        assert!(
            matches!(&err, CoreError::Send(SendError::ProxyAuth { proxy: p }) if *p == proxy.url()),
            "{url}: {err}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bypassed_hosts_go_directly() {
    let addr = server().await;
    let proxy = frida_testserver::spawn_proxy(None).await.unwrap();
    let settings = manual(&proxy.url(), None, "localhost, 127.0.0.0/8");
    let executed = send(get(format!("http://{addr}/echo")), Auth::None, settings)
        .await
        .unwrap();
    assert_eq!(executed.response.status, 200);
    assert!(proxy.requests().is_empty(), "{:?}", proxy.requests());
    assert_eq!(
        executed.sent.proxy.describe_for(&executed.sent.url),
        None,
        "the console shows a direct request"
    );

    // Off: no proxy at all.
    let executed = send(
        get(format!("http://{addr}/echo")),
        Auth::None,
        ProxySettings::Off,
    )
    .await
    .unwrap();
    assert_eq!(executed.response.status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth2_token_calls_and_script_requests_use_the_proxy() {
    let addr = server().await;
    let proxy = frida_testserver::spawn_proxy(None).await.unwrap();
    let auth = Auth::Custom {
        schemes: vec![AuthScheme::OAuth2(Box::new(OAuth2Config {
            grant: OAuth2Grant::ClientCredentials,
            token_url: format!("http://{addr}/oauth/token"),
            client_id: "frida".into(),
            client_secret: "secret".into(),
            scope: "orders/read".into(),
            ..OAuth2Config::default()
        }))],
    };
    let mut request = get(format!("http://{addr}/echo"));
    request.scripts.pre =
        format!("await fri.sendRequest({{ url: \"http://{addr}/echo?from=script\" }});");
    let executed = send(request, auth, manual(&proxy.url(), None, ""))
        .await
        .unwrap();
    assert_eq!(executed.response.status, 200);
    let seen = proxy.requests();
    for expected in [
        format!("GET http://{addr}/echo?from=script"),
        format!("POST http://{addr}/oauth/token"),
        format!("GET http://{addr}/echo"),
    ] {
        assert!(seen.contains(&expected), "{expected} not in {seen:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_proxy_is_named_in_the_error() {
    let addr = server().await;
    // Nothing listens on port 9 (discard).
    let err = send(
        get(format!("http://{addr}/echo")),
        Auth::None,
        manual("http://127.0.0.1:9", None, ""),
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(&err, CoreError::Send(SendError::Proxy { proxy, .. }) if proxy == "http://127.0.0.1:9"),
        "{err}"
    );
    assert!(
        err.to_string()
            .starts_with("could not reach the proxy http://127.0.0.1:9")
    );
}
