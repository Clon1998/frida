import type { ConsoleEntry } from "@/bindings";

/**
 * The whole exchange as raw HTTP text, the way Postman's console shows it in "raw" mode:
 *
 *     POST /orders HTTP/1.1
 *     host: api.example.com
 *     content-type: application/json
 *
 *     {"item":"book"}
 *
 *     HTTP/1.1 201 Created
 *     content-type: application/json
 *
 *     {"id":1}
 */
export function rawExchange(entry: ConsoleEntry): string {
  return `${rawRequest(entry)}\n\n${rawResponse(entry)}`;
}

/** The request line uses the path (the host is in the `host` header), like on the wire. */
export function rawRequest(entry: ConsoleEntry): string {
  const version = entry.response?.httpVersion ?? "HTTP/1.1";
  const lines = [`${entry.method} ${requestTarget(entry.url)} ${version}`];
  for (const h of entry.requestHeaders) lines.push(`${h.name}: ${h.value}`);
  if (entry.requestBody) {
    lines.push("", entry.requestBody.text);
    if (entry.requestBody.truncated) lines.push("… (truncated)");
  }
  return lines.join("\n");
}

export function rawResponse(entry: ConsoleEntry): string {
  const r = entry.response;
  if (!r) return `Error: ${entry.error ?? "no response"}`;
  const lines = [`${r.httpVersion} ${r.status} ${r.statusText}`.trim()];
  for (const h of r.headers) lines.push(`${h.name}: ${h.value}`);
  if (r.body) {
    lines.push("", r.body.text);
    if (r.body.truncated) lines.push("… (truncated)");
  }
  return lines.join("\n");
}

/** `/path?query` of an absolute URL; the URL as is when it can't be parsed. */
export function requestTarget(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return url;
  }
}

/** JSON bodies indented for the pretty view; anything else unchanged. */
export function prettyBody(text: string): string {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return text;
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}
