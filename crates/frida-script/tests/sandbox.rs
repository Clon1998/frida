//! The sandbox end to end: scripts run in QuickJS against a fake host.

use std::sync::Arc;
use std::time::{Duration, Instant};

use frida_script::{
    BoxFuture, HttpCall, HttpReply, Script, ScriptHeader, ScriptHost, ScriptLimits, ScriptPhase,
    ScriptRequest, ScriptResponse, ScriptState, ScriptVars, run_script,
};
use serde_json::json;

/// Answers every `fri.sendRequest` with a JSON echo of the call.
struct FakeHost;

impl ScriptHost for FakeHost {
    fn send_request(&self, call: HttpCall) -> BoxFuture<Result<HttpReply, String>> {
        Box::pin(async move {
            if call.url.contains("fail") {
                return Err("connection refused".into());
            }
            let status = if call.url.contains("404") { 404 } else { 200 };
            Ok(HttpReply {
                status,
                status_text: "OK".into(),
                headers: vec![ScriptHeader {
                    name: "Content-Type".into(),
                    value: "application/json".into(),
                }],
                body: json!({ "method": call.method, "url": call.url, "body": call.body })
                    .to_string(),
                response_time_ms: 1,
            })
        })
    }

    fn interpolate(&self, text: &str, vars: &ScriptVars) -> String {
        let mut out = text.to_string();
        for (name, value) in &vars.runtime {
            let value = value
                .as_str()
                .map(str::to_string)
                .unwrap_or(value.to_string());
            out = out.replace(&format!("{{{{{name}}}}}"), &value);
        }
        out
    }
}

