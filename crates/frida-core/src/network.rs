//! HTTP proxy settings (PLAN.md M16): which proxy a request goes through, if any.
//!
//! Three modes, like browsers offer them: the operating system's proxy settings (reqwest reads
//! `HTTP(S)_PROXY`/`NO_PROXY` and the macOS/Windows system settings), no proxy at all, or a
//! manual proxy with an optional login and a bypass list. In manual mode Frida decides per URL
//! itself ([`ProxySettings::proxy_for`]), so the console can say whether a request went
//! through the proxy.
//!
//! The password is never part of [`ProxyPreferences`] (that is `settings.json`); the app keeps
//! it in the OS keychain and hands it in when it builds [`ProxySettings`].

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;

/// What the Settings dialog stores (in `settings.json`, without the password).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ProxyPreferences {
    pub mode: ProxyMode,
    /// `http://host:port`, `https://host:port` or `socks5://host:port` (manual mode).
    pub url: String,
    /// Login for the proxy, empty for none. The password is in the keychain.
    pub username: String,
    /// Hosts that are reached directly: names (`localhost`), domains with their subdomains
    /// (`*.internal` or `.internal`), IP addresses and CIDR ranges (`10.0.0.0/8`), separated
    /// by commas, spaces or new lines.
    pub bypass: String,
}

impl Default for ProxyPreferences {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            url: String::new(),
            username: String::new(),
            bypass: "localhost, 127.0.0.1, ::1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    /// The operating system's settings and the `HTTP(S)_PROXY`/`NO_PROXY` variables.
    #[default]
    System,
    /// Never use a proxy.
    None,
    /// The proxy in [`ProxyPreferences::url`].
    Manual,
}

/// The proxy for the requests of one send, as the HTTP client applies it.
///
/// Cheap to clone: the manual proxy sits behind an `Arc` (shared, read-only), like passing a
/// reference to an immutable object around in Java.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxySettings {
    #[default]
    System,
    Off,
    Manual(Arc<ManualProxy>),
}

/// A configured proxy with its login. `Debug` is written by hand so the password never ends
/// up in a log or an error message.
#[derive(Clone, PartialEq, Eq)]
pub struct ManualProxy {
    /// Without credentials, e.g. `http://proxy.corp:8080`.
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bypass: Bypass,
}

impl fmt::Debug for ManualProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManualProxy")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("bypass", &self.bypass)
            .finish()
    }
}

/// Why the proxy settings can't be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProxyConfigError {
    #[error("the proxy URL {0:?} is not valid (expected e.g. http://proxy.example.com:8080)")]
    InvalidUrl(String),
    #[error("proxy URLs must start with http://, https:// or socks5:// (got {0:?})")]
    UnsupportedScheme(String),
}

impl ProxySettings {
    /// The settings for `prefs`, with the password from the keychain. A manual proxy without
    /// a URL counts as no proxy configured yet, i.e. the system settings.
    pub fn from_preferences(
        prefs: &ProxyPreferences,
        password: Option<String>,
    ) -> Result<Self, ProxyConfigError> {
        match prefs.mode {
            ProxyMode::System => Ok(Self::System),
            ProxyMode::None => Ok(Self::Off),
            ProxyMode::Manual if prefs.url.trim().is_empty() => Ok(Self::System),
            ProxyMode::Manual => {
                let username = Some(prefs.username.trim().to_string()).filter(|u| !u.is_empty());
                let mut proxy = ManualProxy::parse(prefs.url.trim(), &prefs.bypass)?;
                if username.is_some() {
                    proxy.username = username;
                    proxy.password = password;
                }
                Ok(Self::Manual(Arc::new(proxy)))
            }
        }
    }

    /// The manual proxy that `url` goes through, if it does. `None` for bypassed hosts, for
    /// [`ProxySettings::Off`], and for the system settings (the client decides those itself).
    pub fn proxy_for(&self, url: &str) -> Option<&ManualProxy> {
        let Self::Manual(proxy) = self else {
            return None;
        };
        let host = reqwest::Url::parse(url).ok()?.host_str()?.to_string();
        (!proxy.bypass.matches(&host)).then_some(proxy.as_ref())
    }

    /// For the console: how `url` was sent, e.g. `via proxy http://proxy:8080`. `None` when
    /// it went directly (or the system decided).
    pub fn describe_for(&self, url: &str) -> Option<String> {
        self.proxy_for(url)
            .map(|proxy| format!("via proxy {}", proxy.url))
    }

    /// The same settings for the AWS SDK's HTTP client.
    pub fn for_aws(&self) -> frida_secrets::aws::AwsProxy {
        use frida_secrets::aws::AwsProxy;
        match self {
            Self::System => AwsProxy::FromEnv,
            Self::Off => AwsProxy::Disabled,
            Self::Manual(proxy) => AwsProxy::Manual {
                url: proxy.url.clone(),
                username: proxy.username.clone(),
                password: proxy.password.clone(),
                no_proxy: proxy.bypass.to_no_proxy(),
            },
        }
    }

