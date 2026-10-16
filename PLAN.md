# Frida — Implementation Plan

> **Frida** — *Fast Request Inspector for Developer APIs*.
> A local-first, git-friendly desktop HTTP client (Bruno/Postman alternative) written in Rust.

This document is the source of truth for scope, architecture and milestones. Work through the
milestones in order. Tick the checkboxes as items are completed and keep this file up to date when
decisions change.

---

## 1. Goals and non-goals

### Goals
- Desktop app for **macOS, Linux and Windows**. Client only, no server component, no account, no cloud sync.
- Bruno-like concepts: **Workspaces → Collections → Folders → Requests**.
- Everything a team shares is stored as **plain YAML files in a git repo** (one file per request, clean diffs).
- **Secrets never land in git**. Only their names do. Values live in the OS keychain or come from AWS Secrets Manager.
- First-class **environments**: one global environment layer (workspace) plus a per-collection layer.
- **Variable inspector** that shows every variable a request uses, its resolved value and which layer it came from.
- Built-in **auth**: Basic, Bearer, API key (header/query), OAuth2 (client credentials, auth code + PKCE, password). Combinations such as OAuth2 + API key are supported.
- **Pre/post request scripts and tests** in JavaScript, with an API that is a compatible subset of Bruno's `bru`/`req`/`res`.
- **Importers**: OpenAPI 3.0/3.1 (from file), Bruno collections, curl commands (paste).
- **curl export** for sharing.
- A **small git panel** (status, diff, stage, commit, push, pull, branch switch).
- **AWS integration**: Secrets Manager references as env values, and a Cognito preset for OAuth2.

### Non-goals (for now)
- WebSockets, GraphQL, gRPC, MCP, SSE.
- HTTP proxy support, PAC files.
- Git merge or conflict resolution UI. Conflicts are resolved in the user's normal git tooling.
- Postman import.
- Cloud sync, accounts, telemetry.
- Code signing, notarization and auto-update. These are planned for later.

---

## 2. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| App shell | **Tauri 2.x** | Rust backend plus a web UI in the system webview. No server. Small binaries. |
| UI | **React + TypeScript + Vite** | Patrick knows React. Keep the UI thin and put all logic in Rust. |
| UI components | Tailwind CSS + shadcn/ui (Radix) | Supports dark and light themes, following the OS. |
| Editors | **CodeMirror 6** | Body, scripts, tests and the read-only response viewer. Add `{{var}}` highlighting and a hover tooltip. |
| UI state | Zustand | |
| Drag and drop | dnd-kit | Sidebar reordering. |
| Rust ↔ TS types | **tauri-specta** (specta) | Generate TS bindings for all commands and models. No hand-written duplicate types. |
| Async runtime | tokio | |
| HTTP | **reqwest** (rustls) + reqwest_cookie_store | Custom CA, an option to skip TLS verification, timeouts. |
| Serialization | serde, serde_yaml (or maintained fork, verify), serde_json | |
| Errors / logging | thiserror (libs), anyhow (app), tracing | |
| File watching | notify | Pick up external changes (git pull, editor edits). |
| Secrets | **keyring** crate | macOS Keychain, Windows Credential Manager, Linux Secret Service. Fall back to a local encrypted file. |
| Scripting | **rquickjs** (QuickJS) | Sandboxed, with time and memory limits. |
| AWS | aws-config + aws-sdk-secretsmanager | Standard credential chain with an explicit profile. |
| Git | **shell out to system `git`** | Reuses the user's SSH keys and credential helpers. |
| History | rusqlite (bundled) | Local only. |
| OpenAPI | evaluate `oas3` / `openapiv3`. Must support 3.0 and 3.1. | Fallback: parse into `serde_json::Value` and walk it. |
| Bruno `.bru` parsing | winnow or nom | Hand-written parser for the block format. |
| curl import | shell-words + custom flag parser | |
| Paths | directories crate | App data and config dirs per OS. |

> Before adding dependencies, check the current versions and maintenance status. The versions above are not pinned on purpose.

---

## 3. Repository layout

```
frida/
├── Cargo.toml                 # cargo workspace
├── CLAUDE.md
├── PLAN.md
├── crates/
│   ├── frida-core/            # domain model, YAML storage, var resolution, request building,
│   │                          # HTTP execution, auth, codegen, importers (openapi, bruno, curl), history
│   ├── frida-script/          # QuickJS sandbox + bru/req/res/test/expect API
│   ├── frida-secrets/         # keychain store + secret provider trait + AWS Secrets Manager provider
│   ├── frida-git/             # thin wrapper over the git CLI
│   └── frida-cli/             # (later) headless runner — create the crate only in the CLI milestone
├── app/
│   ├── src-tauri/             # Tauri commands only: thin glue that calls the crates
│   └── ui/                    # React/TS frontend
├── testdata/                  # sample collections, openapi specs, .bru files, curl commands
└── .github/workflows/         # CI (lint, test, build for 3 OSes) + release
```

**Rules:**
- `frida-core` and the other crates have **no dependency on Tauri**. This keeps them testable and reusable for the CLI.
- The UI never does HTTP, file I/O or secret access itself. It always calls a Tauri command.
- Secret values cross into the UI only when the user explicitly clicks "reveal". Responses from the inspector mask them by default.

---

## 4. On-disk format

All files carry a `version: 1` field for future migrations. YAML is the canonical format. Unknown fields are
preserved where practical, or at least never cause a crash.

### 4.1 Workspace
```
my-workspace/
├── frida-workspace.yaml
└── environments/              # GLOBAL environments
    ├── dev.yaml
    └── prod.yaml
```
```yaml
# frida-workspace.yaml
version: 1
name: EIP
collections:                   # relative or absolute paths to collection folders
  - path: ./collections/orders-api
  - path: ../other-repo/flight-events
```

### 4.2 Collection
```
orders-api/
├── collection.yaml
├── environments/
│   ├── dev.yaml
│   └── prod.yaml
├── .gitignore                 # generated: ignores nothing secret by design, but add local-only files if any
├── get-health.yaml
└── orders/
    ├── folder.yaml
    ├── get-order.yaml
    └── create-order.yaml
```
```yaml
# collection.yaml
version: 1
id: 7f1c2e4a-...               # stable UUID, used to key local state (tabs, aws profile mapping, secrets)
name: Orders API
vars:
  - { name: apiVersion, value: v2 }
headers:
  - { name: Accept, value: application/json, enabled: true }
auth:
  mode: none                   # see 4.5
scripts:
  pre: ""
  post: ""
tests: ""
settings:
  timeoutMs: 30000
  tls:
    verify: true
    caCertPaths: []            # PEM files, relative to collection or absolute
docs: ""
```
```yaml
# orders/folder.yaml
version: 1
name: Orders
seq: 1
vars: []
headers: []
auth: { mode: inherit }
scripts: { pre: "", post: "" }
tests: ""
```

