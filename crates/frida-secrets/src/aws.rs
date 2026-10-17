//! AWS Secrets Manager as a secret source (PLAN.md §9.1).
//!
//! - [`list_profiles`] reads the profile names from `~/.aws/config` and `~/.aws/credentials`
//!   (or the files chosen in the settings, see [`AwsFiles`]).
//! - [`AwsSecrets`] fetches secret values with a profile, caches them in memory (15 minutes by
//!   default, changeable in the settings) and picks a key out of JSON secrets. The actual AWS call sits behind [`SecretFetcher`], so
//!   tests use a fake instead of the SDK.
//! - Errors say what to do; an expired SSO session tells the user to run `aws sso login`
//!   (Frida never runs it itself).

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How long fetched values are reused before asking AWS again.
pub const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Where the AWS config and credentials files are. `None` means the usual place:
/// `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE` if set, else `~/.aws/config` and
/// `~/.aws/credentials`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwsFiles {
    pub config: Option<PathBuf>,
    pub credentials: Option<PathBuf>,
}

impl AwsFiles {
    /// The config file that is actually used.
    pub fn config_path(&self) -> PathBuf {
        self.config.clone().unwrap_or_else(|| {
            std::env::var_os("AWS_CONFIG_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir().join(".aws/config"))
        })
    }

    /// The credentials file that is actually used.
    pub fn credentials_path(&self) -> PathBuf {
        self.credentials.clone().unwrap_or_else(|| {
            std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| home_dir().join(".aws/credentials"))
        })
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// A profile from the AWS config files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AwsProfile {
    pub name: String,
    /// Uses IAM Identity Center (SSO) login.
    pub sso: bool,
    pub region: Option<String>,
}

/// Why an AWS secret couldn't be read. The messages are meant for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AwsError {
    #[error("the AWS SSO session of profile {profile:?} has expired or is missing")]
    SsoLogin { profile: String },

    #[error("no AWS credentials found for profile {profile:?}")]
    NoCredentials { profile: String },

    #[error("secret {secret_id:?} was not found (check the id and the region)")]
    NotFound { secret_id: String },

    #[error("profile {profile:?} is not allowed to read secret {secret_id:?}")]
    AccessDenied { profile: String, secret_id: String },

    #[error("secret {secret_id:?} is not JSON, so its key {key:?} can't be read")]
    NotJson { secret_id: String, key: String },

    #[error("secret {secret_id:?} has no key {key:?}")]
    MissingKey { secret_id: String, key: String },

    #[error("asking AWS for secret {secret_id:?} took too long")]
    Timeout { secret_id: String },

    #[error("AWS error: {0}")]
    Other(String),
}

impl AwsError {
    /// The command that fixes the problem, if there is one (for a copy button).
    pub fn fix_command(&self) -> Option<String> {
        match self {
            AwsError::SsoLogin { profile } => Some(format!("aws sso login --profile {profile}")),
            _ => None,
        }
    }
}

/// Which secret to read, and how.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AwsSecretRef {
    pub profile: String,
    /// `None` uses the profile's region.
    pub region: Option<String>,
    pub secret_id: String,
    /// Read this key from a JSON secret instead of using the whole string.
    pub json_key: Option<String>,
}

/// A boxed future that can move between threads (`async fn` can't be used in a `dyn` trait).
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How AWS calls reach AWS (PLAN.md M16): through the proxy from `HTTPS_PROXY`/`NO_PROXY`
/// (the default), directly, or through a manual proxy. Built by the app from its network
/// settings. `Debug` is written by hand so the password never shows.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum AwsProxy {
    #[default]
    FromEnv,
    Disabled,
    Manual {
        /// `http://host:port` or `https://host:port`, without credentials.
        url: String,
        username: Option<String>,
        password: Option<String>,
        /// `NO_PROXY` syntax: `localhost,.internal,10.0.0.0/8`.
        no_proxy: String,
    },
}

impl std::fmt::Debug for AwsProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FromEnv => write!(f, "FromEnv"),
            Self::Disabled => write!(f, "Disabled"),
            Self::Manual {
                url,
                username,
                no_proxy,
                ..
            } => f
                .debug_struct("Manual")
                .field("url", url)
                .field("username", username)
                .field("no_proxy", no_proxy)
                .finish_non_exhaustive(),
        }
    }
}

