# User guide

How Frida is organised and how its main features work. For the file layout on disk see
[file-format.md](file-format.md), for scripts and tests see [scripting.md](scripting.md).

## Concepts

```
Workspace                 frida-workspace.yaml + global environments
└── Collection            collection.yaml + collection environments (often one per API, one per repo)
    ├── Folder            folder.yaml (nested folders allowed)
    │   └── Request       <slug>.yaml
    └── Request
```

- A **workspace** is a folder that lists collections. A collection can live inside the workspace folder or
  anywhere else, for example in the git repo of the API it describes.
- **Collections**, **folders** and **requests** form the sidebar tree. Collections and folders can define headers,
  auth, variables, scripts, tests and settings that everything below them inherits.
- Everything is a plain YAML file. Commit the collection to git and your team gets the same requests, with clean
  diffs. Nothing machine-specific (open tabs, AWS profiles, history, secret values) goes into those files.

Click a request to open it in a tab. Click a folder, or the gear next to a collection's name, to open its settings
as a tab. Each collection has its own set of tabs, which come back the next time you open it.

## Sending requests

- Pick a method (custom ones such as `PROPFIND` work), type the URL and press **Cmd/Ctrl+Enter** or Enter in the
  URL bar.
- Query params and the URL stay in sync: edit either one. `:name` segments in the URL become path params.
- Body modes: JSON, XML, text, form-urlencoded, multipart (text fields and files) and a binary file.
- Tables (params, headers, form fields, variables) have a **bulk edit** mode (`name: value` per line, `//` at the
  start disables a row), rows can be dragged into order and get an optional description.
- **Settings** per request, folder or collection: timeout (inherited, 30 s by default), TLS verification and extra
  CA certificates (PEM files, added to the system's).
- A slow request can be cancelled. Redirects are followed (up to 10).

The response viewer shows status, time and size, and has tabs for the body (pretty or raw, with search and a
JSONPath filter), headers, cookies and test results. Images are previewed. HTML is previewed in a sandboxed frame
with scripts disabled. **Save** writes the body to a file.

The **console** (Cmd/Ctrl+J) logs every send of the session as it went over the wire: the resolved request, all
headers, the response and any script `console.log` output. Secret values are shown as `***`.

## Environments and variables

Write `{{name}}` anywhere: URL, params, headers, bodies, auth fields. While typing `{{`, the editor suggests the
variables in scope. Hovering a variable shows its value and where it comes from; unknown ones are underlined in red.

### Where a value comes from

When the same name is defined in several places, the highest layer wins:

| # | Layer | Where it is edited |
|---|---|---|
| 1 | Runtime variables | Set by scripts (`fri.setVar`), in memory until the app restarts |
| 2 | Request variables | The request's **Vars** tab |
| 3 | Folder variables | The folder's **Vars** tab (nearest folder first) |
| 4 | Collection environment | The active collection environment |
| 5 | Collection variables | The collection's **Vars** tab |
| 6 | Global environment | The active global (workspace) environment |

The **variables inspector** (the drawer on the right of a request) lists every variable the request uses, its
resolved value, the layer it came from and the values it shadowed. "Edit" jumps to the defining layer.

Some names are always available:

- `{{process.env.NAME}}` reads an environment variable of the operating system.
- `{{$uuid}}`, `{{$timestamp}}` (unix seconds), `{{$isoTimestamp}}`, `{{$randomInt}}`, `{{$randomEmail}}`,
  `{{$randomAlphaNumeric}}`: generated fresh on every send.

Values can refer to other variables (`{{baseUrl}}/v2`). Frida follows such chains up to 10 levels deep and reports
a cycle instead of looping. A variable that can't be resolved stays as `{{name}}` in the request and shows a warning;
the request can still be sent.

### Environments

Each workspace has **global** environments and each collection has its own. Choose the active ones with the
environment button in the breadcrumb bar (or **Cmd/Ctrl+E**); the selection is per machine. The environment editor
opens as a tab: add variables, mark them as secret, give an environment a color (prod in red helps), reorder,
duplicate, copy or move environments between the workspace and collections.

### Secrets

Mark a variable as **secret** and its value goes to the operating system's keychain (macOS Keychain, Windows
Credential Manager, Linux Secret Service). The environment file only says `secret: true`. Secret values are
masked in the inspector, the console, history and curl exports.

On Linux systems without a Secret Service, Frida falls back to an encrypted file in its data folder, unlocked with
a password you choose.

A secret can also come from **AWS**. Pick the source in the environment editor's Secrets tab:

- **Secrets Manager**: a secret id or ARN, optionally one key of a JSON secret.
- **Cognito client secret**: a user pool id and app client id; Frida reads the client's secret, e.g. for OAuth2
  client credentials with the Cognito preset. The region comes from the pool id.
- **API Gateway key**: an API key id; Frida reads the key's value, e.g. for an `x-api-key` header.

Frida asks which AWS profile to use for an environment the first time it needs one; the choice stays on your
machine, because profile names differ between people. A secret can instead name an **account alias** (e.g.
`shared-services`) when it lives in another AWS account than the environment's other secrets. The alias is saved in
the file for everyone; each person picks the profile for it once (Frida asks, and Settings → AWS → Accounts lists
them). Values are cached in memory for 15 minutes (Settings → AWS). If an SSO session has expired, the error shows
the `aws sso login --profile …` command to run; if access is denied, it names the IAM permission that's missing.