### 4.3 Request
Request file names are a slug of the name. The display name and order come from the file contents.
```yaml
version: 1
name: Get order
seq: 2
method: GET                    # any method incl. custom (PROPFIND etc.)
url: "{{baseUrl}}/orders/:orderId"
params:
  query:
    - { name: expand, value: items, enabled: true }
  path:
    - { name: orderId, value: "{{lastOrderId}}" }
headers:
  - { name: X-Correlation-Id, value: "{{$uuid}}", enabled: true }
body:
  mode: none                   # none | json | xml | text | form-urlencoded | multipart | binary
  json: ""
  xml: ""
  text: ""
  formUrlEncoded: []           # [{name, value, enabled}]
  multipart: []                # [{name, type: text|file, value, enabled}]  file = path
  binaryPath: ""
auth: { mode: inherit }
vars:
  - { name: limit, value: "10", enabled: true }
scripts:
  pre: |
    bru.setVar("ts", Date.now());
  post: |
    bru.setVar("lastOrderId", res.body.id);
tests: |
  test("status is 200", () => expect(res.status).to.equal(200));
settings:
  timeoutMs: null              # null = inherit
docs: |
  Markdown docs for this request.
```
- Query params and the URL are **kept in sync** in the UI. Editing either one updates the other.
- `:name` segments in the URL define path params.

### 4.4 Environment (global and collection, same format)
```yaml
version: 1
name: dev
seq: 2                         # optional: position in the environment list (drag to reorder)
color: green                   # optional: highlight color in the UI (e.g. red for prod)
variables:
  - name: baseUrl
    value: https://orders.dev.example.com
  - name: clientId
    value: abc123
  - name: clientSecret
    secret: true               # value stored in the OS keychain, NEVER in this file
  - name: apiKey
    secret: true
    source:
      type: aws-secrets-manager
      secretId: eip/dev/orders-api
      jsonKey: apiKey          # optional: pick a key from a JSON secret string
      region: eu-central-1     # optional: falls back to profile region
```
- **The AWS profile is not stored in the env file**, because profile names differ per user. The mapping
  `(workspace/collection id, env name) → aws profile` lives in **local settings** (see 4.6). The UI asks
  for it the first time a secret needs it.
- Keychain entry key: `frida/{scopeId}/{envName}/{varName}`, where `scopeId` is the collection id or the workspace path hash for global envs.

### 4.5 Auth model
```yaml
auth:
  mode: inherit                # inherit | none | custom
  schemes:                     # only when mode = custom; applied in order, combinable
    - type: oauth2
      grant: client_credentials  # client_credentials | authorization_code | password
      tokenUrl: "{{tokenUrl}}"
      authUrl: ""               # auth code only
      redirectUri: http://127.0.0.1:{port}/callback  # auth code only
      clientId: "{{clientId}}"
      clientSecret: "{{clientSecret}}"
      scope: "orders/read"
      usePkce: true
      username: ""              # password grant
      password: ""
      clientAuth: basic_header  # basic_header | body
      tokenPlacement: { in: header, name: Authorization, prefix: Bearer }
      preset: null              # or: cognito  (see 9.2)
    - type: apikey
      in: header                # header | query
      name: x-api-key
      value: "{{apiKey}}"
    # other types:
    # - type: basic   { username, password }
    # - type: bearer  { token }
```
- Inheritance: request → folder (nearest first) → collection. `inherit` walks up the tree. If nothing
  defines auth, the result is `none`.
- The UI shows a "primary scheme" selector plus a "+ Add API key" option. This covers the common OAuth2 + API key case without a fully generic UI.

### 4.6 Local (non-git) state
Stored in the OS app data dir, never in the repo:
- `settings.json`: recent workspaces, active global environment and AWS profile per global env, and the
  `preferences` edited in the Settings dialog (theme, accent color, UI size, editor font size, history size,
  git program, AWS config/credentials files and secret cache time).
- `state/{collectionId}.json`: open tabs, active tab, active environment, AWS profile per env, expanded tree nodes.
- `history.sqlite`: request history.
- Cookie jars per collection, in memory with optional persistence.
- OAuth2 tokens, in memory, optionally persisted in the keychain.
- Runtime variables (`bru.setVar`), in memory per collection and cleared on app restart. A "Clear runtime vars" button is available.

---

## 5. Variable resolution spec

### 5.1 Precedence (highest wins)
1. **Runtime vars**: set by scripts via `bru.setVar`.
2. **Request vars**.
3. **Folder vars**: nearest folder first, then parents.
4. **Collection environment**: the active one.
5. **Collection vars**.
6. **Global environment**: the active one in the workspace.

Separate namespaces:
- `{{process.env.NAME}}` reads the OS environment.
- `{{$dynamic}}` calls a built-in generator.

### 5.2 Syntax and rules
- `{{name}}` with optional whitespace (`{{ name }}`).
- Values may reference other variables. Resolution is **recursive up to depth 10**, with **cycle detection** (an error that names the cycle).
- Unresolved variables are **left as-is** in the output and reported as warnings. The request is still sendable.
- Interpolation happens in the URL, path and query params, headers, all body modes (including form fields and multipart text values), auth fields and settings where applicable.
- Disabled rows are ignored.

### 5.3 Dynamic variables
`$uuid`, `$timestamp` (unix seconds), `$isoTimestamp`, `$randomInt`, `$randomUUID` (alias), `$randomEmail`,
`$randomAlphaNumeric`. Each occurrence is evaluated freshly per send.

### 5.4 Inspector API
The resolver returns structured data, not just strings:
```rust
pub struct ResolvedVar {
    pub name: String,
    pub value: Option<String>,        // None = unresolved
    pub source: VarSource,            // Runtime | Request | Folder{path} | CollectionEnv{name} | Collection | GlobalEnv{name} | ProcessEnv | Dynamic
    pub is_secret: bool,
    pub shadowed: Vec<(VarSource, Option<String>)>, // lower-priority definitions that lost
    pub used_in: Vec<UsageLocation>,  // Url, Header{name}, Body, Auth, ...
}
```
UI requirements:
- An inspector panel per request lists every variable used, with its value, a source badge, an unresolved warning and shadowed values.
- Secrets are masked, with click-to-reveal.
- Hovering `{{var}}` in any editor shows value, source and whether it is a secret.
- Unresolved variables are underlined in red.
- A quick "Edit" jump opens the variable in its defining layer.

---

## 6. Secrets

