# Changelog

## M15: CLI runner `frida-run` (2026-10-17)

### What was built
- **`frida-run`** (`crates/frida-cli`): runs a collection or folder with the app's runner engine, with the
  collection and global environments, `--var` overrides, `--delay`, `--bail` and `--insecure`. Reference:
  [cli.md](cli.md).
- **Secrets without a keychain**: `--secret name=value`, `FRIDA_SECRET_<name>` environment variables, AWS Secrets
  Manager with `--aws-profile`/`AWS_PROFILE`. Every line, report and script log is masked.
- **Reporters**: terminal lines as each request finishes (failing tests below), JUnit XML (suite per folder, case
  per test) and JSON. Exit codes 0 / 1 (test failed) / 2 (error).
- **Releases** attach `frida-run-<target>.tar.gz`/`.zip` for all four targets; `cargo install --path
  crates/frida-cli` works too.
- M14's acceptance (a real update) is postponed until the update key exists.

### Rust things worth understanding in this milestone
1. **A library plus a thin binary** (`crates/frida-cli/src/main.rs`, `lib.rs`). `main` only parses and calls
   `frida_cli::run`; the logic is in the library, so unit tests reach it. Integration tests run the real binary
   with `env!("CARGO_BIN_EXE_frida-run")`, which Cargo sets to the freshly built executable.
2. **`#[derive(Parser)]`** (`args.rs`). clap reads the struct at compile time: fields become options, doc
   comments become `--help` text, `value_parser = parse_pair` plugs in our own `NAME=VALUE` parsing. Like picocli's
   annotations, but checked by the compiler.
3. **Closures that borrow mutably** (`lib.rs`, `on_step`). The step callback pushes into a local `Vec` and sets
   `bailed`; that is fine because the closure is only used during `runner::run(...)`, so the borrows end before we
   read the values. `&mut T` is `Send` when `T` is, which the runner requires.
4. **`impl Write` for our own type** (`Progress`). Implementing `std::io::Write` (two methods) lets the reporter
   write to "stdout or stderr, decided at runtime" and to a `Vec<u8>` in tests through the same `&mut dyn Write`,
   like an `OutputStream` subclass.

## M14: Auto-update (2026-10-17, waiting for the update key and a first release)

### What was built
- **Update check** on start, at most once a day and never in development builds (Settings → Updates switches it off;
  **Check for updates** checks right away). A quiet notice says "Frida X is available"; **Details…** opens the
  release notes with **Install and restart** and **Later**.
- **Installing**: unsaved tabs are offered to be saved first; the download shows progress and can be cancelled;
  nothing is installed while a request or run is in flight. Every installer type updates itself (`.app`, NSIS,
  MSI, AppImage, `.deb`, `.rpm`).
- **Safety**: the plugin verifies each download with Frida's update key, and `requireSignedVersion` binds the
  signature to the version, so an altered `latest.json` can't offer an old release as new. A test pins these settings.
- **Release workflow**: with `TAURI_SIGNING_PRIVATE_KEY` set, installers are signed for the updater and
  `latest.json` is attached (`app/scripts/update-manifest.sh`). Only published releases are offered.
- **Docs**: a rewritten README, plus a user guide, file format, architecture, development and releasing docs.
- **Fix found on the way**: with the updater, rustls has two crypto backends, so the test server's TLS mode
  panicked; the app now fixes the TLS backend at startup so sends can't switch backend after the first update check.

### Rust things worth understanding in this milestone
1. **Cancelling by dropping a future** (`app/src-tauri/src/commands/update.rs` `download`). `tokio::select!` polls the
   download and a `CancellationToken` together and continues with whichever finishes first. When cancel wins, the
   download future is dropped, and a dropped future simply stops: Rust futures only make progress while polled.
   There's no thread to interrupt, unlike `Future.cancel(true)` in Java.
2. **Cargo feature unification** (the rustls fix, `app/src-tauri/src/lib.rs` `run`). Features are additive across
   the whole build: the updater turning on rustls' `ring` feature also turns it on for reqwest's rustls, so suddenly
   two backends exist and "pick the one that's enabled" no longer works. It's like two Maven modules pulling
   different implementations of one SPI onto the same classpath. Choosing one explicitly (`install_default`) settles it.
3. **Numbers that cross into TypeScript** (`frida_core::local_state::AppSettings::last_update_check`). specta refuses
   `u64`, because a JS `number` is a 64-bit float and silently loses precision above 2^53. Unix seconds and download
   sizes fit easily in `u32`, so the types say so and conversion clamps (`clamp_u32`).
