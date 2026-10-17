# Development

Setting up a machine, running the app, and the checks every change has to pass. For how the code is organised,
see [architecture.md](architecture.md).

## Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Rust | stable (via [rustup](https://rustup.rs)) | `rustup update stable` |
| Node.js | 24 | |
| pnpm | 12 | `npm install -g pnpm@12` |
| git | any recent | Used by the git panel and its tests |

Plus the platform's webview and build tools:

- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  ("Desktop development with C++"). WebView2 already ships with Windows 10 and 11.
- **Linux (Debian/Ubuntu):**
  ```sh
  sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev patchelf
  ```

Install the JavaScript dependencies once (one pnpm workspace in `app/`, one lockfile):

```sh
cd app && pnpm install
```

## Running the app

```sh
cd app
pnpm tauri dev
```

This starts Vite and the app window. UI changes reload instantly; Rust changes rebuild and restart the app. The
first start compiles all dependencies and takes a few minutes, later starts take seconds.

Every debug start also regenerates `app/ui/src/bindings.ts`, the TypeScript types and command wrappers.

### The test server

```sh
cargo run -p frida-testserver          # http://localhost:8787 (pass another port as argument)
```

It echoes every request as JSON (method, path, query, headers, body, multipart parts) and has a few special routes:

| Route | Does |
|---|---|
| `/slow?ms=…` | Waits before answering (try cancelling) |
| `/status/{code}` | Answers with that status |
| `/redirect` | Redirects to `/echo` |
| `/image.png` | A 1×1 PNG |
| `/gzip` | A gzip-compressed body |
| `/cookies/set?name=value` | Sets cookies |
| `/sample` | A fixed JSON document used by the script examples |
| `/oauth/authorize`, `/oauth/token` | A mock OAuth2 provider (used by the auth tests) |

Open `testdata/sample-workspace` in the app (**Open workspace…**) for ready-made requests against it: every
method, every body type, variables, scripting and an image.

### Logs

Logs go to `logs/` in the app data folder (see [file-format.md](file-format.md#what-is-not-in-these-files)) and to
the terminal. `RUST_LOG=debug pnpm tauri dev` shows more. Secret values, auth headers and bodies are never logged at
info level; URLs only at debug level.

## Checks

These are the checks CI runs. A change is done when they all pass:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app/ui && pnpm typecheck && pnpm lint && pnpm test
cd app/ui && pnpm e2e
```

- `cargo test` regenerates `bindings.ts`. Commit it when it changes; CI fails on a stale file.
- `pnpm lint` runs ESLint and Prettier (`pnpm format` fixes formatting).
- Snapshot tests use [insta](https://insta.rs). When an importer's output changes on purpose, review the new
  snapshots with `cargo insta review` (`cargo install cargo-insta`) and commit them.

### UI flow tests (Playwright)

```sh
cd app/ui
pnpm exec playwright install chromium webkit    # once
pnpm e2e                                        # all flows, Chromium and WebKit
pnpm e2e --project chromium --ui                # one browser, interactive
```

The flows run the real UI in Vite (port 5183) against an in-memory fake backend (`src/e2e/fakeBackend.ts`) that
`mockIPC` injects when `VITE_E2E=1`; production builds don't contain it. The fixture fails a test on any command the
fake doesn't handle, on page errors and on error toasts, so add new commands to the fake when the flows use them.
WebKit is the engine the app uses on macOS and Linux, so a flow that only fails there usually points at a real
WebKit quirk.

## Building locally

```sh
cd app
pnpm tauri build                   # installers for this OS in target/release/bundle/
pnpm tauri build --bundles app     # macOS: only Frida.app, skips the slower .dmg
pnpm tauri build --no-bundle       # only the executable: target/release/frida-http
```

| OS | Output in `target/release/bundle/` |
|---|---|
| macOS | `macos/Frida.app`, `dmg/Frida_<version>_<arch>.dmg` |
| Windows | `msi/*.msi`, `nsis/*-setup.exe` |
| Linux | `deb/*.deb`, `rpm/*.rpm`, `appimage/*.AppImage` |

On macOS, unsigned builds keep all secrets in one login-keychain item, so the keychain asks once per run. Signed
builds avoid that, see [releasing.md](releasing.md#signed-macos-builds).

## CI

`.github/workflows/ci.yml`:

- **Lint & test** (every push and PR, skipped for docs-only changes): fmt, clippy, tests, a check that
  `bindings.ts` is current, UI typecheck/lint/test.
- **UI flows (Playwright)**: `pnpm e2e`; traces of failed tests are uploaded as an artifact.
- **Build** (pull requests and **Actions → CI → Run workflow**): installers for macOS arm64 and x64, Windows x64
  and Linux x64, uploaded as `frida-http-<platform>` artifacts.

## Conventions

The project rules are in [CLAUDE.md](../CLAUDE.md). In short:

- All logic in `crates/*`, thin commands in `app/src-tauri`, the UI only calls commands.
- `thiserror` in crates, structured `ApiError { code, message, hint }` towards the UI.
- No blocking work on the async runtime: file I/O, SQLite, keychain and git go through `spawn_blocking`.
- Conventional commits (`feat(core): …`, `fix(ui): …`), one focused change per commit.
- Ask before adding heavy dependencies or changing the file format; record decisions in PLAN.md §16.
- At the end of a milestone: tick PLAN.md, add a section to [CHANGELOG.md](CHANGELOG.md).
