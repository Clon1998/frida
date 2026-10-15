/**
 * Bulk editing of key/value rows as text, like Bruno: one `name: value` per line, and a `//`
 * prefix for disabled rows. Only the first `:` separates name and value, so values such as
 * URLs (`Referer: https://x`) keep their colons.
 */

interface Row {
  name: string;
  value: string;
  enabled: boolean;
}

export function rowsToBulk(rows: Row[]): string {
  return rows.map((row) => `${row.enabled ? "" : "//"}${row.name}: ${row.value}`).join("\n");
}

export function bulkToRows(text: string): Row[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const disabled = line.startsWith("//");
      const content = disabled ? line.slice(2).trim() : line;
      const colon = content.indexOf(":");
      const name = colon === -1 ? content : content.slice(0, colon).trim();
      const value = colon === -1 ? "" : content.slice(colon + 1).trim();
      return { name, value, enabled: !disabled };
    })
    .filter((row) => row.name.length > 0);
}