4. **`cfg!` vs `#[cfg]`** (`install_blocker`). `cfg!(debug_assertions)` is an ordinary `bool` expression, so both
   branches must compile on every build; `#[cfg(...)]` removes code entirely (as in M13's macOS-only keychain). Use
   `cfg!` when the code is valid everywhere and only the behaviour differs.

## M13: macOS data protection keychain and signed releases (2026-10-17)

### What was built
- **No more keychain prompts in signed builds**: secrets live in the data protection ("iOS-style") keychain, in
  Frida's own access group `Q4XB68D4T7.io.github.clon1998.frida` (`frida_secrets::ProtectedStore`).
- **Dev builds** (`pnpm tauri dev`, unsigned bundles) keep all secrets in one login-keychain item
  (`VaultStore`): one prompt per run instead of one per secret.
- **Migration**: old per-secret items, and a dev build's vault, move to the new store the first time a secret is
  used, then are deleted (`MigratingStore`). Checked with real secrets on a signed build.
- **Logo**: Patrick's one-line Westie drawing, traced into vector paths (`assets/logo/trace.py`, potrace) as a
  white line on the indigo app icon, and shown on the start screen. All platform icons regenerated.
- **Signing**: `Entitlements.plist`, `tauri.signed.conf.json` (embeds the provisioning profile), and
  `app/scripts/build-signed.sh dev|release`. The release workflow signs with the Developer ID and notarizes the
  macOS builds when the Apple secrets are set.

### Rust things worth understanding in this milestone
1. **Platform-specific code with `#[cfg(target_os = "macos")]`** (`frida-secrets/src/lib.rs`, `app/src-tauri/src/state.rs`
   `os_keychain`). The `protected` module and the `security-framework` dependency
   (`[target.'cfg(target_os = "macos")'.dependencies]`) only exist in macOS builds; Windows and Linux compile the
   other branch. Like a Maven profile per OS, but checked by the compiler.
2. **Composition over a trait object** (`MigratingStore`, `VaultStore`, `CachedStore`). Each wraps a
   `Box<dyn SecretStore>` and is itself a `SecretStore`, so the app stacks them: cache → migrate → protected
   keychain, with the vault → old items as the fallback. The decorator pattern from Java, with the same trait
   everywhere.
3. **OS error codes as values** (`protected.rs`). The keychain answers with `OSStatus` codes; `errSecItemNotFound`
   (-25300) is a normal "no value", anything else (e.g. the missing entitlement in an unsigned build) is a real
   error. `available()` turns that into a startup check without any prompt.

## After M12: feedback round 3 (2026-10-17)

### What was built
- **Environments**: copy to clipboard (YAML, secret values never included), paste, and copy/move between global
  and collections with secret values going along through the keychain.
- **Examples menu** for pre-request scripts, post-response scripts and tests (29 examples in
  `frida_script::snippets`). One test runs every example against the test server's new `/sample` route and checks
  it does what its title says; test examples must also fail on a 404.
- **Descriptions** for query/path params, headers, vars and form fields (optional, only written when set),
  imported from OpenAPI and Bruno `@description`. Editing the URL keeps them.
- **Fixes found by the new tests**: URL-encoded secrets leaked into history and the console; concurrent requests
  each fetched their own OAuth2 token (a browser sign-in each for the auth code grant); an open find bar painted
  over dialogs in WebKit; the changes tab had a second file list.
- **More tests**: console order/levels/masking end to end, variable precedence through a real send, import round
  trips, save idempotence for every testdata file, an explicit sandbox escape list. Playwright now also runs on
  WebKit, the engine the app uses on macOS and Linux.

### Rust things worth understanding in this round
1. **`skip_serializing_if` for backward-compatible fields** (`model/common.rs` `KeyValue::description`). With
   `#[serde(default)]` old files load, and with `skip_serializing_if = "String::is_empty"` the field is left out
   when empty, so existing files are rewritten byte for byte. The TS side must then treat it as optional
   (`app/ui/src/lib/model.ts` `Complete`).
2. **Async mutex vs std mutex** (`auth/oauth2.rs` `fetching`). A lock held across an `.await` must be
   `tokio::sync::Mutex`: a `std::sync::Mutex` guard would block the runtime thread (and isn't `Send`). One
   `Arc<tokio::sync::Mutex<()>>` per token key turns concurrent fetches into one fetch plus waiters.
3. **`flat_map` to widen a list** (`mask.rs` `Masker::new`). Each secret expands into its raw and URL-encoded
   forms in one iterator chain; sorting longest-first afterwards keeps overlapping secrets masked as a whole.
4. **A data table instead of code** (`frida-script/src/snippets.rs`). A `&[(&str, SnippetKind, ...)]` constant
   keeps 29 examples readable; `snippets()` maps it to owned `Snippet`s for serde/specta.

## M12: Search, git diffs in tabs, Frida archives, UI flow tests (2026-10-16)

### What was built
- **Find bar** in every multi-line editor (response body, request body, scripts): Cmd/Ctrl+F or the search button
  in the editor's corner, with "3 of 17", previous/next, match case and regex. Read-only editors are focusable now,
  so Cmd/Ctrl+F also works in the response. The JSONPath filter folds behind a "Filter" button.
- **Changes tab** for git: the changed files on the left, the selected file's diff on the right (unified or side by
  side, YAML highlighting, unchanged stretches folded), stage/unstage from both lists. Built on `@codemirror/merge`;
  the backend returns both versions of a file (`git show`, read-only) instead of a raw `git diff`.
- **Frida archives**: export a workspace, collection, folder or request as `.zip` (the on-disk YAML plus
  `frida-export.yaml`), import it via "Import Frida archive…" (collection menu, tree, start screen). No secret
  values inside; imports reject unsafe paths, symlinks and oversized archives and extract via a staging folder.
- **Playwright UI flow tests** (`pnpm e2e`): eight core flows in Chromium against an in-memory fake backend
  (`src/e2e/fakeBackend.ts`, only bundled with `VITE_E2E=1`), also as a CI job. They found the find bar's missing
  focus on the first run.
- **Keychain prompts**: secrets are read from the keychain once per run and concurrent reads wait for the first
  (`frida_secrets::CachedStore`). The proper fix, the data protection keychain with a signed app, is planned as M13.

### Rust things worth understanding in this milestone
1. **Single-flight with two mutexes** (`frida-secrets/src/cached.rs`). The cache map's lock is only held for a lookup;
   a second `reading` lock serialises slow keychain reads, and the cache is checked again after getting it
   ("double-checked locking", as in Java). The `MutexGuard` in `_reading` unlocks when it goes out of scope.
2. **Bytes vs strings** (`frida-git/src/run.rs` `run_program_bytes`, `lib.rs` `file_versions`). `String` must be valid
   UTF-8, so git output that may be a binary file stays a `Vec<u8>`; `String::from_utf8(bytes).ok()` turns it into
   text only if it is. `Option<Option<String>>` then means "side missing" vs "side exists but isn't text".
3. **Defensive paths** (`frida-core/src/archive.rs` `safe_name`). `Path::components()` splits a path into typed
   parts (`Normal`, `ParentDir`, `RootDir`, `Prefix` for `C:`); accepting only `Normal` rules out traversal on every
   OS. `std::io::Read::take` caps how much is read, whatever size the zip claims.
4. **`tempfile` for atomic results** (`archive.rs` `write_zip`, `extract_to_new_dir`). Write to a temp file or
   folder next to the target, then `persist`/`rename` it into place; if anything fails first, the `TempDir`/
   `NamedTempFile` is dropped and deletes itself (RAII), so no half-written archive or import is left behind.

## After M11: feedback round 2 (2026-10-16)

### What was built
- **Tables keep focus while adding a row**: typing into the empty "add" row of any table (environment variables and
  secrets, headers, params, multipart) no longer creates one row per keystroke.
- **OpenAPI import**: folders come from the URL paths by default (`/legs/resolve/{n}` → `legs/resolve`), with
  "Folders from: tags" as the alternative. A requirement like `api_key + OAuth2` now imports as OAuth2 (primary) plus an
  API key, as the auth editor shows it.
- **History** keeps its list when switching sidebar tabs, loads in the background as soon as a collection is active,
  and shows a spinner while it refreshes.
- **Save response** suggests a file name: the `Content-Disposition` name, else `response.<ext>` from the content type.
- **Environment editor** marks the active environment; right-click sets or clears it.
- **Tab** accepts the highlighted autocomplete suggestion.
- **Tabs**: the close button sits at the right edge; closing unsaved tabs (button, menu, Cmd/Ctrl+W) asks
  *Save / Don't save / Cancel* in one in-app dialog.
- **Cancel** while sending shows only the spinner and its label.
- **Workspaces**: pinned workspaces come first, then at most 5 recent ones; pin/unpin and "Remove from list" on hover
  in the workspace menu and by right-click on the start screen.
- M12 is planned for the larger items: body search, git diffs in tabs, Frida archives and Playwright UI tests.

### Rust things worth understanding in this round
1. **`Iterator::partition`** (`import/openapi.rs` `auth_for`). It splits one iterator into two collections by a
   predicate and keeps the order inside each half, like `Collectors.partitioningBy` in Java. `matches!` is a
   pattern test that returns a `bool`, here used to ask "is this the `ApiKey` variant?" without pulling out its fields.
2. **Recursion instead of juggling `&mut`** (`import/openapi.rs` `insert_at_path`). Walking a tree you are building
   with one `&mut` variable that moves deeper each loop iteration is possible, but the borrow checker makes it fiddly.
   A recursive function holds one `&mut Vec<PlannedFolder>` per level and passes a `&mut` to the child's list down,
   so every borrow ends when its call returns.
3. **Additive serde changes** (`local_state.rs` `pinned_workspaces`, `PlannedFolder::folders`). `#[serde(default)]`
   on the struct lets older `settings.json` files without the new field load, and
   `#[serde(skip_serializing_if = "Vec::is_empty")]` leaves the field out of the output when it's empty, which is
   why the existing OpenAPI snapshots didn't change.

## M11: Collection runner (2026-10-16)

### What was built
- **`frida_core::runner`** (`crates/frida-core/src/runner.rs`): `plan_run` lists the saved requests of a collection or
  folder in tree order; `run` sends them one after another in runner mode, with one shared session and cookie jar,
  an optional delay, and reports a `RunResult` per request (passed, failed, not sent, skipped) and a `RunSummary`.
- **Script control** like Bruno: `fri.runner.skipRequest()`, `fri.setNextRequest(name)`, `setNextRequest(null)` and
  `fri.runner.stopExecution()`; `req.getExecutionMode()` is `"runner"`. A loop guard ends endless jumps.
- **App**: `run_collection` streams a `RunProgress` event per request (with the response, which also goes to the
  response cache and the console; not to history), `plan_run` lists the requests, `cancel_request` stops a run.
- **UI**: "Run collection…" / "Run folder…" in the sidebar menus and a Run button on collection and folder tabs open a
  Runner tab: delay, Run/Stop, live results with status, time and tests, the selected result's response, a summary,
  and a warning when open requests in the run have unsaved changes.
- Also in this round: the **JSONPath filter** and the first release tag, `v0.1.0`.

### Things worth understanding in this milestone
1. **Callbacks as trait objects** (`runner::run`, `on_step: &mut (dyn FnMut(RunStep) + Send)`): `FnMut` because the
   app's callback changes state (it stores responses), `dyn` so callers pass any closure without generics, and
   `Send` because the async function's future (which holds the reference across `.await`s) moves between threads.
2. **Cancelling a wait** (`runner::run`, `tokio::select!`): waits for whichever finishes first, the delay or the
   cancellation, and drops the other. `cancel.child_token()` gives each send a token that is cancelled with the run.
3. **Passing extra outputs through a pipeline** (`http::RunnerFlow`, `Executed::flow`, `CoreError::Skipped`): what
   scripts ask of a run travels as data next to the response; a skipped request is its own error variant, so the
   runner can `match` on it instead of comparing messages.
4. **Numbers across the IPC boundary** (`RunResult::time_ms: Option<u32>`): specta refuses `u64` in TypeScript
   types because JavaScript numbers lose precision above 2^53, so durations are `u32` milliseconds.

## After M10: feedback round (2026-10-16)

### What was built
- **Settings dialog** (gear in the footer and on the start screen, Cmd/Ctrl+,): theme (system/light/dark), accent
  color, UI size, editor font size, git program, AWS config/credentials files and secret cache time, history size.
  Saved as `preferences` in `settings.json` and applied at once.
- **Environments**: variables and secrets can be disabled (`enabled: false`), every table's rows can be reordered by
  dragging, and environments themselves can be reordered (optional `seq` in the environment file).
- **Response pane** only appears once a request is sent; it can be closed (✕) and reopened from the footer, and the
  next send opens it again.
- **Variables everywhere**: auth fields and folder/collection tabs highlight `{{variables}}` and autocomplete them.
  Auth fields are a shadcn input box with a one-line editor inside (`VariableInput`); literal secrets are hidden with
  CSS while not focused.
- **Layout and colors**: checked at the minimum window size (800×500); rows wrap, grid columns shrink, pane-dependent
  layouts use container queries. Status colors are theme variables (`success`, `warning`, `info`).
- **JSONPath filter** for JSON responses (`$.items[*].id`, filters like `$.items[?@.price < 10]`), filtered in Rust
  (`crates/frida-core/src/jsonpath.rs`) on the cached body.
- **CI**: "Run workflow" has checkboxes for the platforms to build and for lint & test; pushes no longer cancel a
  running manual build (that was the "failed" Windows build).
- **Fixes**: no console windows flashing up on Windows when Frida runs git; the window no longer scrolls below the
  footer (editor popup containers had `height: 100%`); secret auth fields accept typing again.

### Things worth understanding in this round
1. **Conditional compilation** (`crates/frida-git/src/run.rs`, `hide_console_window`): `#[cfg(windows)]` compiles a
   function only for Windows, and `std::os::windows::process::CommandExt` is an *extension trait*: importing it adds
   `creation_flags` to `Command`, a bit like a Kotlin extension function. The `#[cfg(not(windows))]` twin is empty.
2. **A process-wide setting** (`run.rs`, `static EXECUTABLE: RwLock<Option<PathBuf>>`): Rust only allows mutable
   statics behind a lock. `RwLock::new` is a `const fn`, so the static needs no lazy initialisation; many git calls
   read it at once, changing the setting writes it.
3. **Settings that change at runtime** (`crates/frida-secrets/src/aws.rs`, `AwsSecrets::configure`): the cache time and
   file locations sit in `RwLock`s inside a struct shared as `Arc<AwsSecrets>`, so `&self` methods can change them.
   This is *interior mutability*: the Rust way to change something through a shared reference.
4. **Serde defaults for evolving files** (`crates/frida-core/src/local_state.rs`, `Preferences`;
   `model/environment.rs`, `seq`): `#[serde(default)]` lets older files load, and
   `skip_serializing_if = "is_zero"` keeps new fields out of files that don't use them, so existing YAML doesn't change.

## M10: Release polish (2026-10-16)

### What was built
- **Release workflow** (`.github/workflows/release.yml`): a `v*` tag checks that it matches the app version, creates a
  draft release with generated notes and attaches the .dmg, .msi, NSIS .exe, AppImage, .deb and .rpm of all four
  targets. README explains releasing and the first start of the unsigned apps.
- **Keyboard**: Cmd/Ctrl+K quick open (fuzzy search over the collection's requests), +T new request, +E environment
  switcher, +/ shortcut list (also from the status bar); one table in `app/ui/src/lib/shortcuts.ts`.
- **Empty states and onboarding**: the main area shows what to do and the main shortcuts; the start screen explains
  workspaces, collections and secrets and points to the importers.

### Things worth understanding in this milestone
1. **Release automation without third-party actions**: the workflow only uses `gh` (preinstalled on GitHub's runners)
   with the job's token and `permissions: contents: write`, so nothing outside GitHub gets write access.
2. **Platform-specific modifiers** (`isMac()` in `lib/shortcuts.ts`): macOS uses Cmd for app shortcuts and keeps
   Ctrl+E/K for text editing, so the handler checks `metaKey` there and `ctrlKey` elsewhere.

## M8: AWS (2026-10-16)

### What was built
- **Secrets Manager** (`crates/frida-secrets/src/aws.rs`): profiles from `~/.aws/config` and `credentials` (SSO
  marked), `GetSecretValue` with the chosen profile and optional region, JSON key picking, a 15-minute in-memory
  cache with "refresh secrets", and actionable errors (expired SSO → `aws sso login --profile X`).
- **Pipeline** (`crates/frida-core/src/secrets.rs` `fill_aws_secrets`): AWS-backed secret variables are filled when
  sending (cache only while inspecting); a missing profile becomes a clear "choose a profile" error.
- **UI**: a source column (Keychain / AWS: secret id, JSON key, region) for secrets, the AWS profile picker and
  "Refresh secrets" in the environment editor, a profile prompt on first use that resends afterwards, the login
  command with a copy button in send errors, and the Cognito preset in the OAuth2 editor.

### Rust things worth understanding in this milestone
1. **Testing around an SDK with a trait** (`SecretFetcher` in `aws.rs`). The real `SdkFetcher` and the tests'
   fake implement the same trait; `AwsSecrets` holds an `Arc<dyn SecretFetcher>`, like injecting an interface in
   Spring and passing a mock in tests.
2. **Not holding a lock across `.await`** (`AwsSecrets::get`). The cache is read, the lock dropped, AWS awaited, then
   the lock taken again to store the value: holding a `std::sync::Mutex` guard over an await could block a runtime
   thread (and the guard isn't `Send`).
3. **Closures vs. borrows** (`profile_index` in `aws.rs`). A closure that pushes into `profiles` borrows it mutably
   for as long as the closure lives, so the loop couldn't touch `profiles` itself; a plain function taking
   `&mut Vec` borrows only for each call.
4. **Error enums with data for the UI** (`CoreError::AwsProfileRequired`, `ApiError.aws`). The error carries the
   environment name and a fix command, so the UI can act on it instead of parsing message text.


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
