# Changelog

## M7: Git panel (2026-10-16)

### What was built
- **`frida-git`**: status (porcelain v2, parsed in `parse.rs`), stage/unstage (per file and all), diffs (also for
  new files), commit, fetch, push (sets upstream on first push), `pull --ff-only`, branches, switch (blocked with
  uncommitted changes), create branch and `git init`. Only allowlisted, non-destructive commands can run.
- **App/UI**: a Git tab in the sidebar (branch menu, fetch/pull/push with ahead/behind, commit box with
  Ctrl/Cmd+Enter, staged and unstaged files with stage/unstage buttons, diff dialog), a live git indicator in the
  status bar, clear messages and hints when git refuses something (diverged pull, missing identity, auth).

### Rust things worth understanding in this milestone
1. **Running a child process with a timeout** (`crates/frida-git/src/run.rs`). `Command::spawn` starts git; its
   output is read on two threads (a full pipe would otherwise block git), and `try_wait` in a loop lets us kill it
   after the deadline. `std::process` has no built-in timeout, unlike Java's `Process.waitFor(timeout)`.
2. **Assertions as a safety net** (`run.rs` `ALLOWED`). `assert!` panics on a programming error (trying to run a
   forbidden command), which tests catch with `#[should_panic]`. It's for bugs, not user errors: those are `Result`s.
3. **Parsing NUL-separated output** (`parse.rs`). `git status -z` separates records with `\0`, so file names with
   spaces or newlines can't break parsing; `split('\0')` and `splitn` keep the path as one piece.
4. **Generic helper with a closure** (`with_repo` in `app/src-tauri/src/commands/git.rs`). Each command passes
   "what to do with the repo" as an `FnOnce` closure that is moved to a blocking thread; the `Send + 'static` bounds
   are what make that move allowed.


## M9: Bruno import (2026-10-16, done before M7)

### What was built
- **Readers** (`crates/frida-core/src/import/bruno/`): `bru.rs` parses Bruno's block format (dictionary, text and
  list blocks, `~disabled` rows, multi-line `'''` values, annotations), `read_bru.rs` walks a `.bru` collection
  (`bruno.json`, `collection.bru`, `folder.bru`, environments), `read_yml.rs` reads the YAML OpenCollection format.
  Both produce one in-between model (`model.rs`), so the mapping to Frida is written once (`mod.rs`).
- **Mapping**: headers, params, all body modes, auth (basic, bearer, API key, OAuth2), variables, scripts and docs
  at collection, folder and request level; post-response variables as script lines; assertions converted to tests
  (`asserts.rs`); environments with secrets (no values) listed for filling in; warnings for everything Frida can't do.
- **UI**: "Import Bruno collection…" next to "Import OpenAPI…", using the same preview/import/summary dialog.
- Also in this round (feedback): duplicating environments (secrets copied inside Rust), Ctrl/Cmd+S in the environment
  editor, editors remembering their sub-tab and the selected environment, the environment editor's unsaved dot (its
  edits now survive tab switches), `{{` autocompletion and green/red variable highlighting (also `:pathParams` and
  key/value cells in the request editor), and a fix for the active collection environment being cleared whenever
  tabs changed (which also broke OAuth2 token requests using environment variables).

### Rust things worth understanding in this milestone
1. **A hand-written line parser** (`bru.rs`). Instead of a parser library, a `while i < lines.len()` loop with an
   index you advance yourself handles blocks and multi-line values; `&lines[start..i]` is a *slice* (a borrowed view,
   like `List.subList` without copying).
2. **One in-between model for two inputs** (`model.rs`). Two readers convert into the same plain structs, so the
   mapper doesn't care which format it came from: the same idea as a DTO layer in Spring.
