# FAQ

## Is Frida free? Can my company use it?

Yes. Frida is open source under the [MIT license](../LICENSE): free for personal and commercial use, including
inside companies, with no account and no paid tier. Like all MIT software, it comes without warranty.

## What are the system requirements?

| | |
|---|---|
| **macOS** | Apple silicon or Intel, macOS 11 (Big Sur) or newer. Signed and notarized. |
| **Windows** | Windows 10 or 11, 64-bit (x64). Uses Microsoft Edge WebView2, which Windows 11 includes; the installer fetches it on Windows 10 if needed. |
| **Linux** | 64-bit (x86_64) with WebKitGTK 4.1, e.g. Ubuntu 22.04, Debian 12, Fedora 39 or newer. `.deb`, `.rpm` or AppImage. |
| **git** | Optional, for the Git panel: your installed `git`. |

There are no ARM builds for Windows or Linux yet. `frida-run` (the CLI) is a single binary for the same platforms.

## What doesn't Frida do?

Frida is a focused HTTP client. Not supported (yet):

- **Protocols**: WebSockets, gRPC, Server-Sent Events. GraphQL works as a plain JSON `POST`, but there is no
  GraphQL-specific UI (schema, autocompletion).
- **Auth**: Digest, NTLM, AWS Signature V4 and client certificates (mutual TLS).
- **Import**: Postman collections (see [Coming from Postman](migrating.md#from-postman) for workarounds), OpenAPI
  from a URL (download the file first).
- **Proxy**: PAC files. Enter such a proxy manually.
- **Git**: merge conflict resolution in the app. Use your usual git tool.
- **Collaboration**: no cloud workspace, comments or real-time sharing. Teams share through git.
- **Other**: mock servers, API documentation publishing, load testing.
- **Builds**: Windows and Linux builds aren't code-signed; no ARM builds for Windows and Linux.

## Is it stable? Will my files keep working?

Frida is at version 0.x: young, and features are still being added. The file format is
versioned (`version: 1` in every file) and changes are additive, so existing files stay valid. A breaking change
would bump the version and come with a migration; a file from a newer format is refused with an "update Frida"
message instead of being misread. What changed in each version is in the [Changelog](CHANGELOG.md).

## Is my data sent anywhere?

No, except what you ask for: your requests, OAuth2 token calls, AWS secrets you configured, git push/pull, and a
daily update check you can switch off. No telemetry, no analytics, no account. Details:
[Security & privacy](security.md).

## How do teams share collections?

Through git: a collection is a folder of YAML files, best kept in the repository of the API it tests. Secret values
stay on each machine (or come from AWS). See [Using Frida in a team](teams.md).

## Can I run my collections on CI?

Yes, with `frida-run`: JUnit or JSON reports, exit codes for pass/fail, secrets from environment variables. See
[frida-run](cli.md).

## Where do I report a bug or ask for a feature?

In [GitHub Issues](https://github.com/Clon1998/frida/issues). For a bug, the version (Settings → Updates), your OS
and the steps to reproduce help most. Logs are in the `logs/` folder of the app data folder
([where that is](security.md#what-frida-stores-and-where)); they never contain secret values, but have a look
before attaching them. Security issues: please report them privately, see [SECURITY.md](../SECURITY.md).

## Why "Frida"?

*Fast Request Inspector for Developer APIs*. The logo is a Westie, traced from a one-line drawing. The binary is called
`frida-http` and the CLI `frida-run`, so they don't clash with the [Frida](https://frida.re) instrumentation
toolkit.