## Auth

Set auth on the collection, a folder or a request. **Inherit** (the default) takes it from the nearest folder, then
the collection.

- **Basic**, **Bearer** and **API key** (header or query).
- **OAuth2**: client credentials, password, and authorization code with PKCE. For the authorization code grant,
  Frida opens your browser and waits for the redirect on `127.0.0.1`. There is a **Cognito** preset that fills the
  URLs from the user pool domain and region.
- Schemes can be combined, for example OAuth2 plus an API key header.

Tokens are cached in memory and refreshed before they expire. The token viewer shows the decoded JWT claims and
the expiry, and has **Get new token** and **Clear**.

## Scripts and tests

Collections, folders and requests can have pre-request scripts, post-response scripts and tests in JavaScript.
The API is compatible with Bruno's (`bru` works as an alias of `fri`). The **Examples** menu in the script editors
inserts common snippets. Full reference: [scripting.md](scripting.md).

## Collection runner

**Run collection…** or **Run folder…** in the context menu (or on the collection/folder tab) opens a Runner tab.
It sends the saved requests in tree order, one after another, with the active environments and one shared session,
so a login request can set a token that later requests use. Results arrive as each request finishes; click one to
see its response. Scripts can skip requests, jump to another one (`fri.setNextRequest`) or stop the run.

## Import and export

- **OpenAPI 3.0/3.1** (YAML or JSON file): **Import OpenAPI…** in the sidebar. Folders come from the URL paths or
  from the tags; you see a summary before anything is written.
- **Bruno** collections, both the `.bru` and the YAML format: **Import Bruno collection…**. Assertions become tests,
  secrets become empty secret variables you fill in afterwards. The Bruno folder is only read.
- **curl**: paste a curl command into the URL bar, or use **New request from curl…**. **Copy as curl** in a
  request's context menu exports one, with variables resolved or kept, for bash or PowerShell. Secrets are `***`
  unless you explicitly include them.
- **Frida archives**: **Export…** on a workspace, collection, folder or request writes a `.zip` of the YAML files.
  **Import Frida archive…** reads one. Archives never contain secret values.

## Git

The **Git** tab of the sidebar shows the branch, ahead/behind counts and changed files of the collection's
repository. You can stage and unstage files, commit, push, pull (fast-forward only) and switch branches.
**Changes** opens a tab with a side-by-side or unified diff of each file. Frida uses your installed `git`, so
your SSH keys and credential helpers just work. It never runs destructive commands (reset, force push, clean);
merge conflicts are resolved in your usual git tool.

When files change on disk (a `git pull`, another editor), the tree reloads. If a tab with unsaved edits changed
on disk, Frida asks whether to keep your version or reload.

## Proxy

**Settings → Network** decides how Frida reaches servers:

- **System settings** (default): the operating system's proxy and the `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY`
  variables. Automatic configuration scripts (PAC files) aren't read; enter such a proxy under Manual.
- **No proxy**: always directly.
- **Manual**: a proxy URL (`http://`, `https://` or `socks5://`), an optional username and password (the password
  is stored in the keychain), and hosts that skip the proxy: names (`localhost`), `*.internal` for a domain and
  its subdomains, IP addresses and ranges like `10.0.0.0/8`.

The proxy applies to requests, OAuth2 token requests, `fri.sendRequest` in scripts and AWS calls.
For APIs on your own machine or network, open the collection's settings and tick **Don't use the proxy** (this
applies on your computer only). When a request went through a manual proxy, the console and history say so. If
the proxy can't be reached or rejects the login (407), the error says which and where to fix it.

## History

Every send is stored locally, per collection, in the **History** tab of the sidebar (500 entries by default,
Settings → History). Secret values and auth credentials are masked before anything is written. Open an entry to see
it, or restore it as a new request.

## Keyboard shortcuts

`Mod` is Cmd on macOS and Ctrl elsewhere. **Mod+/** shows this list in the app.

| Keys | Action |
|---|---|
| Mod+Enter | Send the request |
| Mod+S | Save the tab (or the environment being edited) |
| Mod+W | Close the tab |
| Mod+T | New request in the collection |
| Mod+K | Quick open: search requests |
| Mod+E | Switch environment |
| Mod+J | Show or hide the console |
| Mod+F | Search in the editor that has focus |
| Mod+, | Settings |
| Mod+/ | Show the shortcuts |

On macOS, Ctrl+E and Ctrl+K keep their usual text-editing meaning.

## Settings

The gear in the sidebar footer (or **Mod+,**) opens per-machine settings: theme, accent color, UI and editor size,
the git program, AWS config files and cache time, the history size and updates. Changes apply immediately.

## Updates

Frida looks for a new version when it starts, at most once a day (switch it off in **Settings → Updates**, where
**Check for updates** also checks right away). When there is one, a notice shows up; **Details…** shows the release
notes and **Install and restart**. If tabs have unsaved changes, Frida offers to save them first. The download can be
cancelled; nothing is installed while a request or a collection run is in progress. Every update is verified with
Frida's signing key before it is installed. Builds you compiled yourself don't update themselves.
