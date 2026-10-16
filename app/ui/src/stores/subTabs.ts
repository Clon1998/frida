import { create } from "zustand";

/**
 * Which sub-tab (Params, Headers, Script, ...) is open in each editor, by a key such as
 * `request:<dir>:<path>`. Kept in a store rather than in the components, because the editors
 * unmount when another tab is shown and would otherwise start over on their first sub-tab.
 * In memory only (like VS Code's per-editor view state within a session).
 */
interface SubTabsState {
  open: Record<string, string>;
  set: (key: string, value: string) => void;
}

export const useSubTabs = create<SubTabsState>((set) => ({
  open: {},
  set: (key, value) => set((s) => ({ open: { ...s.open, [key]: value } })),
}));

/** The open sub-tab for `key` (or `fallback`) and a setter, for a controlled `<Tabs>`. */
export function useSubTab(key: string, fallback: string): [string, (value: string) => void] {
  const value = useSubTabs((s) => s.open[key]) ?? fallback;
  const setValue = useSubTabs((s) => s.set);
  return [value, (v: string) => setValue(key, v)];
}
