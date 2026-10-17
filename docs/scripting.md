# Scripting & tests

Frida runs JavaScript before and after a request. Scripts use `req` (the request), `res` (the
response), `fri` (variables, environments and utilities), `test` and `expect`. This page lists
exactly what is supported.

**Coming from Bruno?** `bru` is an alias for `fri`, so scripts written for
[Bruno](https://docs.usebruno.com/testing/script/javascript-reference) run unchanged as far as
the functions below are supported (`bru.setVar(...)` and `fri.setVar(...)` are the same call).

## Where scripts live and when they run

Scripts are saved in the YAML files (`scripts.pre`, `scripts.post`, `tests`) of the collection,
each folder and each request, and edited in their **Script** and **Tests** tabs. The **Examples**
menu of the script and test editors inserts ready-made snippets (log in and store a token, check a status, ...).

On every send:

1. **Pre-request scripts**: collection, then folders (outermost first), then the request.
   They see the request *before* `{{variables}}` are resolved (like Bruno), so they can change
   the URL, method, headers, body and timeout, and set variables that the request then uses.
2. Variables are resolved, auth is applied, the request is sent.
3. **Post-response scripts**: collection, folders, request.
4. **Tests**: collection, folders, request.

Every script runs in a **fresh sandbox**, so plain JS variables don't carry over from one script
to the next. Use `fri.setVar` for that.

- A pre-request script that throws stops the send. The error names the script and line, e.g.
  `folder "Orders", pre-request script (line 3): token is not defined`.
- Errors in post-response scripts and tests are shown in the response's **Tests** tab. The
  response is still shown.
- `console.log/info/warn/error/debug` output goes to the **console** (Ctrl/Cmd+J) as it
  happens. Secret values are shown as `***` there.

## Sandbox and limits

Scripts run in QuickJS (ES2023, `async`/`await` and top-level `await` included):

- **10 seconds** per script, including time spent awaiting `fri.sendRequest` or `fri.sleep`.
  Endless loops are stopped.
- **128 MB** of memory per script.
- **No file system, network, `require` or Node APIs.** The only way out is `fri.sendRequest`.

## `req`

| Function | Notes |
|---|---|
| `req.getUrl()`, `req.setUrl(url)` | The URL as typed (with `{{vars}}` in pre-request scripts). |
| `req.getMethod()`, `req.setMethod(method)` | |
| `req.getHeader(name)`, `req.getHeaders()` | Includes headers inherited from folders and the collection. Names are case-insensitive. |
| `req.setHeader(name, value)`, `req.setHeaders(obj)` | |
| `req.deleteHeader(name)`, `req.deleteHeaders(names)` | Also removes inherited headers. |
| `req.getBody({ raw? })`, `req.setBody(body)` | JSON bodies are objects (`raw: true` gives the string), form bodies are objects of fields, text and XML are strings. Setting an object on a JSON request writes formatted JSON. |
| `req.getTimeout()`, `req.setTimeout(ms)` | |
| `req.getName()`, `req.getAuthMode()` | Auth mode: `inherit`, `none`, `basic`, `bearer`, `apikey`, `oauth2`. |
| `req.getHost()`, `req.getPath()`, `req.getQueryString()` | |
| `req.getExecutionMode()` | `"runner"` in a collection run, otherwise `"standalone"`. |
| `req.url`, `req.method`, `req.headers`, `req.body`, `req.timeout` | Property forms of the above. |

In post-response scripts and tests, `req` is the request **as sent** (variables resolved).

## `res` (post-response scripts and tests)

| | Notes |
|---|---|
| `res.status`, `res.statusText`, `res.headers`, `res.body`, `res.responseTime`, `res.url` | `headers` has lower-case names. `body` is parsed JSON when the body is JSON, otherwise a string. |
| `res.getStatus()`, `res.getStatusText()`, `res.getHeader(name)`, `res.getHeaders()`, `res.getBody()`, `res.getResponseTime()`, `res.getUrl()`, `res.getSize()` | |
| `res.setBody(body)` | Changes the body for later scripts only. |
| `res("data.items[0].id")` | Reads a path from the body. |

## `fri`

**Runtime variables** live in memory for the app session (per collection). They are the
highest-priority variable layer, so `{{name}}` resolves to them. Values may be objects.
The variables inspector shows them; **Collection settings → Vars → Clear runtime variables**
clears them (and the in-memory environment changes below).

`fri.getVar(name)`, `fri.setVar(name, value)`, `fri.hasVar(name)`, `fri.deleteVar(name)`,
`fri.getAllVars()`, `fri.deleteAllVars()`

**Environments.** `fri.setEnvVar(name, value)` changes the active collection environment **for
this session only** (the file is untouched). Pass `{ persist: true }` to also write it to the
environment file. Secret variables are never written to files.

`fri.getEnvName()`, `fri.getEnvVar(name)`, `fri.hasEnvVar(name)`, `fri.setEnvVar(name, value, { persist? })`,
`fri.deleteEnvVar(name)` (also from the file), `fri.getAllEnvVars()`

The same for the global environment: `fri.getGlobalEnvName()`, `fri.getGlobalEnvVar`,
`fri.hasGlobalEnvVar`, `fri.setGlobalEnvVar(name, value, { persist? })`, `fri.deleteGlobalEnvVar`,
`fri.getAllGlobalEnvVars()`

> Difference to Bruno: Bruno writes `bru.setEnvVar` to disk right away. Frida keeps it in memory
> unless you ask for `{ persist: true }`, so scripts don't create git changes by accident.

**Other variables (read-only):** `fri.getCollectionName()`, `fri.getCollectionVar(name)`,
`fri.hasCollectionVar(name)`, `fri.getFolderVar(name)` (nearest folder wins),
`fri.getRequestVar(name)`, `fri.getProcessEnv(name)` (the OS environment).

**Utilities**

| | Notes |
|---|---|
| `await fri.sendRequest(options)` or `fri.sendRequest(options, (err, res) => ...)` | axios-style options: `method`, `url`, `headers`, `data` (objects are sent as JSON, `URLSearchParams` as a form), `timeout`. The response has `status`, `statusText`, `headers`, `data` (parsed JSON when possible) and `duration`. Like axios, statuses outside 2xx reject, with `error.response`. Uses the collection's TLS settings and cookie jar. |
| `await fri.sleep(ms)` | |
| `fri.interpolate(text)` | Resolves `{{variables}}` (and `{{$dynamic}}` ones) with the current values. Objects are interpolated as JSON. |
| `fri.setNextRequest(name)`, `fri.runner.setNextRequest(name)` | In a collection run: continue with the request called `name` (`null` ends the run). No effect on a single send. |
| `fri.runner.stopExecution()` | In a collection run: end the run after this request. |
| `fri.runner.skipRequest()` | In a pre-request script: the request is not sent (shown as *skipped* in a run). |
| `fri.getTestResults()` | The tests so far in this script. |
| `fri.isSafeMode()` | Always `true`: Frida scripts are always sandboxed. |

## Tests

```js
test("status is 200", () => {
  expect(res.status).to.equal(200);
});

test("returns the order", async () => {
  const body = res.getBody();
  expect(body).to.have.property("id").that.is.a("number");
  expect(body.items).to.have.lengthOf(2);
});
```

`expect(value, message?)` supports this subset of [chai](https://www.chaijs.com/api/bdd/):

- Language chains (no effect): `to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`, `have`,
  `with`, `at`, `of`, `same`, `does`, `still`, `also`. Flags: `not`, `deep`.
- `equal` (`equals`, `eq`), `eql` (deep), `a`/`an(type)` (`"string"`, `"number"`, `"object"`,
  `"array"`, `"null"`, ...), `instanceOf`, `property(name[, value])` (supports `"a.b[0]"`
  paths; later assertions apply to the property's value), `include` (`includes`, `contain`,
  `contains`; strings, arrays, object subsets), `above`/`gt`/`greaterThan`,
  `below`/`lt`/`lessThan`, `least`/`gte`, `most`/`lte`, `within(lo, hi)`,
  `lengthOf`/`length(n)`, `match(regex)`, `oneOf(list)`, `keys(...)`, `satisfy(fn)`.
- Properties: `ok`, `true`, `false`, `null`, `undefined`, `NaN`, `exist`, `empty`.
- Response helpers: `expect(res).to.have.status(200)`, `expect(res).to.have.header(name[, value])`.
- `expect.fail(message)`.

## Helpers

| | |
|---|---|
| `console.log/info/warn/error/debug` | To the console panel. Objects are printed as JSON. |
| `btoa`, `atob` | |
| `crypto.md5/sha1/sha256/sha512(text)` | Hex digest of the UTF-8 text. |
| `crypto.hmac(algorithm, key, data)`, `crypto.hmacSHA256(data, key)`, `crypto.hmacSHA512(data, key)` | Hex HMAC. Algorithms: `md5`, `sha1`, `sha256`, `sha512`. |
| `crypto.randomUUID()` | |
| `URLSearchParams`, `TextEncoder`, `TextDecoder` | Small built-in versions. |
| `dayjs` | [dayjs](https://day.js.org/) 1.11 (core, no plugins). |
| `JSON`, `Date`, `Math`, `RegExp`, `Promise`, `Map`, `Set`, typed arrays, ... | Standard JavaScript. |

## Not supported (yet)

Calling these throws an error that says so:

- `require(...)` and npm packages (`axios`, `lodash`, `chai`, `crypto-js`, ...). Use the helpers above.
- `fri.setCollectionVar`, `fri.deleteCollectionVar`, `fri.deleteAllCollectionVars`,
  `fri.deleteAllEnvVars`, `fri.deleteAllGlobalEnvVars`
- `fri.cookies.*`, `fri.getOauth2CredentialVar`, `fri.resetOauth2Credential`, `fri.getSecretVar`
- `fri.runRequest`, `fri.cwd`, `req.setMaxRedirects`, `req.onFail`
- `req.headerList` / `res.headerList` (Postman-style property lists)
- Declarative `assert` blocks. When a Bruno collection is imported, its `assert` rows are converted into
  `test(...)` calls instead.