/// Reads one secret string from AWS. [`SdkFetcher`] is the real one.
pub trait SecretFetcher: Send + Sync {
    fn fetch(
        &self,
        files: &AwsFiles,
        proxy: &AwsProxy,
        profile: &str,
        region: Option<&str>,
        secret_id: &str,
    ) -> BoxFuture<Result<String, AwsError>>;
}

/// `(profile, region, secret id)`: one AWS secret, whatever JSON key is read from it.
type CacheKey = (String, Option<String>, String);

/// Secret values from AWS, cached in memory (with when they were fetched). Share one per app.
pub struct AwsSecrets {
    fetcher: Arc<dyn SecretFetcher>,
    /// The settings can change while the app runs, hence the locks (read often, written rarely).
    ttl: RwLock<Duration>,
    files: RwLock<AwsFiles>,
    proxy: RwLock<AwsProxy>,
    cache: Mutex<HashMap<CacheKey, (Instant, String)>>,
}

impl AwsSecrets {
    pub fn new(fetcher: Arc<dyn SecretFetcher>) -> Self {
        Self::with_ttl(fetcher, CACHE_TTL)
    }

    pub fn with_ttl(fetcher: Arc<dyn SecretFetcher>, ttl: Duration) -> Self {
        Self {
            fetcher,
            ttl: RwLock::new(ttl),
            files: RwLock::new(AwsFiles::default()),
            proxy: RwLock::new(AwsProxy::default()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Applies the settings. Other files can mean other credentials, so changing them empties
    /// the cache.
    pub fn configure(&self, ttl: Duration, files: AwsFiles) {
        *self.ttl.write().expect("aws ttl lock poisoned") = ttl;
        let mut current = self.files.write().expect("aws files lock poisoned");
        if *current != files {
            *current = files;
            self.clear(None);
        }
    }

    /// Sets how later AWS calls reach AWS. Cached values stay valid (same credentials).
    pub fn set_proxy(&self, proxy: AwsProxy) {
        *self.proxy.write().expect("aws proxy lock poisoned") = proxy;
    }

    /// The config files in use (for listing profiles).
    pub fn files(&self) -> AwsFiles {
        self.files.read().expect("aws files lock poisoned").clone()
    }

    /// With the real AWS SDK.
    pub fn with_sdk() -> Self {
        Self::new(Arc::new(SdkFetcher))
    }

    /// The value from the cache, without asking AWS (`None` when not cached or expired).
    pub fn cached(&self, secret: &AwsSecretRef) -> Option<Result<String, AwsError>> {
        let cache = self.cache.lock().expect("aws cache lock poisoned");
        let (fetched, raw) = cache.get(&cache_key(secret))?;
        let ttl = *self.ttl.read().expect("aws ttl lock poisoned");
        (fetched.elapsed() < ttl).then(|| pick(raw, secret))
    }

    /// The secret's value: from the cache when fresh, otherwise from AWS.
    pub async fn get(&self, secret: &AwsSecretRef) -> Result<String, AwsError> {
        if let Some(value) = self.cached(secret) {
            return value;
        }
        let files = self.files();
        let proxy = self.proxy.read().expect("aws proxy lock poisoned").clone();
        let raw = self
            .fetcher
            .fetch(
                &files,
                &proxy,
                &secret.profile,
                secret.region.as_deref(),
                &secret.secret_id,
            )
            .await?;
        // The lock is taken only after the await, never held across it.
        self.cache
            .lock()
            .expect("aws cache lock poisoned")
            .insert(cache_key(secret), (Instant::now(), raw.clone()));
        pick(&raw, secret)
    }

    /// Forgets cached values ("refresh secrets"), all or those of one profile.
    pub fn clear(&self, profile: Option<&str>) {
        let mut cache = self.cache.lock().expect("aws cache lock poisoned");
        match profile {
            Some(p) => cache.retain(|(profile, _, _), _| profile != p),
            None => cache.clear(),
        }
    }
}

fn cache_key(secret: &AwsSecretRef) -> CacheKey {
    (
        secret.profile.clone(),
        secret.region.clone(),
        secret.secret_id.clone(),
    )
}

/// The whole secret string, or one key of a JSON secret.
fn pick(raw: &str, secret: &AwsSecretRef) -> Result<String, AwsError> {
    let Some(key) = secret.json_key.as_deref().filter(|k| !k.is_empty()) else {
        return Ok(raw.to_string());
    };
    let json: serde_json::Value = serde_json::from_str(raw).map_err(|_| AwsError::NotJson {
        secret_id: secret.secret_id.clone(),
        key: key.to_string(),
    })?;
    match json.get(key) {
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(AwsError::MissingKey {
            secret_id: secret.secret_id.clone(),
            key: key.to_string(),
        }),
    }
}

/// The real fetcher: the AWS SDK with the named profile (credentials, SSO and region come from
/// the AWS config files, exactly like the AWS CLI).
pub struct SdkFetcher;

impl SecretFetcher for SdkFetcher {
    fn fetch(
        &self,
        files: &AwsFiles,
        proxy: &AwsProxy,
        profile: &str,
        region: Option<&str>,
        secret_id: &str,
    ) -> BoxFuture<Result<String, AwsError>> {
        let files = files.clone();
        let http_client = http_client(proxy);
        let profile = profile.to_string();
        let region = region.map(str::to_string);
        let secret_id = secret_id.to_string();
        Box::pin(async move {
            let call = async {
                // The HTTP client is used for AWS and for the credential providers (SSO too).
                let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .http_client(http_client)
                    .profile_name(&profile);
                if let Some(region) = region {
                    loader = loader.region(aws_config::Region::new(region));
                }
                if files != AwsFiles::default() {
                    loader = loader.profile_files(profile_files(&files));
                }
                let config = loader.load().await;
                let client = aws_sdk_secretsmanager::Client::new(&config);
                client.get_secret_value().secret_id(&secret_id).send().await
            };
            let output = tokio::time::timeout(Duration::from_secs(30), call)
                .await
                .map_err(|_| AwsError::Timeout {
                    secret_id: secret_id.clone(),
                })?
                .map_err(|err| classify(&error_chain(&err), &profile, &secret_id))?;
            match (output.secret_string(), output.secret_binary()) {
                (Some(text), _) => Ok(text.to_string()),
                (None, Some(blob)) => String::from_utf8(blob.as_ref().to_vec())
                    .map_err(|_| AwsError::Other("the secret is binary, not text".into())),
                (None, None) => Err(AwsError::Other("the secret has no value".into())),
            }
        })
    }
}

/// The SDK's HTTP client with Frida's proxy setting. The SDK's default client ignores proxies,
/// so it is built here like the default one (rustls with aws-lc), plus the proxy.
///
/// `build_with_connector_fn` is the hook the SDK offers for custom connectors; the closure
/// copies what the SDK's own builder passes on (connector settings, the sleep source), so
/// timeouts keep working.
fn http_client(proxy: &AwsProxy) -> aws_sdk_secretsmanager::config::SharedHttpClient {
    use aws_smithy_http_client::proxy::ProxyConfig;
    use aws_smithy_http_client::tls::{Provider, rustls_provider::CryptoMode};
    use aws_smithy_http_client::{Builder, Connector};

    let config = match proxy {
        AwsProxy::FromEnv => ProxyConfig::from_env(),
        AwsProxy::Disabled => ProxyConfig::disabled(),
        AwsProxy::Manual {
            url,
            username,
            password,
            no_proxy,
        } => match ProxyConfig::all(url.as_str()) {
            Ok(mut config) => {
                if let Some(username) = username {
                    config = config
                        .with_basic_auth(username.as_str(), password.as_deref().unwrap_or(""));
                }
                config.no_proxy(no_proxy)
            }
            // e.g. a SOCKS proxy, which the SDK doesn't support.
            Err(err) => {
                tracing::warn!(%err, "AWS calls can't use this proxy; using HTTPS_PROXY instead");
                ProxyConfig::from_env()
            }
        },
    };
    Builder::new().build_with_connector_fn(move |settings, components| {
        let mut builder = Connector::builder().proxy_config(config.clone());
        builder.set_connector_settings(settings.cloned());
        if let Some(components) = components {
            builder.set_sleep_impl(components.sleep_impl());
        }
        builder
            .tls_provider(Provider::Rustls(CryptoMode::AwsLc))
            .build()
    })
}

/// The SDK's view of [`AwsFiles`]: a chosen file replaces the usual one, the other stays.
fn profile_files(files: &AwsFiles) -> aws_runtime::env_config::file::EnvConfigFiles {
    use aws_runtime::env_config::file::{EnvConfigFileKind as ProfileFileKind, EnvConfigFiles};
    let mut builder = EnvConfigFiles::builder();
    builder = match &files.config {
        Some(path) => builder.with_file(ProfileFileKind::Config, path),
        None => builder.include_default_config_file(true),
    };
    builder = match &files.credentials {
        Some(path) => builder.with_file(ProfileFileKind::Credentials, path),
        None => builder.include_default_credentials_file(true),
    };
    builder.build()
}

/// The error and all its causes in one line (the SDK nests the interesting part).
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// Turns an SDK error message into an actionable [`AwsError`].
pub(crate) fn classify(message: &str, profile: &str, secret_id: &str) -> AwsError {
    let lower = message.to_lowercase();
    if lower.contains("sso")
        && (lower.contains("expired")
            || lower.contains("token")
            || lower.contains("session")
            || lower.contains("cache"))
    {
        AwsError::SsoLogin {
            profile: profile.to_string(),
        }
    } else if lower.contains("resourcenotfound")
        || lower.contains("can't find the specified secret")
    {
        AwsError::NotFound {
            secret_id: secret_id.to_string(),
        }
    } else if lower.contains("accessdenied") || lower.contains("not authorized") {
        AwsError::AccessDenied {
            profile: profile.to_string(),
            secret_id: secret_id.to_string(),
        }
    } else if lower.contains("no credentials")
        || lower.contains("credentialsnotloaded")
        || lower.contains("failed to load credentials")
        || lower.contains("could not find profile")
    {
        AwsError::NoCredentials {
            profile: profile.to_string(),
        }
    } else {
        AwsError::Other(message.to_string())
    }
}

/// The profiles in the AWS config and credentials files (see [`AwsFiles`]), sorted by name.
pub fn list_profiles(files: &AwsFiles) -> Vec<AwsProfile> {
    parse_profiles(
        &std::fs::read_to_string(files.config_path()).unwrap_or_default(),
        &std::fs::read_to_string(files.credentials_path()).unwrap_or_default(),
    )
}

/// The position of profile `name`, added if it isn't there yet. (A plain function rather than
/// a closure: a closure capturing `profiles` mutably would keep it borrowed for the whole loop.)
fn profile_index(profiles: &mut Vec<AwsProfile>, name: String) -> usize {
    match profiles.iter().position(|p| p.name == name) {
        Some(i) => i,
        None => {
            profiles.push(AwsProfile {
                name,
                sso: false,
                region: None,
            });
            profiles.len() - 1
        }
    }
}

/// Profile names from the two INI files. In `config` sections are `[profile name]` (and
/// `[default]`); in `credentials` just `[name]`. `[sso-session x]` sections aren't profiles.
pub fn parse_profiles(config: &str, credentials: &str) -> Vec<AwsProfile> {
    let mut profiles: Vec<AwsProfile> = Vec::new();
    for (text, in_config) in [(config, true), (credentials, false)] {
        let mut current: Option<usize> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let section = section.trim();
                let name = if in_config {
                    if section == "default" {
                        Some("default".to_string())
                    } else {
                        section
                            .strip_prefix("profile ")
                            .map(|n| n.trim().to_string())
                    }
                } else {
                    Some(section.to_string())
                };
                current = name
                    .filter(|n| !n.is_empty())
                    .map(|n| profile_index(&mut profiles, n));
                continue;
            }
            let (Some(index), Some((key, value))) = (current, line.split_once('=')) else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key.starts_with("sso_") {
                profiles[index].sso = true;
            } else if key == "region" {
                profiles[index].region = Some(value.to_string());
            }
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Counts calls and answers from a fixed map.
    struct FakeFetcher {
        calls: AtomicUsize,
        answers: HashMap<String, Result<String, AwsError>>,
    }

    impl SecretFetcher for FakeFetcher {
        fn fetch(
            &self,
            _: &AwsFiles,
            _: &AwsProxy,
            _: &str,
            _: Option<&str>,
            id: &str,
        ) -> BoxFuture<Result<String, AwsError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let answer = self.answers.get(id).cloned().unwrap_or_else(|| {
                Err(AwsError::NotFound {
                    secret_id: id.into(),
                })
            });
            Box::pin(async move { answer })
        }
    }

    fn secret(id: &str, key: Option<&str>) -> AwsSecretRef {
        AwsSecretRef {
            profile: "dev".into(),
            region: None,
            secret_id: id.into(),
            json_key: key.map(str::to_string),
        }
    }

    fn fake() -> Arc<FakeFetcher> {
        Arc::new(FakeFetcher {
            calls: AtomicUsize::new(0),
            answers: HashMap::from([
                ("plain".to_string(), Ok("s3cr3t".to_string())),
                (
                    "json".to_string(),
                    Ok(r#"{"password":"pw","port":5432}"#.to_string()),
                ),
            ]),
        })
    }

    #[tokio::test]
    async fn caches_values_until_the_ttl_or_a_refresh() {
        let fetcher = fake();
        let aws = AwsSecrets::new(fetcher.clone());
        assert_eq!(aws.get(&secret("plain", None)).await.unwrap(), "s3cr3t");
        assert_eq!(aws.get(&secret("plain", None)).await.unwrap(), "s3cr3t");
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);

        aws.clear(Some("dev"));
        aws.get(&secret("plain", None)).await.unwrap();
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);

        let expired = AwsSecrets::with_ttl(fetcher.clone(), Duration::ZERO);
        expired.get(&secret("plain", None)).await.unwrap();
        assert!(expired.cached(&secret("plain", None)).is_none());

        // Settings: a longer cache time applies to values already cached; other files empty it.
        expired.configure(Duration::from_secs(60), AwsFiles::default());
        assert!(expired.cached(&secret("plain", None)).is_some());
        let other = AwsFiles {
            config: Some("/elsewhere/config".into()),
            credentials: None,
        };
        expired.configure(Duration::from_secs(60), other.clone());
        assert!(expired.cached(&secret("plain", None)).is_none());
        assert_eq!(expired.files(), other);
    }

    #[tokio::test]
    async fn json_keys_are_picked_from_one_fetch() {
        let fetcher = fake();
        let aws = AwsSecrets::new(fetcher.clone());
        assert_eq!(
            aws.get(&secret("json", Some("password"))).await.unwrap(),
            "pw"
        );
        assert_eq!(
            aws.get(&secret("json", Some("port"))).await.unwrap(),
            "5432"
        );
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            aws.get(&secret("json", Some("user"))).await,
            Err(AwsError::MissingKey { .. })
        ));
        assert!(matches!(
            aws.get(&secret("plain", Some("x"))).await,
            Err(AwsError::NotJson { .. })
        ));
        assert!(matches!(
            aws.get(&secret("missing", None)).await,
            Err(AwsError::NotFound { .. })
        ));
    }

    #[test]
    fn sdk_errors_become_actionable() {
        let sso = classify(
            "dispatch failure: other: the credential provider was not enabled: SSO token for profile dev has expired",
            "dev",
            "db",
        );
        assert_eq!(
            sso,
            AwsError::SsoLogin {
                profile: "dev".into()
            }
        );
        assert_eq!(
            sso.fix_command().as_deref(),
            Some("aws sso login --profile dev")
        );
        assert!(matches!(
            classify(
                "service error: ResourceNotFoundException: Secrets Manager can't find the specified secret.",
                "dev",
                "db"
            ),
            AwsError::NotFound { .. }
        ));
        assert!(matches!(
            classify(
                "service error: AccessDeniedException: User is not authorized",
                "dev",
                "db"
            ),
            AwsError::AccessDenied { .. }
        ));
        assert!(matches!(
            classify(
                "failed to load credentials: no credentials in the property bag",
                "dev",
                "db"
            ),
            AwsError::NoCredentials { .. }
        ));
    }

    #[test]
    fn profiles_come_from_both_files() {
        let config = "
[default]
region = eu-central-1

[profile work]
sso_session = corp
sso_account_id = 123
region = eu-west-1

[sso-session corp]
sso_start_url = https://corp.awsapps.com/start

[profile ci]
role_arn = arn:aws:iam::1:role/ci
";
        let credentials = "
[default]
aws_access_key_id = AKIA

[legacy]
aws_access_key_id = AKIB
";
        let profiles = parse_profiles(config, credentials);
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["ci", "default", "legacy", "work"]);
        let work = profiles.iter().find(|p| p.name == "work").unwrap();
        assert!(work.sso);
        assert_eq!(work.region.as_deref(), Some("eu-west-1"));
    }
}
