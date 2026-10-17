//! JUnit XML (`--reporter junit`), the format CI systems show as test reports.
//!
//! Layout: one `<testsuite>` per folder (in run order), one `<testcase>` per test of a request
//! (`classname` = the request file, `name` = "request › test"). A request without tests is one
//! testcase that passes when it was sent; a request that couldn't be sent is an `<error>`, a
//! failing script a `<failure>`, a skipped request `<skipped/>`.
//!
//! Written by hand instead of with an XML crate: it's a few elements, and [`escape`] is all
//! the care XML needs here.

use std::fmt::Write as _;

use frida_core::runner::RunOutcome;

use super::{Report, RequestReport};

/// One `<testcase>`.
struct Case {
    classname: String,
    name: String,
    time_ms: u32,
    kind: CaseKind,
}

enum CaseKind {
    Passed,
    Failure { message: String },
    Error { message: String },
    Skipped,
}

/// The cases of one request.
fn cases_of(report: &RequestReport) -> Vec<Case> {
    let r = &report.result;
    let classname = r.item.path.trim_end_matches(".yaml").to_string();
    let time_ms = r.time_ms.unwrap_or(0);
    let case = |name: String, time_ms: u32, kind: CaseKind| Case {
        classname: classname.clone(),
        name,
        time_ms,
        kind,
    };
    match r.outcome {
        RunOutcome::Skipped => vec![case(r.item.name.clone(), 0, CaseKind::Skipped)],
        RunOutcome::Error => vec![case(
            r.item.name.clone(),
            time_ms,
            CaseKind::Error {
                message: r.message.clone().unwrap_or_else(|| "not sent".into()),
            },
        )],
        RunOutcome::Passed | RunOutcome::Failed => {
            // The request's time is shared by its tests, so the suite's total stays right.
            let parts = (report.tests.len() + usize::from(!report.script_errors.is_empty())).max(1);
            let share = time_ms / parts as u32;
            let mut cases: Vec<Case> = report
                .tests
                .iter()
                .map(|t| {
                    let kind = if t.passed {
                        CaseKind::Passed
                    } else {
                        CaseKind::Failure {
                            message: t.error.clone().unwrap_or_else(|| "failed".into()),
                        }
                    };
                    case(format!("{} › {}", r.item.name, t.name), share, kind)
                })
                .collect();
            if !report.script_errors.is_empty() {
                cases.push(case(
                    format!("{} › scripts", r.item.name),
                    share,
                    CaseKind::Failure {
                        message: report.script_errors.join("\n"),
                    },
                ));
            }
            if cases.is_empty() {
                cases.push(case(r.item.name.clone(), time_ms, CaseKind::Passed));
            }
            cases
        }
    }
}

/// Counts of a list of cases: (tests, failures, errors, skipped, time in ms).
fn counts(cases: &[Case]) -> (usize, usize, usize, usize, u64) {
    let mut c = (cases.len(), 0, 0, 0, 0u64);
    for case in cases {
        c.4 += u64::from(case.time_ms);
        match case.kind {
            CaseKind::Passed => {}
            CaseKind::Failure { .. } => c.1 += 1,
            CaseKind::Error { .. } => c.2 += 1,
            CaseKind::Skipped => c.3 += 1,
        }
    }
    c
}

