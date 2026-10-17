//! M8 and M17 acceptance (PLAN.md §15): AWS-backed secret variables with a fake AWS: Secrets
//! Manager, Cognito app client secrets, API Gateway keys, and per-secret account aliases.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use frida_core::CoreError;
use frida_core::model::{EnvVariable, Environment, SecretSource};
use frida_core::secrets::{AwsFetch, AwsProfiles, fill_aws_secrets};
use frida_secrets::aws::{
    AwsError, AwsFiles, AwsProxy, AwsSecrets, AwsTarget, BoxFuture, SecretFetcher,
};

/// Answers by secret id, Cognito client id or API key id, and records `(profile, region, id)`.
struct Fake {
    calls: AtomicUsize,
    seen: Mutex<Vec<(String, Option<String>, String)>>,
    values: HashMap<&'static str, Result<String, AwsError>>,
}

impl SecretFetcher for Fake {
    fn fetch(
        &self,
        _: &AwsFiles,
        _: &AwsProxy,
        profile: &str,
        region: Option<&str>,
        target: &AwsTarget,
    ) -> BoxFuture<Result<String, AwsError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let id = match target {
            AwsTarget::Secret { secret_id } => secret_id.clone(),
            AwsTarget::CognitoClientSecret { client_id, .. } => client_id.clone(),
            AwsTarget::ApiGatewayKey { api_key_id } => api_key_id.clone(),
        };
        self.seen.lock().unwrap().push((
            profile.to_string(),
            region.map(str::to_string),
            id.clone(),
        ));
        let value = if profile == "expired" {
            Err(AwsError::SsoLogin {
                profile: profile.into(),
            })
        } else {
            self.values
                .get(id.as_str())
                .cloned()
                .unwrap_or(Err(AwsError::NotFound {
                    what: target.describe(),
                }))
        };
        Box::pin(async move { value })
    }
}

fn secret(name: &str, source: SecretSource) -> EnvVariable {
    EnvVariable {
        name: name.into(),
        value: None,
        secret: true,
        source: Some(source),
        enabled: true,
    }
}

fn env() -> Environment {
    let mut env = Environment::new("prod");
    env.variables = vec![
        secret(
            "dbPassword",
            SecretSource::AwsSecretsManager {
                secret_id: "prod/db".into(),
                json_key: Some("password".into()),
                region: Some("eu-central-1".into()),
                account: None,
            },
        ),
        EnvVariable {
            name: "keychainToken".into(),
            value: Some("from-keychain".into()),
            secret: true,
            source: None,
            enabled: true,
        },
    ];
    env
}

