# File format

Frida stores everything a team shares as plain YAML, one file per request, so collections live happily in git.
This page is a practical overview; the full specification is [PLAN.md §4](../PLAN.md#4-on-disk-format).

## Layout

```
my-workspace/
├── frida-workspace.yaml          # name + list of collection folders
├── environments/                 # global environments
│   └── dev.yaml
└── collections/
    └── orders-api/
        ├── collection.yaml       # id, name, shared headers/auth/vars/scripts/settings
        ├── environments/         # this collection's environments
        │   ├── dev.yaml
        │   └── prod.yaml
        ├── get-health.yaml       # a request
        └── orders/
            ├── folder.yaml       # folder name, order and shared settings
            ├── get-order.yaml
            └── create-order.yaml
```

- A collection can live outside the workspace folder: `frida-workspace.yaml` lists collections by relative or
  absolute path, so a collection can sit in the repo of the API it tests.
- File names are slugs of the request or folder name (`Get order (v2)` → `get-order-v2.yaml`). The display name
  and the position in the tree come from `name` and `seq` inside the file, so renaming a request in the app renames
  its file too, while reordering only touches `seq`.
- Every file starts with `version: 1`. A file with a newer version is refused with an "update Frida" message
  instead of being misread.
- Writes are atomic (a temporary file, then a rename), so a crash never leaves half a file behind.

## A request

```yaml
version: 1
name: "Get order"
seq: 2
method: GET
url: "{{baseUrl}}/orders/:orderId?expand=items"
params:
  query:
  - name: expand
    value: "items"
    enabled: true
  path:
  - name: orderId
    value: "{{lastOrderId}}"
    description: The order to load      # optional, only written when set
headers:
- name: X-Correlation-Id
  value: "{{$uuid}}"
  enabled: true
body:
  mode: none        # none | json | xml | text | form-urlencoded | multipart | binary
  json: ""
  xml: ""
  text: ""
  formUrlEncoded: []
  multipart: []
  binaryPath: ""
auth:
  mode: inherit     # inherit | none | custom (with a list of schemes)
vars: []
scripts:
  pre: ""
  post: |
    fri.setVar("lastOrderId", res.body.id);
tests: |
  test("status is 200", () => expect(res.status).to.equal(200));
settings:
  timeoutMs: null   # null = inherit
docs: ""
```

Collections and folders use the same fields for `headers`, `auth`, `vars`, `scripts`, `tests`, `settings` and
`docs`; everything below them inherits those.

## An environment

```yaml
version: 1
name: dev
seq: 2              # optional: position in the environment list
color: green        # optional: highlight in the UI
variables:
- name: baseUrl
  value: https://orders.dev.example.com
- name: clientSecret
  secret: true      # the value is in the OS keychain, never here
- name: apiKey
  secret: true
  source:
    type: aws-secrets-manager
    secretId: eip/dev/orders-api
    jsonKey: apiKey   # optional: one key of a JSON secret
    region: eu-central-1
- name: legacyHost
  value: old.example.com
  enabled: false    # only written when disabled
```

## What is *not* in these files

Frida keeps machine-specific and sensitive data out of the repo, in its app data folder:

| Data | Where |
|---|---|
| Secret values | OS keychain (or `secrets.enc` when no keychain exists) |
| Recent and pinned workspaces, active global environment, preferences | `settings.json` |
| Open tabs, active environment, AWS profile per environment, expanded tree nodes | `state/<collection id>.json` |
| Request history (secrets masked) | `history.sqlite` |
| Logs (rotated daily, 7 kept) | `logs/` |
| Runtime variables, cookies, OAuth2 tokens, fetched AWS secrets | memory only |

The app data folder is `~/Library/Application Support/io.github.clon1998.frida/` on macOS,
`%APPDATA%\io.github.clon1998.frida\` on Windows and `~/.local/share/io.github.clon1998.frida/` on Linux.

Secret values are stored in the keychain under the scope of the collection id (or a hash of the workspace path for
global environments), the environment name and the variable name. That is why a collection's `id` should not be
changed by hand: its secrets would no longer be found.

## Compatibility rules

- New fields are added so old files stay valid and unchanged: optional fields are only written when set
  (`description`, `color`, `seq`, `enabled: false`).
- Unknown fields are ignored when reading. They are not preserved when Frida saves the file.
- Breaking changes would bump `version` and come with a migration.