pub fn junit_xml(report: &Report) -> String {
    // Folders in the order they were first reached, each with its cases.
    let mut suites: Vec<(String, Vec<Case>)> = Vec::new();
    for request in &report.requests {
        let folder = request.folder();
        let name = if folder.is_empty() {
            report.collection.clone()
        } else {
            format!("{}/{folder}", report.collection)
        };
        match suites.iter_mut().find(|(n, _)| *n == name) {
            Some((_, cases)) => cases.extend(cases_of(request)),
            None => suites.push((name, cases_of(request))),
        }
    }

    let all: Vec<&Case> = suites.iter().flat_map(|(_, c)| c).collect();
    let errors = all
        .iter()
        .filter(|c| matches!(c.kind, CaseKind::Error { .. }))
        .count();
    let failures = all
        .iter()
        .filter(|c| matches!(c.kind, CaseKind::Failure { .. }))
        .count();
    let skipped = all
        .iter()
        .filter(|c| matches!(c.kind, CaseKind::Skipped))
        .count();

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    // `write!` into a `String` can't fail, hence the ignored results.
    let _ = writeln!(
        xml,
        r#"<testsuites name="{}" tests="{}" failures="{failures}" errors="{errors}" skipped="{skipped}" time="{}">"#,
        escape(&report.collection),
        all.len(),
        seconds(u64::from(report.summary.duration_ms)),
    );
    for (name, cases) in &suites {
        let (tests, failures, errors, skipped, time) = counts(cases);
        let _ = writeln!(
            xml,
            r#"  <testsuite name="{}" tests="{tests}" failures="{failures}" errors="{errors}" skipped="{skipped}" time="{}">"#,
            escape(name),
            seconds(time),
        );
        for case in cases {
            let open = format!(
                r#"    <testcase classname="{}" name="{}" time="{}""#,
                escape(&case.classname),
                escape(&case.name),
                seconds(u64::from(case.time_ms)),
            );
            match &case.kind {
                CaseKind::Passed => {
                    let _ = writeln!(xml, "{open}/>");
                }
                CaseKind::Skipped => {
                    let _ = writeln!(xml, "{open}>\n      <skipped/>\n    </testcase>");
                }
                CaseKind::Failure { message } | CaseKind::Error { message } => {
                    let tag = if matches!(case.kind, CaseKind::Error { .. }) {
                        "error"
                    } else {
                        "failure"
                    };
                    let first_line = message.lines().next().unwrap_or("");
                    let _ = writeln!(
                        xml,
                        "{open}>\n      <{tag} message=\"{}\">{}</{tag}>\n    </testcase>",
                        escape(first_line),
                        escape(message),
                    );
                }
            }
        }
        xml.push_str("  </testsuite>\n");
    }
    xml.push_str("</testsuites>\n");
    xml
}

/// Milliseconds as JUnit's seconds with three decimals.
fn seconds(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// Escapes text for XML attributes and content. Control characters other than tab, newline and
/// carriage return aren't allowed in XML 1.0 at all, so they are dropped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\t' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::TestReport;
    use frida_core::runner::{RunItem, RunResult, RunSummary};

    fn request(
        path: &str,
        name: &str,
        outcome: RunOutcome,
        tests: Vec<TestReport>,
    ) -> RequestReport {
        RequestReport {
            result: RunResult {
                step: 0,
                item: RunItem {
                    path: path.into(),
                    name: name.into(),
                    method: "GET".into(),
                },
                outcome,
                status: (outcome != RunOutcome::Error).then_some(200),
                time_ms: Some(30),
                tests_passed: tests.iter().filter(|t| t.passed).count() as u32,
                tests_total: tests.len() as u32,
                message: (outcome == RunOutcome::Error).then(|| "could not connect".into()),
            },
            tests,
            script_errors: vec![],
        }
    }

    fn test(name: &str, error: Option<&str>) -> TestReport {
        TestReport {
            name: name.into(),
            passed: error.is_none(),
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn suites_per_folder_and_cases_per_test() {
        let report = Report {
            collection: "Shop & Co".into(),
            environment: None,
            global_environment: None,
            summary: RunSummary {
                duration_ms: 1234,
                ..RunSummary::default()
            },
            requests: vec![
                request("login.yaml", "Login", RunOutcome::Passed, vec![]),
                request(
                    "orders/list.yaml",
                    "List",
                    RunOutcome::Failed,
                    vec![
                        test("is 200", None),
                        test("has <items>", Some("expected [] to have length 2")),
                    ],
                ),
                request("orders/create.yaml", "Create", RunOutcome::Error, vec![]),
                request("orders/delete.yaml", "Delete", RunOutcome::Skipped, vec![]),
            ],
        };
        let xml = junit_xml(&report);
        assert_eq!(
            xml,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="Shop &amp; Co" tests="5" failures="1" errors="1" skipped="1" time="1.234">
  <testsuite name="Shop &amp; Co" tests="1" failures="0" errors="0" skipped="0" time="0.030">
    <testcase classname="login" name="Login" time="0.030"/>
  </testsuite>
  <testsuite name="Shop &amp; Co/orders" tests="4" failures="1" errors="1" skipped="1" time="0.060">
    <testcase classname="orders/list" name="List › is 200" time="0.015"/>
    <testcase classname="orders/list" name="List › has &lt;items&gt;" time="0.015">
      <failure message="expected [] to have length 2">expected [] to have length 2</failure>
    </testcase>
    <testcase classname="orders/create" name="Create" time="0.030">
      <error message="could not connect">could not connect</error>
    </testcase>
    <testcase classname="orders/delete" name="Delete" time="0.000">
      <skipped/>
    </testcase>
  </testsuite>
</testsuites>
"#
        );
    }

    #[test]
    fn escaping_handles_quotes_newlines_and_control_characters() {
        assert_eq!(escape("a\"b'c\nd\u{1}e"), "a&quot;b&apos;c&#10;de");
    }
}
