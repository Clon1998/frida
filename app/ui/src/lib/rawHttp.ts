import type { ConsoleEntry } from "@/bindings";

/**
 * The entry as raw HTTP text, the way Postman's and Bruno's consoles show it:
 *
 *     POST https://api.example.com/orders
 *     Content-Type: application/json
 *
 *     {"item":"book"}
 *
 *     HTTP/1.1 201 Created
 *     content-type: application/json
 *
 *     {"id":1}
 */
export function rawRequest(entry: ConsoleEntry): string {
  const lines = [`${entry.method} ${entry.url}`];
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