- Define a `SecretProvider` trait (async): `get(ref) -> Result<SecretValue>`.
- Implementations:
  - **Keychain**: values entered by the user in the env editor and stored through `keyring`.
  - **AWS Secrets Manager**: see section 9.
- Fallback when no keychain backend is available (headless Linux): an encrypted local file in the app data dir, protected with a user-set master password (argon2 + chacha20poly1305). Show a clear notice when the fallback is active.
- Secret values are **never** written to YAML, logs, history, error messages or curl exports (unless the user explicitly toggles "include secrets" for an export). Add a test that greps serialized YAML for known secret values.
- If a secret variable has no value yet, the env editor shows an "unset" state and the inspector flags it.

---

## 7. HTTP execution

- All methods, including custom ones.
- Request pipeline:
  1. Load the request with its inherited folder and collection data.
  2. Build the variable scope.
  3. Run the collection pre-script, then the folder pre-scripts (outermost first), then the request pre-script.
  4. Resolve variables.
  5. Apply auth (fetch or refresh the OAuth2 token if needed).
  6. Merge headers (collection → folder → request; later wins).
  7. Send.
  8. Run the post-scripts in order request → folder → collection. This matches Bruno's default; verify it.
  9. Run tests.
  10. Write history.
- Settings: timeout (request → folder → collection → global default of 30s), TLS verification toggle, custom CA PEMs (added to the defaults, not replacing them). Redirects are followed by default (max 10) with no UI toggle in v1.
- A cookie jar per collection, with a viewer and clear option per domain or all.
- Cancel in-flight requests from the UI.
- The response captures status, status text, headers, body bytes, size, total time and the final URL.

## 8. Auth details

- **Basic**, **Bearer**, **API key** (header or query).
- **OAuth2**:
  - Client credentials and password grants: a token request to `tokenUrl`.
  - Authorization code + PKCE: start a loopback HTTP server on 127.0.0.1 (a configurable port, or a random one if the provider allows it), open the system browser, receive the code, exchange it for a token, and show a "waiting for browser…" state with a cancel button.
  - Token caching is keyed by (tokenUrl, clientId, scope, grant, user), which allows sharing across requests.
  - Auto-refresh happens before expiry (with a 30s skew). If a refresh token exists, it is used; otherwise the token is fetched again.
  - A token viewer shows the access token (decoded JWT claims if it is a JWT), expiry and scopes, plus "fetch new token" and "clear token" actions.
- **Combinations**: schemes are applied in list order, for example OAuth2 + API key.

## 9. AWS integration

### 9.1 Secrets Manager
- List the profiles from `~/.aws/config` and `~/.aws/credentials`, including SSO profiles.
- Map a profile per environment (global and collection) in local state. The UI prompts for it when needed and it can be changed in the env settings.
- Fetching: use the SDK with the selected profile, then `GetSecretValue`. If `jsonKey` is set, parse the secret string as JSON and pick that key.
- Cache in memory only, with a 15 min TTL (changeable in Settings → AWS). Provide a "refresh secrets" button per env.
- Errors are mapped to actionable messages. For an expired or missing SSO session, show: *"Run `aws sso login --profile X`"* with a copy button. The app never runs the login itself.

### 9.2 Cognito preset for OAuth2
- Preset fields: user pool domain prefix (or custom domain), region, client id/secret, scopes.
- The preset fills `tokenUrl` as `https://{domain}.auth.{region}.amazoncognito.com/oauth2/token`, and `authUrl` for the auth code grant.
- The client secret is typically an env secret, either from the keychain or from Secrets Manager.

## 10. Scripting & tests

- Engine: **rquickjs**. Use a fresh context per script execution. Limits: a 10s interrupt timeout and a 128 MB memory limit (both configurable). Enable async support so promises and `await` work with tokio.
- **Before implementing, read Bruno's scripting docs** and implement a compatible subset. Document every supported and unsupported function in `docs/scripting.md`.
- API (minimum):
  - `req`: `getUrl/setUrl`, `getMethod/setMethod`, `getHeader/getHeaders/setHeader/deleteHeader`, `getBody/setBody`, `setTimeout`.
  - `res` (post-script and tests only): `status`, `statusText`, `headers`, `body` (parsed JSON when possible), `responseTime`, `getStatus()`, `getHeader()`, `getBody()`.
  - `bru`: `getVar/setVar/hasVar/deleteVar` (runtime), `getEnvVar/setEnvVar` (in-memory override for the session; `{persist: true}` writes to the env file, but never for secrets), `getGlobalEnvVar/setGlobalEnvVar`, `getCollectionVar`, `getRequestVar`, `getProcessEnv`, `getEnvName`, `sleep(ms)`, `sendRequest(opts)` (async), `setNextRequest(name)` (runner only, a no-op otherwise), `interpolate(str)`.
  - `test(name, fn)` and `expect(...)`: a chai-like subset (`to.equal`, `to.eql`, `to.be.a`, `to.have.property`, `to.include`, `to.be.above/below`, `to.have.status`, `to.be.ok/true/false/null/undefined`, `not`).
  - Helpers: `console.log/warn/error` (go to the script console panel), `btoa/atob`, `crypto` helpers (sha1, sha256, sha512, md5, hmac, randomUUID), `URLSearchParams`, `TextEncoder/TextDecoder`, and a small date library bundled as JS (e.g. dayjs). There is no `require` in v1.
- UI:
  - A script console panel with logs per run.
  - A tests panel with pass/fail and error messages.
  - Script errors are shown clearly with the script location (collection/folder/request, pre/post) and line number.

## 11. Importers & codegen

### 11.1 OpenAPI 3.0 / 3.1 (file only: YAML or JSON)
- Create a collection in a target folder chosen by the user.
- Tags become folders; untagged operations go to the root. An operation with multiple tags goes into its first tag.
- Each operation becomes a request: the name is `summary` or `operationId` or `METHOD path`. Path params become `:param`. Query and header params are created disabled unless required.
- The request body uses the example from the spec, or one generated from the schema (handle `$ref`, `allOf/oneOf/anyOf` with a sensible pick, enums, formats; depth-limited).
- `servers[0].url` becomes a `baseUrl` variable in a created `default` collection env. Server variables are resolved to their defaults.
- `securitySchemes` map to collection auth: apiKey → apikey, http basic/bearer → basic/bearer, oauth2 clientCredentials or authorizationCode → oauth2 with URLs pre-filled.
- Only local `$ref`s are resolved; external refs produce a warning.
- Show an import summary listing counts and warnings.

