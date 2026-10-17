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
  /**
   * "Choose an AWS profile" for an environment with AWS secrets (PLAN.md §9.1), opened when a
   * send needs one; `then` retries the send.
   */
  awsProfile: {
    dir: string;
    environment: string;
    global: boolean;
    /** Set when a secret's account alias needs a profile, rather than the environment. */
    account?: string;
    then: () => void;
  } | null;
  openAwsProfile: (request: NonNullable<DialogsState["awsProfile"]>) => void;
  /**
   * "Import Frida archive…": `dir`/`parent` is where a folder or request archive goes (`dir`
   * `null` when no collection is open).
   */
  archiveImport: { dir: string | null; parent: string } | null;
  openArchiveImport: (dir: string | null, parent: string) => void;
  /** Tabs waiting for the "unsaved changes" question before they close (see closeTabsAsking). */
  closeTabs: { dir: string; paths: string[] } | null;
  openCloseTabs: (dir: string, paths: string[]) => void;
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
  awsProfile: null,
  closeTabs: null,
  archiveImport: null,
  openArchiveImport: (dir, parent) => set({ archiveImport: { dir, parent } }),
  openCloseTabs: (dir, paths) => set({ closeTabs: { dir, paths } }),
  openAwsProfile: (awsProfile) => set({ awsProfile }),
  openCurlExport: (dir, path) => set({ curlExport: { dir, path } }),
  openCurlImport: (dir, parent) => set({ curlImport: { dir, parent } }),
  close: () =>
    set({
      curlExport: null,
      curlImport: null,
      awsProfile: null,
      closeTabs: null,
      archiveImport: null,
    }),
  bumpHistory: () => set((s) => ({ historyVersion: s.historyVersion + 1 })),
}));
