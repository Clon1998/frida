import { create } from "zustand";

/**
 * App-wide dialogs that can be opened from several places (tree, tabs, editor). They are
 * mounted once in the app shell.
 */
interface DialogsState {
  /** "Copy as curl" for the request at `path`. */
  curlExport: { dir: string; path: string } | null;
  /** "New request from curl" in folder `parent` (`""` = collection root). */
  curlImport: { dir: string; parent: string } | null;
  /** Bumped after every send, so the history list refreshes. */
  historyVersion: number;
  openCurlExport: (dir: string, path: string) => void;
  openCurlImport: (dir: string, parent: string) => void;
  close: () => void;
  bumpHistory: () => void;
}

export const useDialogs = create<DialogsState>((set) => ({
  curlExport: null,
  curlImport: null,
  historyVersion: 0,
  openCurlExport: (dir, path) => set({ curlExport: { dir, path } }),
  openCurlImport: (dir, parent) => set({ curlImport: { dir, parent } }),
  close: () => set({ curlExport: null, curlImport: null }),
  bumpHistory: () => set((s) => ({ historyVersion: s.historyVersion + 1 })),
}));