fn state(phase: ScriptPhase) -> ScriptState {
    ScriptState {
        phase,
        execution_mode: Default::default(),
        collection_name: "Shop".into(),
        req: ScriptRequest {
            name: "Login".into(),
            method: "POST".into(),
            url: "{{baseUrl}}/login?x=1".into(),
            headers: vec![ScriptHeader {
                name: "Accept".into(),
                value: "*/*".into(),
            }],
            body: json!({ "user": "ann" }),
            body_mode: "json".into(),
            timeout_ms: None,
            auth_mode: "none".into(),
        },
        res: (phase != ScriptPhase::Pre).then(|| ScriptResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![ScriptHeader {
                name: "Content-Type".into(),
                value: "application/json".into(),
            }],
            body: r#"{"token":"abc","items":[{"id":7}]}"#.into(),
            response_time_ms: 12,
            url: "http://x/login".into(),
            size_bytes: 33,
        }),
        vars: ScriptVars {
            env_name: Some("dev".into()),
            env: [("host".to_string(), "dev.local".to_string())].into(),
            ..ScriptVars::default()
        },
        env_writes: vec![],
        logs: vec![],
        tests: vec![],
        next_request: None,
        skip_request: false,
        stop_execution: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn runner_controls_are_recorded() {
    let mut input = state(ScriptPhase::Post);
    input.execution_mode = frida_script::ExecutionMode::Runner;
    let (out, error) = run(
        r#"
        test("mode", () => expect(req.getExecutionMode()).to.equal("runner"));
        fri.setNextRequest("Get order");
        "#,
        input,
    )
    .await;
    assert!(error.is_none(), "{error:?}");
    assert!(out.tests[0].passed, "{:?}", out.tests);
    assert_eq!(out.next_request.as_deref(), Some("Get order"));
    assert!(!out.stop_execution);

    let (out, _) = run("bru.setNextRequest(null);", state(ScriptPhase::Post)).await;
    assert!(out.stop_execution);
    let (out, _) = run("bru.runner.stopExecution();", state(ScriptPhase::Post)).await;
    assert!(out.stop_execution);
    let (out, _) = run(
        r#"test("mode", () => expect(req.getExecutionMode()).to.equal("standalone"));"#,
        state(ScriptPhase::Post),
    )
    .await;
    assert!(out.tests[0].passed, "{:?}", out.tests);
}

async fn run(source: &str, state: ScriptState) -> (ScriptState, Option<frida_script::ScriptError>) {
    run_with(source, state, ScriptLimits::default()).await
}

async fn run_with(
    source: &str,
    state: ScriptState,
    limits: ScriptLimits,
) -> (ScriptState, Option<frida_script::ScriptError>) {
    let script = Script {
        owner: "request \"Login\"".into(),
        source: source.into(),
    };
    run_script(script, state, Arc::new(FakeHost), limits).await
}

#[tokio::test(flavor = "multi_thread")]
async fn pre_request_scripts_change_the_request_and_variables() {
    let (out, error) = run(
        r#"
        req.setHeader("X-Trace", "1");
        req.deleteHeader("accept");
        req.setUrl(req.getUrl().replace("x=1", "x=2"));
        req.setMethod("put");
        const body = req.getBody();
        body.user = "bob";
        req.setBody(body);
        fri.setVar("started", Date.now() > 0);
        fri.setVar("user", { id: 1 });
        fri.setEnvVar("host", "other.local", { persist: true });
        console.log("host was", fri.getEnvVar("host"), { a: 1 });
        "#,
        state(ScriptPhase::Pre),
    )
    .await;
    assert_eq!(error, None);
    assert_eq!(out.req.method, "PUT");
    assert_eq!(out.req.url, "{{baseUrl}}/login?x=2");
    assert_eq!(out.req.headers.len(), 1);
    assert_eq!(out.req.headers[0].name, "X-Trace");
    assert_eq!(out.req.body, json!({ "user": "bob" }));
    assert_eq!(out.vars.runtime["started"], json!(true));
    assert_eq!(out.vars.runtime["user"], json!({ "id": 1 }));
    assert_eq!(out.env_writes.len(), 1);
    assert!(out.env_writes[0].persist);
    assert_eq!(out.logs[0].message, "host was other.local {\n  \"a\": 1\n}");
}

#[tokio::test(flavor = "multi_thread")]
async fn tests_record_passes_and_failures() {
    let (out, error) = run(
        r#"
        test("status is 200", () => expect(res.status).to.equal(200));
        test("has token", () => expect(res.body).to.have.property("token", "abc"));
        test("res() reads paths", () => expect(res("items[0].id")).to.eql(7));
        test("status helper", () => expect(res).to.have.status(201));
        test("negation", () => expect([1, 2]).to.not.include(3));
        test("async", async () => {
          await fri.sleep(5);
          expect({ a: [1] }).to.deep.equal({ a: [1] });
        });
        test("throws", () => { throw new Error("boom"); });
        "#,
        state(ScriptPhase::Tests),
    )
    .await;
    assert_eq!(error, None);
    let results: Vec<(&str, bool)> = out
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed))
        .collect();
    assert_eq!(
        results,
        vec![
            ("status is 200", true),
            ("has token", true),
            ("res() reads paths", true),
            ("status helper", false),
            ("negation", true),
            ("async", true),
            ("throws", false),
        ]
    );
    assert_eq!(
        out.tests[3].error.as_deref(),
        Some("expected response to have status 201 but got 200")
    );
    assert_eq!(out.tests[6].error.as_deref(), Some("boom"));
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_name_the_script_and_line() {
    let (out, error) = run(
        "console.log('before');\nconst a = 1;\nnotDefined();\n",
        state(ScriptPhase::Pre),
    )
    .await;
    let error = error.expect("script should fail");
    assert_eq!(error.location, "request \"Login\", pre-request script");
    assert_eq!(error.line, Some(3));
    assert!(error.message.contains("notDefined"), "{}", error.message);
    // What happened before the error is kept.
    assert_eq!(out.logs[0].message, "before");
}

#[tokio::test(flavor = "multi_thread")]
async fn endless_loops_are_stopped_by_the_time_limit() {
    let limits = ScriptLimits {
        timeout: Duration::from_millis(300),
        ..ScriptLimits::default()
    };
    let started = Instant::now();
    let (out, error) = run_with(
        "console.log('looping'); while (true) {}",
        state(ScriptPhase::Pre),
        limits,
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(5));
    let error = error.expect("script should be stopped");
    assert!(error.message.contains("time limit"), "{}", error.message);
    assert_eq!(out.logs[0].message, "looping");
}

