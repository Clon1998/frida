# Releasing

How a version gets from `main` to a published GitHub Release, and how macOS signing fits in.

## Cutting a release

1. Pick the version (semver; `0.x` while the format and features settle) and set it in all three places:
   - `app/src-tauri/tauri.conf.json` → `"version"`
   - `Cargo.toml` → `[workspace.package] version` (then `cargo check` to update `Cargo.lock`)
   - `app/ui/package.json` → `"version"` (not checked by the workflow, kept in sync by convention)
2. Commit: `chore: release 0.3.0`.
3. Tag and push:
   ```sh
   git tag v0.3.0
   git push origin main v0.3.0
   ```
4. `.github/workflows/release.yml` checks that the tag matches the versions in `tauri.conf.json` and `Cargo.toml`, creates a **draft** release with notes
   generated from the commits, builds the installers on all four targets and attaches them.
5. Review the draft on GitHub (edit the notes if needed) and **publish** it.

If the workflow fails, fix it on `main`, delete the draft release and the tag (`git push --delete origin v0.3.0`,
`git tag -d v0.3.0`) and tag again.

## What gets built

| Platform | Files | Signed |
|---|---|---|
| macOS arm64 / x64 | `.dmg` | Developer ID + notarized (when the secrets below are set) |
| Windows x64 | `.msi`, NSIS `-setup.exe` | No: SmartScreen shows *More info → Run anyway* |
| Linux x64 | `.AppImage`, `.deb`, `.rpm` | No |

## Signed macOS builds

Why it matters: only a signed app with a provisioning profile can use the **data protection keychain**, where
Frida's secrets never cause a password prompt. Unsigned builds fall back to a single login-keychain item (one prompt
per run). Background and decisions: PLAN.md M13.

What's in the repo:

- `app/src-tauri/Entitlements.plist`: application identifier and keychain access group
  (`Q4XB68D4T7.io.github.clon1998.frida`).
- `app/src-tauri/tauri.signed.conf.json`: merged into the Tauri config for signed builds; embeds the provisioning
  profile as `Contents/embedded.provisionprofile`.
- `app/scripts/build-signed.sh`: local signed builds.

What's **not** in the repo (Patrick's machine and GitHub secrets):

- Certificates in the login keychain: *Apple Development* (local) and *Developer ID Application* (releases).
- Provisioning profiles in `~/.frida-signing/` (or `$FRIDA_SIGNING_DIR`): `Frida_Dev.provisionprofile` and
  `Frida_Release.provisionprofile`.
- The App Store Connect API key (`.p8`) for notarization.

### Locally

```sh
cd app
scripts/build-signed.sh dev       # Apple Development: runs on the Macs registered in the "Frida Dev" profile
scripts/build-signed.sh release   # Developer ID + notarization; needs APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH
```

Check a build:

```sh
codesign --verify --deep --strict --verbose=2 target/release/bundle/macos/Frida.app
codesign -d --entitlements - target/release/bundle/macos/Frida.app
```

On start, the log says `secrets: data protection keychain` when the signed store is in use.

### In CI

`release.yml` signs and notarizes when these repository secrets exist (otherwise it builds unsigned):

| Secret | Content |
|---|---|
| `APPLE_CERTIFICATE` | The Developer ID Application certificate as `.p12`, base64 |
| `APPLE_CERTIFICATE_PASSWORD` | Its password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Name (Q4XB68D4T7)` |
| `KEYCHAIN_PASSWORD` | Any random string (for the temporary CI keychain) |
| `APPLE_API_ISSUER`, `APPLE_API_KEY` | App Store Connect API key issuer and key id |
| `APPLE_API_KEY_P8` | The `.p8` file's content |
| `FRIDA_RELEASE_PROFILE` | The "Frida Release" provisioning profile, base64 |

Base64 a file for a secret with `base64 -i file.p12 | pbcopy`.

## Installing unsigned builds

For builds from the CI **Build** job or unsigned releases:

- **macOS:** the first open is blocked. Go to **System Settings → Privacy & Security → Open Anyway**, or run
  `xattr -dr com.apple.quarantine /Applications/Frida.app`. Apps you build locally aren't affected.
- **Windows:** SmartScreen shows "Windows protected your PC": **More info → Run anyway**.
- **Linux:** `chmod +x Frida_*.AppImage`, or install the `.deb`/`.rpm`.
