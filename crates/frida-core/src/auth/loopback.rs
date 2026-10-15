//! The local HTTP listener that receives the browser redirect in the authorization code flow
//! (PLAN.md §8): `http://127.0.0.1:{port}/callback?code=...&state=...`.

use reqwest::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Used when the OAuth2 config doesn't set a redirect URI. `{port}` means "any free port".
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:{port}/callback";

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>Frida</title>\
<body style=\"font-family:sans-serif;margin:3em\"><h2>Signed in</h2>\
<p>You can close this window and return to Frida.</p></body>";

/// What the browser brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    /// e.g. `access_denied` when the user declined.
    pub error: Option<String>,
}

pub struct Loopback {
    listener: TcpListener,
    path: String,
    /// The redirect URI with the real port filled in. Must be sent unchanged to the provider.
    pub redirect_uri: String,
}

impl Loopback {
    /// Starts listening on the host and port of `redirect_uri` (`{port}` = a free port).
    pub async fn bind(redirect_uri: &str) -> Result<Self, String> {
        let template = if redirect_uri.trim().is_empty() {
            DEFAULT_REDIRECT_URI
        } else {
            redirect_uri.trim()
        };
        let any_port = template.contains("{port}");
        let url = Url::parse(&template.replace("{port}", "1"))
            .map_err(|e| format!("invalid redirect URI {template:?}: {e}"))?;
        let host = url.host_str().unwrap_or_default().to_string();
        if host != "127.0.0.1" && host != "localhost" {
            return Err(format!(
                "the redirect URI must point to 127.0.0.1 or localhost so Frida can receive the code, not {host:?}"
            ));
        }
        let port = if any_port {
            0
        } else {
            url.port_or_known_default().unwrap_or(80)
        };
        let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
            format!("could not listen on 127.0.0.1:{port} for the OAuth2 redirect: {e}")
        })?;
        let actual = listener.local_addr().map_err(|e| e.to_string())?.port();
        Ok(Self {
            listener,
            path: url.path().to_string(),
            redirect_uri: template.replace("{port}", &actual.to_string()),
        })
    }

    /// Waits for the browser to hit the redirect path. Other requests (e.g. `/favicon.ico`)
    /// get a 404 and are ignored.
    pub async fn wait(self) -> Result<Callback, String> {
        loop {
            let (mut stream, _) = self.listener.accept().await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 8192];
            let mut len = 0;
            // Read until the end of the request head (we don't need the body).
            while len < buf.len() {
                let n = stream
                    .read(&mut buf[len..])
                    .await
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                len += n;
                if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf[..len]);
            let target = head
                .lines()
                .next()
                .and_then(|line| line.split(' ').nth(1))
                .unwrap_or("/");
            let url =
                Url::parse(&format!("http://localhost{target}")).map_err(|e| e.to_string())?;

            if url.path() != self.path {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                continue;
            }
            let get = |key: &str| {
                url.query_pairs()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.to_string())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                PAGE.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return Ok(Callback {
                code: get("code"),
                state: get("state"),
                error: get("error"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receives_the_code_on_a_free_port() {
        let loopback = Loopback::bind("").await.unwrap();
        let uri = loopback.redirect_uri.clone();
        assert!(
            uri.starts_with("http://127.0.0.1:") && uri.ends_with("/callback"),
            "{uri}"
        );

        let waiting = tokio::spawn(loopback.wait());
        reqwest::get(format!(
            "{}/../favicon.ico",
            uri.trim_end_matches("/callback")
        ))
        .await
        .unwrap();
        let page = reqwest::get(format!("{uri}?code=abc&state=xyz"))
            .await
            .unwrap();
        assert_eq!(page.status(), 200);

        let callback = waiting.await.unwrap().unwrap();
        assert_eq!(callback.code.as_deref(), Some("abc"));
        assert_eq!(callback.state.as_deref(), Some("xyz"));
    }

    #[tokio::test]
    async fn refuses_non_local_redirects() {
        let err = Loopback::bind("https://example.com/callback")
            .await
            .err()
            .unwrap();
        assert!(err.contains("127.0.0.1"), "{err}");
    }
}