    /// Applies the settings to a reqwest client builder.
    pub(crate) fn apply(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        match self {
            // reqwest's default: system settings and environment variables.
            Self::System => builder,
            Self::Off => builder.no_proxy(),
            Self::Manual(proxy) => {
                let decide = Arc::clone(proxy);
                // Asked for every URL (also after redirects): the proxy, or `None` to go
                // directly. The closure owns its own `Arc`, so it can outlive this function.
                let mut custom = reqwest::Proxy::custom(move |url| {
                    let host = url.host_str()?;
                    (!decide.bypass.matches(host)).then(|| decide.url.clone())
                });
                if let Some(username) = &proxy.username {
                    custom = custom.basic_auth(username, proxy.password.as_deref().unwrap_or(""));
                }
                builder.proxy(custom)
            }
        }
    }
}

impl ManualProxy {
    /// Checks the URL and splits credentials written into it (`http://user:pw@proxy:8080`,
    /// as `frida-run --proxy` allows) from the URL that is shown.
    pub fn parse(url: &str, bypass: &str) -> Result<Self, ProxyConfigError> {
        let mut parsed =
            reqwest::Url::parse(url).map_err(|_| ProxyConfigError::InvalidUrl(url.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h") {
            return Err(ProxyConfigError::UnsupportedScheme(url.to_string()));
        }
        if parsed.host_str().is_none_or(str::is_empty) {
            return Err(ProxyConfigError::InvalidUrl(url.to_string()));
        }
        let username = Some(percent_decode(parsed.username())).filter(|u| !u.is_empty());
        let password = parsed.password().map(percent_decode);
        // Setting empty credentials can't fail for URLs with a host.
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
        Ok(Self {
            url: parsed.as_str().trim_end_matches('/').to_string(),
            username,
            password,
            bypass: Bypass::parse(bypass),
        })
    }
}

/// `%40` → `@` in credentials written into a URL.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The hosts that don't go through the proxy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bypass {
    rules: Vec<BypassRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BypassRule {
    /// `localhost`, `api.example.com`: this host only.
    Host(String),
    /// `*.internal` or `.internal`: the domain and all its subdomains.
    Domain(String),
    /// `10.0.0.0/8`, `::1`: addresses (a single address is a /32 or /128 range).
    Range(IpAddr, u8),
}

impl Bypass {
    /// Rules separated by commas, semicolons, spaces or new lines; case doesn't matter.
    /// A port (`localhost:8080`) is ignored: rules are about hosts.
    pub fn parse(text: &str) -> Self {
        let rules = text
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .filter_map(|rule| {
                let rule = rule.to_ascii_lowercase();
                if let Some((ip, bits)) = rule.split_once('/') {
                    let ip: IpAddr = ip.trim_matches(['[', ']']).parse().ok()?;
                    let max = if ip.is_ipv4() { 32 } else { 128 };
                    let bits: u8 = bits.parse().ok().filter(|b| *b <= max)?;
                    return Some(BypassRule::Range(ip, bits));
                }
                if let Ok(ip) = rule.trim_matches(['[', ']']).parse::<IpAddr>() {
                    let bits = if ip.is_ipv4() { 32 } else { 128 };
                    return Some(BypassRule::Range(ip, bits));
                }
                if let Some(domain) = rule.strip_prefix("*.").or_else(|| rule.strip_prefix('.')) {
                    return Some(BypassRule::Domain(domain.to_string()));
                }
                let host = rule.split(':').next().unwrap_or(&rule);
                Some(BypassRule::Host(host.to_string()))
            })
            .collect();
        Self { rules }
    }

