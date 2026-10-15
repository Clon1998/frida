# Changelog

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