### 11.2 Bruno import
- Input is a Bruno collection folder (`bruno.json`, `collection.bru`, `folder.bru`, `*.bru`, `environments/*.bru`).
- Write a parser for the `.bru` block format: `meta`, `get/post/...`, `params:query`, `params:path`, `headers`, `body:*`, `auth:*`, `vars:pre-request`, `vars:post-response`, `script:pre-request`, `script:post-response`, `tests`, `docs`, `assert`, `settings`.
- Also check whether current Bruno versions use an additional YAML-based format. If so, support it too.
- Map everything to Frida's YAML. Bruno `assert` blocks are kept as a comment in `tests` with a warning, since declarative assertions come later.
- Bruno secret env vars have no value in the repo. Import them as `secret: true` with no value and list them in the import summary so the user can fill them in.
- Scripts are copied verbatim. A warning lists any `bru.*`/`req.*`/`res.*` calls Frida doesn't support.
- Use real sample `.bru` collections in `testdata/` and snapshot-test the output.

### 11.3 curl
- **Export**: a "Copy as curl" action on a request or tab. There is an option to resolve variables (default on) or keep `{{placeholders}}`. Secrets are masked as `***` unless "include secrets" is explicitly toggled, with a confirmation. The output is multi-line and readable, with options for bash and PowerShell quoting.
- **Import**: pasting a curl command into the URL bar, or using a "New request from curl" dialog. Support `-X`, `-H`, `-d/--data/--data-raw/--data-binary/--data-urlencode`, `-F`, `-u`, `--url`, `-k/--insecure`, `--compressed`, `-G`, `-b/--cookie`, and the `Content-Type` → body mode heuristic.
- Snapshot tests round-trip export → import for a corpus of requests.

## 12. Git panel

- Implemented by shelling out to `git`, using `--porcelain=v2` where available. Detect a missing git binary and a folder that isn't a repo (offer `git init`).
- Features:
  - Current branch with ahead/behind counts (via `fetch` on demand).
  - Changed files list, grouped staged/unstaged with status letters.
  - A diff viewer per file (unified, with syntax highlighting).
  - Stage/unstage per file and all.
  - Commit with a message.
  - Push.
  - Pull (`--ff-only`). If it fails, show git's message and suggest resolving outside Frida.
  - Branch list and switch, blocked when there are uncommitted changes, with a clear message.
- Show a live git status indicator per collection (via the file watcher). Each collection can live in its own repo.
- Never run destructive commands such as reset, force push or clean.

## 13. History

- SQLite in the app data dir. Each entry stores a timestamp, collection id, request path, the resolved request (method, URL, headers, body truncated to 1 MB), response meta (status, time, size, headers) and the response body (truncated to 1 MB). Secrets are masked using the known secret values from the current scope.
- Retention: the last 500 entries (configurable). Provide a clear-all option.
- A history sidebar tab supports search. Clicking an entry opens it read-only; "Restore as new request" creates a request from it.

## 14. UI specification

- **Layout**:
  - A left sidebar with tabs for Collections, History and Git.
  - Main area: a request tab bar, a request editor and a response viewer.
  - The request/response split is horizontal or vertical (toggle) and resizable.
  - A status bar shows the active environment, git branch and network status.
- **Sidebar tree**: workspace → collections → folders → requests.
  - Method badges.
  - Drag-and-drop reorders items (writes `seq`) and moves them between folders (moves files).
  - Context menu: new request, new folder, rename, duplicate, delete (with confirmation), copy as curl, reveal in file manager.
- **Tabs**: one per open request. An unsaved-changes dot appears on modified tabs, and a confirmation is shown on close. **Each collection has its own tab set.** Switching the active collection swaps the tab bar and restores the last open tabs (from local state). Middle-click closes a tab.
- **Request editor**:
  - A method dropdown and a URL bar with variable highlighting.
  - A Send button (Ctrl/Cmd+Enter).
  - Sub-tabs: Params, Body, Headers, Auth, Vars, Script, Tests, Docs, Settings.
  - An inspector toggle (right drawer).
- **Environment selectors**: two dropdowns in the top bar, one for global and one for the collection, plus an environment editor dialog with a table: name, value, secret toggle, source (plain/keychain/AWS) and a reveal action.
- **Response viewer**:
  - Status, time and size chips.
  - Tabs: Body (pretty with folding / raw), Headers, Cookies, Timeline (basic: request sent, including resolved headers), Tests, Console.
  - Image preview and HTML preview (in a sandboxed iframe with scripts disabled).
  - A JSONPath filter input.
  - "Save to file" and "Copy".
  - Large bodies (>5 MB) show a truncated view with a "save to file" option.
- **Theme**: dark/light following the OS, with a manual override.
- **Shortcuts**: Ctrl/Cmd+Enter send, Ctrl/Cmd+S save, Ctrl/Cmd+W close tab, Ctrl/Cmd+T new request, Ctrl/Cmd+E environment switcher, Ctrl/Cmd+K quick open (fuzzy search of requests).
- **External changes**: the file watcher reloads the tree. If an open tab has unsaved edits and its file changed on disk, prompt the user to keep their version or reload.

---

## 15. Milestones

Each milestone ends with: all tests green, `cargo clippy -- -D warnings` clean, `cargo fmt` applied,
the UI typechecked and linted, PLAN.md checkboxes updated, and a short summary in `docs/CHANGELOG.md`.

### M0: Scaffold & CI
- [x] Cargo workspace with the crates from §3 (empty but compiling). Tauri 2 app with React/TS/Vite/Tailwind/shadcn.
- [x] tauri-specta wiring: one example command with generated TS bindings.
- [x] GitHub Actions: fmt, clippy, cargo test, UI typecheck and lint and tests, plus a Tauri build matrix (macOS arm64 + x64, Windows x64, Linux x64) uploading artifacts.
- [x] `tracing` logging to a rotating file in the app data dir.

**Acceptance:** `pnpm tauri dev` opens a window on all 3 OSes, and CI produces downloadable builds.
*Status 2026-10-15: done. CI is green on all 4 targets with artifacts. The window was verified on macOS; Windows and Linux are covered by CI builds only, which is accepted for now.*

### M1: Storage, tree, requests, sending
- [x] Domain model + YAML (de)serialization for workspace, collection, folder, request, env (§4), with round-trip tests.
- [x] Open, create and close a workspace. Add an existing collection folder or create a new one.
- [x] Sidebar tree with CRUD, drag-and-drop reordering and moving, and a context menu.
- [x] Tabs per collection, persisted in local state.
- [x] Request editor: method, URL ↔ query params sync, path params, headers, all body modes.
- [x] HTTP execution (§7) without auth, scripts or vars. Timeouts, TLS toggle, custom CA, cancel.
- [x] Response viewer: pretty/raw body, headers, status/time/size, image and HTML preview, save, copy.
- [x] File watcher with reload and a conflict prompt.

