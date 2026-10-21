/**
 * An in-memory stand-in for the Rust backend, for the Playwright UI flow tests (PLAN.md M12).
 *
 * Only loaded when the dev server runs with `VITE_E2E=1` (see `main.tsx`), so production builds
 * don't contain it. It answers the Tauri commands the core flows use with plausible behaviour:
 * one workspace with one collection, requests that can be created/renamed/saved/deleted,
 * environments, a send that echoes the request, history, and a git status that lists saved
 * requests as changed. Anything else logs a warning and returns `null`.
 *
 * The real backend is covered by the Rust integration tests; this checks the UI around it.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  AppSettings,
  Collection,
  CollectionState,
  EnvironmentInfo_Serialize,
  Environment_Serialize,
  EnvScope,
  FileChange,
  Folder,
  GitInfo,
  HistorySummary,
  Request,
  ResponseView,
  TokenInfo,
  TreeNode,
  UpdateInfo,
  WorkspaceOverview,
} from "@/bindings";

const WORKSPACE = "/fake/workspace";
const COLLECTION = `${WORKSPACE}/collections/echo-api`;
/** A second collection, only used as a target for environments (its tree is never opened). */
const OTHER_COLLECTION = `${WORKSPACE}/collections/billing`;

/** A folder with every field set, as the real backend sends it. */
const folder = (name: string, seq: number): Folder => ({
  version: 1,
  name,
  seq,
  vars: [],
  headers: [],
  auth: { mode: "inherit" },
  scripts: { pre: "", post: "" },
  tests: "",
  settings: { timeoutMs: null },
  docs: "",
});

const request = (name: string, method: string, url: string, seq: number): Request => ({
  version: 1,
  name,
  seq,
  method,
  url,
  params: { query: [], path: [] },
  headers: [],
  body: { mode: "none" },
  auth: { mode: "inherit" },
  vars: [],
  scripts: { pre: "", post: "" },
  tests: "",
  settings: { timeoutMs: null },
  docs: "",
});

/** Everything the fake keeps; `reset` starts every test from the same state. */
function initialState() {
  const collection: Collection = {
    version: 1,
    id: "c0ffee00-0000-4000-8000-000000000001",
    name: "Echo API",
    vars: [],
    headers: [],
    auth: { mode: "none" },
    scripts: { pre: "", post: "" },
    tests: "",
    settings: { timeoutMs: null, tls: { verify: true, caCertPaths: [] } },
    docs: "",
  };
  const folders = new Map<string, Folder>([["orders", folder("Orders", 1)]]);
  const requests = new Map<string, Request>([
    ["health.yaml", request("Health", "GET", "{{baseUrl}}/health", 2)],
    ["orders/get-order.yaml", request("Get order", "GET", "{{baseUrl}}/orders/42", 1)],
    ["orders/create-order.yaml", request("Create order", "POST", "{{baseUrl}}/orders", 2)],
  ]);
  const environments: Environment_Serialize[] = [
    {
      version: 1,
      name: "local",
      seq: 1,
      color: null,
      variables: [
        {
          name: "baseUrl",
          value: "http://localhost:8080",
          secret: false,
          source: null,
          enabled: true,
        },
      ],
    },
    { version: 1, name: "prod", seq: 2, color: null, variables: [] },
  ];
  return {
    settings: {
      recentWorkspaces: [WORKSPACE],
      pinnedWorkspaces: [],
      activeGlobalEnvironments: {},
      awsProfiles: {},
      // Complete, like the real backend's defaults.
      preferences: {
        theme: "system",
        accent: "neutral",
        uiScale: 100,
        editorFontSize: 13,
        historySize: 500,
        gitPath: null,
        aws: { configFile: null, credentialsFile: null, cacheMinutes: 15 },
        checkForUpdates: true,
        proxy: { mode: "system", url: "", username: "", bypass: "localhost, 127.0.0.1, ::1" },
      },
    } as AppSettings,
    /** The proxy password "in the keychain". */
    proxyPassword: null as string | null,
    /** What `check_for_update` finds (set by a test through `offerUpdate`). */
    update: null as UpdateInfo | null,
    /** How often `install_update` ran (the real one restarts the app). */
    updateInstalls: 0,
    workspaceOpen: false,
    collection,
    folders,
    requests,
    /** The committed version of every request, for the fake git status and diffs. */
    committed: new Map([...requests].map(([path, r]) => [path, structuredClone(r)])),
    staged: new Set<string>(),
    environments,
    globalEnvironments: [] as Environment_Serialize[],
    otherEnvironments: [] as Environment_Serialize[],
    active: { global: null as string | null, collection: null as string | null },
    secrets: new Map<string, string>(),
    history: [] as HistorySummary[],
    /** Full history entries (request and response), by id. */
    historyResponses: new Map<number, ResponseView>(),
    /** Open tabs etc. per collection id, as `save_collection_state` stored them. */
    collectionStates: new Map<string, CollectionState>(),
    branch: "main",
    branches: ["main", "feature/login"],
    /** Running sends and runs that can be cancelled, by request/run id. */
    cancelled: new Set<string>(),
    /** Files written by "Save response" (path → response id). */
    savedBodies: new Map<string, string>(),
    /** The options of the last "save file" dialog (e.g. the suggested file name). */
    lastSaveDialog: null as unknown,
    /** Cached OAuth2 tokens: one from another environment and token URL to start with. */
    oauthTokens: [
      oauthToken("https://login.prod.example.com/oauth2/token", "prod", "other-token-value"),
    ] as TokenInfo[],
    /** What the last "Get new token" sent along (path and the auth as edited). */
    lastTokenFetch: null as unknown,
  };
}