3. **`Box` in enums** (`PlannedItem`, `model::Auth::OAuth2`). An enum value is as big as its largest variant; boxing
   the large ones keeps every value small (clippy's `large_enum_variant` lint points this out).
4. **Iterator chaining** (`unsupported_calls`). `SUPPORTED.iter().copied().chain(once(...))` reuses the `bru` list
   for `fri` without building a new array.


## M6: OpenAPI import (2026-10-16)

### What was built
- **Mapper** (`crates/frida-core/src/import/openapi.rs`): OpenAPI 3.0/3.1 in YAML or JSON → a collection plan: tags
  become folders, operations become requests (name from summary, operationId or `METHOD path`), `{id}` → `:id`,
  query/header parameters (disabled unless required), example bodies from the spec or generated from the schema
  (`$ref`, `allOf`/`oneOf`/`anyOf`, enums, formats, recursion and depth limits), `servers[0]` with its variables as
  `baseUrl` in a `default` environment, security schemes as collection or request auth with credential variables.
- **Import summary**: counts, base URL, secrets to fill in and warnings (external refs, unsupported auth, relative
  server URL, ...), shown before and after importing.
- **Scripting**: the scripting object is now `fri` (Frida's own name); `bru` stays as an alias for Bruno scripts.
- **UI**: "Import OpenAPI…" in the collection menu and the empty sidebar; a dialog that previews, lets you pick the
  target folder and shows the result.

### Rust things worth understanding in this milestone
1. **Walking untyped JSON** (`openapi.rs`). `serde_json::Value` is like Jackson's `JsonNode`: `get`, `pointer`
   (`/components/schemas/Pet`, used to follow `$ref`s) and `as_str`/`as_array` return `Option`s, so missing fields are
   handled with `?`, `unwrap_or` and `let ... else` instead of null checks.
2. **Cloning to escape borrow puzzles** (`Mapper::resolve`). Returning `&Value` into the document while also
   pushing warnings onto `&mut self` tied the borrows together; returning an owned copy (`Value`) makes the code simple.
   For documents of this size the copy costs nothing noticeable: boring beats clever.
3. **Plan, then write** (`map_openapi` vs `write_plan`). The mapping is a pure function returning data, so the tests
   snapshot it as YAML (`insta`) without touching the disk; only `write_plan` does I/O.
4. **Recursion guards** (`Mapper::example`). A `refs` stack (`Vec<String>`) passed as `&mut` detects a schema that
   refers to itself, and a depth limit stops runaway nesting; the stack is pushed before and popped after each `$ref`.

## M5: Scripting & tests (2026-10-16)

### What was built
- **Sandbox** (`crates/frida-script`): a fresh QuickJS runtime per script (rquickjs) with a 10 s deadline (interrupt
  handler for busy loops, a tokio timeout for awaits) and a 128 MB memory limit. Errors carry the script's location and
  line. The Bruno-compatible API lives in `src/prelude.js`; `docs/scripting.md` lists everything supported.
- **Pipeline** (`crates/frida-core/src/http/scripting.rs`): pre-request scripts (collection → folders → request) run
  on the unresolved request and their changes are applied before resolving; post-response scripts and tests run after
  the send. Runtime variables and in-memory environment changes live in a per-collection `SessionStore`;
  `{ persist: true }` writes non-secret values to the environment file. `bru.sendRequest` uses the real HTTP stack.
- **UI**: JavaScript highlighting in script and test editors, a Tests tab (and a pass count) on the response, script
  `console.log` lines in the console, script errors with location and line. A "Scripting" folder in the sample
  workspace shows a login → token → authenticated request chain.
- Also in this round: the console's Pretty/Raw views with the headers as actually sent (`host`, `user-agent`,
  `cookie`, ...), and responses keep `content-encoding` because Frida decompresses bodies itself.

### Rust things worth understanding in this milestone
1. **`!Send` types and `spawn_blocking` + `Handle::block_on`** (`crates/frida-script/src/engine.rs` `run_script`).
   A QuickJS runtime may only be used on the thread that created it, but tokio tasks hop between threads at every
   `.await`. So each script gets its own blocking thread, and `block_on` lets that thread still await async work
   (HTTP, sleep) on the app's runtime.
2. **`Rc<Cell<bool>>` for single-threaded shared flags** (`engine.rs` interrupt handler). The handler closure and the
   code after the script share "is the time limit armed?". `Rc` is the cheap, single-thread version of `Arc`, and
   `Cell` allows changing a `bool` through a shared reference. The compiler would reject `Rc` if it ever crossed threads.
3. **Trait objects for callbacks into the app** (`ScriptHost` in `frida-script/src/lib.rs`, `on_log` in
   `http/execute.rs`). The sandbox crate knows nothing about HTTP; it calls `host.send_request(..)`, which returns a
   `Pin<Box<dyn Future + Send>>` because `async fn` can't be called through `dyn` yet.
4. **Borrowed session handles** (`http::Session<'a>` and `AppState::session`). A send borrows the runtime-variable
   mutex and the override maps instead of copying them; the lifetime `'a` makes the compiler check that the store
   outlives the send. Locks are only held inside single statements, never across `.await`.

## M4: curl export/import + history (2026-10-15)

### What was built
- **curl export** (`crates/frida-core/src/codegen/curl.rs`, `http::export_curl`): bash or PowerShell, multi-line,
  variables resolved or kept as `{{placeholders}}`, auth included, secrets masked as `***` unless explicitly included.
- **curl import** (`crates/frida-core/src/import/curl.rs`): the flags of §11.3 plus continuation lines from bash,
  PowerShell and cmd, a Content-Type based body mode heuristic, and warnings for anything unsupported. A corpus in
  `testdata/curl` is snapshot-tested; exports round-trip through import.
- **History** (`crates/frida-core/src/history.rs`): SQLite (rusqlite, bundled), the request as sent and the response,
  bodies capped at 1 MB, newest 500 kept, searchable; secrets and auth values masked before writing.
- **UI**: "Copy as curl" (tree, tab menu, request toolbar), "New request from curl" with a live preview, pasting curl
  into the URL bar, a History sidebar tab, read-only history tabs with "Restore as new request".

### Rust things worth understanding in this milestone
1. **Owning vs borrowing results** (`http/execute.rs` `Executed`). `execute` now returns the prepared request it
   sent. The struct *owns* it (moved in, no copy), so the caller can hand it to the history writer on another thread.
2. **Fire-and-forget blocking work** (`app/src-tauri/src/commands/http.rs` `record_history`). `spawn_blocking`
   without `.await` runs the SQLite write in the background; everything the closure needs is moved into it
   (`Arc` clone of the store, owned entry), which is why it compiles without lifetimes.
3. **`rusqlite` and `Mutex<Connection>`** (`history.rs`). A SQLite connection may not be used from two threads at
   once, so it sits behind a mutex; `query_row(...).optional()` turns "no row" into `Ok(None)` instead of an error.
4. **Small pure helpers over clever code** (`import/curl.rs`). The importer is a plain `while let` loop over tokens
   with a `match` on flags: easy to read and to extend, with every unknown flag turned into a warning.

## M3: Auth (2026-10-15)

### What was built
- **Schemes** (`crates/frida-core/src/auth/`): Basic, Bearer, API key (header or query) and OAuth2, combinable and
  applied in order (OAuth2 + API key sends both); inherited request → folders → collection; `{{variables}}` resolved in
  every auth field.
- **OAuth2** (`auth/oauth2.rs`): client credentials, password, and authorization code + PKCE (S256) with a loopback
  listener on 127.0.0.1 and the system browser; client auth via Basic header or body; token cache keyed by token URL,
  client, scope, grant and user; refresh 30 s before expiry using the refresh token, falling back to a new token.
- **Mock provider** in `frida-testserver` (`/oauth/authorize`, `/oauth/token`) and acceptance tests in `tests/auth.rs`.
- **App/UI**: the Auth tab for requests, folders and the collection (primary scheme + "Add API key"), OAuth2 fields,
  "Get new token" (cancellable) and a token viewer with expiry and decoded JWT claims. Also from UI feedback: table
  layout for key/value editors and a single environment picker.

### Rust things worth understanding in this milestone
1. **Closures stored as trait objects** (`auth/oauth2.rs` `BrowserOpener = Arc<dyn Fn(&str) -> ... + Send + Sync>`).
   The core crate doesn't know Tauri, so the app passes in "how to open a browser" as a function value. Tests pass a
   fake that follows the redirect instead.
2. **`tokio::select!` with timeouts and cancellation** (`oauth2.rs` `authorize`). Waiting for the browser races three
   things: the redirect arriving, a 5-minute timeout, and the user clicking Cancel. Whichever finishes first wins and the
   others are dropped.
3. **Temporaries and borrows** (`tests/http_send.rs`). `&OAuth2Manager::new(..)` inside a closure borrowed a value that
   died when the closure returned; the compiler refused it. Creating the manager once outside and borrowing that is
   the fix, a typical first encounter with Rust's lifetime rules.
4. **Scoped mutable borrows** (`oauth2.rs` `query_pairs_mut` in its own `{ }` block). The URL is borrowed mutably while
   query pairs are appended; ending the block ends the borrow so the URL can be used again.

## M2: Environments, variables, secrets, inspector (2026-10-15, awaiting manual acceptance)

### What was built
- **Resolver** (`crates/frida-core/src/vars/`): `{{name}}` with optional whitespace, the six layers of PLAN.md §5.1
  (runtime → request → folders, nearest first → collection env → collection → global env), recursion up to depth 10,
  cycles reported as `a → b → a`, `{{$uuid}}` and the other dynamic variables (fresh per occurrence),
  `{{process.env.NAME}}`. Unresolved names stay as written. Every part of a request is resolved, including inherited
  folder and collection headers. The report lists each variable's value, source, shadowed definitions and usage, and
  can be masked for the UI.
- **Environments** (`storage/environments.rs`): global ones in `<workspace>/environments`, collection ones in
  `<collection>/environments`; create, rename (moves secrets), delete (deletes secrets). Secret values are stripped
  before every write.
- **Secrets** (`crates/frida-secrets`): OS keychain via `keyring` 4; an encrypted file (Argon2id +
  XChaCha20-Poly1305) when no keychain exists, unlocked with a master password; values are filled in only in memory
  just before sending or inspecting.
- **Cookie jar** (`http/cookies.rs`): one per collection, shared by every send; list and clear per domain or all.
- **Tauri commands** for environments, secrets (set, delete, reveal), the active selection, `inspect_request`,
  runtime variables and cookies. Keychain access runs in `spawn_blocking`.
- **UI**: environment selectors in the header bar; an Environments tab (secret toggle, unset badges, click-to-reveal,
  unlock form for the file fallback); a variables inspector (`{}` button, with a badge counting unresolved variables);
  `{{var}}` highlighting with hover tooltips and red underlines in the URL bar and body editors; cookie viewer and
  "clear runtime variables" in the collection tab.
- **Fixtures**: `testdata/six-layers` (M2 acceptance) and a "Variables" folder plus `local`/`shared` environments in
  the sample workspace.

### Rust things worth understanding in this milestone
1. **Traits and trait objects** (`crates/frida-secrets/src/lib.rs`, `app/src-tauri/src/state.rs`). `SecretStore` is an
   interface; `Arc<dyn SecretStore>` holds "some store" chosen at runtime (keychain or encrypted file), like a Java
   interface reference. `Send + Sync` on the trait is what allows sharing it across Tauri's threads.
2. **Blocking work in async code** (`app/src-tauri/src/error.rs` `blocking`, `commands/env.rs`). Keychain calls block
   the thread. `tokio::task::spawn_blocking` moves them to a thread pool meant for that, so the async runtime keeps
   serving other commands. The closure must be `'static`, which is why commands clone the `Arc` before moving it in.
3. **Borrowing iterators and lifetimes in the resolver** (`vars/scope.rs` `definitions`, `vars/resolver.rs`).
   `definitions` returns an iterator that borrows the scope (`impl Iterator<Item = (&VarSource, &VarDef)> + 's`) instead
   of copying every definition; the `'s` ties the iterator's lifetime to the scope it reads from.
4. **Errors converting with `#[from]` across crates** (`crates/frida-core/src/error.rs` `Secret`, app `error.rs`).
   A `SecretError` becomes a `CoreError` via `?`, and then an `ApiError` with its own code (`secret_locked`,
   `wrong_password`) and hint, without any glue code at the call sites.

## M1: Storage, tree, requests, sending (2026-10-15)

### What was built
- **On-disk format** (`crates/frida-core/src/model/`, `yaml.rs`): typed models for every §4 file, YAML via
  serde-saphyr, a format-version check before parsing, and atomic writes. The §4 examples live in `testdata/format/`
  and are round-trip and snapshot tested.
- **Storage** (`crates/frida-core/src/storage/`): workspaces, collections, folders and requests. Tree scan (broken
  files show up as "invalid" instead of failing), create/rename/duplicate/delete/move/reorder with slugged, unique
  file names. Paths from the UI can't escape the collection folder.
- **HTTP** (`crates/frida-core/src/http/`): `prepare` merges headers collection → folders → request, fills `:path`
  params, builds every body mode and resolves the timeout chain. `send` uses reqwest with redirects, custom CAs
  merged into the OS roots, a TLS-verify toggle and cancellation. `ResponseView` shapes the result for the UI
  (text or base64, truncated above 5 MB, full body kept for "Save").
- **Local state** (`local_state.rs`): settings and recent workspaces, plus per-collection open tabs and expanded
  folders, as JSON in the app data dir.
- **File watcher** (`watch.rs`): debounced, ignores `.git` and Frida's own temp files.
- **Tauri commands** (`app/src-tauri/src/commands/`): 30 thin commands returning structured `ApiError
  { code, message, hint }`, plus the `fs-changed` event.
- **UI**: start screen, sidebar (collection switcher, tree with context menu and drag-and-drop), per-collection tabs
  with unsaved-changes dots, request editor (URL ↔ query sync, path params, headers, all body modes, timeout),
  response viewer (status/time/size, pretty/raw, image and sandboxed HTML preview, headers, copy, save), collection
  settings (timeout, TLS, CA certificates), keep/reload prompt when a dirty tab changes on disk.
- **Testing aids**: `crates/frida-testserver` (echo server, HTTPS mode with a throwaway CA) and
  `testdata/sample-workspace`, whose every request is sent end to end in `tests/sample_workspace.rs`.

### Rust things worth understanding in this milestone
1. **Enums with data and `#[serde(tag = ...)]`** (`model/common.rs` `Auth`, `storage/collection.rs` `TreeNode`).
   Each variant carries its own fields, like a sealed interface with records in Java. serde's internal tagging
   writes the variant name into a field (`mode: custom`), and specta turns it into a TS discriminated union.
   `Box<OAuth2Config>` keeps the enum small, because an enum is as big as its largest variant.
2. **`?` and `From`** (`error.rs` `#[from]`, `app/src-tauri/src/error.rs`). `?` returns early on an error *and*
   converts it with `From`. That's how a reqwest error becomes a `SendError`, then a `CoreError`, then an `ApiError`
   for the UI, without any explicit wrapping code in the commands.
3. **Lifetimes on a borrowing struct** (`http/prepare.rs` `RequestContext<'a>`). The struct holds references
   instead of copies, and `'a` lets the compiler prove the referenced collection and folders outlive it.
4. **Sharing state across threads** (`watch.rs`, `app/src-tauri/src/state.rs`). `Arc<Mutex<T>>` for the watcher roots
   shared with its callback thread; `Mutex` fields in Tauri-managed state; and why a `std::sync::Mutex` is fine as
   long as it is never held across an `.await`. `tokio::select!` plus a `CancellationToken` cancels a request by
   dropping its future (`http/send.rs`).

## M0: Scaffold & CI (2026-10-15)

### What was built
- **Cargo workspace** (`Cargo.toml`) with `frida-core`, `frida-script`, `frida-secrets`, `frida-git` and the
  Tauri app `app/src-tauri` (binary `frida-http`). Shared dependency versions and lints live in the root manifest.
- **Tauri 2 app** with a React 19 + TypeScript + Vite 8 + Tailwind 4 + shadcn/ui frontend in `app/ui`.
  The window shows a placeholder start screen that follows the OS light/dark theme.
- **tauri-specta wiring**: the example command `get_about_info` returns `frida_core::AboutInfo`. The UI calls it
  through the generated, typed `commands.getAboutInfo()` in `app/ui/src/bindings.ts`.
- **Logging**: `tracing` writes to `<app data dir>/logs/frida.YYYY-MM-DD.log`, rotated daily with 7 files kept,
  and also to stderr in debug builds. On macOS the app data dir is `~/Library/Application Support/io.github.clon1998.frida`.
- **CI** (`.github/workflows/ci.yml`): fmt, clippy, cargo test, a bindings drift check, UI typecheck/lint/test,
  then a build matrix for macOS arm64/x64, Windows x64 and Linux x64 that uploads the bundles as artifacts.
- Placeholder app icon generated from `assets/app-icon.svg` with `pnpm tauri icon`.

### Rust things worth understanding in this milestone
1. **Workspaces and `workspace = true`** (`Cargo.toml`, `crates/*/Cargo.toml`). The root manifest works like a
   Maven parent POM with `<dependencyManagement>`: versions are declared once, and member crates inherit them.
   `[workspace.lints]` applies `unsafe_code = "deny"` to every crate that opts in.
2. **Derive macros as code generation** (`crates/frida-core/src/about.rs`). `#[derive(Serialize, Type)]` generates
   trait implementations at compile time, similar to an annotation processor in Java (think Lombok or MapStruct)
   rather than runtime reflection. specta's `Type` is how tauri-specta knows the TS shape of `AboutInfo`.
3. **RAII and "guards"** (`app/src-tauri/src/logging.rs`). `init` returns a `WorkerGuard`. When a Rust value goes
   out of scope it is dropped, deterministically, like `try-with-resources` calling `close()`. Dropping this guard
   flushes and stops the log-writer thread, so `lib.rs` hands it to `app.manage(guard)` to keep it alive for the
   app's whole lifetime.
4. **`cfg!` vs `#[cfg]`** (`app/src-tauri/src/lib.rs`, `logging.rs`). `#[cfg(debug_assertions)]` removes code from
   release builds entirely (bindings export), while `cfg!(debug_assertions)` is just a compile-time `bool` used in
   normal code (the stderr log layer). `Option<Layer>` being a layer itself is a neat pattern for optional layers.