    /// The rules in `NO_PROXY` syntax (`localhost,.internal,10.0.0.0/8`), for the AWS SDK.
    pub fn to_no_proxy(&self) -> String {
        self.rules
            .iter()
            .map(|rule| match rule {
                BypassRule::Host(host) => host.clone(),
                BypassRule::Domain(domain) => format!(".{domain}"),
                BypassRule::Range(ip, bits) => format!("{ip}/{bits}"),
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Whether `host` (a name or an IP address, IPv6 with or without brackets) is bypassed.
    pub fn matches(&self, host: &str) -> bool {
        let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
        let ip: Option<IpAddr> = host.parse().ok();
        self.rules.iter().any(|rule| match rule {
            BypassRule::Host(name) => *name == host,
            BypassRule::Domain(domain) => host == *domain || host.ends_with(&format!(".{domain}")),
            BypassRule::Range(net, bits) => ip.is_some_and(|ip| in_range(ip, *net, *bits)),
        })
    }
}

/// Whether `ip` is in `net/bits`. Compares the first `bits` bits of both addresses.
fn in_range(ip: IpAddr, net: IpAddr, bits: u8) -> bool {
    let (ip, net, width) = match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(b)) => (u128::from(u32::from(a)), u128::from(u32::from(b)), 32),
        (IpAddr::V6(a), IpAddr::V6(b)) => (u128::from(a), u128::from(b), 128),
        _ => return false,
    };
    if bits == 0 {
        return true;
    }
    let shift = width - u32::from(bits);
    (ip >> shift) == (net >> shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_rules_match_hosts_domains_and_ranges() {
        let bypass =
            Bypass::parse("localhost, *.internal\n.corp.example;10.0.0.0/8 ::1 api.test:8080");
        for host in [
            "localhost",
            "LOCALHOST",
            "internal",
            "svc.internal",
            "a.b.internal",
            "corp.example",
            "x.corp.example",
            "10.1.2.3",
            "::1",
            "[::1]",
            "api.test",
        ] {
            assert!(bypass.matches(host), "{host} should be bypassed");
        }
        for host in [
            "example.com",
            "notinternal",
            "11.0.0.1",
            "127.0.0.1",
            "test",
        ] {
            assert!(!bypass.matches(host), "{host} should use the proxy");
        }
    }

    #[test]
    fn ranges_compare_the_prefix_bits() {
        assert!(in_range(
            "192.168.1.77".parse().unwrap(),
            "192.168.0.0".parse().unwrap(),
            16
        ));
        assert!(!in_range(
            "192.169.0.1".parse().unwrap(),
            "192.168.0.0".parse().unwrap(),
            16
        ));
        assert!(in_range(
            "1.2.3.4".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
            0
        ));
        assert!(in_range(
            "fd00::1".parse().unwrap(),
            "fd00::".parse().unwrap(),
            8
        ));
        assert!(!in_range(
            "10.0.0.1".parse().unwrap(),
            "::".parse().unwrap(),
            0
        ));
        // Invalid prefixes are dropped instead of matching everything.
        assert_eq!(Bypass::parse("10.0.0.0/33, x/8"), Bypass::default());
        assert_eq!(
            Bypass::parse("localhost *.internal 10.0.0.0/8 ::1").to_no_proxy(),
            "localhost,.internal,10.0.0.0/8,::1/128"
        );
    }

    #[test]
    fn manual_proxies_split_credentials_out_of_the_url() {
        let proxy = ManualProxy::parse("http://ann:p%40ss@proxy.corp:3128/", "").unwrap();
        assert_eq!(proxy.url, "http://proxy.corp:3128");
        assert_eq!(proxy.username.as_deref(), Some("ann"));
        assert_eq!(proxy.password.as_deref(), Some("p@ss"));
        assert!(
            !format!("{proxy:?}").contains("p@ss"),
            "password in Debug output"
        );

        assert_eq!(
            ManualProxy::parse("ftp://proxy:21", ""),
            Err(ProxyConfigError::UnsupportedScheme("ftp://proxy:21".into()))
        );
        assert!(matches!(
            ManualProxy::parse("proxy.corp:3128", ""),
            Err(ProxyConfigError::InvalidUrl(_) | ProxyConfigError::UnsupportedScheme(_))
        ));
        assert!(ManualProxy::parse("socks5://127.0.0.1:1080", "").is_ok());
    }

    #[test]
    fn preferences_become_settings() {
        let manual = ProxyPreferences {
            mode: ProxyMode::Manual,
            url: " http://proxy:8080 ".into(),
            username: "ann".into(),
            bypass: "localhost".into(),
        };
        let settings = ProxySettings::from_preferences(&manual, Some("pw".into())).unwrap();
        let proxy = settings.proxy_for("https://api.example.com/x").unwrap();
        assert_eq!(proxy.url, "http://proxy:8080");
        assert_eq!(proxy.password.as_deref(), Some("pw"));
        assert_eq!(settings.proxy_for("http://localhost:8787/echo"), None);
        assert_eq!(
            settings.describe_for("http://a.b/").as_deref(),
            Some("via proxy http://proxy:8080")
        );

        // Without a username the keychain password isn't used.
        let anonymous = ProxyPreferences {
            username: " ".into(),
            ..manual.clone()
        };
        let settings = ProxySettings::from_preferences(&anonymous, Some("pw".into())).unwrap();
        assert_eq!(settings.proxy_for("http://a.b/").unwrap().password, None);

        let empty = ProxyPreferences {
            url: "".into(),
            ..manual
        };
        assert_eq!(
            ProxySettings::from_preferences(&empty, None).unwrap(),
            ProxySettings::System
        );
        let off = ProxyPreferences {
            mode: ProxyMode::None,
            ..ProxyPreferences::default()
        };
        assert_eq!(
            ProxySettings::from_preferences(&off, None).unwrap(),
            ProxySettings::Off
        );
        assert_eq!(ProxySettings::Off.proxy_for("http://a.b/"), None);
    }
}
