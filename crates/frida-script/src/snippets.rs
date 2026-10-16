//! Ready-made examples for pre-request scripts, post-response scripts and tests, offered in the
//! editors' "Examples" menu.
//!
//! They live here (not in the UI) so one test can run every single one through the real sandbox
//! and send pipeline: `crates/frida-core/tests/snippets.rs` sends them against the test server's
//! `/sample` route, which returns the kind of JSON the examples talk about (`id`, `name`,
//! `items`, `token`, `createdAt`). An example that stops working fails CI.

use serde::Serialize;
use specta::Type;

/// Where a snippet goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SnippetKind {
    PreRequest,
    PostResponse,
    Tests,
}

/// One example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    /// Stable, for tests (e.g. `pre.set-header`).
    pub id: String,
    pub kind: SnippetKind,
    pub title: String,
    /// One sentence for the menu.
    pub description: String,
    pub code: String,
}

/// `(id, kind, title, description, code)`; turned into [`Snippet`]s by [`snippets`].
/// A table of `&'static str` keeps the examples readable as one list.
const SNIPPETS: &[(&str, SnippetKind, &str, &str, &str)] = &[
    // --- Pre-request scripts -------------------------------------------------------------
    (
        "pre.set-header",
        SnippetKind::PreRequest,
        "Set a header",
        "Adds a header to the request, here a fresh id per send.",
        r#"req.setHeader("X-Request-Id", crypto.randomUUID());"#,
    ),
    (
        "pre.bearer-from-env",
        SnippetKind::PreRequest,
        "Bearer token from the environment",
        "Sends the environment's `token` as a bearer token.",
        r#"const token = fri.getVar("token") ?? fri.getEnvVar("token");
if (token) {
  req.setHeader("Authorization", `Bearer ${token}`);
}"#,
    ),
    (
        "pre.timestamp-var",
        SnippetKind::PreRequest,
        "Timestamp variable",
        "Sets {{timestamp}} for use in the URL, headers or body.",
        r#"fri.setVar("timestamp", new Date().toISOString());"#,
    ),
    (
        "pre.edit-json-body",
        SnippetKind::PreRequest,
        "Change the JSON body",
        "Adds a field to the JSON body before it is sent.",
        r#"const body = req.getBody() ?? {};
body.sentAt = new Date().toISOString();
req.setBody(body);"#,
    ),
    (
        "pre.hmac-signature",
        SnippetKind::PreRequest,
        "HMAC signature",
        "Signs timestamp + body with the environment's `apiSecret` (HMAC-SHA256).",
        r#"const timestamp = Date.now().toString();
const secret = fri.getEnvVar("apiSecret") ?? "change-me";
const signature = crypto.hmacSHA256(timestamp + req.getBody({ raw: true }), secret);
req.setHeader("X-Timestamp", timestamp);
req.setHeader("X-Signature", signature);"#,
    ),
    (
        "pre.basic-auth-header",
        SnippetKind::PreRequest,
        "Basic auth header by hand",
        "Builds an Authorization header from `username` and `password` variables.",
        r#"const user = fri.getEnvVar("username") ?? "user";
const password = fri.getEnvVar("password") ?? "";
req.setHeader("Authorization", "Basic " + btoa(`${user}:${password}`));"#,
    ),
    (
        "pre.fetch-token",
        SnippetKind::PreRequest,
        "Fetch a token first",
        "Calls a login endpoint with fri.sendRequest and keeps the token for this request.",
        r#"const login = await fri.sendRequest({
  method: "POST",
  url: fri.interpolate("{{baseUrl}}/sample"),
  data: { username: "ann", password: "secret" },
});
fri.setVar("token", login.data.token);
req.setHeader("Authorization", `Bearer ${login.data.token}`);"#,
    ),
    (
        "pre.skip-in-runner",
        SnippetKind::PreRequest,
        "Skip in a collection run",
        "Leaves this request out of collection runs, but still sends it on its own.",
        r#"if (req.getExecutionMode() === "runner") {
  fri.runner.skipRequest();
}"#,
    ),
    (
        "pre.log-request",
        SnippetKind::PreRequest,
        "Log the request",
        "Writes method and URL to the console (Ctrl/Cmd+J).",
        r#"console.log(req.getMethod(), req.getUrl());"#,
    ),
    // --- Post-response scripts -----------------------------------------------------------
    (
        "post.save-var",
        SnippetKind::PostResponse,
        "Save a value for the next request",
        "Keeps the response's `id` as {{petId}} (a runtime variable).",
        r#"fri.setVar("petId", res.body.id);"#,
    ),
    (
        "post.save-token-env",
        SnippetKind::PostResponse,
        "Save a token to the environment",
        "Sets the environment's `token` for this session (add { persist: true } to write the file).",
        r#"fri.setEnvVar("token", res.body.token);"#,
    ),
    (
        "post.save-nested",
        SnippetKind::PostResponse,
        "Save a nested value",
        "Reads a path from the body, here the first item's id.",
        r#"fri.setVar("firstItemId", res("items[0].id"));"#,
    ),
    (
        "post.log-response",
        SnippetKind::PostResponse,
        "Log the response",
        "Writes status and body to the console.",
        r#"console.log(res.status, res.body);"#,
    ),
    (
        "post.warn-slow",
        SnippetKind::PostResponse,
        "Warn about slow responses",
        "Logs a warning when the response took longer than a second.",
        r#"if (res.responseTime > 1000) {
  console.warn(`slow response: ${res.responseTime} ms`);
}"#,
    ),
    (
        "post.next-request",
        SnippetKind::PostResponse,
        "Choose the next request in a run",
        "In a collection run, continue with the request named here (no effect on a single send).",
        r#"if (res.status === 200) {
  fri.setNextRequest("Get pet");
}"#,
    ),
    // --- Tests ----------------------------------------------------------------------------
    (
        "tests.status-200",
        SnippetKind::Tests,
        "Status is 200",
        "The most common check.",
        r#"test("status is 200", () => {
  expect(res.status).to.equal(200);
});"#,
    ),
    (
        "tests.status-2xx",
        SnippetKind::Tests,
        "Status is a success (2xx)",
        "Any 2xx status passes.",
        r#"test("status is 2xx", () => {
  expect(res.status).to.be.within(200, 299);
});"#,
    ),
    (
        "tests.response-time",
        SnippetKind::Tests,
        "Response time",
        "Fails when the response takes longer than a second.",
        r#"test("responds within 1 s", () => {
  expect(res.responseTime).to.be.below(1000);
});"#,
    ),
    (
        "tests.json-content-type",
        SnippetKind::Tests,
        "JSON content type",
        "Checks the Content-Type header.",
        r#"test("is JSON", () => {
  expect(res.getHeader("content-type")).to.include("application/json");
});"#,
    ),
    (
        "tests.header-present",
        SnippetKind::Tests,
        "Header is present",
        "Checks that a response header exists.",
        r#"test("has a request id", () => {
  expect(res).to.have.header("x-request-id");
});"#,
    ),
    (
        "tests.property-type",
        SnippetKind::Tests,
        "Field exists with a type",
        "Checks a field of the JSON body and its type.",
        r#"test("has a numeric id", () => {
  expect(res.body).to.have.property("id").that.is.a("number");
});"#,
    ),
    (
        "tests.property-value",
        SnippetKind::Tests,
        "Field has a value",
        "Compares a field with an expected value.",
        r#"test("is available", () => {
  expect(res.body).to.have.property("status", "available");
});"#,
    ),
    (
        "tests.array-length",
        SnippetKind::Tests,
        "List length",
        "Checks how many items a list has.",
        r#"test("has two items", () => {
  expect(res.body.items).to.be.an("array");
  expect(res.body.items).to.have.lengthOf(2);
});"#,
    ),
    (
        "tests.every-item",
        SnippetKind::Tests,
        "Every item has a field",
        "Runs a check for each element of a list.",
        r#"test("every item has an id and a price", () => {
  for (const item of res.body.items) {
    expect(item).to.have.property("id");
    expect(item.price).to.be.a("number");
  }
});"#,
    ),
    (
        "tests.includes",
        SnippetKind::Tests,
        "List contains a value",
        "Checks that a list (or a string) contains something.",
        r#"test("is tagged friendly", () => {
  expect(res.body.tags).to.include("friendly");
});"#,
    ),
    (
        "tests.matches-pattern",
        SnippetKind::Tests,
        "Value matches a pattern",
        "Checks a string with a regular expression, here an ISO date.",
        r#"test("createdAt is an ISO date", () => {
  expect(res.body.createdAt).to.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/);
});"#,
    ),
    (
        "tests.date",
        SnippetKind::Tests,
        "Date is in the past",
        "Parses a date with dayjs and compares it.",
        r#"test("was created in the past", () => {
  const created = dayjs(res.body.createdAt);
  expect(created.isValid()).to.be.true;
  expect(created.isBefore(dayjs().add(1, "minute"))).to.be.true;
});"#,
    ),
    (
        "tests.no-error",
        SnippetKind::Tests,
        "No error",
        "Fails on an error status (4xx/5xx), an empty body or an `error` field.",
        r#"test("no error", () => {
  expect(res.status).to.be.below(400);
  expect(res.body).to.not.be.empty;
  expect(res.body).to.not.have.property("error");
});"#,
    ),
    (
        "tests.deep-equal",
        SnippetKind::Tests,
        "Compare a whole object",
        "Deep-compares part of the body with an expected object.",
        r#"test("first item", () => {
  expect(res.body.items[0]).to.eql({ id: 1, price: 9.5 });
});"#,
    ),
    (
        "tests.matches-request",
        SnippetKind::Tests,
        "Response matches a saved variable",
        "Compares the body with a value saved earlier (here by a post-response script).",
        r#"test("id matches the saved one", () => {
  const saved = fri.getVar("petId");
  if (saved !== undefined) {
    expect(res.body.id).to.equal(saved);
  }
});"#,
    ),
];

/// All examples, in menu order (pre-request, post-response, tests).
pub fn snippets() -> Vec<Snippet> {
    SNIPPETS
        .iter()
        .map(|(id, kind, title, description, code)| Snippet {
            id: (*id).to_string(),
            kind: *kind,
            title: (*title).to_string(),
            description: (*description).to_string(),
            code: (*code).to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_match_their_kind() {
        let all = snippets();
        let mut ids: Vec<_> = all.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "duplicate snippet ids");
        for s in &all {
            let prefix = match s.kind {
                SnippetKind::PreRequest => "pre.",
                SnippetKind::PostResponse => "post.",
                SnippetKind::Tests => "tests.",
            };
            assert!(s.id.starts_with(prefix), "{} has kind {:?}", s.id, s.kind);
            if s.kind == SnippetKind::Tests {
                assert!(s.code.contains("test("), "{} defines no test", s.id);
            }
        }
    }
}