function oauthToken(tokenUrl: string, environment: string | null, accessToken: string): TokenInfo {
  return {
    tokenUrl,
    clientId: "frida",
    scope: "orders/read",
    grant: "client_credentials",
    username: "",
    accessToken,
    tokenType: "Bearer",
    hasRefreshToken: false,
    expiresAt: new Date(Date.now() + 3_600_000).toISOString(),
    expired: false,
    claims: JSON.stringify({ sub: "frida", scope: "orders/read" }, null, 2),
    environment,
    fetchedAt: new Date().toISOString(),
    current: false,
  };
}

/** The token URL of the OAuth2 scheme in `auth`, if it has one. */
function oauthTokenUrl(auth: unknown): string | null {
  const schemes = (auth as { schemes?: { type: string; tokenUrl?: string }[] } | null)?.schemes;
  return schemes?.find((s) => s.type === "oauth2")?.tokenUrl ?? null;
}

/** All tokens, the ones for `auth` marked current and first (like the real backend). */
function tokensFor(auth: unknown): TokenInfo[] {
  const url = oauthTokenUrl(auth);
  const marked = state.oauthTokens.map((t) => ({ ...t, current: t.tokenUrl === url }));
  return [...marked.filter((t) => t.current), ...marked.filter((t) => !t.current)];
}

let state = initialState();

const overview = (): WorkspaceOverview => ({
  dir: WORKSPACE,
  name: "Fake workspace",
  collections: [
    { dir: COLLECTION, id: state.collection.id, name: state.collection.name, error: null },
    {
      dir: OTHER_COLLECTION,
      id: "b111ing0-0000-4000-8000-000000000002",
      name: "Billing",
      error: null,
    },
  ],
});

const slug = (name: string) =>
  name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "request";

const join = (parent: string, name: string) => (parent ? `${parent}/${name}` : name);
const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** A free `<slug>.yaml` (requests) or `<slug>` (folders) in `parent`. */
function freePath(parent: string, name: string, folder: boolean) {
  const taken = (p: string) => state.requests.has(p) || state.folders.has(p);
  for (let n = 1; ; n++) {
    const base = n === 1 ? slug(name) : `${slug(name)}-${n}`;
    const path = join(parent, folder ? base : `${base}.yaml`);
    if (!taken(path)) return path;
  }
}

