# Coming from Postman or Bruno

What carries over, what doesn't, and how the concepts map.

## From Bruno

Frida's concepts come from Bruno, so most things carry over directly.

1. **Import Bruno collection…** in the sidebar, then pick the Bruno collection folder. Both the `.bru` format and
   Bruno's YAML format are read; the Bruno folder itself is only read, never changed.
2. You get a summary first. Requests, folders, headers, auth, variables, environments, scripts, tests and docs are
   converted. Bruno's `assert` blocks become tests.
3. Bruno secret variables have no value in the repository, so they arrive as empty **secret** variables. Fill them
   in once in the environment editor.

**Scripts** run as they are: `bru` is an alias for `fri`, so `bru.setVar(...)` works. The import lists any calls
Frida doesn't support yet; [Scripting & tests](scripting.md) has the complete list of what is supported and what
differs (for example, `bru.setEnvVar` changes the environment in memory unless you pass `{ persist: true }`).

| Bruno | Frida |
|---|---|
| Collection (`bruno.json`, `collection.bru`) | Collection (`collection.yaml`) |
| Folder (`folder.bru`) | Folder (`folder.yaml`) |
| Request (`*.bru`) | Request (`<name>.yaml`) |
| Collection environments | Collection environments |
| Global environments | Global environments (per workspace) |
| `bru.*` | `fri.*` (`bru` still works) |

## From Postman

There is **no Postman import yet**. These routes work today:

- **Import the API's OpenAPI description** if it has one (**Import OpenAPI…**, 3.0 or 3.1, YAML or JSON). This is
  usually the quickest way to get every endpoint, with folders by path or by tag.
- **Convert the Postman collection to OpenAPI** with a third-party converter such as
  [`@readme/postman-to-openapi`](https://www.npmjs.com/package/@readme/postman-to-openapi), then import that.
  Scripts and environments don't survive this route, and results vary, so check the import preview.
- **Bring single requests over as curl**: in Postman, open the request's code snippet (`</>`), choose cURL, copy
  it, and paste it into Frida's URL bar (or **New request from curl…**).

Environments are quick to recreate by hand: create one per stage and add the variables. Mark tokens and passwords
as **secret**.

### Concepts

| Postman | Frida |
|---|---|
| Workspace (in the cloud) | Workspace (a local folder) |
| Collection | Collection (a folder of YAML files, shared through git) |
| Folder | Folder |
| Environment | Collection environment, or a global environment of the workspace |
| Globals | Global environment |
| Collection variables | Collection variables (the collection's **Vars** tab) |
| Secret / vault variables | Secret variables (OS keychain or AWS) |
| Collection runner | Collection runner, and `frida-run` on CI (instead of Newman) |
| Sharing via Postman cloud | Sharing via git |

### Scripts

Postman's `pm.*` API isn't supported; scripts need small changes. The common calls:

| Postman | Frida |
|---|---|
| `pm.environment.get("x")` / `.set("x", v)` | `fri.getEnvVar("x")` / `fri.setEnvVar("x", v)` |
| `pm.variables.get("x")` / `.set("x", v)` | `fri.getVar("x")` / `fri.setVar("x", v)` |
| `pm.response.json()` | `res.getBody()` (JSON is already parsed) |
| `pm.response.code` | `res.status` |
| `pm.request.headers.add(...)` | `req.setHeader(name, value)` |
| `pm.sendRequest(...)` | `await fri.sendRequest({ method, url, headers, data })` |
| `pm.test("name", fn)` | `test("name", fn)` |
| `pm.expect(x).to.equal(y)` | `expect(x).to.equal(y)` |
| `postman.setNextRequest("name")` | `fri.setNextRequest("name")` |

`expect` is Chai-style, like Postman's, so most assertions stay as they are.

## What you won't find (yet)

Frida is a focused HTTP client. No WebSockets, GraphQL-specific UI, gRPC or SSE, no mock servers, no API
documentation publishing and no cloud workspace. See [FAQ](faq.md#what-doesnt-frida-do) for the full list.
