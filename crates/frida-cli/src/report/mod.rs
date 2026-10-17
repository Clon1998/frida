//! What a run produced, in a form every reporter can use: the totals plus, per request, its
//! result and every test. Built while the run goes on; all text is masked on the way in, so
//! no reporter can print a secret by accident.

mod cli;
mod junit;

pub use cli::Terminal;
pub use junit::junit_xml;

use frida_core::mask::Masker;
use frida_core::runner::{RunResult, RunStep, RunSummary};
use serde::Serialize;

/// The whole run, as written by `--reporter json`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub collection: String,
    pub environment: Option<String>,
    pub global_environment: Option<String>,
    pub summary: RunSummary,
    pub requests: Vec<RequestReport>,
}

/// One request of the run (a request sent twice by `setNextRequest` has two).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestReport {
    /// `#[serde(flatten)]` writes the result's fields (`item`, `outcome`, `status`, ...)
    /// directly into this object instead of nesting them under `result`.
    #[serde(flatten)]
    pub result: RunResult,
    pub tests: Vec<TestReport>,
    /// Post-response script and test script errors (`location (line n): message`).
    pub script_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReport {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

impl RequestReport {
    /// The report of one run step, with every text masked.
    pub fn from_step(step: &RunStep, masker: &Masker) -> Self {
        let mut result = step.result.clone();
        result.message = result.message.map(|m| masker.text(&m));
        let (tests, script_errors) = match &step.executed {
            Ok(executed) => (
                executed
                    .scripts
                    .tests
                    .iter()
                    .map(|t| TestReport {
                        name: masker.text(&t.name),
                        passed: t.passed,
                        error: t.error.as_deref().map(|e| masker.text(e)),
                    })
                    .collect(),
                executed
                    .scripts
                    .errors
                    .iter()
                    .map(|e| masker.text(&e.to_string()))
                    .collect(),
            ),
            Err(_) => (Vec::new(), Vec::new()),
        };
        Self {
            result,
            tests,
            script_errors,
        }
    }

    /// The folder the request lives in (`""` for the collection root), for grouping.
    pub fn folder(&self) -> &str {
        self.result
            .item
            .path
            .rsplit_once('/')
            .map(|(folder, _)| folder)
            .unwrap_or("")
    }
}
