//! Secret values for a run without the OS keychain (PLAN.md M15): `--secret name=value`,
//! `FRIDA_SECRET_<name>` environment variables (what CI secret stores inject), and AWS
//! Secrets Manager with a profile from the command line or `AWS_PROFILE`.

use std::collections::HashMap;

use frida_core::model::Environment;

/// The prefix of environment variables that carry secret values.
pub const ENV_PREFIX: &str = "FRIDA_SECRET_";

/// Fills the secret variables of `env` from `given` (`--secret`) or, failing that, from the
/// environment through `lookup` (`std::env::var` in the real program, a map in tests).
///
/// A secret given here wins over an AWS source: its `source` is dropped so AWS isn't asked.
/// Returns the names of enabled secrets that are still without a value (and not from AWS),
/// for a warning.
pub fn fill_given_secrets(
    env: &mut Environment,
    given: &HashMap<String, String>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut missing = Vec::new();
    for var in env.variables.iter_mut().filter(|v| v.secret && v.enabled) {
        let value = given
            .get(&var.name)
            .cloned()
            .or_else(|| env_names(&var.name).iter().find_map(|name| lookup(name)));
        match value {
            Some(value) => {
                var.value = Some(value);
                var.source = None;
            }
            None if var.source.is_none() => missing.push(var.name.clone()),
            None => {} // filled from AWS later
        }
    }
    missing
}

/// The environment variable names for the secret `var`: exactly as written
/// (`FRIDA_SECRET_clientSecret`), then upper case with anything but letters and digits as `_`
/// (`FRIDA_SECRET_CLIENTSECRET`), since Windows ignores case and many CI systems only allow
/// upper-case names.
pub fn env_names(var: &str) -> Vec<String> {
    let exact = format!("{ENV_PREFIX}{var}");
    let normalized: String = var
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let normalized = format!("{ENV_PREFIX}{normalized}");
    if normalized == exact {
        vec![exact]
    } else {
        vec![exact, normalized]
    }
}

/// Every secret value of the environments, for masking the output.
pub fn secret_values(envs: &[&Environment]) -> Vec<String> {
    envs.iter()
        .flat_map(|env| env.variables.iter())
        .filter(|v| v.secret)
        .filter_map(|v| v.value.clone())
        .filter(|v| !v.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use frida_core::model::{EnvVariable, SecretSource};

    fn secret(name: &str) -> EnvVariable {
        EnvVariable {
            name: name.into(),
            value: None,
            secret: true,
            source: None,
            enabled: true,
        }
    }

    #[test]
    fn env_variable_names_cover_exact_and_upper_case() {
        assert_eq!(
            env_names("clientSecret"),
            ["FRIDA_SECRET_clientSecret", "FRIDA_SECRET_CLIENTSECRET"]
        );
        assert_eq!(env_names("api-key.v2")[1], "FRIDA_SECRET_API_KEY_V2");
        assert_eq!(env_names("TOKEN"), ["FRIDA_SECRET_TOKEN"]);
    }

    #[test]
    fn secrets_come_from_arguments_then_the_environment() {
        let mut env = Environment::new("dev");
        let mut aws = secret("fromAws");
        aws.source = Some(SecretSource::AwsSecretsManager {
            secret_id: "id".into(),
            json_key: None,
            region: None,
            account: None,
        });
        let mut overridden = aws.clone();
        overridden.name = "awsOverridden".into();
        let mut disabled = secret("disabled");
        disabled.enabled = false;
        env.variables = vec![
            secret("token"),
            secret("apiKey"),
            secret("unset"),
            aws,
            overridden,
            disabled,
        ];
        let given = HashMap::from([
            ("token".to_string(), "t0k3n".to_string()),
            ("awsOverridden".to_string(), "local".to_string()),
        ]);
        let lookup = |name: &str| (name == "FRIDA_SECRET_APIKEY").then(|| "k3y".to_string());

        let missing = fill_given_secrets(&mut env, &given, &lookup);

        let value = |name: &str| {
            env.variables
                .iter()
                .find(|v| v.name == name)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(value("token").as_deref(), Some("t0k3n"));
        assert_eq!(value("apiKey").as_deref(), Some("k3y"));
        assert_eq!(value("awsOverridden").as_deref(), Some("local"));
        assert!(
            env.variables[4].source.is_none(),
            "given value wins over AWS"
        );
        assert!(env.variables[3].source.is_some(), "still fetched from AWS");
        assert_eq!(value("disabled"), None);
        assert_eq!(missing, ["unset"]);
        let mut values = secret_values(&[&env]);
        values.sort();
        assert_eq!(values, ["k3y", "local", "t0k3n"]);
    }
}
