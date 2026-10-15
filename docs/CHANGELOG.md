# Changelog

## M0: Scaffold & CI (2026-10-15)

### What was built
- **Cargo workspace** (`Cargo.toml`) with `frida-core`, `frida-script`, `frida-secrets`, `frida-git` and the
  Tauri app `app/src-tauri` (binary `frida-http`). Shared dependency versions and lints live in the root manifest.
- **Tauri 2 app** with a React 19 + TypeScript + Vite 8 + Tailwind 4 + shadcn/ui frontend in `app/ui`.
  The window shows a placeholder start screen that follows the OS light/dark theme.
- **tauri-specta wiring**: the example command `get_about_info` returns `frida_core::AboutInfo`. The UI calls it
  through the generated, typed `commands.getAboutInfo()` in `app/ui/src/bindings.ts`.
- **Logging**: `tracing` writes to `<app data dir>/logs/frida.YYYY-MM-DD.log`, rotated daily with 7 files kept,
  and also to stderr in debug builds. On macOS the app data dir is `~/Library/Application Support/dev.frida-http.app`.
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