**Acceptance:** you can create a collection, add requests, send all methods with each body type against the local test server, and the files on disk look like §4 examples.
*Status 2026-10-15: done. Automated: `tests/sample_workspace.rs` sends every method and body type of
`testdata/sample-workspace` to the test server, and `tests/format_roundtrip.rs` snapshots the files. Manually checked
in the UI; further UI polish is deferred.*

### M2: Environments, variables, secrets, inspector
- [x] Global and collection environments, selectors, and the env editor.
- [x] The resolver (§5) with precedence, recursion, cycle detection, dynamic vars and `process.env`. Extensive unit tests.
- [x] Keychain secrets (§6) with the encrypted-file fallback. Test that secrets never appear in YAML.
- [x] Inspector panel, hover tooltips and unresolved-variable highlighting.
- [x] Cookie jar and viewer.

**Acceptance:** the inspector shows the correct source and shadowing for a fixture that defines the same variable in all 6 layers.
*Status 2026-10-15: implemented. Automated: `crates/frida-core/tests/six_layers.rs` against `testdata/six-layers`;
`tests/vars.rs` covers the resolver. Waiting for a manual pass through the UI (sample workspace, "Variables" folder).*

### M3: Auth
- [x] Basic, bearer, API key, OAuth2 (client credentials, password, auth code + PKCE), combined schemes and inheritance.
- [x] Token cache, auto-refresh, token viewer.

**Acceptance:** integration tests run against a mock OAuth2 server (client credentials + password + auth code with the browser step simulated) and the OAuth2 + API key combination sends both.
*Status 2026-10-15: done (automated): `crates/frida-core/tests/auth.rs` against the mock provider in `frida-testserver`.*

### M4: curl export/import + history
- [x] curl export with the options from §11.3, and curl import (paste + dialog).
- [x] History (§13).

**Acceptance:** a snapshot corpus of curl examples passes, and history entries contain no secret values.
*Status 2026-10-15: done (automated): `tests/curl.rs` (corpus `testdata/curl`, export → import round trip) and
`tests/history.rs` (end-to-end send with secrets; nothing in history or curl).*

### M5: Scripting & tests
- [x] rquickjs sandbox with limits, the API from §10 and `docs/scripting.md`.
- [x] Pre/post script chain across collection, folder and request levels. Tests panel and console panel.

**Acceptance:**
- A fixture collection chains a login request into `bru.setVar` and then an authenticated request.
- An infinite loop script is killed by the timeout.
- `bru.sendRequest` works.
*Status 2026-10-16: done (automated): `crates/frida-core/tests/scripting.rs` (fixture `testdata/scripting`: login →
`bru.setVar` → authenticated request whose folder script awaits `bru.sendRequest`),
`crates/frida-script/tests/sandbox.rs` (endless loop and long `await` stopped, memory limit, line numbers). The
sample workspace has a "Scripting" folder for trying it by hand.*

### M6: OpenAPI import
- [x] §11.1 with the import summary dialog.

**Acceptance:** snapshot tests pass for at least 3 real-world specs (e.g. the Petstore 3.0 and 3.1 examples plus one internal-style spec with security schemes).
*Status 2026-10-16: done (automated): `crates/frida-core/tests/openapi.rs` snapshots four specs in `testdata/openapi`
(OAI Petstore expanded 3.0, OAI Tic Tac Toe 3.1, Swagger Petstore 3.0 JSON, an internal-style 3.1 spec with server
variables, bearer/basic/OAuth2 and per-operation security), checks the tricky mappings and sends an imported request.*

### M7: Git panel
- [x] §12.

**Acceptance:** integration tests run against a temporary repo with a local bare remote, covering status, stage, commit, push, ff-pull, a refused non-ff pull and a blocked branch switch.
*Status 2026-10-16: done (automated): `crates/frida-git/tests/repo.rs` (bare remote + two clones: status, stage,
unstage, diff, commit, push, fetch, ff-pull, refused diverged pull, blocked switch, collection inside a bigger repo,
`git init`). Destructive commands are refused by an allowlist (`crates/frida-git/src/run.rs`, unit-tested).*

### M8: AWS
- [x] Profile discovery, profile mapping per env, the Secrets Manager provider, TTL caching and a refresh action. Actionable SSO errors.
- [x] Cognito OAuth2 preset.

**Acceptance:** unit tests run with a mocked SDK client (or a localstack-style test if feasible), and the UI flow prompts for a profile on first use.
*Status 2026-10-16: done (automated): `crates/frida-secrets/src/aws.rs` (fake fetcher: cache/TTL/refresh, JSON keys,
error classification, profile parsing), `crates/frida-core/tests/aws.rs` (filling AWS secrets, missing profile,
expired SSO), `app/ui/src/components/env/AwsProfileDialog.test.tsx` (first send asks for a profile, saves it and
sends again). Not tested against real AWS here.*

### M9: Bruno import
- [x] §11.2. (Done before M7, see the decisions log.)

**Acceptance:** sample Bruno collections in `testdata/` import with an expected snapshot output and a warnings list.
*Status 2026-10-16: done (automated): `crates/frida-core/tests/bruno.rs` snapshots a hand-written `.bru` collection
(`testdata/bruno/shop`) and Bruno's own YAML test collection, imports Bruno's full `.bru` testbench (227 requests)
with a snapshot of its warnings, and runs an imported collection end to end (login → post-response variable →
inherited bearer auth, converted assertions passing).*

### M10: Release polish
- [x] Tauri bundler output: .dmg, .msi/.exe (NSIS), AppImage and .deb, plus an app icon.
- [x] A release workflow on git tag that attaches artifacts to a GitHub Release.
- [x] Ctrl/Cmd+K quick open, keyboard shortcuts audit, empty states, a basic onboarding screen ("open or create workspace").

*Status 2026-10-16: bundles are built by CI for all four targets (`targets: all`, icon from M0);
`.github/workflows/release.yml` builds on `v*` tags and uploads to a draft release (not run yet: no tag pushed).
Shortcuts: Cmd/Ctrl+Enter, S, W, T (new request), K (quick open), E (environment switcher), J (console), / (list);
Cmd on macOS only, so Ctrl+E/K keep their text-editing meaning there. Code signing is not set up (README explains
the first-start step).*

### M11: Collection runner
- [x] `frida_core::runner`: run the saved requests of a collection or folder in tree order (folders depth-first by
  `seq`), sequentially, with the active environments, one shared session (runtime vars, env overrides) and the
  collection's cookie jar. Optional delay between requests. Cancellable.
