//! A cookie jar per collection (PLAN.md §7): cookies set by responses are sent with later
//! requests to the same site, like in a browser. Kept in memory only.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::vars::iso_timestamp;

/// The jar reqwest reads and writes. A `Mutex` inside, so it can be shared between requests.
pub type CookieJar = reqwest_cookie_store::CookieStoreMutex;

/// One cookie as the cookie viewer shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CookieView {
    pub domain: String,
    pub path: String,
    pub name: String,
    pub value: String,
    /// ISO timestamp, or `None` for session cookies.
    pub expires: Option<String>,
    pub secure: bool,
    pub http_only: bool,
}

/// All cookies in the jar, sorted by domain, path and name.
pub fn list_cookies(jar: &CookieJar) -> Vec<CookieView> {
    let store = jar.lock().expect("cookie jar lock poisoned");
    let mut cookies: Vec<CookieView> = store
        .iter_any()
        .map(|cookie| CookieView {
            domain: cookie
                .domain
                .as_cow()
                .map(|d| d.to_string())
                .unwrap_or_default(),
            path: String::from(&cookie.path),
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            expires: expires_iso(&cookie.expires),
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
        })
        .collect();
    cookies.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
    cookies
}

fn expires_iso(expiration: &cookie_store::CookieExpiration) -> Option<String> {
    match expiration {
        cookie_store::CookieExpiration::AtUtc(at) => {
            let millis = at.unix_timestamp_nanos() / 1_000_000;
            u128::try_from(millis).ok().map(iso_timestamp)
        }
        cookie_store::CookieExpiration::SessionEnd => None,
    }
}

/// Removes every cookie of `domain`, or all cookies when `domain` is `None`.
pub fn clear_cookies(jar: &CookieJar, domain: Option<&str>) {
    let mut store = jar.lock().expect("cookie jar lock poisoned");
    let Some(domain) = domain else {
        store.clear();
        return;
    };
    let doomed: Vec<(String, String, String)> = store
        .iter_any()
        .filter(|cookie| cookie.domain.as_cow().is_some_and(|d| d == domain))
        .map(|cookie| {
            (
                domain.to_string(),
                String::from(&cookie.path),
                cookie.name().to_string(),
            )
        })
        .collect();
    for (domain, path, name) in doomed {
        store.remove(&domain, &path, &name);
    }
}
