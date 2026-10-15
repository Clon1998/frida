/** `1536` → `"1.5 KB"`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

/** `850` → `"850 ms"`, `2300` → `"2.30 s"`. */
export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`;
}

/** Pretty-prints JSON; returns the input unchanged when it isn't valid JSON. */
export function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

export type BodyKind = "json" | "xml" | "html" | "image" | "text" | "binary";

/** What kind of body a response has, from its content type (falling back to sniffing). */
export function bodyKind(contentType: string | null, text: string | null): BodyKind {
  const type = (contentType ?? "").toLowerCase();
  if (type.startsWith("image/")) return "image";
  if (type.includes("json")) return "json";
  if (type.includes("html")) return "html";
  if (type.includes("xml")) return "xml";
  if (text === null) return "binary";
  const start = text.trimStart();
  if (start.startsWith("{") || start.startsWith("[")) return "json";
  if (start.startsWith("<")) return "xml";
  return "text";
}
