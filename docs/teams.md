# Using Frida in a team

How a team shares collections, environments and secrets with Frida, and runs the same requests on CI. There is
nothing to set up centrally: git is the sharing mechanism, the OS keychain (or AWS) holds the secrets.

## Where collections live

A collection is a folder of YAML files, and it can live in any git repository. Two setups work well:

- **Next to the API it tests** (recommended): a `collections/orders-api/` folder in the orders service's
  repository. A change to the API and its requests go into the same pull request, and CI can run them against the
  build.
- **One shared repository** with all collections, when the requests span several services or the API repos are
  owned by other teams.

Each developer keeps their own **workspace**, a folder with a `frida-workspace.yaml` that lists the collections
they work with (**Add existing collection…** picks a folder anywhere). The workspace can be committed too, but it
doesn't have to be: it is just a list of paths.

## Environments

Commit an environment per stage (`local`, `dev`, `staging`, `prod`) in the collection's `environments/` folder.
Everything that isn't sensitive goes in there as a value: base URLs, tenant ids, OAuth2 URLs and client ids.
Give production a color (red, say), so it is obvious when it's active.

Which environment is active is stored per machine, not in the files, so switching never shows up as a change in
git.

Personal values (your own test user, a local port) belong in a **global environment** of your own workspace, or in
a local-only environment you don't commit. Collection environments win over global ones, see
[where a value comes from](user-guide.md#where-a-value-comes-from).

## Secrets

Mark sensitive variables as **secret**. The file then contains only the name and `secret: true`:

```yaml
variables:
- name: clientSecret
  secret: true
```

Each person enters the value once on their machine; it goes into their OS keychain. Or point the variable at AWS,
so nobody has to copy secrets around:

```yaml
- name: clientSecret
  secret: true
  source:
    type: aws-secrets-manager
    secretId: orders/dev/client
    jsonKey: clientSecret
```

Frida reads the value with each person's own AWS profile and their own permissions. Profile names differ between
people, so they aren't in the file: Frida asks once per environment. Secrets in another AWS account use an account
alias that each person maps to a profile once. Details: [Secrets](user-guide.md#secrets).

## Reviewing changes

Requests are one file each, written in a stable order, so a pull request shows exactly what changed: a new header,
a changed URL, an edited test. Renaming a request renames its file; reordering only changes `seq`.

In the app, the **Git** tab shows the changed files with a side-by-side diff, and can stage, commit, push, pull
and switch branches. Merge conflicts are resolved in your usual git tool; the files are plain YAML, so that is
rarely hard.

## Running on CI

`frida-run` runs a collection with its scripts and tests, without the app or a keychain. Secrets come from
environment variables named `FRIDA_SECRET_<name>` (or from AWS), and are masked in all output.

```yaml
- name: API tests
  run: |
    curl -sSL https://github.com/Clon1998/frida/releases/latest/download/frida-run-x86_64-unknown-linux-gnu.tar.gz | tar xz
    ./frida-run collections/orders-api --env ci --reporter junit --output frida-report.xml
  env:
    FRIDA_SECRET_clientSecret: ${{ secrets.ORDERS_CLIENT_SECRET }}
```

JUnit and JSON reports, exit codes and every option: [frida-run](cli.md).

## Rolling it out

1. Create the collection in the API's repository, with `local` and `dev` environments and secrets marked as such.
2. Commit and open a pull request; colleagues review the YAML like any other change.
3. Everyone installs Frida, **Add existing collection…** → the folder in their clone, enters the secret values
   (or picks an AWS profile).
4. Add a `frida-run` step to the API's CI pipeline.

Keep everyone on a recent version; Frida offers updates itself (and asks before installing). Files carry a format
`version`, so if a colleague's newer Frida ever writes a newer format, an older Frida refuses the file with an
"update Frida" message instead of misreading it.
