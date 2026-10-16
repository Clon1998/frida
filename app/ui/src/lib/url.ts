import type { KeyValue as RawKeyValue, PathParam as RawPathParam } from "@/bindings";
import type { Complete } from "@/lib/model";

type KeyValue = Complete<RawKeyValue>;
type PathParam = Complete<RawPathParam>;

/**
 * Helpers that keep the URL bar and the Params tables in sync (PLAN.md §4.3).
 *
 * Values are kept exactly as typed (no percent-decoding), so `{{variables}}` and
 * already-encoded values survive a round trip unchanged.
 */

interface UrlParts {
  base: string;
  query: string | null;
  hash: string;
}

function splitUrl(url: string): UrlParts {
  const hashIndex = url.indexOf("#");
  const beforeHash = hashIndex === -1 ? url : url.slice(0, hashIndex);
  const hash = hashIndex === -1 ? "" : url.slice(hashIndex);
  const queryIndex = beforeHash.indexOf("?");
  if (queryIndex === -1) return { base: beforeHash, query: null, hash };
  return { base: beforeHash.slice(0, queryIndex), query: beforeHash.slice(queryIndex + 1), hash };
}

/** The query string of `url` as rows: `?a=1&b` → `[{a,1},{b,""}]`. */
export function parseQuery(url: string): { name: string; value: string }[] {
  const { query } = splitUrl(url);
  if (!query) return [];
  return query
    .split("&")
    .filter((pair) => pair.length > 0)
    .map((pair) => {
      const eq = pair.indexOf("=");
      return eq === -1
        ? { name: pair, value: "" }
        : { name: pair.slice(0, eq), value: pair.slice(eq + 1) };
    });
}

/**
 * New query rows after the user edited the URL. Enabled rows come from the URL, in its order;
 * disabled rows aren't part of the URL, so they are kept from `previous` at their position
 * (the user may have ordered them). Extra URL params are added at the end.
 */
export function queryParamsFromUrl(url: string, previous: KeyValue[]): KeyValue[] {
  const fromUrl = parseQuery(url).map((p) => ({ ...p, enabled: true }));
  const rows: KeyValue[] = [];
  let next = 0;
  for (const row of previous) {
    if (!row.enabled) {
      rows.push(row);
    } else if (next < fromUrl.length) {
      const fromUrlRow = fromUrl[next++];
      if (fromUrlRow) rows.push(fromUrlRow);
    }
  }
  rows.push(...fromUrl.slice(next));
  return rows;
}

/** New URL after the user edited the query rows. Only enabled rows with a name are included. */
export function urlWithQueryParams(url: string, params: KeyValue[]): string {
  const { base, hash } = splitUrl(url);
  const query = params
    .filter((p) => p.enabled && p.name.length > 0)
    .map((p) => (p.value.length > 0 ? `${p.name}=${p.value}` : p.name))
    .join("&");
  return query.length > 0 ? `${base}?${query}${hash}` : `${base}${hash}`;
}

/** Names of `:param` segments in the path (never the `:port` of the host). */
export function pathParamNames(url: string): string[] {
  const { base } = splitUrl(url);
  const afterScheme = base.includes("://") ? base.slice(base.indexOf("://") + 3) : base;
  const slash = afterScheme.indexOf("/");
  if (slash === -1) return [];
  return afterScheme
    .slice(slash + 1)
    .split("/")
    .filter((segment) => segment.startsWith(":") && segment.length > 1)
    .map((segment) => segment.slice(1));
}

/** Path param rows for the current URL, keeping values the user already entered. */
export function syncPathParams(url: string, previous: PathParam[]): PathParam[] {
  return pathParamNames(url).map((name) => ({
    name,
    value: previous.find((p) => p.name === name)?.value ?? "",
  }));
}