- [x] Script control like Bruno: `fri.runner.skipRequest()` (pre) marks the request *skipped*;
  `fri.setNextRequest(name)` jumps to the request with that name; `setNextRequest(null)` and
  `fri.runner.stopExecution()` end the run; `req.getExecutionMode()` is `"runner"`. A loop guard ends runs that
  revisit requests too often (max. 10 × the number of requests).
- [x] Results per request (method, name, status, time, tests, script errors, error, outcome
  passed/failed/error/skipped) streamed to the UI as each one finishes, plus a summary. Responses go to the response
  cache so a result can be opened. The console logs every send; history does not (a run would flood it).
- [x] UI: "Run" in the collection/folder context menu and on their tabs opens a Runner tab (`@runner/<folder>`):
  options, Run/Stop, progress, the results list and the selected result's response. A warning when tabs in the run
  have unsaved changes (the runner uses the saved files).

**Acceptance:** an integration test runs `testdata/scripting` (login → setVar → authenticated request) as a run and
checks the results; unit tests cover the order, skip, setNextRequest jumps, stop, the loop guard and cancel.
*Status 2026-10-16: done (automated): `crates/frida-core/tests/runner.rs` (the `testdata/scripting` login chain as a
run; skip, jump, repeat and stop; loop guard; failed test and unknown jump; cancel), `runner.rs` unit test for the
order, `crates/frida-script/tests/sandbox.rs` for the script controls, `app/ui/src/stores/runner.test.ts`. The UI
was checked in a browser with a mocked backend; a manual run in the app is still open.*

### Later (backlog, not scheduled)
- `frida-cli` headless runner (`frida run <collection> --env dev`).
- Declarative assertions (Bruno `assert` style).
- OpenAPI re-sync that preserves user edits. OpenAPI import from URL.
- SSM Parameter Store, SigV4 auth.
- Code signing / notarization, auto-update.
- Additional codegen targets (HTTPie, fetch, Java HttpClient, Python).
- Postman import.

---

## 16. Decisions log

Deviations from and clarifications of this plan, newest last.

- **2026-10-15 (M0) tauri-specta RC.** The Tauri 2 line of tauri-specta exists only as release candidates.
  We use `tauri-specta =2.0.0-rc.25` with `specta =2.0.0-rc.25` and `specta-typescript 0.0.12`, pinned with `=`
  as upstream recommends. Upgrade all three together.
- **2026-10-15 (M0) TypeScript 6.0, not 7.** `typescript-eslint` supports only `typescript <6.1`. Revisit when it supports TS 7.
- **2026-10-15 (M0) `frida-core` depends on `specta` directly** (not behind a feature flag), so model types can
  derive `specta::Type` without `cfg_attr` noise. specta is not a Tauri dependency, so the "no Tauri in crates" rule holds.
- **2026-10-15 (M0) `unsafe_code = "deny"` via `[workspace.lints]`** in the root `Cargo.toml` instead of a
  `#![deny(unsafe_code)]` attribute per crate. The effect is the same, and every crate opts in with `[lints] workspace = true`.
- **2026-10-15 (M0) shadcn `cn` package.** Current shadcn/ui imports the `cn` helper from the `cn` npm package
  (by shadcn-ui) instead of a local `lib/utils.ts` with clsx + tailwind-merge. We follow upstream.
- **2026-10-15 (M0) pnpm workspace in `app/`.** `app/package.json` holds the Tauri CLI, and `app/ui` is a workspace
  member. There is a single lockfile at `app/pnpm-lock.yaml`. We respect pnpm's default `minimumReleaseAge`
  (no exclusions), so very fresh releases are skipped for a day.
- **2026-10-15 (M0) Bundle identifier `io.github.clon1998.frida`** (the GitHub-hosted convention). It is like an
  Android `applicationId` and also names the app data dir. If Frida gets its own domain, switch to that domain
  before the first public release, while few users have local data under the old id.
- **2026-10-15 (M0) Logs live in `<app data dir>/logs`**, rotated daily, keeping 7 files. Level via `RUST_LOG` (default `info`).
- **2026-10-15 (M0) TS bindings are regenerated by `cargo test`** (and on every debug start). CI fails if the
  committed `app/ui/src/bindings.ts` is stale.
- **2026-10-15 (M1) YAML crate: `serde-saphyr`.** `serde_yaml` is deprecated, `serde_yml` has a RustSec advisory, and
  `serde_yaml_ng`/`serde_norway` have been stale since 2024 and build on `unsafe-libyaml`. serde-saphyr is pure Rust and
  actively maintained, writes multi-line strings (scripts, docs) as `|` block scalars, and round-trips cleanly.
- **2026-10-15 (M1) Lists are written in block style** (one field per line) rather than the inline `{ name, value }`
  style in the §4 examples. Both parse; block style gives cleaner diffs. `enabled` is always written for key/value rows.
- **2026-10-15 (M1) Folders also get `settings` and `docs`**, needed for timeout inheritance (§7) and symmetry with requests.
- **2026-10-15 (M1) Format safety.** Files with an unknown `version` are rejected with an "update Frida" error before
  full parsing. Unknown fields are ignored, not preserved (preserving them is not practical with typed serde models).
  Writes are atomic (temp file + rename).
- **2026-10-15 (M1) CI**: lint & test on every push/PR (skipping docs-only changes); the 4-OS build matrix only on PRs
  and manual dispatch.
- **2026-10-15 (M1) `crates/frida-testserver`** (not in §3): a dev-only axum echo server with an HTTPS mode using a
  throwaway CA. Used by the HTTP integration tests and for manual testing (`cargo run -p frida-testserver`, port 8787).
- **2026-10-15 (M1) HTTP details.** The URL is the source of truth for the query string when sending (`params.query`
  mirrors it for the UI). A URL without a scheme gets `http://`. A new reqwest client is built per send in M1; connection
  reuse and cookie jars come with M2. reqwest's default system-proxy detection is left on (no proxy UI, per non-goals).
- **2026-10-15 (M1) Optional fields in generated TS.** specta marks `#[serde(default)]` fields as optional in TS (they
  may be missing on input). Values from Rust are always complete, so the UI converts them once with `Complete<T>`
  (`app/ui/src/lib/model.ts`), a type derived from the generated bindings rather than a hand-written copy.
- **2026-10-15 (M1) Editors.** A small CodeMirror 6 wrapper of our own (`components/editor/CodeEditor.tsx`) instead of a
  React wrapper package, to keep control for `{{var}}` decorations in M2. Split panes use `react-resizable-panels`
  (via shadcn Resizable); toasts use `sonner`.
