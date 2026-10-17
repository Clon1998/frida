//! HTTPS mode: serves the same routes over TLS with a freshly generated, throwaway CA.

use std::net::SocketAddr;

use axum::serve::Listener;
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use time::{Duration, OffsetDateTime};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::server::TlsStream;

/// A running HTTPS test server.
pub struct TlsServer {
    pub addr: SocketAddr,
    /// PEM of the CA that signed the server certificate. Not trusted by the OS.
    pub ca_pem: String,
}

/// Starts an HTTPS server on a free port of 127.0.0.1.
pub async fn spawn_tls() -> std::io::Result<TlsServer> {
    let (config, ca_pem) = generate_certs().map_err(std::io::Error::other)?;
    let tcp = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let addr = tcp.local_addr()?;
    let listener = TlsListener {
        tcp,
        acceptor: TlsAcceptor::from(std::sync::Arc::new(config)),
    };
    tokio::spawn(async move {
        axum::serve(listener, crate::router())
            .await
            .expect("TLS test server crashed");
    });
    Ok(TlsServer { addr, ca_pem })
}

/// A CA plus a `localhost`/`127.0.0.1` server certificate signed by it.
fn generate_certs() -> Result<(ServerConfig, String), Box<dyn std::error::Error + Send + Sync>> {
    let now = OffsetDateTime::now_utc();

    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Frida Test CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params.not_before = now - Duration::days(1);
    ca_params.not_after = now + Duration::days(30);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate()?)?;

    let mut leaf_params = CertificateParams::new(Vec::<String>::new())?;
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    leaf_params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into()?),
        SanType::IpAddress([127, 0, 0, 1].into()),
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    // macOS rejects TLS server certificates that are valid for too long.
    leaf_params.not_before = now - Duration::days(1);
    leaf_params.not_after = now + Duration::days(30);
    let leaf_key = KeyPair::generate()?;
    let leaf = leaf_params.signed_by(&leaf_key, &ca)?;

    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    // The provider is named explicitly: with more than one rustls backend in the build (the
    // app's updater brings `ring`, reqwest uses `aws-lc-rs`), `ServerConfig::builder()` can't
    // pick one and panics.
    let provider = std::sync::Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![leaf.der().clone(), ca.der().clone()], key)?;
    Ok((config, ca.pem()))
}

/// Lets `axum::serve` accept TLS connections: each TCP connection is wrapped in a TLS
/// handshake before axum sees it. Failed handshakes (e.g. a client that rejected our
/// certificate) are skipped.
struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((stream, addr)) = self.tcp.accept().await else {
                continue;
            };
            if let Ok(tls) = self.acceptor.accept(stream).await {
                return (tls, addr);
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}