function tree(parent = ""): TreeNode[] {
  const nodes: TreeNode[] = [];
  for (const [path, folder] of state.folders) {
    if (parentOf(path) === parent)
      nodes.push({
        kind: "folder",
        path,
        name: folder.name,
        seq: folder.seq ?? 0,
        children: tree(path),
      });
  }
  for (const [path, r] of state.requests) {
    if (parentOf(path) === parent)
      nodes.push({ kind: "request", path, name: r.name, seq: r.seq ?? 0, method: r.method });
  }
  return nodes.sort((a, b) =>
    a.kind === "invalid" || b.kind === "invalid"
      ? 0
      : a.seq - b.seq || a.name.localeCompare(b.name),
  );
}

const envList = (scope: EnvScope) =>
  scope.kind === "global"
    ? state.globalEnvironments
    : scope.dir === OTHER_COLLECTION
      ? state.otherEnvironments
      : state.environments;

/** Keychain scope of an environment list, like the real collection id / workspace hash. */
const scopeKey = (scope: EnvScope) =>
  scope.kind === "global" ? "global" : scope.dir === OTHER_COLLECTION ? "billing" : "echo";

/** `name`, else `name 2`, `name 3`, ... (like the real `free_environment_name`). */
function freeEnvName(list: Environment_Serialize[], name: string) {
  let candidate = name;
  for (let n = 2; list.some((e) => e.name === candidate); n++) candidate = `${name} ${n}`;
  return candidate;
}

/** Adds a copy of `env` under a free name, without secret values. */
function addEnv(list: Environment_Serialize[], env: Environment_Serialize) {
  const added = {
    ...structuredClone(env),
    name: freeEnvName(list, env.name),
    seq: 99,
    variables: env.variables.map((v) => (v.secret ? { ...v, value: null } : v)),
  };
  list.push(added);
  return added;
}

const envInfo = (scope: EnvScope): EnvironmentInfo_Serialize[] =>
  envList(scope).map((environment) => ({
    environment,
    setSecrets: environment.variables
      .filter((v) => v.secret && state.secrets.has(`${environment.name}/${v.name}`))
      .map((v) => v.name),
  }));

/** `{{name}}` from the active collection environment, like a (much) simpler resolver. */
function resolve(text: string) {
  const env = state.environments.find((e) => e.name === state.active.collection);
  return text.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (all, name: string) => {
    const v = env?.variables.find((x) => x.name === name && x.enabled !== false);
    return v?.value ?? all;
  });
}

const yamlish = (r: Request | undefined) =>
  r ? `name: ${r.name}\nmethod: ${r.method}\nurl: ${r.url ?? ""}\n` : null;

function gitInfo(): GitInfo {
  const paths = new Set([...state.requests.keys(), ...state.committed.keys()]);
  const files: FileChange[] = [];
  for (const path of [...paths].sort()) {
    const now = yamlish(state.requests.get(path));
    const before = yamlish(state.committed.get(path));
    if (now === before) continue;
    const letter = before === null ? "A" : now === null ? "D" : "M";
    const staged = state.staged.has(path);
    files.push({
      path,
      origPath: null,
      staged: staged ? letter : null,
      unstaged: staged ? null : letter === "A" ? "?" : letter,
      untracked: !staged && letter === "A",
      conflicted: false,
    });
  }
  return {
    kind: "repo",
    status: {
      branch: state.branch,
      upstream: "origin/main",
      ahead: 0,
      behind: 0,
      noCommits: false,
      files,
    },
  };
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** A response that echoes the request as JSON. */
function echo(r: Request, id: string): ResponseView {
  const url = resolve(r.url ?? "");
  const body = JSON.stringify({ method: r.method, url, echo: true }, null, 2);
  return {
    id,
    status: 200,
    statusText: "OK",
    headers: [
      { name: "content-type", value: "application/json", enabled: true },
      { name: "x-request-id", value: id, enabled: true },
    ],
    contentType: "application/json",
    sizeBytes: body.length,
    timeMs: 12,
    finalUrl: url,
    bodyText: body,
    bodyBase64: null,
    truncated: false,
    scripts: { tests: [], errors: [] },
  };
}

/** Echoes the request. URLs containing "slow" take 10 s, unless cancelled. */
async function send(args: { requestId: string; path: string; request: Request }) {
  if ((args.request.url ?? "").includes("slow")) {
    for (let waited = 0; waited < 10_000; waited += 50) {
      if (state.cancelled.has(args.requestId))
        throw { code: "cancelled", message: "The request was cancelled.", hint: null };
      await sleep(50);
    }
  }
  const id = state.history.length + 1;
  const response = echo(args.request, `response-${id}`);
  state.history.unshift({
    id,
    timestamp: new Date().toISOString(),
    collectionId: state.collection.id,
    requestPath: args.path,
    requestName: args.request.name,
    method: args.request.method,
    url: response.finalUrl,
    status: 200,
    timeMs: 12,
    error: null,
  });
  state.historyResponses.set(id, response);
  return response;
}

/** The requests of `folder` in tree order, like the real `plan_run`. */
function plan(folder: string) {
  const items: { path: string; name: string; method: string }[] = [];
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      if (node.kind === "folder") walk(node.children);
      else if (node.kind === "request")
        items.push({ path: node.path, name: node.name, method: node.method });
    }
  };
  walk(tree(folder));
  return items;
}

