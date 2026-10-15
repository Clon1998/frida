//! `cargo run -p frida-testserver [port]` starts the echo server (default port 8787).

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let addr = frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    println!("Frida test server listening on http://{addr}  (Ctrl+C to stop)");
    tokio::signal::ctrl_c().await
}
