//! M11 acceptance: the collection runner against the echo server.

use std::net::SocketAddr;
use std::path::Path;

use frida_core::auth::OAuth2Manager;
use frida_core::http::SessionStore;
use frida_core::runner::{
    self, MAX_VISITS_PER_REQUEST, RunInput, RunOptions, RunOutcome, RunResult, RunStop, RunSummary,
};
use frida_core::storage;
use tokio_util::sync::CancellationToken;

async fn server() -> SocketAddr {
    frida_testserver::spawn(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
}

/// Runs `folder` of the collection in `dir` and returns every result plus the summary.
async fn run_folder(
    dir: &Path,
    folder: &str,
    addr: SocketAddr,
    cancel: CancellationToken,
) -> (Vec<RunResult>, RunSummary) {
    let env = storage::find_environment(dir, "dev").unwrap();
    let session = SessionStore::with_runtime(&[("baseUrl", &format!("http://{addr}"))]);
    let oauth = OAuth2Manager::new(std::sync::Arc::new(|_| Ok(())));
    let items = runner::plan_run(dir, folder).unwrap();
    let mut results = Vec::new();
    let summary = runner::run(
        RunInput {
            collection_dir: dir,
            collection_env: env.as_ref(),
            global_env: None,
            session: session.session(None),
            cookies: None,
            oauth: &oauth,
        },
        &items,
        &RunOptions::default(),
        cancel,
        &|_| {},
        &mut |step| results.push(step.result),
    )
    .await;
    (results, summary)
}

fn names(results: &[RunResult]) -> Vec<&str> {
    results.iter().map(|r| r.item.name.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn login_then_authenticated_request() {
    let addr = server().await;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/scripting");
    let (results, summary) = run_folder(&dir, "", addr, CancellationToken::new()).await;

    assert_eq!(names(&results), ["Login", "List orders"]);
    for result in &results {
        assert_eq!(result.outcome, RunOutcome::Passed, "{result:?}");
        assert_eq!(result.status, Some(200));
        assert!(result.tests_total > 0 && result.tests_passed == result.tests_total);
    }
    assert_eq!((summary.passed, summary.failed, summary.errors), (2, 0, 0));
    assert_eq!(summary.stopped, None);
}

/// A collection whose scripts steer the run.
fn flow_collection() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |rel: &str, yaml: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, yaml).unwrap();
    };
    write("collection.yaml", "version: 1\nid: c\nname: Flow\n");
    write("environments/dev.yaml", "version: 1\nname: dev\n");
    let request = |name: &str, seq: u32, scripts: &str| {
        format!(
            "version: 1\nname: {name}\nseq: {seq}\nmethod: GET\nurl: \"{{{{baseUrl}}}}/echo\"\n{scripts}"
        )
    };
    let script = |phase: &str, js: &str| format!("scripts:\n  {phase}: |\n    {js}\n");

    // Main flow: Skip is skipped, Jump goes to Again, Again repeats once and stops the run.
    write("main/folder.yaml", "version: 1\nname: Main\nseq: 1\n");
    write(
        "main/skip.yaml",
        &request("Skip", 1, &script("pre", "fri.runner.skipRequest();")),
    );
    write(
        "main/jump.yaml",
        &request("Jump", 2, &script("post", "fri.setNextRequest(\"Again\");")),
    );
    write(
        "main/never.yaml",
        &request(
            "Never",
            3,
            "tests: |\n  test(\"fails\", () => expect(1).to.equal(2));\n",
        ),
    );
    write(
        "main/again.yaml",
        &request(
            "Again",
            4,
            &script(
                "post",
                "const n = (fri.getVar(\"n\") ?? 0) + 1; fri.setVar(\"n\", n); \
                 if (n < 2) { fri.setNextRequest(\"Again\"); } else { fri.runner.stopExecution(); }",
            ),
        ),
    );
    write("main/after.yaml", &request("After stop", 5, ""));

    // A request that jumps to itself forever.
    write("loop/folder.yaml", "version: 1\nname: Loop\nseq: 2\n");
    write(
        "loop/self.yaml",
        &request("Self", 1, &script("post", "fri.setNextRequest(\"Self\");")),
    );

    // A jump to a request that isn't in the run, and a failing test.
    write("other/folder.yaml", "version: 1\nname: Other\nseq: 3\n");
    write(
        "other/fail.yaml",
        &request(
            "Fail",
            1,
            "tests: |\n  test(\"is 201\", () => expect(res.status).to.equal(201));\n",
        ),
    );
    write(
        "other/away.yaml",
        &request(
            "Away",
            2,
            &script("post", "fri.setNextRequest(\"Elsewhere\");"),
        ),
    );
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn scripts_skip_jump_repeat_and_stop() {
    let addr = server().await;
    let dir = flow_collection();
    let (results, summary) = run_folder(dir.path(), "main", addr, CancellationToken::new()).await;

    assert_eq!(names(&results), ["Skip", "Jump", "Again", "Again"]);
    assert_eq!(results[0].outcome, RunOutcome::Skipped);
    assert_eq!(results[1].outcome, RunOutcome::Passed);
    assert_eq!(results[3].step, 3);
    assert_eq!(summary.stopped, Some(RunStop::Script));
    assert_eq!((summary.passed, summary.skipped), (3, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn endless_jumps_are_stopped_by_the_loop_guard() {
    let addr = server().await;
    let dir = flow_collection();
    let (results, summary) = run_folder(dir.path(), "loop", addr, CancellationToken::new()).await;

    assert_eq!(results.len(), MAX_VISITS_PER_REQUEST);
    assert_eq!(summary.stopped, Some(RunStop::LoopGuard));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_tests_and_unknown_jumps() {
    let addr = server().await;
    let dir = flow_collection();
    let (results, summary) = run_folder(dir.path(), "other", addr, CancellationToken::new()).await;

    assert_eq!(results[0].outcome, RunOutcome::Failed);
    assert!(
        results[0].message.as_deref().unwrap().starts_with("is 201"),
        "{:?}",
        results[0].message
    );
    assert_eq!(
        summary.stopped,
        Some(RunStop::UnknownNextRequest {
            name: "Elsewhere".into()
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_run_sends_nothing() {
    let addr = server().await;
    let dir = flow_collection();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let (results, summary) = run_folder(dir.path(), "", addr, cancel).await;

    assert!(results.is_empty());
    assert_eq!(summary.stopped, Some(RunStop::Cancelled));
}
