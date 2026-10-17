//! `frida-run`: runs a Frida collection from the command line (PLAN.md M15), e.g. in CI.
//!
//! It is a thin layer over [`frida_core::runner`], the same engine as the app's Runner tab: it
//! loads the environments, fills secret values from the command line, the environment or AWS
//! (no OS keychain, so it works on CI machines), runs, and reports. Like the app, it never
//! prints a secret value: every text goes through [`Masker`] first.

pub mod args;
mod report;
mod secrets;

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use frida_core::auth::OAuth2Manager;
use frida_core::http::{CookieJar, ScriptLog, SessionStore};
use frida_core::mask::Masker;
use frida_core::model::{Environment, SecretSource};
use frida_core::runner::{self, RunInput, RunOptions, RunOutcome, RunStep, RunStop, RunSummary};
use frida_core::secrets::{AwsFetch, fill_aws_secrets};
use frida_core::storage;
use tokio_util::sync::CancellationToken;

pub use args::{Args, Reporter};
use report::{Report, RequestReport, Terminal, junit_xml};

/// Everything passed (skipped requests count as passed).
pub const EXIT_PASSED: u8 = 0;
/// A test or a post-response script failed.
pub const EXIT_FAILED: u8 = 1;
/// A request couldn't be sent, the run was stopped abnormally, or the arguments are wrong.
pub const EXIT_ERROR: u8 = 2;

/// Runs the collection and returns the process exit code. Problems before the run starts
/// (unknown environment, not a collection, ...) are printed to stderr with exit code 2.
pub async fn run(args: Args) -> u8 {
    match run_collection(args).await {
        Ok(code) => code,
        Err(err) => {
            // `{:#}` prints the whole chain: "can't read the collection: <io error>".
            eprintln!("frida-run: {err:#}");
            EXIT_ERROR
        }
    }
}

