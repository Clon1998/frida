//! The command line of `frida-run` (PLAN.md M15).
//!
//! `#[derive(Parser)]` makes clap generate the parsing, `--help` and error messages from this
//! struct: each field is an option, its doc comment is the help text. It's the same idea as
//! picocli's annotated fields in Java.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "frida-run",
    version,
    about = "Runs the requests of a Frida collection, with their scripts and tests.",
    after_help = "Exit codes: 0 everything passed, 1 a test or script failed, 2 an error \
                  (a request couldn't be sent, bad arguments).\n\n\
                  Secret variables: --secret name=value, or the environment variable \
                  FRIDA_SECRET_<name> (also FRIDA_SECRET_<NAME> in upper case, with characters \
                  other than letters and digits as _)."
)]
pub struct Args {
    /// The collection folder (the one with collection.yaml).
    pub collection: PathBuf,

    /// Only run this folder of the collection (its path, e.g. orders/admin).
    #[arg(long)]
    pub folder: Option<String>,

    /// The collection environment to use.
    #[arg(long, short = 'e')]
    pub env: Option<String>,

    /// The global (workspace) environment to use.
    #[arg(long)]
    pub global_env: Option<String>,

    /// The workspace folder with the global environments. Default: the nearest folder above
    /// the collection that has a frida-workspace.yaml.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Sets a variable for the run, above every other layer (repeatable).
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_pair)]
    pub vars: Vec<(String, String)>,

    /// The value of a secret variable (repeatable). Prefer FRIDA_SECRET_<name>, which keeps
    /// the value out of the shell history and process list.
    #[arg(long = "secret", value_name = "NAME=VALUE", value_parser = parse_pair)]
    pub secrets: Vec<(String, String)>,

    /// The AWS profile for secrets from AWS Secrets Manager. Default: AWS_PROFILE, else
    /// "default" (credentials in AWS_ACCESS_KEY_ID etc. are used either way).
    #[arg(long)]
    pub aws_profile: Option<String>,

    /// Pause between two requests, in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 0)]
    pub delay: u32,

    /// Stop at the first request that fails or can't be sent.
    #[arg(long)]
    pub bail: bool,

    /// How to report the results.
    #[arg(long, value_enum, default_value_t = Reporter::Cli)]
    pub reporter: Reporter,

    /// Write the report to this file instead of standard output (for junit and json).
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Don't verify TLS certificates (like curl -k).
    #[arg(long, short = 'k')]
    pub insecure: bool,

    /// Also print the scripts' console output.
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Reporter {
    /// One line per request, failures with details.
    Cli,
    /// JUnit XML, for CI test reports.
    Junit,
    /// The results as JSON.
    Json,
}

/// `name=value` → `("name", "value")`. The value may contain `=`; the name must not be empty.
fn parse_pair(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() => {
            Ok((name.trim().to_string(), value.to_string()))
        }
        _ => Err(format!("expected NAME=VALUE, got {text:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_split_at_the_first_equals_sign() {
        assert_eq!(parse_pair("a=b=c").unwrap(), ("a".into(), "b=c".into()));
        assert_eq!(parse_pair("token=").unwrap(), ("token".into(), "".into()));
        assert!(parse_pair("novalue").is_err());
        assert!(parse_pair("=x").is_err());
    }

    #[test]
    fn parses_a_typical_ci_call() {
        let args = Args::try_parse_from([
            "frida-run",
            "api",
            "--env",
            "dev",
            "--var",
            "baseUrl=http://x",
            "--reporter",
            "junit",
            "-o",
            "report.xml",
            "--bail",
        ])
        .unwrap();
        assert_eq!(args.collection, PathBuf::from("api"));
        assert_eq!(args.env.as_deref(), Some("dev"));
        assert_eq!(args.vars, [("baseUrl".into(), "http://x".into())]);
        assert_eq!(args.reporter, Reporter::Junit);
        assert!(args.bail && !args.insecure);
    }
}
