import type { ResolvedVar, UsageLocation, VarSource } from "@/bindings";

/** Human-readable name of where a value came from. */
export function sourceLabel(source: VarSource | null): string {
  if (!source) return "not defined";
  switch (source.kind) {
    case "runtime":
      return "Runtime";
    case "request":
      return "Request";
    case "folder":
      return `Folder: ${source.name}`;
    case "collectionEnv":
      return `Collection env: ${source.name}`;
    case "collection":
      return "Collection";
    case "globalEnv":
      return `Global env: ${source.name}`;
    case "processEnv":
      return "Process env";
    case "dynamic":
      return "Dynamic";
  }
}

export function usageLabel(location: UsageLocation): string {
  switch (location.kind) {
    case "url":
      return "URL";
    case "pathParam":
      return `Path param :${location.name}`;
    case "header":
      return `Header ${location.name}`;
    case "body":
      return "Body";
    case "auth":
      return "Auth";
    case "variable":
      return `Inside {{${location.name}}}`;
  }
}

/** What to show as a variable's value: masked secrets, unset secrets and unresolved names. */
export function displayValue(v: ResolvedVar): string {
  if (v.masked) return "•••••••• (secret)";
  if (!v.resolved) return v.isSecret ? "unset secret" : "unresolved";
  return v.value ?? "";
}

/** Matches `{{ name }}`; group 1 is the name with surrounding whitespace. */
export const VARIABLE_PATTERN = /\{\{([^{}]*?)\}\}/g;

/** The `{{…}}` placeholder covering position `pos` in `text`, if any. */
export function variableAt(
  text: string,
  pos: number,
): { name: string; from: number; to: number } | null {
  for (const match of text.matchAll(VARIABLE_PATTERN)) {
    const from = match.index;
    const to = from + match[0].length;
    if (pos >= from && pos <= to) {
      const name = (match[1] ?? "").trim();
      return name ? { name, from, to } : null;
    }
  }
  return null;
}
