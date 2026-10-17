//! `frida-run` entry point: parse the arguments, run, exit with the run's code.
//! Everything else is in the library (`lib.rs`), so integration tests can drive it too.

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

/// `#[tokio::main]` turns `async fn main` into a normal `main` that starts the tokio runtime
/// and blocks on this future, like `SpringApplication.run` starting the container for you.
#[tokio::main]
async fn main() -> ExitCode {
    // Library warnings go to stderr (never stdout, which may carry a JUnit/JSON report).
    // `RUST_LOG=debug frida-run ...` shows more.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .init();
    // Wrong arguments: clap prints the problem and exits with code 2, like our errors.
    let args = frida_cli::Args::parse();
    ExitCode::from(frida_cli::run(args).await)
}