async fn run_collection(args: Args) -> anyhow::Result<u8> {
    let collection_dir = args.collection.clone();
    let collection = storage::load_collection(&collection_dir).with_context(|| {
        format!(
            "{} is not a Frida collection (no readable collection.yaml)",
            collection_dir.display()
        )
    })?;
    let workspace_dir = match &args.workspace {
        Some(dir) => Some(dir.clone()),
        None => find_workspace(&collection_dir),
    };

    // Environments, with their secret values.
    let mut collection_env = match &args.env {
        Some(name) => Some(environment(&collection_dir, name, "collection")?),
        None => None,
    };
    let mut global_env = match (&args.global_env, &workspace_dir) {
        (Some(name), Some(dir)) => Some(environment(dir, name, "global")?),
        (Some(_), None) => bail!(
            "--global-env needs a workspace: none found above {} (pass --workspace)",
            collection_dir.display()
        ),
        (None, _) => None,
    };
    fill_secrets(&args, collection_env.as_mut(), global_env.as_mut()).await?;

    let envs: Vec<&Environment> = collection_env.iter().chain(global_env.iter()).collect();
    let mut known_secrets = secrets::secret_values(&envs);
    known_secrets.extend(args.secrets.iter().map(|(_, value)| value.clone()));

    let folder = args.folder.as_deref().unwrap_or("");
    let items = runner::plan_run(&collection_dir, folder)
        .with_context(|| format!("there is no folder {folder:?} in the collection"))?;
    if items.is_empty() {
        bail!("there are no requests to run");
    }

    // The progress lines go to stdout, unless stdout carries the JUnit/JSON report.
    let progress_to_stdout = args.reporter == Reporter::Cli || args.output.is_some();
    let color = std::env::var_os("NO_COLOR").is_none()
        && if progress_to_stdout {
            std::io::stdout().is_terminal()
        } else {
            std::io::stderr().is_terminal()
        };
    let name_width = items
        .iter()
        .map(|i| i.name.chars().count())
        .max()
        .unwrap_or(0)
        .min(40);
    let terminal = Terminal { color, name_width };
    let mut progress = Progress {
        to_stdout: progress_to_stdout,
    };
    let mut title = format!("Frida · {}", collection.name);
    if let Some(env) = &collection_env {
        title.push_str(&format!(" · {}", env.name));
    }
    terminal.header(&mut progress, &title, items.len())?;

    let session = SessionStore {
        skip_tls_verify: args.insecure,
        ..SessionStore::with_runtime(
            &args
                .vars
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
        )
    };
    // No browser to automate on a CI machine: print the sign-in URL for a local run.
    let oauth = OAuth2Manager::new(Arc::new(|url: &str| {
        eprintln!("Sign in to continue: open {url}");
        Ok(())
    }));
    let cancel = CancellationToken::new();
    {
        // Ctrl+C ends the run cleanly (and still writes the report).
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel.cancel();
            }
        });
    }

    let log_masker = Masker::new(&known_secrets);
    let verbose = args.verbose;
    let on_log = move |log: ScriptLog| {
        if verbose {
            eprintln!(
                "    [{}] {}",
                log.request_name,
                log_masker.text(&log.message)
            );
        }
    };

    // The step callback borrows these mutably. That's allowed because the borrows end when
    // `runner::run` returns (the callback isn't used after it), and `&mut` to `Send` data is
    // itself `Send`, which the runner needs (its future may move between threads).
    let mut requests: Vec<RequestReport> = Vec::new();
    let mut bailed = false;
    let mut write_error: Option<std::io::Error> = None;
    let mut on_step = |step: RunStep| {
        if let Ok(executed) = &step.executed {
            known_secrets.extend(executed.sensitive.iter().cloned());
        }
        let report = RequestReport::from_step(&step, &Masker::new(&known_secrets));
        if let Err(err) = terminal.request(&mut progress, &report) {
            write_error.get_or_insert(err);
        }
        let failed = matches!(
            report.result.outcome,
            RunOutcome::Failed | RunOutcome::Error
        );
        if args.bail && failed && !cancel.is_cancelled() {
            bailed = true;
            cancel.cancel();
        }
        requests.push(report);
    };

    let summary = runner::run(
        RunInput {
            collection_dir: &collection_dir,
            collection_env: collection_env.as_ref(),
            global_env: global_env.as_ref(),
            session: session.session(workspace_dir.as_deref()),
            cookies: Some(Arc::new(CookieJar::default())),
            oauth: &oauth,
        },
        &items,
        &RunOptions {
            delay_ms: args.delay,
        },
        cancel.clone(),
        &on_log,
        &mut on_step,
    )
    .await;
    if let Some(err) = write_error {
        return Err(err).context("can't write the output");
    }
    terminal.summary(&mut progress, &summary, bailed)?;

    let report = Report {
        collection: collection.name.clone(),
        environment: collection_env.as_ref().map(|e| e.name.clone()),
        global_environment: global_env.as_ref().map(|e| e.name.clone()),
        summary: summary.clone(),
        requests,
    };
    write_report(&args, &report)?;
    Ok(exit_code(&summary, bailed))
}

/// Where the progress lines go. A small `Write` so both the reporter and the tests treat it
/// like any other output; it locks stdout/stderr per write, so it is `Send`.
struct Progress {
    to_stdout: bool,
}

impl Write for Progress {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.to_stdout {
            std::io::stdout().write(buf)
        } else {
            std::io::stderr().write(buf)
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if self.to_stdout {
            std::io::stdout().flush()
        } else {
            std::io::stderr().flush()
        }
    }
}

/// The exit code for a finished run (see [`EXIT_PASSED`] and friends).
fn exit_code(summary: &RunSummary, bailed: bool) -> u8 {
    let abnormal_stop = match &summary.stopped {
        None | Some(RunStop::Script) => false,
        Some(RunStop::Cancelled) => !bailed,
        Some(RunStop::UnknownNextRequest { .. }) | Some(RunStop::LoopGuard) => true,
    };
    if summary.errors > 0 || abnormal_stop {
        EXIT_ERROR
    } else if summary.failed > 0 {
        EXIT_FAILED
    } else {
        EXIT_PASSED
    }
}

/// Writes the JUnit or JSON report to `--output` or stdout. The terminal reporter has already
/// printed everything as the run went on; with `--output` it also writes its report there.
fn write_report(args: &Args, report: &Report) -> anyhow::Result<()> {
    let text = match args.reporter {
        Reporter::Cli => return Ok(()),
        Reporter::Junit => junit_xml(report),
        Reporter::Json => serde_json::to_string_pretty(report)? + "\n",
    };
    match &args.output {
        Some(path) => std::fs::write(path, text)
            .with_context(|| format!("can't write the report to {}", path.display())),
        None => {
            std::io::stdout().write_all(text.as_bytes())?;
            Ok(())
        }
    }
}

