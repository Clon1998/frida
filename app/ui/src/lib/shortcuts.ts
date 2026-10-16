/** Every keyboard shortcut, for the shortcuts dialog (PLAN.md §14). `Mod` is Cmd or Ctrl. */
export const SHORTCUTS: { keys: string; action: string }[] = [
  { keys: "Mod+Enter", action: "Send the request" },
  { keys: "Mod+S", action: "Save the tab (or the environment being edited)" },
  { keys: "Mod+W", action: "Close the tab" },
  { keys: "Mod+T", action: "New request in the collection" },
  { keys: "Mod+K", action: "Quick open: search requests" },
  { keys: "Mod+E", action: "Switch environment" },
  { keys: "Mod+J", action: "Show or hide the console" },
  { keys: "Mod+/", action: "Show these shortcuts" },
  { keys: "Mod+,", action: "Settings" },
  { keys: "Enter", action: "In the URL bar: send" },
  { keys: "{{", action: "In editors: suggest variables" },
  { keys: "Mod+Enter", action: "In the commit message: commit" },
];

/** `Mod+K` → `⌘K` on macOS, `Ctrl+K` elsewhere. */
export function formatKeys(keys: string, mac = isMac()): string {
  return mac ? keys.replace("Mod+", "⌘") : keys.replace("Mod+", "Ctrl+");
}

export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
}
