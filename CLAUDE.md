# CLAUDE.md — Frida

Frida is a local-first desktop HTTP client (Bruno/Postman alternative) built with **Rust + Tauri 2**
and a thin **React/TypeScript** UI. **PLAN.md is the spec.** Read the relevant section before starting
any milestone, and keep its checkboxes up to date.

## About the maintainer
Patrick is an experienced **Java/Spring + React** developer and is **new to Rust**. He will read the code
to learn. Therefore:
- Prefer clear, idiomatic, boring Rust over clever Rust. Avoid unnecessary generics, lifetimes and macros.
- When non-obvious Rust concepts appear (ownership/borrowing tricks, lifetimes, trait objects vs generics,
  `Arc<Mutex<_>>`, `Send + Sync`, async pitfalls), add a short doc comment explaining *why*. Java analogies are welcome.
- At the end of each milestone, add to `docs/CHANGELOG.md` a summary of what was built and 2–4 "Rust things worth
  understanding in this milestone" with file pointers.

## Workflow
- Work **milestone by milestone** (PLAN.md §15). Do not start the next milestone until the current one meets
  its acceptance criteria.
- Make small, focused commits with conventional commit messages (`feat(core): …`, `fix(ui): …`).
- **Ask before** adding a heavy dependency, changing the on-disk format (PLAN.md §4), or deviating from the plan.
  When you deviate, record the decision in PLAN.md.
- Check the current crate and npm versions before adding them. Do not rely on memory for versions or APIs. Read the docs.

## Commands
(Keep this section accurate as the project evolves.)
```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app/ui && pnpm install && pnpm typecheck && pnpm lint && pnpm test
cd app/ui && pnpm e2e              # UI flows in Chromium against a fake backend (first: pnpm exec playwright install chromium webkit)
cd app && pnpm tauri dev       # run the app
cargo run -p frida-testserver  # local echo server on :8787 for manual testing (testdata/sample-workspace uses it)
cd app && pnpm tauri build     # bundle
cd app && scripts/build-signed.sh dev   # macOS: signed bundle (data protection keychain), see README
```
A milestone is only done when all of these pass.

## Architecture rules
- All logic lives in `crates/*`. `app/src-tauri` contains **only** thin Tauri commands that call into the crates.
- The crates must **not depend on Tauri**, so they stay testable and reusable for the future CLI.
- The UI never performs HTTP, file I/O, git or secret access directly. It always goes through a Tauri command.
- TS types for commands and models are **generated with tauri-specta**. Never hand-duplicate Rust types in TS.
- Errors: `thiserror` enums in library crates and `anyhow` only at the app edge. Errors sent to the UI are
  structured (`code`, `message`, optional `hint`) and user-actionable.
- Use `tracing` for logs. **Never log secret values, auth headers, tokens or request bodies at info level.**
- Async: tokio. Don't block the async runtime (use `spawn_blocking` for git CLI calls, keychain and SQLite).

## Security rules (non-negotiable)
- Secret values are **never** written to YAML, history, logs, error messages or exports, unless the user
  explicitly opts in for a single curl export.
- Scripts run sandboxed in QuickJS with time and memory limits. They get no filesystem or network access except via
  `bru.sendRequest`.
- The HTML response preview is rendered in a sandboxed iframe with scripts disabled.
- Git commands are limited to the set in PLAN.md §12. No destructive commands (reset, force push, clean).

## Code style
- Rust: rustfmt defaults, clippy clean, `#![deny(unsafe_code)]` in all crates unless a dependency forces otherwise.
- TS: strict mode, ESLint + Prettier, function components, Zustand stores per domain (workspace, tabs, envs, git).
- Colors come from theme variables (`index.css`): `primary`, `muted-foreground`, `destructive` and the status colors
  `success` / `warning` / `info` (e.g. `text-success`, `bg-warning/10`). No palette colors (`text-emerald-600 dark:…`)
  except for deliberate category colors (environment colors, accent swatches).
- Layout must hold at the minimum window size (800×500): rows that can get crowded wrap (`flex-wrap`), grid columns use
  `minmax(0, 1fr)`, and pane-dependent layouts use container queries (`@container`, `@md:`), not window breakpoints.
- Tests are expected for every non-trivial function in `crates/*`. Use `insta` snapshots for importers and codegen.
- Fixtures live in `testdata/`.

## Naming
- Product: **Frida** (*Fast Request Inspector for Developer APIs*).
- Crates use the `frida-` prefix. Binary / bundle name: **`frida-http`**. This avoids clashing with the existing
  `frida` instrumentation toolkit CLI that many developers have on their PATH.
- On-disk files: `frida-workspace.yaml`, `collection.yaml`, `folder.yaml`, `<slug>.yaml`.
