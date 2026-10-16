/** `code` after the existing script, separated by a blank line (or alone in an empty editor). */
export function appendSnippet(current: string, code: string) {
  return current.trim() ? `${current.trimEnd()}\n\n${code}\n` : `${code}\n`;
}