fn aws() -> (Arc<Fake>, AwsSecrets) {
    let fake = Arc::new(Fake {
        calls: AtomicUsize::new(0),
        seen: Mutex::new(Vec::new()),
        values: HashMap::from([
            ("prod/db", Ok(r#"{"password":"pw-1"}"#.to_string())),
            ("app-client", Ok("cl1ent-s3cret".to_string())),
            ("key-id", Ok("ap1-key".to_string())),
        ]),
    });
    (fake.clone(), AwsSecrets::new(fake))
}

/// The environment's profile, no account aliases.
fn profile(name: Option<&'static str>) -> AwsProfiles<'static> {
    static NO_ACCOUNTS: BTreeMap<String, String> = BTreeMap::new();
    AwsProfiles {
        environment: name,
        accounts: &NO_ACCOUNTS,
    }
}

#[tokio::test]
async fn aws_secrets_are_filled_and_cached() {
    let (fake, aws) = aws();
    let mut e = env();
    fill_aws_secrets(&mut e, false, profile(Some("dev")), &aws, AwsFetch::Fetch)
        .await
        .unwrap();
    assert_eq!(e.variables[0].value.as_deref(), Some("pw-1"));
    assert_eq!(e.variables[1].value.as_deref(), Some("from-keychain"));

    // The inspector only reads the cache, which is now filled.
    let mut again = env();
    fill_aws_secrets(
        &mut again,
        false,
        profile(Some("dev")),
        &aws,
        AwsFetch::CacheOnly,
    )
    .await
    .unwrap();
    assert_eq!(again.variables[0].value.as_deref(), Some("pw-1"));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_missing_profile_asks_the_ui_to_choose_one() {
    let (fake, aws) = aws();
    let mut e = env();
    let err = fill_aws_secrets(&mut e, true, profile(None), &aws, AwsFetch::Fetch)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::AwsProfileRequired { ref environment, global: true } if environment == "prod")
    );
    // Inspecting never asks and never calls AWS.
    fill_aws_secrets(&mut e, true, profile(None), &aws, AwsFetch::CacheOnly)
        .await
        .unwrap();
    assert_eq!(e.variables[0].value, None);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_expired_sso_session_says_how_to_log_in() {
    let (_, aws) = aws();
    let mut e = env();
    let err = fill_aws_secrets(
        &mut e,
        false,
        profile(Some("expired")),
        &aws,
        AwsFetch::Fetch,
    )
    .await
    .unwrap_err();
    let CoreError::Aws {
        source, variable, ..
    } = err
    else {
        panic!("expected an AWS error")
    };
    assert_eq!(variable, "dbPassword");
    assert_eq!(
        source.fix_command().as_deref(),
        Some("aws sso login --profile expired")
    );
}

#[tokio::test]
async fn cognito_client_secrets_and_api_gateway_keys_are_filled() {
    let (fake, aws) = aws();
    let mut e = Environment::new("prod");
    e.variables = vec![
        secret(
            "clientSecret",
            SecretSource::CognitoClientSecret {
                user_pool_id: "eu-west-1_AbC123".into(),
                client_id: " app-client ".into(),
                region: None,
                account: None,
            },
        ),
        secret(
            "apiKey",
            SecretSource::ApiGatewayKey {
                api_key_id: "key-id".into(),
                region: Some("us-east-1".into()),
                account: None,
            },
        ),
    ];
    fill_aws_secrets(&mut e, false, profile(Some("dev")), &aws, AwsFetch::Fetch)
        .await
        .unwrap();
    assert_eq!(e.variables[0].value.as_deref(), Some("cl1ent-s3cret"));
    assert_eq!(e.variables[1].value.as_deref(), Some("ap1-key"));
    // Trimmed ids; the user pool names its region, an explicit region wins.
    assert_eq!(
        *fake.seen.lock().unwrap(),
        [
            ("dev".into(), Some("eu-west-1".into()), "app-client".into()),
            ("dev".into(), Some("us-east-1".into()), "key-id".into()),
        ]
    );
}

#[tokio::test]
async fn secrets_with_an_account_alias_use_that_accounts_profile() {
    let (fake, aws) = aws();
    let mut e = env();
    e.variables.push(secret(
        "apiKey",
        SecretSource::ApiGatewayKey {
            api_key_id: "key-id".into(),
            region: None,
            account: Some("shared-services".into()),
        },
    ));

    // The alias isn't mapped yet: the UI asks which profile it is on this machine.
    let err = fill_aws_secrets(&mut e, false, profile(Some("dev")), &aws, AwsFetch::Fetch)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CoreError::AwsAccountRequired { account, environment } if account == "shared-services" && environment == "prod"),
        "{err}"
    );
    // Inspecting skips it quietly.
    fill_aws_secrets(
        &mut e,
        false,
        profile(Some("dev")),
        &aws,
        AwsFetch::CacheOnly,
    )
    .await
    .unwrap();

    let accounts = BTreeMap::from([("shared-services".to_string(), "shared-sso".to_string())]);
    fill_aws_secrets(
        &mut e,
        false,
        AwsProfiles {
            environment: Some("dev"),
            accounts: &accounts,
        },
        &aws,
        AwsFetch::Fetch,
    )
    .await
    .unwrap();
    assert_eq!(e.variables[0].value.as_deref(), Some("pw-1"));
    assert_eq!(e.variables[2].value.as_deref(), Some("ap1-key"));
    let fetched: Vec<String> = fake
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|(p, _, id)| format!("{id}@{p}"))
        .collect();
    // One environment, two accounts: the environment's profile and the alias's.
    assert!(fetched.contains(&"prod/db@dev".to_string()), "{fetched:?}");
    assert!(
        fetched.contains(&"key-id@shared-sso".to_string()),
        "{fetched:?}"
    );

    // Only secrets without an alias need the environment's profile.
    let mut only_alias = Environment::new("shared");
    only_alias.variables = vec![e.variables[2].clone()];
    only_alias.variables[0].value = None;
    fill_aws_secrets(
        &mut only_alias,
        false,
        AwsProfiles {
            environment: None,
            accounts: &accounts,
        },
        &aws,
        AwsFetch::Fetch,
    )
    .await
    .unwrap();
    assert_eq!(only_alias.variables[0].value.as_deref(), Some("ap1-key"));
}

#[tokio::test]
async fn disabled_aws_secrets_are_never_fetched() {
    let (fake, aws) = aws();
    let mut e = env();
    e.variables[0].enabled = false;
    fill_aws_secrets(&mut e, false, profile(None), &aws, AwsFetch::Fetch)
        .await
        .unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}
