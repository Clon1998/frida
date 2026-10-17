//! The readable terminal output: one line per request as it finishes, details of failures
//! below it, and the totals at the end.

use std::io::{self, Write};

use frida_core::runner::{RunOutcome, RunStop, RunSummary};

use super::RequestReport;

/// Writes the terminal output, with ANSI colors when `color` is on.
pub struct Terminal {
    pub color: bool,
    /// Request names are padded to this many characters, so the status column lines up.
    pub name_width: usize,
}

impl Terminal {
    pub fn header(&self, out: &mut dyn Write, title: &str, count: usize) -> io::Result<()> {
        let requests = if count == 1 { "request" } else { "requests" };
        writeln!(out, "{} ({count} {requests})", self.bold(title))
    }

    pub fn request(&self, out: &mut dyn Write, report: &RequestReport) -> io::Result<()> {
        let r = &report.result;
        let (mark, paint): (&str, fn(&Self, &str) -> String) = match r.outcome {
            RunOutcome::Passed => ("✓", Self::green),
            RunOutcome::Failed => ("✗", Self::red),
            RunOutcome::Error => ("!", Self::red),
            RunOutcome::Skipped => ("-", Self::dim),
        };
        let mut line = format!(
            "  {} {:<7} {:<width$}",
            paint(self, mark),
            r.item.method,
            r.item.name,
            width = self.name_width
        );
        match r.outcome {
            RunOutcome::Skipped => line.push_str(&self.dim("  skipped")),
            RunOutcome::Error => {
                let message = r.message.as_deref().unwrap_or("not sent");
                line.push_str(&format!("  {}", self.red(message)));
            }
            RunOutcome::Passed | RunOutcome::Failed => {
                if let Some(status) = r.status {
                    line.push_str(&format!("  {status}"));
                }
                if let Some(ms) = r.time_ms {
                    line.push_str(&self.dim(&format!("  {ms} ms")));
                }
                if r.tests_total > 0 {
                    line.push_str(&format!("  {}/{} tests", r.tests_passed, r.tests_total));
                }
            }
        }
        writeln!(out, "{line}")?;
        for test in report.tests.iter().filter(|t| !t.passed) {
            match &test.error {
                Some(error) => writeln!(out, "      {} {}: {error}", self.red("✗"), test.name)?,
                None => writeln!(out, "      {} {}", self.red("✗"), test.name)?,
            }
        }
        for error in &report.script_errors {
            writeln!(out, "      {} {error}", self.red("✗"))?;
        }
        Ok(())
    }

    pub fn summary(
        &self,
        out: &mut dyn Write,
        summary: &RunSummary,
        bailed: bool,
    ) -> io::Result<()> {
        let total = summary.passed + summary.failed + summary.errors + summary.skipped;
        let mut parts = vec![self.green(&format!("{} passed", summary.passed))];
        if summary.failed > 0 {
            parts.push(self.red(&format!("{} failed", summary.failed)));
        }
        if summary.errors > 0 {
            let errors = if summary.errors == 1 {
                "error"
            } else {
                "errors"
            };
            parts.push(self.red(&format!("{} {errors}", summary.errors)));
        }
        if summary.skipped > 0 {
            parts.push(format!("{} skipped", summary.skipped));
        }
        writeln!(
            out,
            "\n{total} requests: {} ({} ms)",
            parts.join(", "),
            summary.duration_ms
        )?;
        let stopped = match &summary.stopped {
            None => None,
            Some(RunStop::Cancelled) if bailed => {
                Some("stopped at the first failure (--bail)".into())
            }
            Some(RunStop::Cancelled) => Some("cancelled".to_string()),
            Some(RunStop::Script) => Some("stopped by a script".to_string()),
            Some(RunStop::UnknownNextRequest { name }) => Some(format!(
                "stopped: fri.setNextRequest(\"{name}\") names no request of this run"
            )),
            Some(RunStop::LoopGuard) => {
                Some("stopped: too many sends, probably a setNextRequest loop".to_string())
            }
        };
        if let Some(stopped) = stopped {
            writeln!(out, "{}", self.dim(&stopped))?;
        }
        Ok(())
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
    fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }
    fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }
    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::TestReport;
    use frida_core::runner::{RunItem, RunResult};

    fn report(outcome: RunOutcome, message: Option<&str>) -> RequestReport {
        RequestReport {
            result: RunResult {
                step: 0,
                item: RunItem {
                    path: "orders/list.yaml".into(),
                    name: "List orders".into(),
                    method: "GET".into(),
                },
                outcome,
                status: Some(500),
                time_ms: Some(12),
                tests_passed: 1,
                tests_total: 2,
                message: message.map(str::to_string),
            },
            tests: vec![
                TestReport {
                    name: "has items".into(),
                    passed: true,
                    error: None,
                },
                TestReport {
                    name: "status is 200".into(),
                    passed: false,
                    error: Some("expected 500 to equal 200".into()),
                },
            ],
            script_errors: vec![],
        }
    }

    fn render(f: impl FnOnce(&Terminal, &mut Vec<u8>) -> io::Result<()>) -> String {
        let mut out = Vec::new();
        let terminal = Terminal {
            color: false,
            name_width: 0,
        };
        f(&terminal, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn failed_requests_list_their_failing_tests() {
        let text = render(|t, out| t.request(out, &report(RunOutcome::Failed, None)));
        assert_eq!(
            text,
            "  ✗ GET     List orders  500  12 ms  1/2 tests\n      ✗ status is 200: expected 500 to equal 200\n"
        );
    }

    #[test]
    fn names_are_padded_so_columns_line_up() {
        let terminal = Terminal {
            color: false,
            name_width: 14,
        };
        let mut skipped = report(RunOutcome::Skipped, None);
        skipped.tests.clear();
        let mut out = Vec::new();
        terminal.request(&mut out, &skipped).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "  - GET     List orders     skipped\n"
        );
    }

    #[test]
    fn errors_show_why_nothing_was_sent() {
        let mut r = report(RunOutcome::Error, Some("could not connect"));
        r.tests.clear();
        let text = render(|t, out| t.request(out, &r));
        assert_eq!(text, "  ! GET     List orders  could not connect\n");
    }

    #[test]
    fn the_summary_counts_and_explains_a_bail() {
        let summary = RunSummary {
            passed: 2,
            failed: 1,
            errors: 0,
            skipped: 1,
            duration_ms: 40,
            stopped: Some(RunStop::Cancelled),
        };
        let text = render(|t, out| t.summary(out, &summary, true));
        assert_eq!(
            text,
            "\n4 requests: 2 passed, 1 failed, 1 skipped (40 ms)\nstopped at the first failure (--bail)\n"
        );
    }
}
