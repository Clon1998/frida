# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for security problems. Report them privately through GitHub:
[Report a vulnerability](https://github.com/Clon1998/frida/security/advisories/new) (the **Security** tab of the
repository). Include what you found, how to reproduce it, and which version and OS you used.

Frida is maintained by one person in their spare time, so there is no guaranteed response time; expect an answer
within a couple of weeks. Fixes go into a new release, and the advisory is published once that release is out.

## Supported versions

Only the latest release gets security fixes. Frida updates itself, so please stay on the latest version.

## Scope

Of particular interest: secret values leaking into files, logs, history or exports; escaping the script sandbox;
the HTML response preview reaching the app; the update mechanism. How Frida handles these is described in
[docs/security.md](docs/security.md).
