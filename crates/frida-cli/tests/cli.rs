//! M15 acceptance: the real `frida-run` binary against the echo server.
//!
//! Cargo builds the binary before integration tests and tells us where it is
//! (`CARGO_BIN_EXE_frida-run`); the test server runs inside the test process.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SECRET: &str = "s3cr3t-Value-42";

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name)
}

/// Runs `frida-run` with `args` (and no inherited FRIDA_SECRET_* variables).
fn frida_run(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_frida-run"));
    command.args(args).env("NO_COLOR", "1");
    for (key, _) in std::env::vars() {
        if key.starts_with("FRIDA_SECRET_") {
            command.env_remove(key);
        }
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn write(root: &Path, rel: &str, yaml: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, yaml).unwrap();
}

/// A collection with a secret `token` sent as a header, one passing and one failing test
/// whose message contains the secret, and a script that logs it.
fn secret_collection(root: &Path) {
    write(
        root,
        "collection.yaml",
        "version: 1\nid: c-1\nname: Secrets\n",
    );
    write(
        root,
        "environments/ci.yaml",
        "version: 1\nname: ci\nvariables:\n- name: token\n  secret: true\n",
    );
    write(
        root,
        "echo.yaml",
        r#"version: 1
name: Echo
seq: 1
method: GET
url: "{{baseUrl}}/echo?t={{token}}"
headers:
- name: Authorization
  value: "Bearer {{token}}"
  enabled: true
scripts:
  post: |
    console.log("token is", res.body.headers.authorization);
tests: |
  test("answers", () => expect(res.status).to.equal(200));
  test("leaks the token", () => expect(res.body.headers.authorization).to.equal("nope"));
"#,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn junit_report_for_the_scripting_fixture() {
    let addr = server().await;
    let out_dir = tempfile::tempdir().unwrap();
    let report = out_dir.path().join("report.xml");
    let output = frida_run(
        &[
            testdata("scripting").to_str().unwrap(),
            "--env",
            "dev",
            "--var",
            &format!("baseUrl=http://{addr}"),
            "--reporter",
            "junit",
            "--output",
            report.to_str().unwrap(),
        ],
        &[],
    );
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(stdout.contains("✓ POST    Login"), "{stdout}");
    assert!(stdout.contains("2 requests: 2 passed"), "{stdout}");

    let xml = std::fs::read_to_string(&report).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites "));
    assert!(xml.contains(r#"failures="0" errors="0""#), "{xml}");
    assert!(
        xml.contains(r#"<testcase classname="login" name="Login › login ok""#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<testsuite name="Scripting/orders""#),
        "{xml}"
    );
    // Every opened element is closed.
    assert_eq!(
        xml.matches("<testsuite ").count(),
        xml.matches("</testsuite>").count()
    );
    assert!(xml.trim_end().ends_with("</testsuites>"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_test_exits_with_1_and_secrets_never_show() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    secret_collection(dir.path());
    let json = dir.path().join("report.json");
    let output = frida_run(
        &[
            dir.path().to_str().unwrap(),
            "--env",
            "ci",
            "--var",
            &format!("baseUrl=http://{addr}"),
            "--reporter",
            "json",
            "-o",
            json.to_str().unwrap(),
            "--verbose",
        ],
        &[("FRIDA_SECRET_TOKEN", SECRET)],
    );
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    let report = std::fs::read_to_string(&json).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");

    // The failing assertion compared the echoed header, so the secret was in its message.
    assert!(
        stdout.contains("✗ leaks the token: expected 'Bearer ***'"),
        "{stdout}"
    );
    assert!(stderr.contains("token is Bearer ***"), "{stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(parsed["summary"]["failed"], 1);
    assert_eq!(parsed["requests"][0]["tests"][1]["passed"], false);
    for (name, out) in [("stdout", &stdout), ("stderr", &stderr), ("json", &report)] {
        assert!(!out.contains(SECRET), "secret in {name}: {out}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn secrets_can_be_passed_as_arguments_and_junit_goes_to_stdout() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    secret_collection(dir.path());
    let output = frida_run(
        &[
            dir.path().to_str().unwrap(),
            "-e",
            "ci",
            "--var",
            &format!("baseUrl=http://{addr}"),
            "--secret",
            &format!("token={SECRET}"),
            "--reporter",
            "junit",
        ],
        &[],
    );
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    // Only the report on stdout; the progress lines moved to stderr.
    assert!(stdout.starts_with("<?xml"), "{stdout}");
    assert!(
        stdout.contains("<failure message=\"expected &apos;Bearer ***&apos;"),
        "{stdout}"
    );
    assert!(
        stderr.contains("Frida · Secrets · ci (1 request)"),
        "{stderr}"
    );
    assert!(!stdout.contains(SECRET) && !stderr.contains(SECRET));
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_exit_with_2() {
    let dir = tempfile::tempdir().unwrap();
    secret_collection(dir.path());

    // Unknown environment: the message names the ones there are.
    let output = frida_run(&[dir.path().to_str().unwrap(), "--env", "prod"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr)
            .contains("there is no collection environment \"prod\" (there are: ci)"),
        "{}",
        text(&output.stderr)
    );

    // Nothing listening: the request can't be sent. A missing secret is only a warning.
    let output = frida_run(
        &[
            dir.path().to_str().unwrap(),
            "--env",
            "ci",
            "--var",
            "baseUrl=http://127.0.0.1:9",
        ],
        &[],
    );
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(
        stdout.contains("! GET     Echo  could not connect"),
        "{stdout}"
    );
    assert!(
        text(&output.stderr).contains("secret \"token\" of environment \"ci\" has no value"),
        "{}",
        text(&output.stderr)
    );

    // Not a collection, and bad arguments.
    let output = frida_run(&[dir.path().join("nope").to_str().unwrap()], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("is not a Frida collection"));
    let output = frida_run(&[dir.path().to_str().unwrap(), "--var", "novalue"], &[]);
    assert_eq!(output.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn insecure_skips_tls_verification() {
    let server = frida_testserver::spawn_tls().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "collection.yaml",
        "version: 1\nid: c-2\nname: TLS\n",
    );
    write(
        dir.path(),
        "echo.yaml",
        &format!(
            "version: 1\nname: Echo\nmethod: GET\nurl: https://localhost:{}/echo\n",
            server.addr.port()
        ),
    );
    let path = dir.path().to_str().unwrap();
    assert_eq!(
        frida_run(&[path], &[]).status.code(),
        Some(2),
        "untrusted certificate"
    );
    let output = frida_run(&[path, "--insecure"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
}

#[tokio::test(flavor = "multi_thread")]
async fn bail_stops_at_the_first_failure() {
    let addr = server().await;
    let dir = tempfile::tempdir().unwrap();
    secret_collection(dir.path());
    write(
        dir.path(),
        "later.yaml",
        "version: 1\nname: Later\nseq: 2\nmethod: GET\nurl: \"{{baseUrl}}/echo\"\n",
    );
    let base = format!("baseUrl=http://{addr}");
    let args = [dir.path().to_str().unwrap(), "--var", base.as_str()];

    let output = frida_run(&args, &[]);
    assert!(text(&output.stdout).contains("Later"));
    let output = frida_run(&[&args[..], &["--bail"]].concat(), &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(!stdout.contains("Later"), "{stdout}");
    assert!(
        stdout.contains("stopped at the first failure (--bail)"),
        "{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn proxy_with_login_and_bypass() {
    let addr = server().await;
    let proxy = frida_testserver::spawn_proxy(Some(("ann", "pr0xy-Pass")))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "collection.yaml",
        "version: 1\nid: c-3\nname: Proxy\n",
    );
    write(
        dir.path(),
        "echo.yaml",
        &format!("version: 1\nname: Echo\nmethod: GET\nurl: http://{addr}/echo\n"),
    );
    let path = dir.path().to_str().unwrap();
    let with_login = format!("http://ann:pr0xy-Pass@{}", proxy.addr);

    // 127.0.0.1 is bypassed by default; an empty bypass list sends it through the proxy.
    let output = frida_run(&[path, "--proxy", &with_login], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(proxy.requests().is_empty(), "{:?}", proxy.requests());

    let output = frida_run(&[path, "--proxy", &with_login, "--proxy-bypass", ""], &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
    assert_eq!(proxy.requests(), [format!("GET http://{addr}/echo")]);

    let wrong = format!("http://ann:nope@{}", proxy.addr);
    let output = frida_run(&[path, "--proxy", &wrong, "--proxy-bypass", ""], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(
        stdout.contains("407 Proxy Authentication Required"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("nope") && !stdout.contains("pr0xy-Pass"),
        "{stdout}"
    );
}