/** Runs the plan, one request every 400 ms, sending `run-progress` events; stoppable. */
async function runCollection(args: { runId: string; folder: string }) {
  const started = Date.now();
  const summary = (passed: number, stopped: { kind: "cancelled" } | null) => ({
    passed,
    failed: 0,
    errors: 0,
    skipped: 0,
    durationMs: Date.now() - started,
    stopped,
  });
  let passed = 0;
  for (const [step, item] of plan(args.folder).entries()) {
    await sleep(400);
    if (state.cancelled.has(args.runId)) return summary(passed, { kind: "cancelled" });
    const r = state.requests.get(item.path) ?? request(item.name, item.method, "", 0);
    await emit("run-progress", {
      runId: args.runId,
      result: {
        step,
        item,
        outcome: "passed",
        status: 200,
        timeMs: 12,
        testsPassed: 0,
        testsTotal: 0,
        message: null,
      },
      response: echo(r, `run-${step}`),
    });
    passed++;
  }
  return summary(passed, null);
}

/** Just enough curl parsing for the tests: `-X`, `-H`, `--data`, and the URL. */
function parseCurl(command: string) {
  const words =
    command.match(/'[^']*'|"[^"]*"|\S+/g)?.map((w) => w.replace(/^['"]|['"]$/g, "")) ?? [];
  if (words[0] !== "curl")
    throw { code: "invalid_input", message: "not a curl command", hint: null };
  const r = request("New request", "GET", "", 99);
  const headers = [];
  for (let i = 1; i < words.length; i++) {
    const w = words[i] ?? "";
    if (w === "-X" || w === "--request") r.method = (words[++i] ?? "GET").toUpperCase();
    else if (w === "-H" || w === "--header") {
      const [name = "", ...rest] = (words[++i] ?? "").split(":");
      headers.push({ name: name.trim(), value: rest.join(":").trim(), enabled: true });
    } else if (w === "-d" || w === "--data" || w === "--data-raw") {
      r.body = { mode: "text", text: words[++i] ?? "" };
      if (r.method === "GET") r.method = "POST";
    } else if (!w.startsWith("-")) r.url = w;
  }
  r.headers = headers;
  return { request: r, warnings: [] };
}

/** `$.a.b` and `$.a[0]` only: the real filter is RFC 9535 JSONPath in Rust. */
function filterJson(text: string, path: string) {
  if (!path.startsWith("$"))
    throw { code: "invalid_input", message: "invalid JSONPath: must start with $", hint: null };
  let value: unknown = JSON.parse(text);
  for (const part of path.slice(1).match(/\.\w+|\[\d+\]/g) ?? []) {
    const key = part.startsWith(".") ? part.slice(1) : Number(part.slice(1, -1));
    value = (value as Record<string | number, unknown> | undefined)?.[key];
  }
  return JSON.stringify(value ?? null, null, 2);
}

/** What the fake "picked" archive contains: one request. */
const archiveSummary = {
  manifest: {
    format: 1,
    kind: "request",
    name: "Imported request",
    fridaVersion: "e2e",
    exportedAt: "2026-10-16T10:00:00Z",
  },
  files: 1,
  warnings: [],
};

