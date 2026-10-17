//! A small forward proxy for the proxy tests (PLAN.md M16): plain HTTP requests in absolute
//! form (`GET http://host/path`) and `CONNECT host:port` tunnels for HTTPS, with optional Basic
//! authentication. Every request is recorded, so tests can check what went through it.
//!
//! It handles one request per connection (it answers with `Connection: close`), which is all
//! a test needs.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A running test proxy.
pub struct ProxyServer {
    pub addr: SocketAddr,
    /// `"GET http://127.0.0.1:1234/echo"` or `"CONNECT localhost:4321"`, in arrival order,
    /// including refused ones.
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl ProxyServer {
    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("proxy log poisoned").clone()
    }
}

/// Starts a proxy on a free port. With `login`, requests without that Basic login get
/// `407 Proxy Authentication Required`.
pub async fn spawn_proxy(login: Option<(&str, &str)>) -> std::io::Result<ProxyServer> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let requests = Arc::new(Mutex::new(Vec::new()));
    // The Base64 token of `user:password`, as in `Proxy-Authorization: Basic <token>`.
    let expected = login.map(|(user, password)| {
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    });
    let log = Arc::clone(&requests);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let log = Arc::clone(&log);
            let expected = expected.clone();
            tokio::spawn(async move {
                let _ = handle(stream, log, expected).await;
            });
        }
    });
    Ok(ProxyServer { addr, requests })
}

async fn handle(
    mut client: TcpStream,
    log: Arc<Mutex<Vec<String>>>,
    expected: Option<String>,
) -> std::io::Result<()> {
    let (head, rest) = read_head(&mut client).await?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    log.lock()
        .expect("proxy log poisoned")
        .push(format!("{method} {target}"));

    if let Some(expected) = expected {
        // The scheme ignores case, the Base64 token doesn't.
        let ok = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("proxy-authorization"))
            .and_then(|(_, v)| v.split_once(' '))
            .is_some_and(|(scheme, token)| {
                scheme.eq_ignore_ascii_case("basic") && token.trim() == expected
            });
        if !ok {
            client
                .write_all(
                    b"HTTP/1.1 407 Proxy Authentication Required\r\n\
                      Proxy-Authenticate: Basic realm=\"frida-test\"\r\n\
                      Content-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
            return Ok(());
        }
    }

    if method.eq_ignore_ascii_case("CONNECT") {
        let Ok(mut upstream) = TcpStream::connect(&target).await else {
            client
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                .await?;
            return Ok(());
        };
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        upstream.write_all(&rest).await?;
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        return Ok(());
    }

    // `GET http://host:port/path` → connect to host:port and send `GET /path`.
    let Some(without_scheme) = target.strip_prefix("http://") else {
        client
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    let (authority, path) = match without_scheme.find('/') {
        Some(i) => (&without_scheme[..i], &without_scheme[i..]),
        None => (without_scheme, "/"),
    };
    let Ok(mut upstream) = TcpStream::connect(authority).await else {
        client
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    let mut forwarded = format!("{method} {path} {version}\r\n");
    for (name, value) in &headers {
        let skip = name.eq_ignore_ascii_case("proxy-authorization")
            || name.eq_ignore_ascii_case("proxy-connection")
            || name.eq_ignore_ascii_case("connection");
        if !skip {
            forwarded.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    forwarded.push_str("Connection: close\r\n\r\n");
    upstream.write_all(forwarded.as_bytes()).await?;
    upstream.write_all(&rest).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

/// Reads up to the end of the request head; returns the head and any body bytes read with it.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf[end + 4..].to_vec();
            buf.truncate(end);
            return Ok((String::from_utf8_lossy(&buf).into_owned(), rest));
        }
        if buf.len() > 64 * 1024 {
            break;
        }
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), Vec::new()))
}