#[tokio::test(flavor = "multi_thread")]
async fn awaiting_too_long_is_stopped_too() {
    let limits = ScriptLimits {
        timeout: Duration::from_millis(200),
        ..ScriptLimits::default()
    };
    let (_, error) = run_with("await fri.sleep(10000);", state(ScriptPhase::Pre), limits).await;
    assert!(error.expect("stopped").message.contains("time limit"));
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_is_limited() {
    let limits = ScriptLimits {
        memory_bytes: 16 * 1024 * 1024,
        ..ScriptLimits::default()
    };
    let (_, error) = run_with(
        "const parts = []; while (true) parts.push('x'.repeat(1024 * 1024));",
        state(ScriptPhase::Pre),
        limits,
    )
    .await;
    assert!(error.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn send_request_works_with_await_and_callbacks() {
    let (out, error) = run(
        r#"
        const r = await fri.sendRequest({ method: "post", url: "http://api/token", data: { a: 1 } });
        fri.setVar("method", r.data.method);
        fri.setVar("body", r.data.body);
        await fri.sendRequest({ url: "http://api/404" }, (err, res) => {
          fri.setVar("callbackStatus", err.response.status);
        });
        try { await fri.sendRequest("http://fail"); } catch (e) { fri.setVar("failed", e.message); }
        "#,
        state(ScriptPhase::Pre),
    )
    .await;
    assert_eq!(error, None);
    assert_eq!(out.vars.runtime["method"], json!("POST"));
    assert_eq!(out.vars.runtime["body"], json!("{\"a\":1}"));
    assert_eq!(out.vars.runtime["callbackStatus"], json!(404));
    assert_eq!(
        out.vars.runtime["failed"],
        json!("fri.sendRequest: connection refused")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn helpers_and_sandbox_boundaries() {
    let (out, error) = run(
        r#"
        fri.setVar("b64", btoa("hello"));
        fri.setVar("sha", crypto.sha256("abc"));
        fri.setVar("uuid", crypto.randomUUID().length);
        fri.setVar("year", dayjs("2026-10-16").year());
        fri.setVar("qs", new URLSearchParams({ a: "1 2", b: "x" }).toString());
        fri.setVar("bytes", new TextEncoder().encode("é").length);
        fri.setVar("name", "ann");
        fri.setVar("interp", fri.interpolate("hi {{name}}"));
        fri.setVar("native", typeof __native);
        try { require("fs"); } catch (e) { fri.setVar("require", e.message); }
        try { fri.cwd(); } catch (e) { fri.setVar("cwd", e.message); }
        "#,
        state(ScriptPhase::Pre),
    )
    .await;
    assert_eq!(error, None);
    let v = &out.vars.runtime;
    assert_eq!(v["b64"], json!("aGVsbG8="));
    assert_eq!(
        v["sha"],
        json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(v["uuid"], json!(36));
    assert_eq!(v["year"], json!(2026));
    assert_eq!(v["qs"], json!("a=1+2&b=x"));
    assert_eq!(v["bytes"], json!(2));
    assert_eq!(v["interp"], json!("hi ann"));
    assert_eq!(v["native"], json!("undefined"));
    assert!(v["require"].as_str().unwrap().contains("not available"));
    assert!(v["cwd"].as_str().unwrap().contains("not supported"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bru_is_an_alias_for_fri() {
    let (out, error) = run(
        r#"
        bru.setVar("a", 1);
        fri.setVar("b", bru.getVar("a") + 1);
        bru.setVar("same", bru === fri);
        "#,
        state(ScriptPhase::Pre),
    )
    .await;
    assert_eq!(error, None);
    assert_eq!(out.vars.runtime["b"], json!(2));
    assert_eq!(out.vars.runtime["same"], json!(true));
}