- **2026-10-15 (M1 feedback) "New collection" asks only for a name** and creates `<workspace>/collections/<slug>`.
  "Add existing collection" still picks any folder, e.g. a collection in another repo.
- **2026-10-15 (M1 feedback) Additions not in the original plan:** Bruno-style bulk edit (`name: value` per line,
  `//` = disabled) for query params, headers and form fields; a breadcrumb bar above the request editor (the
  environment selectors from §14 go there in M2).
- **2026-10-15 (M1 feedback) Folder and collection settings open as tabs, like Bruno.** Clicking a folder opens its tab
  (and expands it); the gear next to the collection name opens the collection tab. Both have Headers, Auth, Vars,
  Script, Tests, Docs and Settings sub-tabs, and requests got the missing §14 sub-tabs. Everything is saved to YAML
  now; vars are applied from M2, auth from M3, scripts and tests from M5, and each tab says so. The tab kind is
  derived from the path (`""` collection, `*.yaml` request, otherwise folder), so persisted tab state stays a list of
  paths. Hover "+" buttons on folders and the tree root, and an empty-state with New request / New folder.
- **2026-10-15 (M2) Secret store API.** `frida-secrets` has a synchronous `SecretStore` trait (keychain via `keyring` 4,
  an Argon2id + XChaCha20-Poly1305 encrypted file, and an in-memory store for tests). Callers run it in
  `spawn_blocking`. The async `SecretProvider` for AWS Secrets Manager (§6) is added in M8 on top of it. Keychain
  entries use service `frida` and account `{scopeId}/{env}/{var}`. Global environments use `ws-<sha256(path)[..16]>`
  as scope id.
- **2026-10-15 (M2) Resolver details.** The highest layer that *defines* a name wins; a secret defined there without a
  value is reported as unresolved instead of falling back to a lower layer (so a missing secret is noticed). Depth 10
  means `{{v0}}` may reference down to `{{v10}}`. A variable whose value contains a secret counts as secret too.
  `process.env.*` and `$dynamic` names can't be shadowed by layers. Auth fields are resolved with M3.
- **2026-10-15 (M2) Active environments** are local state: the collection's in `state/{collectionId}.json`
  (`activeEnvironment`), the global one per workspace folder in `settings.json`.
- **2026-10-15 (M2) Environment editor is a tab** (path `@environments`), following the M1 feedback that settings
  should open as tabs rather than dialogs. The URL bar became a single-line CodeMirror editor for `{{var}}` highlighting.
  Key/value table inputs are not highlighted yet (plain inputs).
- **2026-10-15 (M2) Cookies** live in memory per collection and are shown in a "Cookies" sub-tab of the collection
  tab. The response-level "Cookies" tab from §14 is still open.
- **2026-10-15 (M3) Auth details.** Auth replaces a header of the same name set elsewhere. API keys in the query are
  URL-encoded and appended. Token endpoint requests use the collection's TLS settings. Tokens live in memory only (no
  keychain persistence yet). The redirect URI must be `127.0.0.1`/`localhost`; `{port}` picks a free port. The token
  viewer lists all cached tokens. "Get new token" on folders/collections uses their saved settings.
- **2026-10-15 (UI feedback) Table layout and environment picker.** Key/value editors are bordered tables (no
  description column: that would change the on-disk format, pending Patrick's call). One environment button with a
  menu for both scopes replaces the two selects.
- **2026-10-15 Environment colors (on-disk format, additive).** Environments have an optional `color` (a named UI
  color such as `red`), stored in the environment file so a team shares "prod is red". Old files stay valid; the
  field is omitted when unset. Requested by Patrick.
- **2026-10-15 (M4) curl and history details.** Import drops a Content-Type that equals the body mode's default (Frida
  adds it itself), maps `-u` to Basic auth, `-G -d` to query params and `--max-time` to the timeout; `-k` and cookie
  files only produce warnings. Export puts method and URL on the first line and uses `curl.exe` for PowerShell. History
  is per collection in the sidebar, masks resolved secrets *and* auth values (tokens, keys, Basic credentials), records
  failed sends (with the typed request) but not cancelled ones, and keeps 500 entries (`historySize` in settings).
