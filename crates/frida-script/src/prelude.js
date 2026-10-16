// The script API (see docs/scripting.md), built on the state that Rust passes in as
// `__stateJson`. Runs before every script in a fresh QuickJS context.
//
// Frida's own object is `fri`. `bru` is the same object under Bruno's name, so scripts written
// for Bruno (and collections imported from it) run unchanged.
//
// Only `__native` reaches outside the sandbox; it is captured here and then removed from the
// global object, so scripts can only use it through this API.
(function () {
  "use strict";

  const state = JSON.parse(globalThis.__stateJson);
  const native = globalThis.__native;
  delete globalThis.__stateJson;
  delete globalThis.__native;

  /** Async `test(...)` callbacks still running; awaited by `__frida.finish()`. */
  const pending = [];

  const define = (name, value) =>
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });

  const unsupported = (name) => () => {
    throw new Error(`${name} is not supported in Frida yet (see docs/scripting.md)`);
  };

  // ---------------------------------------------------------------- console

  const format = (value) => {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.stack ? `${value}\n${value.stack}` : String(value);
    if (value === undefined) return "undefined";
    if (typeof value === "function") return `[Function ${value.name || "anonymous"}]`;
    try {
      return JSON.stringify(value, null, 2) ?? String(value);
    } catch {
      return String(value);
    }
  };
  const logger = (level) => (...args) => {
    state.logs.push({ level, message: args.map(format).join(" ") });
  };
  define("console", {
    log: logger("log"),
    info: logger("info"),
    warn: logger("warn"),
    error: logger("error"),
    debug: logger("debug"),
  });

  // ---------------------------------------------------------------- helpers

  const lower = (s) => String(s).toLowerCase();
  const headersToObject = (list) => {
    const out = {};
    for (const h of list) out[h.name] = h.value;
    return out;
  };
  const lowerHeaders = (list) => {
    const out = {};
    for (const h of list) out[lower(h.name)] = h.value;
    return out;
  };
  const findHeader = (list, name) => list.find((h) => lower(h.name) === lower(name));
  const tryJson = (text) => {
    if (typeof text !== "string") return text;
    const t = text.trim();
    if (!t.startsWith("{") && !t.startsWith("[")) return text;
    try {
      return JSON.parse(t);
    } catch {
      return text;
    }
  };
  /** Minimal URL split (QuickJS has no `URL`): scheme://host[:port]/path?query#hash. */
  const splitUrl = (url) => {
    const m = /^(?:([a-z][a-z0-9+.-]*):\/\/)?([^/?#]*)([^?#]*)(?:\?([^#]*))?/i.exec(url) || [];
    const hostPort = m[2] || "";
    return {
      host: hostPort.replace(/:\d+$/, ""),
      path: m[3] || "",
      query: m[4] || "",
    };
  };
  /** `get(obj, "a.b[0].c")`, used by `res("a.b")`. */
  const getPath = (obj, path) => {
    const parts = String(path)
      .replace(/\[(\w+)\]/g, ".$1")
      .split(".")
      .filter((p) => p !== "");
    let cur = obj;
    for (const p of parts) {
      if (cur === null || cur === undefined) return undefined;
      cur = cur[p];
    }
    return cur;
  };
  const varName = /^[\w.-]+$/;
  const checkName = (fn, name) => {
    if (!varName.test(String(name))) {
      throw new Error(
        `${fn}: "${name}" is not a valid variable name (letters, digits, "_", "-" and "." only)`,
      );
    }
  };

  // ---------------------------------------------------------------- req

  const r = state.req;
  const req = {
    getUrl: () => r.url,
    setUrl: (url) => {
      r.url = String(url);
    },
    getHost: () => splitUrl(r.url).host,
    getPath: () => splitUrl(r.url).path,
    getQueryString: () => splitUrl(r.url).query,
    getMethod: () => r.method,
    setMethod: (method) => {
      r.method = String(method).toUpperCase();
    },
    getName: () => r.name,
    getAuthMode: () => r.authMode,
    getHeader: (name) => findHeader(r.headers, name)?.value,
    getHeaders: () => headersToObject(r.headers),
    setHeader: (name, value) => {
      const existing = findHeader(r.headers, name);
      if (existing) existing.value = String(value);
      else r.headers.push({ name: String(name), value: String(value) });
    },
    setHeaders: (headers) => {
      for (const [name, value] of Object.entries(headers || {})) req.setHeader(name, value);
    },
    deleteHeader: (name) => {
      r.headers = r.headers.filter((h) => lower(h.name) !== lower(name));
    },
    deleteHeaders: (names) => {
      for (const name of names || []) req.deleteHeader(name);
    },
    getBody: (options) => {
      if (options && options.raw) {
        return typeof r.body === "string" || r.body === null ? r.body : JSON.stringify(r.body);
      }
      return r.body;
    },
    setBody: (body) => {
      r.body = body === undefined ? null : body;
    },
    getTimeout: () => r.timeoutMs,
    setTimeout: (ms) => {
      r.timeoutMs = Number(ms);
    },
    getExecutionMode: () => state.executionMode ?? "standalone",
    setMaxRedirects: unsupported("req.setMaxRedirects"),
    onFail: unsupported("req.onFail"),
    getTags: () => [],
  };
  // Bruno also exposes these as properties.
  Object.defineProperties(req, {
    url: { get: req.getUrl, set: req.setUrl, enumerable: true },
    method: { get: req.getMethod, set: req.setMethod, enumerable: true },
    headers: { get: req.getHeaders, enumerable: true },
    body: { get: () => req.getBody(), set: req.setBody, enumerable: true },
    timeout: { get: req.getTimeout, set: req.setTimeout, enumerable: true },
  });
  define("req", req);

  // ---------------------------------------------------------------- res

  if (state.res) {
    const s = state.res;
    let body = tryJson(s.body);
    // Bruno's `res` is also a function: `res("data.items[0].id")` reads from the body.
    const res = (path) => getPath(body, path);
    Object.assign(res, {
      status: s.status,
      statusText: s.statusText,
      headers: lowerHeaders(s.headers),
      responseTime: s.responseTimeMs,
      url: s.url,
      getStatus: () => s.status,
      getStatusText: () => s.statusText,
      getHeader: (name) => findHeader(s.headers, name)?.value,
      getHeaders: () => lowerHeaders(s.headers),
      getBody: () => body,
      setBody: (b) => {
        body = b;
      },
      getResponseTime: () => s.responseTimeMs,
      getUrl: () => s.url,
      getSize: () => {
        const headers = s.headers.reduce((n, h) => n + h.name.length + h.value.length + 4, 0);
        return { body: s.sizeBytes, headers, total: s.sizeBytes + headers };
      },
    });
    Object.defineProperty(res, "body", { get: () => body, set: (b) => (body = b) });
    define("res", res);
  }

  // ---------------------------------------------------------------- fri (alias: bru)

  const v = state.vars;
  const envWrite = (global, name, value, options) => {
    state.envWrites.push({
      global,
      name: String(name),
      value: value === undefined ? null : value,
      persist: !!(options && options.persist),
    });
  };
  const toEnvValue = (value) => (typeof value === "string" ? value : JSON.stringify(value));

  const fri = {
    // Runtime variables (this session).
    hasVar: (name) => Object.prototype.hasOwnProperty.call(v.runtime, name),
    getVar: (name) => v.runtime[name],
    setVar: (name, value) => {
      checkName("fri.setVar", name);
      v.runtime[name] = value === undefined ? null : value;
    },
    deleteVar: (name) => {
      delete v.runtime[name];
    },
    getAllVars: () => ({ ...v.runtime }),
    deleteAllVars: () => {
      for (const k of Object.keys(v.runtime)) delete v.runtime[k];
    },

    // Collection environment: in memory for the session, `{ persist: true }` also saves it.
    getEnvName: () => v.envName ?? undefined,
    hasEnvVar: (name) => Object.prototype.hasOwnProperty.call(v.env, name),
    getEnvVar: (name) => v.env[name],
    setEnvVar: (name, value, options) => {
      if (v.envName == null) throw new Error("fri.setEnvVar: no collection environment is selected");
      checkName("fri.setEnvVar", name);
      v.env[name] = toEnvValue(value);
      envWrite(false, name, v.env[name], options);
    },
    deleteEnvVar: (name) => {
      if (v.envName == null) return;
      delete v.env[name];
      envWrite(false, name, null, { persist: true });
    },
    getAllEnvVars: () => ({ ...v.env }),

    // Global environment, the same way.
    getGlobalEnvName: () => v.globalEnvName ?? undefined,
    hasGlobalEnvVar: (name) => Object.prototype.hasOwnProperty.call(v.globalEnv, name),
    getGlobalEnvVar: (name) => v.globalEnv[name],
    setGlobalEnvVar: (name, value, options) => {
      if (v.globalEnvName == null) {
        throw new Error("fri.setGlobalEnvVar: no global environment is selected");
      }
      checkName("fri.setGlobalEnvVar", name);
      v.globalEnv[name] = toEnvValue(value);
      envWrite(true, name, v.globalEnv[name], options);
    },
    deleteGlobalEnvVar: (name) => {
      if (v.globalEnvName == null) return;
      delete v.globalEnv[name];
      envWrite(true, name, null, { persist: true });
    },
    getAllGlobalEnvVars: () => ({ ...v.globalEnv }),

    // Read-only layers.
    getCollectionName: () => state.collectionName,
    getCollectionVar: (name) => v.collection[name],
    hasCollectionVar: (name) => Object.prototype.hasOwnProperty.call(v.collection, name),
    getFolderVar: (name) => v.folder[name],
    getRequestVar: (name) => v.request[name],
    getProcessEnv: (name) => native.processEnv(String(name)) ?? undefined,

    interpolate: (value) => {
      const vars = JSON.stringify(v);
      if (typeof value === "string") return native.interpolate(value, vars);
      return JSON.parse(native.interpolate(JSON.stringify(value), vars));
    },
    sleep: (ms) => native.sleep(Number(ms) || 0),
    isSafeMode: () => true,
    getTestResults: () => state.tests.map((t) => ({ ...t, status: t.passed ? "pass" : "fail" })),

    sendRequest: (options, callback) => {
      const promise = sendRequest(options);
      if (typeof callback === "function") {
        return promise.then(
          (response) => callback(null, response),
          (error) => callback(error, error.response ?? null),
        );
      }
      return promise;
    },

    // Like Bruno: `null` ends the run instead of naming the next request.
    setNextRequest: (name) => {
      if (name === null) {
        state.stopExecution = true;
      } else {
        state.nextRequest = String(name);
      }
    },
    runner: {
      setNextRequest: (name) => fri.setNextRequest(name),
      skipRequest: () => {
        state.skipRequest = true;
      },
      stopExecution: () => {
        state.stopExecution = true;
      },
    },

    setCollectionVar: unsupported("fri.setCollectionVar"),
    deleteCollectionVar: unsupported("fri.deleteCollectionVar"),
    deleteAllCollectionVars: unsupported("fri.deleteAllCollectionVars"),
    deleteAllEnvVars: unsupported("fri.deleteAllEnvVars"),
    deleteAllGlobalEnvVars: unsupported("fri.deleteAllGlobalEnvVars"),
    getOauth2CredentialVar: unsupported("fri.getOauth2CredentialVar"),
    resetOauth2Credential: unsupported("fri.resetOauth2Credential"),
    getSecretVar: unsupported("fri.getSecretVar"),
    runRequest: unsupported("fri.runRequest"),
    cwd: unsupported("fri.cwd"),
    getAssertionResults: () => [],
    cookies: new Proxy(
      {},
      {
        get: (_, prop) => unsupported(`fri.cookies.${String(prop)}`),
      },
    ),
  };
  define("fri", fri);
  // Bruno compatibility: the same API under Bruno's name.
  define("bru", fri);

  /** `fri.sendRequest`: axios-style options and response, like Bruno. */
  async function sendRequest(options) {
    const o = typeof options === "string" ? { url: options } : options || {};
    if (!o.url) throw new Error("fri.sendRequest: `url` is required");
    const headers = Object.entries(o.headers || {}).map(([name, value]) => ({
      name,
      value: String(value),
    }));
    let body = o.data ?? o.body;
    if (body !== undefined && body !== null && typeof body !== "string") {
      if (body instanceof URLSearchParamsImpl) {
        body = body.toString();
        if (!findHeader(headers, "content-type")) {
          headers.push({ name: "Content-Type", value: "application/x-www-form-urlencoded" });
        }
      } else {
        body = JSON.stringify(body);
        if (!findHeader(headers, "content-type")) {
          headers.push({ name: "Content-Type", value: "application/json" });
        }
      }
    }
    const call = {
      method: String(o.method || "GET").toUpperCase(),
      url: String(o.url),
      headers,
      body: body ?? null,
      timeoutMs: o.timeout ? Number(o.timeout) : null,
    };
    const reply = JSON.parse(await native.send(JSON.stringify(call)));
    if (reply.error) throw new Error(`fri.sendRequest: ${reply.error}`);
    const x = reply.ok;
    const response = {
      status: x.status,
      statusText: x.statusText,
      headers: lowerHeaders(x.headers),
      data: tryJson(x.body),
      duration: x.responseTimeMs,
    };
    // Like axios: statuses outside 2xx reject, with the response attached.
    if (x.status < 200 || x.status > 299) {
      const error = new Error(`Request failed with status code ${x.status}`);
      error.response = response;
      error.status = x.status;
      throw error;
    }
    return response;
  }

  // ---------------------------------------------------------------- test / expect

  define("test", (name, fn) => {
    const result = { name: String(name), passed: true, error: null };
    state.tests.push(result);
    const fail = (e) => {
      result.passed = false;
      result.error = e && e.message !== undefined ? String(e.message) : String(e);
    };
    try {
      const out = fn();
      if (out && typeof out.then === "function") pending.push(out.then(() => {}, fail));
    } catch (e) {
      fail(e);
    }
  });

  const show = (value) => {
    if (typeof value === "string") return `'${value}'`;
    if (value === undefined) return "undefined";
    try {
      return JSON.stringify(value) ?? String(value);
    } catch {
      return String(value);
    }
  };
  const typeOf = (value) => {
    if (value === null) return "null";
    if (Array.isArray(value)) return "array";
    if (value instanceof Date) return "date";
    if (value instanceof RegExp) return "regexp";
    return typeof value;
  };
  const deepEqual = (a, b) => {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    if (a instanceof Date && b instanceof Date) return a.getTime() === b.getTime();
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]));
  };

  class AssertionError extends Error {
    constructor(message) {
      super(message);
      this.name = "AssertionError";
    }
  }

  /** A chai-like assertion. Language words (`to`, `be`, ...) return the same object. */
  class Assertion {
    constructor(subject, message) {
      this.subject = subject;
      this.message = message;
      this.negate = false;
      this.isDeep = false;
    }
    assert(ok, positive, negative) {
      if (this.negate ? ok : !ok) {
        const text = this.negate ? negative : positive;
        throw new AssertionError(this.message ? `${this.message}: ${text}` : text);
      }
      return this;
    }
    get not() {
      this.negate = !this.negate;
      return this;
    }
    get deep() {
      this.isDeep = true;
      return this;
    }
    equal(expected) {
      const s = this.subject;
      const ok = this.isDeep ? deepEqual(s, expected) : s === expected;
      return this.assert(
        ok,
        `expected ${show(s)} to ${this.isDeep ? "deeply " : ""}equal ${show(expected)}`,
        `expected ${show(s)} to not ${this.isDeep ? "deeply " : ""}equal ${show(expected)}`,
      );
    }
    eql(expected) {
      const s = this.subject;
      return this.assert(
        deepEqual(s, expected),
        `expected ${show(s)} to deeply equal ${show(expected)}`,
        `expected ${show(s)} to not deeply equal ${show(expected)}`,
      );
    }
    a(type) {
      const actual = typeOf(this.subject);
      const t = lower(type);
      return this.assert(
        actual === t,
        `expected ${show(this.subject)} to be a ${t}`,
        `expected ${show(this.subject)} not to be a ${t}`,
      );
    }
    instanceOf(cls) {
      return this.assert(
        this.subject instanceof cls,
        `expected ${show(this.subject)} to be an instance of ${cls.name}`,
        `expected ${show(this.subject)} to not be an instance of ${cls.name}`,
      );
    }
    property(name, ...value) {
      const s = this.subject;
      const has = s !== null && s !== undefined && getPath(s, name) !== undefined;
      if (value.length === 0) {
        this.assert(
          has,
          `expected ${show(s)} to have property '${name}'`,
          `expected ${show(s)} to not have property '${name}'`,
        );
      } else {
        const actual = has ? getPath(s, name) : undefined;
        const ok = has && (this.isDeep ? deepEqual(actual, value[0]) : actual === value[0]);
        this.assert(
          ok,
          `expected ${show(s)} to have property '${name}' of ${show(value[0])}, but got ${show(actual)}`,
          `expected ${show(s)} to not have property '${name}' of ${show(value[0])}`,
        );
      }
      // Like chai, later assertions apply to the property's value.
      if (has && !this.negate) this.subject = getPath(s, name);
      return this;
    }
    include(item) {
      const s = this.subject;
      let ok;
      if (typeof s === "string") ok = s.includes(String(item));
      else if (Array.isArray(s)) {
        ok = this.isDeep || (typeof item === "object" && item !== null)
          ? s.some((x) => deepEqual(x, item))
          : s.includes(item);
      } else if (s && typeof s === "object" && item && typeof item === "object") {
        ok = Object.keys(item).every((k) => deepEqual(s[k], item[k]));
      } else ok = false;
      return this.assert(
        ok,
        `expected ${show(s)} to include ${show(item)}`,
        `expected ${show(s)} to not include ${show(item)}`,
      );
    }
    above(n) {
      return this.assert(
        this.subject > n,
        `expected ${show(this.subject)} to be above ${n}`,
        `expected ${show(this.subject)} to be at most ${n}`,
      );
    }
    below(n) {
      return this.assert(
        this.subject < n,
        `expected ${show(this.subject)} to be below ${n}`,
        `expected ${show(this.subject)} to be at least ${n}`,
      );
    }
    least(n) {
      return this.assert(
        this.subject >= n,
        `expected ${show(this.subject)} to be at least ${n}`,
        `expected ${show(this.subject)} to be below ${n}`,
      );
    }
    most(n) {
      return this.assert(
        this.subject <= n,
        `expected ${show(this.subject)} to be at most ${n}`,
        `expected ${show(this.subject)} to be above ${n}`,
      );
    }
    within(lo, hi) {
      return this.assert(
        this.subject >= lo && this.subject <= hi,
        `expected ${show(this.subject)} to be within ${lo}..${hi}`,
        `expected ${show(this.subject)} to not be within ${lo}..${hi}`,
      );
    }
    lengthOf(n) {
      const len = this.subject == null ? undefined : this.subject.length;
      return this.assert(
        len === n,
        `expected ${show(this.subject)} to have a length of ${n} but got ${len}`,
        `expected ${show(this.subject)} to not have a length of ${n}`,
      );
    }
    match(re) {
      return this.assert(
        re.test(String(this.subject)),
        `expected ${show(this.subject)} to match ${re}`,
        `expected ${show(this.subject)} not to match ${re}`,
      );
    }
    oneOf(list) {
      return this.assert(
        list.some((x) => (this.isDeep ? deepEqual(x, this.subject) : x === this.subject)),
        `expected ${show(this.subject)} to be one of ${show(list)}`,
        `expected ${show(this.subject)} to not be one of ${show(list)}`,
      );
    }
    keys(...keys) {
      const list = keys.length === 1 && Array.isArray(keys[0]) ? keys[0] : keys;
      const actual = this.subject && typeof this.subject === "object" ? Object.keys(this.subject) : [];
      return this.assert(
        list.every((k) => actual.includes(k)),
        `expected ${show(this.subject)} to have keys ${show(list)}`,
        `expected ${show(this.subject)} to not have keys ${show(list)}`,
      );
    }
    /** `expect(res).to.have.status(200)`. */
    status(code) {
      const s = this.subject;
      const actual = s && (typeof s.getStatus === "function" ? s.getStatus() : s.status);
      return this.assert(
        actual === code,
        `expected response to have status ${code} but got ${actual}`,
        `expected response to not have status ${code}`,
      );
    }
    /** `expect(res).to.have.header("content-type", "application/json")`. */
    header(name, ...value) {
      const s = this.subject;
      const actual = s && typeof s.getHeader === "function" ? s.getHeader(name) : undefined;
      const ok = actual !== undefined && (value.length === 0 || actual === value[0]);
      return this.assert(
        ok,
        `expected response to have header '${name}'${value.length ? ` of ${show(value[0])}` : ""}`,
        `expected response to not have header '${name}'`,
      );
    }
    satisfy(fn) {
      return this.assert(
        !!fn(this.subject),
        `expected ${show(this.subject)} to satisfy the condition`,
        `expected ${show(this.subject)} to not satisfy the condition`,
      );
    }
    get ok() {
      return this.assert(!!this.subject, `expected ${show(this.subject)} to be truthy`, `expected ${show(this.subject)} to be falsy`);
    }
    get true() {
      return this.assert(this.subject === true, `expected ${show(this.subject)} to be true`, `expected ${show(this.subject)} to not be true`);
    }
    get false() {
      return this.assert(this.subject === false, `expected ${show(this.subject)} to be false`, `expected ${show(this.subject)} to not be false`);
    }
    get null() {
      return this.assert(this.subject === null, `expected ${show(this.subject)} to be null`, `expected ${show(this.subject)} to not be null`);
    }
    get undefined() {
      return this.assert(this.subject === undefined, `expected ${show(this.subject)} to be undefined`, `expected ${show(this.subject)} to not be undefined`);
    }
    get NaN() {
      return this.assert(Number.isNaN(this.subject), `expected ${show(this.subject)} to be NaN`, `expected ${show(this.subject)} to not be NaN`);
    }
    get exist() {
      return this.assert(this.subject != null, `expected ${show(this.subject)} to exist`, `expected ${show(this.subject)} to not exist`);
    }
    get empty() {
      const s = this.subject;
      const empty =
        typeof s === "string" || Array.isArray(s)
          ? s.length === 0
          : s && typeof s === "object"
            ? Object.keys(s).length === 0
            : false;
      return this.assert(empty, `expected ${show(s)} to be empty`, `expected ${show(s)} to not be empty`);
    }
  }
  // Chainable language words and aliases, as in chai.
  for (const word of ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "does", "still", "also"]) {
    Object.defineProperty(Assertion.prototype, word, {
      get() {
        return this;
      },
    });
  }
  const alias = (name, target) => {
    Assertion.prototype[name] = Assertion.prototype[target];
  };
  alias("equals", "equal");
  alias("eq", "equal");
  alias("eqls", "eql");
  alias("an", "a");
  alias("instanceof", "instanceOf");
  alias("includes", "include");
  alias("contain", "include");
  alias("contains", "include");
  alias("gt", "above");
  alias("greaterThan", "above");
  alias("lt", "below");
  alias("lessThan", "below");
  alias("gte", "least");
  alias("greaterThanOrEqual", "least");
  alias("lte", "most");
  alias("lessThanOrEqual", "most");
  alias("length", "lengthOf");
  alias("matches", "match");
  alias("key", "keys");
  alias("satisfies", "satisfy");

  const expect = (subject, message) => new Assertion(subject, message);
  expect.fail = (message) => {
    throw new AssertionError(message || "expect.fail()");
  };
  define("expect", expect);
  define("AssertionError", AssertionError);

  // ---------------------------------------------------------------- web helpers

  define("btoa", (text) => native.btoa(String(text)));
  define("atob", (text) => native.atob(String(text)));

  const hash = (algorithm) => (data) => native.hash(algorithm, String(data));
  define("crypto", {
    md5: hash("md5"),
    sha1: hash("sha1"),
    sha256: hash("sha256"),
    sha512: hash("sha512"),
    hmac: (algorithm, key, data) => native.hmac(lower(algorithm), String(key), String(data)),
    hmacSHA256: (data, key) => native.hmac("sha256", String(key), String(data)),
    hmacSHA512: (data, key) => native.hmac("sha512", String(key), String(data)),
    randomUUID: () => native.uuid(),
  });

  class URLSearchParamsImpl {
    constructor(init) {
      this.list = [];
      if (typeof init === "string") {
        for (const part of init.replace(/^\?/, "").split("&")) {
          if (!part) continue;
          const i = part.indexOf("=");
          const dec = (s) => decodeURIComponent(s.replace(/\+/g, " "));
          this.list.push(i < 0 ? [dec(part), ""] : [dec(part.slice(0, i)), dec(part.slice(i + 1))]);
        }
      } else if (Array.isArray(init)) {
        for (const [k, val] of init) this.list.push([String(k), String(val)]);
      } else if (init && typeof init === "object") {
        for (const [k, val] of Object.entries(init)) this.list.push([k, String(val)]);
      }
    }
    append(k, val) {
      this.list.push([String(k), String(val)]);
    }
    set(k, val) {
      this.delete(k);
      this.append(k, val);
    }
    get(k) {
      const e = this.list.find(([n]) => n === k);
      return e ? e[1] : null;
    }
    getAll(k) {
      return this.list.filter(([n]) => n === k).map(([, val]) => val);
    }
    has(k) {
      return this.list.some(([n]) => n === k);
    }
    delete(k) {
      this.list = this.list.filter(([n]) => n !== k);
    }
    forEach(fn) {
      for (const [k, val] of this.list) fn(val, k, this);
    }
    keys() {
      return this.list.map(([k]) => k)[Symbol.iterator]();
    }
    values() {
      return this.list.map(([, val]) => val)[Symbol.iterator]();
    }
    entries() {
      return this.list.map((e) => [...e])[Symbol.iterator]();
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    toString() {
      const enc = (s) => encodeURIComponent(s).replace(/%20/g, "+");
      return this.list.map(([k, val]) => `${enc(k)}=${enc(val)}`).join("&");
    }
  }
  define("URLSearchParams", URLSearchParamsImpl);

  class TextEncoderImpl {
    get encoding() {
      return "utf-8";
    }
    encode(text = "") {
      const bin = unescape(encodeURIComponent(String(text)));
      const out = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
      return out;
    }
  }
  class TextDecoderImpl {
    get encoding() {
      return "utf-8";
    }
    decode(bytes) {
      if (!bytes) return "";
      const arr = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes.buffer || bytes);
      let bin = "";
      for (let i = 0; i < arr.length; i++) bin += String.fromCharCode(arr[i]);
      try {
        return decodeURIComponent(escape(bin));
      } catch {
        return bin;
      }
    }
  }
  define("TextEncoder", TextEncoderImpl);
  define("TextDecoder", TextDecoderImpl);

  define("require", (name) => {
    throw new Error(`require("${name}") is not available in Frida scripts (see docs/scripting.md)`);
  });

  // ---------------------------------------------------------------- for the Rust side

  define("__frida", {
    exportState: () => JSON.stringify(state),
    finish: () => Promise.all(pending),
  });
})();
