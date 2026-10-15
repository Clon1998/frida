import { create } from "zustand";
import { type ApiError, commands, type ResponseView } from "@/bindings";
import { showError, toApiError, unwrap } from "@/lib/api";
import { fromRust, type RequestModel as Request } from "@/lib/model";
import { isUnder } from "@/lib/tree";

export interface Tab {
  /** Collection-relative path of the request file. Identifies the tab. */
  path: string;
  /** The request as last loaded from / saved to disk. */
  original: Request;
  /** The request as currently edited. */
  draft: Request;
  response: ResponseView | null;
  error: ApiError | null;
  /** Set while a send is in flight; used to cancel it. */
  requestId: string | null;
  /** The file changed on disk while this tab had unsaved edits. */
  conflict: Request | null;
}

interface CollectionTabs {
  tabs: Tab[];
  active: string | null;
  /** Folders expanded in the sidebar tree (persisted together with the tabs). */
  expanded: string[];
}

interface TabsState {
  /** Tab sets by collection folder: each collection has its own tabs (PLAN.md §14). */
  byCollection: Record<string, CollectionTabs>;

  openTab: (dir: string, path: string) => Promise<void>;
  closeTab: (dir: string, path: string) => void;
  setActive: (dir: string, path: string) => void;
  updateDraft: (dir: string, path: string, update: (request: Request) => Request) => void;
  save: (dir: string, path: string) => Promise<void>;
  send: (dir: string, path: string) => Promise<void>;
  cancel: (dir: string, path: string) => Promise<void>;
  /** Restores the open tabs of a collection from local state. */
  restore: (dir: string, collectionId: string) => Promise<void>;
  /** A request or folder moved from `from` to `to`: update matching tab paths. */
  renamePaths: (dir: string, from: string, to: string) => void;
  /** A request or folder was deleted: close its tabs. */
  removePaths: (dir: string, path: string) => void;
  /** Files changed on disk: reload clean tabs, flag conflicts on dirty ones. */
  syncFromDisk: (dir: string) => Promise<void>;
  /** Keep my edits after a conflict, or take the version from disk. */
  resolveConflict: (dir: string, path: string, keep: "mine" | "disk") => void;
  toggleExpanded: (dir: string, folder: string, expanded?: boolean) => void;
}

const EMPTY: CollectionTabs = { tabs: [], active: null, expanded: [] };

export const isDirty = (tab: Tab) => !sameRequest(tab.draft, tab.original);

function sameRequest(a: Request, b: Request) {
  return JSON.stringify(a) === JSON.stringify(b);
}

