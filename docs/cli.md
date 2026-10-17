# frida-run: collections on the command line

`frida-run` runs the requests of a collection with their scripts and tests, like the app's collection runner, and
reports the results in the terminal, as JUnit XML or as JSON. It needs no keychain and no window, so it runs on CI
machines.

## Install

- **From a release:** download `frida-run-<target>.tar.gz` (`.zip` on Windows) from
  [GitHub Releases](https://github.com/Clon1998/frida/releases). Targets: `aarch64-apple-darwin`,
  `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`.
- **From source:** `cargo install --path crates/frida-cli` (installs `frida-run` into `~/.cargo/bin`).

The binary is called `frida-run`, not `frida`, so it doesn't clash with the frida instrumentation toolkit.

## Usage

```sh
frida-run <collection folder> [options]
```

```sh
# Everything, with the collection's "dev" environment
frida-run collections/orders-api --env dev

# One folder, against a local server, stop at the first failure
frida-run collections/orders-api --folder orders/admin --var baseUrl=http://localhost:8080 --bail

# JUnit for CI
frida-run collections/orders-api --env ci --reporter junit --output frida-report.xml
```

| Option | |
|---|---|
| `--folder <path>` | Only this folder (its path in the collection, e.g. `orders/admin`) |
| `-e, --env <name>` | The collection environment |
| `--global-env <name>` | The global environment of the workspace |
| `--workspace <dir>` | Where the global environments are. Default: the nearest folder above the collection with a `frida-workspace.yaml` |
| `--var name=value` | Sets a variable above every other layer (repeatable) |
| `--secret name=value` | The value of a secret variable (repeatable; prefer `FRIDA_SECRET_*`, see below) |
| `--aws-profile <name>` | AWS profile for Secrets Manager values. Default: `AWS_PROFILE`, else `default` |
| `--delay <ms>` | Pause between requests |
| `--bail` | Stop at the first request that fails or can't be sent |
| `--reporter cli\|junit\|json` | Output format (default `cli`) |
| `-o, --output <file>` | Write the JUnit/JSON report to a file instead of stdout |
| `-k, --insecure` | Don't verify TLS certificates (like `curl -k`) |
| `-v, --verbose` | Also print the scripts' `console.log` output (to stderr) |

The run is the same as in the app: requests in tree order, pre-request scripts, variables, auth (OAuth2 tokens are
fetched as usual), post-response scripts and tests, and `fri.setNextRequest`, `fri.runner.skipRequest()` and
`fri.runner.stopExecution()`. `req.getExecutionMode()` is `"runner"`. Files are only read; `fri.setEnvVar(..., {
persist: true })` is the one exception, as in the app.

### Output

With `--reporter cli` everything goes to stdout. With `junit` or `json` and no `--output`, the report goes to stdout
and the progress lines to stderr, so `frida-run … --reporter junit > report.xml` works.

```
Frida · Orders API · dev (3 requests)
  ✓ POST    Login          200  41 ms  1/1 tests
  ✗ GET     List orders    200  12 ms  1/2 tests
      ✗ has two orders: expected [] to have length 2
  - DELETE  Delete order   skipped

3 requests: 1 passed, 1 failed, 1 skipped (96 ms)
```

JUnit layout: one `<testsuite>` per folder, one `<testcase>` per test (`name` is "request › test"). A request
without tests is one testcase; a request that couldn't be sent is an `<error>`, a failing script a `<failure>`, a
skipped request `<skipped/>`.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Everything passed (skipped requests count as passed) |
| 1 | A test or a post-response script failed |
| 2 | A request couldn't be sent, the run stopped abnormally (`setNextRequest` loop, unknown request name, Ctrl+C), or the arguments are wrong |

## Secrets

There is no keychain on a CI machine, so secret variables get their values from:

1. `--secret name=value`;
2. the environment variable `FRIDA_SECRET_<name>`, exactly as the variable is named (`FRIDA_SECRET_clientSecret`)
   or in upper case with other characters as `_` (`FRIDA_SECRET_CLIENTSECRET`, `api-key` → `FRIDA_SECRET_API_KEY`);
3. AWS Secrets Manager, for variables with an AWS source, using `--aws-profile`, `AWS_PROFILE` or `default`.
   Credentials from `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` (or the CI's OIDC role) work too. A value from 1 or
   2 wins over the AWS source.

A secret without a value is a warning, not an error: the request is sent with `{{name}}` unresolved.

Secret values never appear in the output: the terminal lines, the JUnit and JSON reports and the script output show
`***` instead, also for OAuth2 tokens and other auth values. Prefer `FRIDA_SECRET_*` over `--secret`: command line
arguments can end up in the shell history and process list.

The authorization code grant needs a browser sign-in; `frida-run` prints the URL to open, which only helps for
local runs. In CI, use client credentials or a token from a secret.

## GitHub Actions

```yaml
- name: API tests
  run: |
    curl -sSL https://github.com/Clon1998/frida/releases/latest/download/frida-run-x86_64-unknown-linux-gnu.tar.gz | tar xz
    ./frida-run collections/orders-api --env ci --reporter junit --output frida-report.xml
  env:
    FRIDA_SECRET_clientSecret: ${{ secrets.ORDERS_CLIENT_SECRET }}

- name: Show the test report
  if: always()
  uses: mikepenz/action-junit-report@v6
  with:
    report_paths: frida-report.xml
```