/** One handler per command; `args` are the command's arguments by name. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const handlers: Record<string, (args: any) => unknown> = {
  list_snippets: () => [
    {
      id: "pre.set-header",
      kind: "preRequest",
      title: "Set a header",
      description: "Adds a header to the request.",
      code: 'req.setHeader("X-Request-Id", crypto.randomUUID());',
    },
    {
      id: "tests.status-200",
      kind: "tests",
      title: "Status is 200",
      description: "The most common check.",
      code: 'test("status is 200", () => {\n  expect(res.status).to.equal(200);\n});',
    },
  ],
  get_about_info: () => ({ productName: "Frida", coreVersion: "e2e" }),
  // The check on start finds nothing, so a notice never gets in the way of other tests.
  check_for_update: ({ manual }) => (manual ? state.update : null),
  install_update: async () => {
    await emit("update-progress", { downloaded: 512, total: 1024 });
    state.updateInstalls += 1;
  },
  cancel_update: () => {},
  get_settings: () => state.settings,
  save_preferences: ({ preferences }) => {
    // Like Rust: a manual proxy URL must be http(s) or socks5 with a host.
    const proxy = preferences.proxy;
    if (proxy?.mode === "manual" && proxy.url && !/^(https?|socks5h?):\/\/[^/\s]+/.test(proxy.url))
      throw {
        code: "invalid_input",
        message: `the proxy URL "${proxy.url}" is not valid`,
        hint: null,
      };
    return (state.settings.preferences = preferences);
  },
  forget_workspace: ({ dir }) => {
    state.settings.recentWorkspaces = state.settings.recentWorkspaces?.filter((d) => d !== dir);
    return state.settings;
  },
  pin_workspace: ({ dir, pinned }) => {
    const list = (state.settings.pinnedWorkspaces ?? []).filter((d) => d !== dir);
    state.settings.pinnedWorkspaces = pinned ? [...list, dir] : list;
    return state.settings;
  },
  open_workspace: () => {
    state.workspaceOpen = true;
    return overview();
  },
  reload_workspace: () => overview(),
  close_workspace: () => {
    state.workspaceOpen = false;
    return null;
  },
  get_collection_state: ({ collectionId }) =>
    state.collectionStates.get(collectionId) ?? {
      openTabs: [],
      activeTab: null,
      expandedFolders: [],
      activeEnvironment: null,
      awsProfiles: {},
      bypassProxy: false,
    },
  // Like the real `save_view_state`: the proxy switch is set separately and kept.
  save_collection_state: ({ collectionId, collectionState }) => {
    const bypassProxy = state.collectionStates.get(collectionId)?.bypassProxy ?? false;
    state.collectionStates.set(collectionId, { ...collectionState, bypassProxy });
    return null;
  },
  set_collection_bypass_proxy: ({ bypass }) => {
    const id = state.collection.id;
    const current = state.collectionStates.get(id) ?? {
      openTabs: [],
      activeTab: null,
      expandedFolders: [],
      activeEnvironment: null,
      awsProfiles: {},
    };
    state.collectionStates.set(id, { ...current, bypassProxy: bypass });
    return null;
  },
  set_proxy_password: ({ password }) => {
    state.proxyPassword = password;
    return null;
  },
  has_proxy_password: () => state.proxyPassword !== null,
  read_collection: () => state.collection,
  save_collection: ({ collection }) => {
    state.collection = collection;
    return null;
  },
  get_tree: () => tree(),
  read_request: ({ path }) => {
    const r = state.requests.get(path);
    if (!r) throw { code: "not_found", message: `${path} was not found`, hint: null };
    return r;
  },
  save_request: ({ path, request }) => {
    state.requests.set(path, { ...request, method: request.method.toUpperCase() });
    return null;
  },
  read_folder: ({ path }) => state.folders.get(path),
  save_folder: ({ path, folder }) => {
    state.folders.set(path, folder);
    return null;
  },
  create_request: ({ parent, name }) => {
    const path = freePath(parent, name, false);
    state.requests.set(path, request(name, "GET", "", 99));
    return path;
  },
  create_folder: ({ parent, name }) => {
    const path = freePath(parent, name, true);
    state.folders.set(path, folder(name, 99));
    return path;
  },
  rename_item: ({ path, name }) => {
    const existing = state.requests.get(path);
    if (existing) {
      const next = freePath(parentOf(path), name, false);
      state.requests.delete(path);
      state.requests.set(next, { ...existing, name });
      return next;
    }
    const folder = state.folders.get(path);
    if (folder) folder.name = name;
    return path;
  },
  delete_item: ({ path }) => {
    state.requests.delete(path);
    for (const p of [...state.requests.keys()])
      if (p.startsWith(`${path}/`)) state.requests.delete(p);
    state.folders.delete(path);
    return null;
  },
  list_environments: ({ scope }) => envInfo(scope),
  create_environment: ({ scope, name }) => {
    const env: Environment_Serialize = { version: 1, name, seq: 99, color: null, variables: [] };
    envList(scope).push(env);
    return env;
  },
  save_environment: ({ scope, originalName, environment }) => {
    const list = envList(scope);
    const index = list.findIndex((e) => e.name === originalName);
    const saved = {
      ...environment,
      // Like the real backend: secret values never go into the environment file.
      variables: (environment.variables ?? []).map(
        (v: { secret?: boolean; value?: string | null }) => (v.secret ? { ...v, value: null } : v),
      ),
    };
    if (index >= 0) list[index] = saved;
    // Like the real backend: a renamed active environment stays selected.
    const key = scope.kind === "global" ? "global" : "collection";
    if (state.active[key] === originalName) state.active[key] = environment.name;
    return null;
  },
  delete_environment: ({ scope, name }) => {
    const list = envList(scope);
    list.splice(
      list.findIndex((e) => e.name === name),
      1,
    );
    const key = scope.kind === "global" ? "global" : "collection";
    if (state.active[key] === name) state.active[key] = null;
    return null;
  },
  reorder_environments: () => null,
  // The real backend sends YAML; the UI only passes the text through, so JSON does here.
  environment_yaml: ({ scope, name }) =>
    JSON.stringify(
      envList(scope).find((e) => e.name === name),
      null,
      2,
    ),
  paste_environment: ({ scope, yaml }) => {
    let env: Environment_Serialize;
    try {
      env = JSON.parse(yaml);
    } catch {
      throw {
        code: "invalid_input",
        message: "the clipboard doesn't contain a Frida environment",
        hint: null,
      };
    }
    return addEnv(envList(scope), env).name;
  },
  transfer_environment: ({ from, name, to, mode }) => {
    const source = envList(from);
    const original = source.find((e) => e.name === name);
    if (!original) throw { code: "not_found", message: `${name} not found`, hint: null };
    const added = addEnv(envList(to), original);
    for (const v of original.variables.filter((x) => x.secret)) {
      const value = state.secrets.get(`${original.name}/${v.name}`);
      if (value !== undefined) state.secrets.set(`${scopeKey(to)}:${added.name}/${v.name}`, value);
    }
    if (mode === "move") {
      source.splice(source.indexOf(original), 1);
      if (from.kind === "global" && state.active.global === name) state.active.global = null;
      if (from.kind === "collection" && state.active.collection === name)
        state.active.collection = null;
    }
    return added.name;
  },
  get_active_environments: () => state.active,
  set_active_environment: ({ scope, name }) => {
    if (scope.kind === "global") state.active.global = name;
    else state.active.collection = name;
    return null;
  },
  secret_backend_status: () => ({ keychain: true, unlocked: true, fileExists: false }),
  set_secret: ({ environment, variable, value }) => {
    state.secrets.set(`${environment}/${variable}`, value);
    return null;
  },
  delete_secret: ({ environment, variable }) => {
    state.secrets.delete(`${environment}/${variable}`);
    return null;
  },
  reveal_secret: ({ environment, variable }) =>
    state.secrets.get(`${environment}/${variable}`) ?? null,
  list_aws_profiles: () => [
    { name: "work", sso: true, region: "eu-central-1" },
    { name: "shared-sso", sso: true, region: null },
  ],
  get_aws_profile: () => null,
  set_aws_profile: () => null,
  refresh_aws_secrets: () => null,
  set_aws_account_profile: ({ account, profile }) => {
    const others = Object.entries(state.settings.awsAccounts ?? {}).filter(([a]) => a !== account);
    state.settings.awsAccounts = Object.fromEntries(
      profile ? [...others, [account, profile]] : others,
    );
    return null;
  },
  list_cookies: () => [],
  clear_cookies: () => null,
  clear_runtime_vars: () => null,
  list_history: ({ search }) =>
    state.history.filter((h) =>
      `${h.requestName} ${h.url}`.toLowerCase().includes(search.toLowerCase()),
    ),
  clear_history: () => {
    state.history = [];
    return null;
  },
  inspect_request: () => {
    const env = state.environments.find((e) => e.name === state.active.collection);
    return {
      vars: [],
      errors: [],
      available: (env?.variables ?? []).map((v) => ({
        name: v.name,
        source: { kind: "collectionEnv", name: env?.name ?? "" },
        value: v.secret ? null : (v.value ?? null),
        isSecret: v.secret ?? false,
        hasValue: v.value !== null,
      })),
    };
  },
  send_request: send,
  fetch_oauth2_tokens: ({ path, request, auth }) => {
    state.lastTokenFetch = { path, auth };
    const url = oauthTokenUrl(auth ?? request?.auth);
    if (!url) throw { code: "invalid_input", message: "OAuth2 is missing a token URL", hint: null };
    const env = state.active.collection;
    state.oauthTokens = [
      ...state.oauthTokens.filter((t) => t.tokenUrl !== url),
      oauthToken(url, env, `token-${state.oauthTokens.length + 1}-abcdefghijklmnopqrstuvwxyz`),
    ];
    return tokensFor(auth ?? request?.auth);
  },
  list_oauth2_tokens: ({ request, auth }) => tokensFor(auth ?? request?.auth),
  clear_oauth2_tokens: ({ token }) => {
    state.oauthTokens = token
      ? state.oauthTokens.filter((t) => t.tokenUrl !== (token as TokenInfo).tokenUrl)
      : [];
    return null;
  },
  cancel_request: ({ requestId }) => {
    state.cancelled.add(requestId);
    return null;
  },
  filter_response: ({ responseId, bodyText, path }) => {
    const cached = [...state.historyResponses.values()].find((r) => r.id === responseId);
    return filterJson(bodyText ?? cached?.bodyText ?? "{}", path);
  },
  save_response_body: ({ responseId, target }) => {
    state.savedBodies.set(target, responseId);
    return null;
  },
  get_history_entry: ({ id }) => ({
    summary: state.history.find((h) => h.id === id),
    requestHeaders: [],
    requestBody: null,
    requestBodyTruncated: false,
    response: state.historyResponses.get(id) ?? null,
    proxy: null,
  }),
  restore_history_entry: ({ id }) => {
    const entry = state.history.find((h) => h.id === id);
    const name = entry?.requestName ?? "Restored";
    const path = freePath("", name, false);
    state.requests.set(path, request(name, entry?.method ?? "GET", entry?.url ?? "", 99));
    return path;
  },
  duplicate_request: ({ path }) => {
    const original = state.requests.get(path) ?? request("Copy", "GET", "", 99);
    const copy = { ...structuredClone(original), name: `${original.name} copy` };
    const next = freePath(parentOf(path), copy.name, false);
    state.requests.set(next, copy);
    return next;
  },
  create_request_from: ({ parent, request: r }) => {
    const path = freePath(parent, r.name, false);
    state.requests.set(path, r);
    return path;
  },
  import_curl: ({ command }) => parseCurl(command),
  export_curl: ({ request: r }) => {
    const headers = (r.headers ?? [])
      .map((h: { name: string; value: string }) => ` \\\n  -H '${h.name}: ${h.value}'`)
      .join("");
    return `curl -X ${r.method} '${resolve(r.url ?? "")}'${headers}`;
  },
  plan_run: ({ folder }) => plan(folder),
  run_collection: runCollection,

  git_info: () => gitInfo(),
  git_branches: () =>
    state.branches.map((name) => ({ name, current: name === state.branch, remote: false })),
  git_switch: ({ branch }) => {
    const info = gitInfo();
    if (info.kind === "repo" && info.status.files.length > 0)
      throw {
        code: "git",
        message: "There are uncommitted changes. Commit them first, then switch branches.",
        hint: null,
      };
    state.branch = branch;
    return null;
  },
  git_create_branch: ({ name }) => {
    state.branches.push(name);
    state.branch = name;
    return null;
  },
  git_stage: ({ paths }) => {
    const changed = gitInfo().kind === "repo" ? gitInfo() : null;
    const all = changed?.kind === "repo" ? changed.status.files.map((f) => f.path) : [];
    for (const p of paths.length ? paths : all) state.staged.add(p);
    return null;
  },
  git_unstage: ({ paths }) => {
    if (paths.length === 0) state.staged.clear();
    for (const p of paths) state.staged.delete(p);
    return null;
  },
  git_commit: () => {
    for (const p of state.staged) {
      const now = state.requests.get(p);
      if (now) state.committed.set(p, structuredClone(now));
      else state.committed.delete(p);
    }
    state.staged.clear();
    return null;
  },
  git_file_versions: ({ path }) => ({
    old: yamlish(state.committed.get(path)),
    new: yamlish(state.requests.get(path)),
    oldPath: null,
    binary: false,
    tooLarge: false,
  }),
  preview_openapi: () => ({
    collectionName: "Petstore",
    requests: 3,
    folders: 2,
    environments: 1,
    baseUrl: "https://petstore.example.com/v1",
    secretsToFill: [],
    warnings: [],
  }),
  // The file dialogs: the tests can't click through a native dialog, so they get fixed paths.
  preview_archive: () => archiveSummary,
  import_archive: ({ target }) => {
    const parent = target.parent ?? "";
    const path = freePath(parent, "Imported request", false);
    state.requests.set(path, request("Imported request", "PUT", "{{baseUrl}}/imported", 99));
    return {
      imported: { summary: archiveSummary, dir: target.collectionDir, path },
      overview: null,
    };
  },
  "plugin:dialog|open": () => "/fake/picked",
  "plugin:dialog|save": ({ options }) => {
    state.lastSaveDialog = options;
    return "/fake/saved.zip";
  },
};

/** Starts answering IPC calls. `window.__fridaE2E.reset()` puts the state back. */
export function installFakeBackend() {
  mockIPC(
    async (cmd, args) => {
      const handler = handlers[cmd];
      if (!handler) {
        console.warn(`[e2e fake backend] unhandled command ${cmd}`, args);
        return null;
      }
      // A copy, like data coming over IPC: handing out live objects would let the UI mutate the
      // fake's state, and stores wouldn't notice changes to an object they already hold.
      const result = await handler(args ?? {});
      return result === undefined ? null : structuredClone(result);
    },
    { shouldMockEvents: true },
  );
  (window as unknown as { __fridaE2E: unknown }).__fridaE2E = {
    reset: () => {
      state = initialState();
    },
    /** Changes a request "outside Frida" (an editor, git pull) and sends `fs-changed`. */
    changeOnDisk: async (path: string, patch: Partial<Request>) => {
      const current = state.requests.get(path);
      if (current) state.requests.set(path, { ...current, ...patch });
      await emit("fs-changed", { paths: [`${COLLECTION}/${path}`] });
    },
    /** Makes "Check for updates" find `info`. */
    offerUpdate: (info: UpdateInfo) => {
      state.update = info;
    },
    /** A plain copy for the tests (`Map`s don't survive `page.evaluate`). */
    state: () => ({
      ...state,
      requests: Object.fromEntries(state.requests),
      folders: Object.fromEntries(state.folders),
      committed: Object.fromEntries(state.committed),
      secrets: Object.fromEntries(state.secrets),
      staged: [...state.staged],
      historyResponses: Object.fromEntries(state.historyResponses),
      collectionStates: Object.fromEntries(state.collectionStates),
      cancelled: [...state.cancelled],
      savedBodies: Object.fromEntries(state.savedBodies),
    }),
  };
}