- **2026-10-16 Console panel (addition, requested by Patrick).** A bottom panel (Ctrl/Cmd+J, status bar) logs every
  send of the session like Postman's/Bruno's console: the raw request as sent and the raw response, or the error.
  Secrets are masked (same masking as history), bodies capped at 64 KB, 300 entries kept in memory. Script
  `console.log` output (M5) will go to the same panel. Entries expand to a "Pretty" view (sections like Postman's) or
  a "Raw" view (the whole exchange as HTTP text, request line with path). To show *all* headers like Postman:
  requests get default `User-Agent: frida-http/<version>`, `Accept: */*` and `Accept-Encoding: gzip, deflate, br,
  zstd` unless set; the console records the headers as sent (plus `host`, `cookie`, `content-length`); and the main
  send decompresses bodies itself (flate2/brotli/zstd, already in the tree via reqwest) because reqwest's own
  decompression strips `Content-Encoding`/`Content-Length` from the response headers.

- **2026-10-16 Scripting design (M5).** The `req`/`res`/`bru`/`test`/`expect` API is plain JavaScript
  (`crates/frida-script/src/prelude.js`) over a JSON state; Rust only provides a few native hooks (`sendRequest`,
  `sleep`, `interpolate`, hashes, base64). Each script gets a fresh QuickJS runtime on a blocking thread (QuickJS is
  `!Send`). Pre-request scripts see the unresolved request (Bruno behaviour); changed headers replace the inherited set.
  `bru.setEnvVar` is a session override unless `{ persist: true }` (Bruno persists immediately; we avoid surprise git
  changes), never for secrets. `crypto` is a small built-in helper set instead of `crypto-js`; dayjs is vendored.
  `bru.sendRequest` follows axios (non-2xx rejects). Runtime variables are JSON values (objects allowed, as in Bruno).
  The Tests tab, the response's test count and script `console.log` lines in the console panel complete it. Added
  crates: rquickjs (with `futures`), sha1, md-5, hmac; npm: @codemirror/lang-javascript for script highlighting.

- **2026-10-16 OpenAPI import (M6).** No OpenAPI crate: the spec is read into `serde_json::Value` (YAML via
  serde-saphyr) and walked by hand, one code path for 3.0 and 3.1 (`openapiv3` is 3.0-only; `oas3` is strict about
  real-world mistakes). `serde_json`'s `preserve_order` feature is on so folders and requests keep the spec's order.
  Details beyond §11.1: tag order follows the top-level `tags` list; operation `security` that differs from the API's
  becomes request auth (`[]` → no auth); several alternatives → the first, with a warning; credentials become
  `{{variables}}` in the `default` environment (secrets without value, listed in the summary); implicit OAuth2 and
  cookie API keys/parameters produce warnings; `readOnly` properties are left out of example bodies; XML bodies are
  rendered from the example; non-JSON `+json` types get an explicit `Content-Type` header. The import dialog previews
  the summary before writing, and the target folder defaults to `<workspace>/collections/<title>`.

- **2026-10-16 `fri` scripting object (requested by Patrick).** The scripting object is `fri` (Frida's own name);
  `bru` is the same object under Bruno's name, so Bruno scripts and imported Bruno collections run unchanged. Docs,
  samples and error messages use `fri`. §10's `bru.*` list applies to `fri.*`.

- **2026-10-16 Milestone order (requested by Patrick).** M9 (Bruno import) was done right after M6, before M7;
  then the original order continues (M7, M8, M10).
- **2026-10-16 Bruno import (M9).** Both formats: `.bru` (own block parser following Bruno's grammar) and the YAML
  OpenCollection format (default since Bruno 3.1). Folders and requests keep Bruno's order (folders first, by `seq`).
  Deviation from §11.2: `assert` rows are **converted to tests** (`res.status: eq 200` →
  `test(..., () => expect(res.status).to.eql(200))`, all of Bruno's operators); only rows that can't be converted
  stay as comments with a warning. `vars:post-response` become `fri.setVar(...)` lines at the start of the
  post-response script. GraphQL requests become JSON POST bodies; SPARQL a text body with its content type. Relative
  file paths (multipart, file bodies) become absolute paths into the Bruno collection. Unsupported auth (digest,
  NTLM, AWS SigV4, OAuth1, WSSE, Akamai) and the implicit OAuth2 grant become "none" with a warning; gRPC/WebSocket
  requests are skipped with a warning. Script calls Frida lacks are listed per script; disabled environment
  variables are left out with a warning. The Bruno folder is only read, never changed.

- **2026-10-16 Git panel (M7).** `frida-git` runs the system `git` through one function with an allowlist of
  subcommands (`status`, `add`, `restore --staged`, `rm --cached`, `diff`, `commit`, `fetch`, `push`, `pull`,
  `for-each-ref`, `switch`, `init`, `rev-parse`) and forbidden flags (`--force`, `--hard`, ...); `GIT_TERMINAL_PROMPT=0`
  and timeouts keep it from hanging. Status, "stage all" and file paths are limited to the collection folder, so a
  collection inside a bigger repository doesn't touch other files. Pushing a branch without upstream sets it to
  `origin/<branch>`. Branch switching is refused while *tracked* files have changes (untracked files don't block).
  The status bar shows branch, changed files and ahead/behind; it refreshes on file changes and window focus
  (the watcher ignores `.git`). Diffs open in a dialog with a simple diff highlighter.

- **2026-10-16 AWS (M8).** The SDK sits behind a `SecretFetcher` trait (`frida-secrets::aws`) so tests use a fake.
  The profile per environment is local state (collection state / app settings), never the shared file. Sending
  fetches AWS secrets that aren't cached; the inspector (and curl export) only reads the cache, so typing never
  calls AWS. An environment with AWS secrets but no profile fails the send with `aws_profile_required`; the UI asks
  for a profile and sends again. Errors carry the environment and, for expired SSO, the `aws sso login --profile X`
  command (copy button). Profiles are parsed from the INI files (`AWS_CONFIG_FILE`/`AWS_SHARED_CREDENTIALS_FILE`
  respected). The Cognito preset is a UI helper that fills token/auth URLs from domain + region (stored only as
  `preset: cognito` and the URLs). Added crates: aws-config, aws-sdk-secretsmanager (planned in §2).

- **2026-10-16 Disabled environment variables (on-disk change, requested by Patrick).** Environment variables and
  secrets can be disabled like other rows: `enabled: false` in the environment file (only written when false, so
  existing files are unchanged); disabled variables are ignored when resolving and never fetched from the keychain or
  AWS. Bruno imports keep disabled environment variables (disabled) instead of dropping them. All key/value tables
  (params, headers, form and multipart fields, variables at every level, environment variables/secrets) can be
  reordered by dragging (or with the keyboard); editing the URL keeps disabled query params at their position.

- **2026-10-16 Environment order (on-disk change, approved by Patrick).** Environments can be reordered by dragging
  them in the environment editor. The position is an optional `seq` in each environment file (like requests), only
  written once the list has been ordered, so existing files don't change. Unordered environments (`seq` 0 or missing)
  come after ordered ones, by name. Once a scope is ordered, new and duplicated environments go last. Saving an
  environment keeps the `seq` that is on disk; only `reorder_environments` changes it.

- **2026-10-16 Settings dialog (requested by Patrick).** App settings open as a dialog (gear in the footer and on
  the start screen, Ctrl/Cmd+,), not a tab: they are per machine and must work without a collection. Every change is
  saved and applied at once. Sections: Appearance (theme system/light/dark, accent color, UI size via the root font
  size, editor font size), Git (program path, checked with `git --version`), AWS (config and credentials file,
  secret cache time, "forget fetched secrets"), History (entries to keep). Stored as `preferences` in
  `settings.json`; the old top-level `theme` and `historySize` (never editable) are dropped. Rust clamps values to
  allowed ranges. The git path is a process-wide setting in `frida-git`; AWS files go to the SDK via
  `EnvConfigFiles` (`aws-runtime`, already a dependency of aws-config, now listed directly).

- **2026-10-16 Remaining §14 response viewer items.** The JSONPath filter is built: a field next to Pretty/Raw,
  filtered in Rust (`frida_core::jsonpath`, crate `serde_json_path`, RFC 9535; filter expressions are parsed, never
  run as code) on the cached full body, so typing doesn't send megabytes through IPC. One match shows the value,
  several or none an array; Copy copies what is shown. The **Timeline** tab is not built: the console panel shows the
  request as sent (resolved headers, body) and the response, which covers it. The **network status** in the status
  bar is dropped: a failed send already says what went wrong, and online/offline says nothing about a given API.

## 17. Testing strategy

- **Unit tests** in each crate cover the resolver, YAML round-trips, auth header building, curl gen/parse, `.bru` parser and OpenAPI mapping.
- **Snapshot tests** use `insta` for importers and codegen.
- **Integration tests** for HTTP use a local mock server (`wiremock` or `axum` test server) for all methods, body types, TLS with a self-signed CA, timeouts, cookies and OAuth2 flows.
- **Git tests** use temp dirs plus a local bare remote.
- **UI**: Vitest for stores and utilities, plus a small Playwright or WebDriver smoke test later (optional).
- Fixtures live in `testdata/`.
