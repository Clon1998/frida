//! M8 acceptance (PLAN.md §15): AWS-backed secret variables with a mocked Secrets Manager.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use frida_core::CoreError;
use frida_core::model::{EnvVariable, Environment, SecretSource};
use frida_core::secrets::{AwsFetch, fill_aws_secrets};
use frida_secrets::aws::{AwsError, AwsFiles, AwsSecrets, BoxFuture, SecretFetcher};

struct Fake {
    calls: AtomicUsize,
    values: HashMap<&'static str, Result<String, AwsError>>,
}

impl SecretFetcher for Fake {
    fn fetch(
        &self,
        _: &AwsFiles,
        profile: &str,
        _: Option<&str>,
        id: &str,
    ) -> BoxFuture<Result<String, AwsError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let value = if profile == "expired" {
            Err(AwsError::SsoLogin {
                profile: profile.into(),
            })
        } else {
            self.values
                .get(id)
                .cloned()
                .unwrap_or(Err(AwsError::NotFound {
                    secret_id: id.into(),
                }))
        };
        Box::pin(async move { value })
    }
}

fn env() -> Environment {
    let mut env = Environment::new("prod");
    env.variables = vec![
        EnvVariable {
            name: "dbPassword".into(),
            value: None,
            secret: true,
            source: Some(SecretSource::AwsSecretsManager {
                secret_id: "prod/db".into(),
                json_key: Some("password".into()),
                region: Some("eu-central-1".into()),
            }),
            enabled: true,
        },
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
        values: HashMap::from([("prod/db", Ok(r#"{"password":"pw-1"}"#.to_string()))]),
    });
    (fake.clone(), AwsSecrets::new(fake))
}

#[tokio::test]
async fn aws_secrets_are_filled_and_cached() {
    let (fake, aws) = aws();
    let mut e = env();
    fill_aws_secrets(&mut e, false, Some("dev"), &aws, AwsFetch::Fetch)
        .await
        .unwrap();
    assert_eq!(e.variables[0].value.as_deref(), Some("pw-1"));
    assert_eq!(e.variables[1].value.as_deref(), Some("from-keychain"));

    // The inspector only reads the cache, which is now filled.
    let mut again = env();
    fill_aws_secrets(&mut again, false, Some("dev"), &aws, AwsFetch::CacheOnly)
        .await
        .unwrap();
    assert_eq!(again.variables[0].value.as_deref(), Some("pw-1"));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_missing_profile_asks_the_ui_to_choose_one() {
    let (fake, aws) = aws();
    let mut e = env();
    let err = fill_aws_secrets(&mut e, true, None, &aws, AwsFetch::Fetch)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::AwsProfileRequired { ref environment, global: true } if environment == "prod")
    );
    // Inspecting never asks and never calls AWS.
    fill_aws_secrets(&mut e, true, None, &aws, AwsFetch::CacheOnly)
        .await
        .unwrap();
    assert_eq!(e.variables[0].value, None);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_expired_sso_session_says_how_to_log_in() {
    let (_, aws) = aws();
    let mut e = env();
    let err = fill_aws_secrets(&mut e, false, Some("expired"), &aws, AwsFetch::Fetch)
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
