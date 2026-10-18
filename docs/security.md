# Security & privacy

What Frida stores, what it sends where, and how it protects secrets. Written for the person who has to approve a
tool before a team may use it. To report a vulnerability, see [SECURITY.md](../SECURITY.md).

## In short

- A desktop app with **no server component, no account, no cloud sync and no telemetry**.
- Frida goes online only for what you ask it to do, plus a daily update check that can be switched off.
- **Secret values never go into the files you commit**, nor into history, logs, error messages or exports.
- Scripts run in a sandbox without file system or network access.
- Open source under the [MIT license](../LICENSE): you can read, build and audit every line.

## What Frida stores, and where

| What | Where | Shared through git? |
|---|---|---|
| Requests, folders, collections, environments (without secret values) | YAML files in your workspace and collection folders | Yes, that's the point |
| Secret values | The OS keychain: macOS Keychain, Windows Credential Manager, Linux Secret Service | No |
| Settings, open tabs, history, logs | The app data folder (below) | No |
| OAuth2 tokens, AWS secret values | Memory only (AWS values for 15 minutes by default) | No |

The app data folder is `~/Library/Application Support/io.github.clon1998.frida/` on macOS,
`%APPDATA%\io.github.clon1998.frida\` on Windows and `~/.local/share/io.github.clon1998.frida/` on Linux.
Nothing machine-specific (open tabs, AWS profile names, history, secret values) is written into the shared files;
[File format](file-format.md#what-is-not-in-these-files) lists the details.

On Linux systems without a Secret Service, secret values go to an encrypted file in the app data folder instead,
unlocked with a password you choose (Argon2id key derivation, XChaCha20-Poly1305 encryption). Frida tells you when
this fallback is active.

## What goes over the network

Only these connections, each started by something you configured or did:

- **Your requests**, and what they need: OAuth2 token calls, and `fri.sendRequest` calls in your scripts.
- **AWS**, for secrets with an AWS source (Secrets Manager, Cognito, API Gateway), with your AWS profile.
- **git** push, pull and fetch, through your installed `git` with your SSH keys and credential helpers.
- **The update check**: at most once a day, a download of
  `https://github.com/Clon1998/frida/releases/latest/download/latest.json`. Switch it off in **Settings →
  Updates**.

There is no analytics, crash reporting or usage tracking. Proxies (system, manual or none) apply to requests,
OAuth2, scripts and AWS; git uses its own proxy configuration.

## How secrets are protected

- A variable marked **secret** is written to the environment file as `secret: true` with no value. Its value is in
  the keychain of each person's machine (or comes from AWS).
- Secret values are shown as `***` in the console, the history, curl exports and the variables inspector, and
  never written to logs or error messages. A curl export includes them only when you explicitly ask for it, for
  that one export.
- Frida archives (exports of collections) never contain secret values.
- `frida-run` on CI reads secrets from environment variables (`FRIDA_SECRET_*`) or AWS and masks them in all
  output: the terminal, JUnit and JSON reports, and script logs.

## Scripts

Pre-request scripts, post-response scripts and tests run in **QuickJS**, an embedded JavaScript engine, not in
Node.js:

- No file system, no network, no `require`, no Node APIs. The only way out is `fri.sendRequest`, which goes
  through the same HTTP stack (and proxy and TLS settings) as your requests.
- Each script gets a fresh sandbox with a 10-second time limit and a 128 MB memory limit.

Scripts are part of the collection files, so review them like any other code your team commits.

## Responses

HTML responses are previewed in a sandboxed frame with every restriction on: no scripts, forms, popups or access
to the app. The app itself runs with a Content Security Policy that only allows its own code.

## Updates and builds

- Release builds are made by GitHub Actions from the public repository; the workflow is in
  `.github/workflows/release.yml`.
- **macOS** builds are signed with an Apple Developer ID and notarized by Apple.
- **Windows and Linux** builds aren't code-signed yet.
- **Updates** are verified against Frida's update signing key (built into the app) before anything is installed,
  and only installed when you click **Install and restart**. Updates come only from published GitHub releases.
- You can always [build Frida yourself](development.md#building-locally) from source. Self-built copies don't update
  themselves.

## TLS

Certificates are verified by default. Per collection, folder or request you can add CA certificates (PEM files, on
top of the system's) or, for local development, switch verification off. Client certificates (mutual TLS) aren't
supported yet.