/// The environment `name` in `dir`, or an error that lists the ones there are.
fn environment(dir: &Path, name: &str, kind: &str) -> anyhow::Result<Environment> {
    if let Some(env) = storage::find_environment(dir, name)? {
        return Ok(env);
    }
    let names: Vec<String> = storage::list_environments(dir)?
        .into_iter()
        .map(|e| e.name)
        .collect();
    if names.is_empty() {
        bail!("there is no {kind} environment {name:?} (there are none)");
    }
    bail!(
        "there is no {kind} environment {name:?} (there are: {})",
        names.join(", ")
    )
}

/// The nearest folder at or above `collection_dir` with a `frida-workspace.yaml`.
fn find_workspace(collection_dir: &Path) -> Option<PathBuf> {
    let start = collection_dir.canonicalize().ok()?;
    start
        .ancestors()
        .find(|dir| dir.join(storage::WORKSPACE_FILE).is_file())
        .map(Path::to_path_buf)
}

/// Fills secret values: `--secret` and `FRIDA_SECRET_*` first, then AWS Secrets Manager for
/// the rest. Secrets left without a value get a warning; the run still starts (requests using
/// them show the variable as unresolved).
async fn fill_secrets(
    args: &Args,
    collection_env: Option<&mut Environment>,
    global_env: Option<&mut Environment>,
) -> anyhow::Result<()> {
    let given: HashMap<String, String> = args.secrets.iter().cloned().collect();
    let lookup = |name: &str| std::env::var(name).ok();
    let profile = args
        .aws_profile
        .clone()
        .or_else(|| std::env::var("AWS_PROFILE").ok())
        .unwrap_or_else(|| "default".to_string());
    // Created only when an environment really has AWS secrets (it reads the AWS config).
    let mut aws = None;

    for (env, global) in [(collection_env, false), (global_env, true)] {
        let Some(env) = env else { continue };
        for name in secrets::fill_given_secrets(env, &given, &lookup) {
            eprintln!(
                "warning: secret {name:?} of environment {:?} has no value (pass --secret {name}=… or set {})",
                env.name,
                secrets::env_names(&name)[0]
            );
        }
        let needs_aws = env.variables.iter().any(|v| {
            v.secret
                && v.enabled
                && matches!(v.source, Some(SecretSource::AwsSecretsManager { .. }))
        });
        if needs_aws {
            let aws = aws.get_or_insert_with(frida_secrets::aws::AwsSecrets::with_sdk);
            fill_aws_secrets(env, global, Some(&profile), aws, AwsFetch::Fetch)
                .await
                .with_context(|| format!("AWS profile {profile:?}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(failed: u32, errors: u32, stopped: Option<RunStop>) -> RunSummary {
        RunSummary {
            passed: 1,
            failed,
            errors,
            skipped: 0,
            duration_ms: 0,
            stopped,
        }
    }

    #[test]
    fn exit_codes() {
        assert_eq!(exit_code(&summary(0, 0, None), false), EXIT_PASSED);
        assert_eq!(
            exit_code(&summary(0, 0, Some(RunStop::Script)), false),
            EXIT_PASSED
        );
        assert_eq!(exit_code(&summary(2, 0, None), false), EXIT_FAILED);
        assert_eq!(
            exit_code(&summary(1, 0, Some(RunStop::Cancelled)), true),
            EXIT_FAILED
        );
        assert_eq!(
            exit_code(&summary(0, 0, Some(RunStop::Cancelled)), false),
            EXIT_ERROR
        );
        assert_eq!(exit_code(&summary(1, 1, None), false), EXIT_ERROR);
        assert_eq!(
            exit_code(&summary(0, 0, Some(RunStop::LoopGuard)), false),
            EXIT_ERROR
        );
    }

    #[test]
    fn the_workspace_is_found_above_the_collection() {
        let dir = tempfile::tempdir().unwrap();
        let collection = dir.path().join("collections/api");
        std::fs::create_dir_all(&collection).unwrap();
        assert_eq!(find_workspace(&collection), None);
        std::fs::write(
            dir.path().join(storage::WORKSPACE_FILE),
            "version: 1\nname: W\n",
        )
        .unwrap();
        assert_eq!(
            find_workspace(&collection),
            Some(dir.path().canonicalize().unwrap())
        );
    }
}