export const useTabs = create<TabsState>((set, get) => {
  const tabsOf = (dir: string) => get().byCollection[dir] ?? EMPTY;

  const setCollection = (dir: string, update: (c: CollectionTabs) => CollectionTabs) =>
    set((s) => ({
      byCollection: { ...s.byCollection, [dir]: update(s.byCollection[dir] ?? EMPTY) },
    }));

  const updateTab = (dir: string, path: string, update: (tab: Tab) => Tab) =>
    setCollection(dir, (c) => ({
      ...c,
      tabs: c.tabs.map((t) => (t.path === path ? update(t) : t)),
    }));

  const newTab = (path: string, request: Request): Tab => ({
    path,
    original: request,
    draft: request,
    response: null,
    error: null,
    requestId: null,
    conflict: null,
  });

  return {
    byCollection: {},

    openTab: async (dir, path) => {
      if (tabsOf(dir).tabs.some((t) => t.path === path)) {
        get().setActive(dir, path);
        return;
      }
      try {
        const request = fromRust(await unwrap(commands.readRequest(dir, path)));
        setCollection(dir, (c) => ({
          ...c,
          tabs: [...c.tabs, newTab(path, request)],
          active: path,
        }));
      } catch (e) {
        showError(e);
      }
    },

    closeTab: (dir, path) =>
      setCollection(dir, (c) => {
        const index = c.tabs.findIndex((t) => t.path === path);
        const tabs = c.tabs.filter((t) => t.path !== path);
        let active = c.active;
        if (active === path) {
          // Activate the neighbour, like browsers do.
          active = tabs[Math.min(index, tabs.length - 1)]?.path ?? null;
        }
        return { ...c, tabs, active };
      }),

    setActive: (dir, path) => setCollection(dir, (c) => ({ ...c, active: path })),

    updateDraft: (dir, path, update) =>
      updateTab(dir, path, (t) => ({ ...t, draft: update(t.draft) })),

    save: async (dir, path) => {
      const tab = tabsOf(dir).tabs.find((t) => t.path === path);
      if (!tab) return;
      try {
        await unwrap(commands.saveRequest(dir, path, tab.draft));
        // Read back what was written (e.g. the method is normalized to upper case).
        const saved = fromRust(await unwrap(commands.readRequest(dir, path)));
        updateTab(dir, path, (t) => ({ ...t, original: saved, draft: saved, conflict: null }));
      } catch (e) {
        showError(e);
      }
    },

    send: async (dir, path) => {
      const tab = tabsOf(dir).tabs.find((t) => t.path === path);
      if (!tab || tab.requestId) return;
      const requestId = crypto.randomUUID();
      updateTab(dir, path, (t) => ({ ...t, requestId, error: null }));
      try {
        const response = await unwrap(commands.sendRequest(requestId, dir, path, tab.draft));
        updateTab(dir, path, (t) => ({ ...t, response, error: null, requestId: null }));
      } catch (e) {
        updateTab(dir, path, (t) => ({
          ...t,
          response: null,
          error: toApiError(e),
          requestId: null,
        }));
      }
    },

    cancel: async (dir, path) => {
      const requestId = tabsOf(dir).tabs.find((t) => t.path === path)?.requestId;
      if (requestId) await commands.cancelRequest(requestId);
    },

    restore: async (dir, collectionId) => {
      if (get().byCollection[dir]) return; // already open in this session
      try {
        const saved = await unwrap(commands.getCollectionState(collectionId));
        const state = fromRust(saved);
        const tabs: Tab[] = [];
        for (const path of state.openTabs) {
          // Files may have been deleted since; skip those quietly.
          const result = await commands.readRequest(dir, path);
          if (result.status === "ok") tabs.push(newTab(path, fromRust(result.data)));
        }
        const active = tabs.some((t) => t.path === state.activeTab)
          ? state.activeTab
          : (tabs[0]?.path ?? null);
        setCollection(dir, () => ({ tabs, active, expanded: state.expandedFolders }));
      } catch (e) {
        showError(e);
      }
    },

    renamePaths: (dir, from, to) =>
      setCollection(dir, (c) => {
        const rename = (p: string) => (isUnder(p, from) ? to + p.slice(from.length) : p);
        return {
          tabs: c.tabs.map((t) => ({ ...t, path: rename(t.path) })),
          active: c.active ? rename(c.active) : null,
          expanded: c.expanded.map(rename),
        };
      }),

    removePaths: (dir, path) => {
      for (const tab of tabsOf(dir).tabs.filter((t) => isUnder(t.path, path))) {
        get().closeTab(dir, tab.path);
      }
    },

    syncFromDisk: async (dir) => {
      for (const tab of tabsOf(dir).tabs) {
        const result = await commands.readRequest(dir, tab.path);
        if (result.status !== "ok") continue; // deleted or broken: leave the tab as is
        const disk = fromRust(result.data);
        if (sameRequest(disk, tab.original)) continue; // our own save, or no real change
        updateTab(dir, tab.path, (t) =>
          isDirty(t) ? { ...t, conflict: disk } : { ...t, original: disk, draft: disk },
        );
      }
    },

    toggleExpanded: (dir, folder, expanded) =>
      setCollection(dir, (c) => {
        const isOpen = c.expanded.includes(folder);
        const open = expanded ?? !isOpen;
        if (open === isOpen) return c;
        return {
          ...c,
          expanded: open ? [...c.expanded, folder] : c.expanded.filter((f) => f !== folder),
        };
      }),

    resolveConflict: (dir, path, keep) =>
      updateTab(dir, path, (t) => {
        if (!t.conflict) return t;
        return keep === "disk"
          ? { ...t, original: t.conflict, draft: t.conflict, conflict: null }
          : // Keep my edits; the disk version becomes the new baseline, so saving overwrites it.
            { ...t, original: t.conflict, conflict: null };
      }),
  };
});
