# Frida

*Fast Request Inspector for Developer APIs*: a local-first desktop HTTP client (a Bruno/Postman
alternative) built with Rust, Tauri 2 and React. Requests are plain YAML files that live in your git
repo. See [PLAN.md](PLAN.md) for the full scope and roadmap, and [docs/CHANGELOG.md](docs/CHANGELOG.md)
for what has been built so far.

## Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Rust | stable (via [rustup](https://rustup.rs)) | `rustup update stable` |
| Node.js | 24 | |
| pnpm | 12 | `npm install -g pnpm@12` |

Plus the platform's webview and build tools:

- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  ("Desktop development with C++"). WebView2 already ships with Windows 10 and 11.
- **Linux (Debian/Ubuntu):**
  ```sh
  sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev patchelf
  ```

Then install the JavaScript dependencies once:

```sh
cd app && pnpm install
```

## Run it without an installer (development)

```sh
cd app
pnpm tauri dev
```

This starts the Vite dev server and the app window. UI changes reload instantly, and Rust changes
rebuild and restart the app automatically. The first start compiles all Rust dependencies and takes a
few minutes; later starts take seconds.

### Try it with the sample workspace

1. In a second terminal, start the local echo server:
   ```sh
   cargo run -p frida-testserver
   ```
   It listens on `http://localhost:8787` and echoes every request back as JSON.
2. In the app, choose **Open workspace…** and select `testdata/sample-workspace`.
3. Send the requests under *Methods* and *Body types*. *Get image* shows the image preview, and
   *Slow request* is there to try cancelling.

## Build and install the app

### Build an installer on your own machine

```sh
cd app
pnpm tauri build
```

The installers end up in `target/release/bundle/`:

| OS | Output |
|---|---|
| macOS | `macos/Frida.app` and `dmg/Frida_<version>_<arch>.dmg` |
| Windows | `msi/*.msi` and `nsis/*-setup.exe` |
| Linux | `deb/*.deb`, `rpm/*.rpm` and `appimage/*.AppImage` |

On macOS you can drag `Frida.app` straight into `/Applications`, or open the `.dmg`.

Useful variations:

```sh
pnpm tauri build --bundles app     # macOS: only Frida.app, skip the slower .dmg step
pnpm tauri build --no-bundle       # just the executable: target/release/frida-http
```

### Download an installer from CI

The build matrix in GitHub Actions doesn't run on every push. To get installers:

1. Go to **Actions → CI → Run workflow** on GitHub (or open a pull request).
2. When the run finishes, download the artifact for your platform (`frida-http-macos-arm64`,
   `frida-http-macos-x64`, `frida-http-windows-x64`, `frida-http-linux-x64`) from the run's summary page.

The builds aren't code-signed yet (planned for later), so the OS warns about them:

- **macOS:** the first open is blocked. Go to **System Settings → Privacy & Security** and click
  **Open Anyway**, or remove the quarantine flag:
  `xattr -dr com.apple.quarantine /Applications/Frida.app`. Apps you build locally aren't affected.
- **Windows:** SmartScreen shows "Windows protected your PC". Click **More info → Run anyway**.

## Where Frida keeps its local data

Settings, open tabs and logs are stored per machine (never in your repo):

- **macOS:** `~/Library/Application Support/io.github.clon1998.frida/`
- **Windows:** `%APPDATA%\io.github.clon1998.frida\`
- **Linux:** `~/.local/share/io.github.clon1998.frida/`

Logs are in the `logs/` subfolder. Set `RUST_LOG=debug` before starting the app for more detail.

## Checks

These are the same checks CI runs. A change is only done when they all pass:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app/ui && pnpm typecheck && pnpm lint && pnpm test
```

`cargo test` also regenerates the TypeScript bindings in `app/ui/src/bindings.ts`. Commit that file
when it changes.
