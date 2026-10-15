import { create } from "zustand";
import { type ApiError, commands, type ResponseView } from "@/bindings";
import { showError, toApiError, unwrap } from "@/lib/api";
import { type CollectionModel, type FolderModel, fromRust, type RequestModel } from "@/lib/model";
import { isUnder } from "@/lib/tree";
import { useDialogs } from "./dialogs";
import { useWorkspace } from "./workspace";

/** Fields every tab has: the document as saved on disk and as edited. */
interface DocTab<Kind extends string, Doc> {
  kind: Kind;
  /** Collection-relative path: a request file, a folder, or `""` for the collection itself. */
  path: string;
  /** The document as last loaded from / saved to disk. */
  original: Doc;
  /** The document as currently edited. */
  draft: Doc;
  /** The file changed on disk while this tab had unsaved edits. */
  conflict: Doc | null;
}

export interface RequestTab extends DocTab<"request", RequestModel> {
  response: ResponseView | null;
  error: ApiError | null;
  /** Set while a send is in flight; used to cancel it. */
  requestId: string | null;
}
export type FolderTab = DocTab<"folder", FolderModel>;
export type CollectionTab = DocTab<"collection", CollectionModel>;

/** Path of the environment editor tab (not a file, so it can't clash with a real path). */
export const ENVIRONMENTS_TAB = "@environments";

/** The environment editor. It edits several files and tracks its own changes. */
export interface EnvironmentsTab {
  kind: "environments";
  path: typeof ENVIRONMENTS_TAB;
}

/** A history entry, opened read-only (PLAN.md §13). Path: `@history/<id>`. */
export interface HistoryTab {
  kind: "history";
  path: string;
  /** e.g. "GET Get order", shown on the tab. */
  title: string;
}

export const HISTORY_TAB_PREFIX = "@history/";

/** A tab backed by one file, with a saved and an edited version. */
export type DocumentTab = RequestTab | FolderTab | CollectionTab;

/** A tab shows a request, a folder's or the collection's settings, or the environments. */
export type Tab = DocumentTab | EnvironmentsTab | HistoryTab;
export type TabKind = Tab["kind"];
type DocKind = DocumentTab["kind"];

interface CollectionTabs {
  tabs: Tab[];
  active: string | null;
  /** Folders expanded in the sidebar tree (persisted together with the tabs). */
  expanded: string[];
  /**
   * The preview tab (italic), like Bruno and VS Code: opened by a single click, replaced by the
   * next single click and closed when another tab is activated. Editing or double-clicking it
   * makes it a normal tab.
   */
  preview?: string | null;
}

interface TabsState {
  /** Tab sets by collection folder: each collection has its own tabs (PLAN.md §14). */
  byCollection: Record<string, CollectionTabs>;

  /**
   * Opens (or focuses) the tab for `path`. The kind follows from the path, see {@link kindOf}.
   * With `preview`, it opens as the preview tab (see {@link CollectionTabs.preview}).
   */
  openTab: (dir: string, path: string, options?: { preview?: boolean }) => Promise<void>;
  /** Turns the preview tab at `path` into a normal tab. */
  pin: (dir: string, path: string) => void;
  closeTab: (dir: string, path: string) => void;
  /** Closes several tabs at once (close others / to the right / saved / all). */
  closeTabs: (dir: string, paths: string[]) => void;
  /**
   * A request, folder or the collection (`""`) at `path` was renamed to `name` on disk: update
   * the open tab's saved and edited name, so the change isn't mistaken for an external edit.
   */
  applyRename: (dir: string, path: string, name: string) => void;
  /** Throws away unsaved edits of a tab. */
  revert: (dir: string, path: string) => void;
  setActive: (dir: string, path: string) => void;
  /** Edits a tab's draft. `T` says which kind of tab the caller expects. */
  updateDraft: <T extends DocumentTab>(
    dir: string,
    path: string,
    update: (draft: T["draft"]) => T["draft"],
  ) => void;
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

const EMPTY: CollectionTabs = { tabs: [], active: null, expanded: [], preview: null };

/**
 * Makes `path` the active tab. An untouched preview tab is closed when another tab becomes
 * active, so browsing the sidebar doesn't pile up tabs.
 */
function activate(c: CollectionTabs, path: string | null): CollectionTabs {
  const preview = c.preview ?? null;
  if (preview === null || preview === path) return { ...c, active: path };
  const previewTab = c.tabs.find((t) => t.path === preview);
  if (previewTab && isDirty(previewTab)) return { ...c, active: path, preview: null };
  return { ...c, tabs: c.tabs.filter((t) => t.path !== preview), active: path, preview: null };
}

/**
 * The kind of tab for a path: `""` is the collection, `*.yaml` a request, anything else a
 * folder. Deriving it from the path keeps the persisted tab list a plain list of paths.
 */
export function kindOf(path: string): TabKind {
  if (path === ENVIRONMENTS_TAB) return "environments";
  if (path.startsWith(HISTORY_TAB_PREFIX)) return "history";
  if (path === "") return "collection";
  return path.endsWith(".yaml") ? "request" : "folder";
}

export const isDocument = (tab: Tab): tab is DocumentTab =>
  tab.kind !== "environments" && tab.kind !== "history";

export const isDirty = (tab: Tab) => isDocument(tab) && !same(tab.draft, tab.original);

/** The name shown on the tab and in the header. */
export const tabTitle = (tab: Tab) => {
  switch (tab.kind) {
    case "environments":
      return "Environments";
    case "history":
      return tab.title;
    default:
      return tab.draft.name;
  }
};

function same(a: unknown, b: unknown) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Reads the document a tab shows from disk. */
async function readDoc(dir: string, kind: DocKind, path: string): Promise<DocumentTab["draft"]> {
  switch (kind) {
    case "request":
      return fromRust(await unwrap(commands.readRequest(dir, path)));
    case "folder":
      return fromRust(await unwrap(commands.readFolder(dir, path)));
    case "collection":
      return fromRust(await unwrap(commands.readCollection(dir)));
  }
}

async function writeDoc(dir: string, tab: DocumentTab) {
  switch (tab.kind) {
    case "request":
      return unwrap(commands.saveRequest(dir, tab.path, tab.draft));
    case "folder":
      return unwrap(commands.saveFolder(dir, tab.path, tab.draft));
    case "collection":
      return unwrap(commands.saveCollection(dir, tab.draft));
  }
}

function newTab(kind: DocKind, path: string, doc: DocumentTab["draft"]): Tab {
  // The casts are safe: `doc` was read for exactly this `kind` by `readDoc`.
  switch (kind) {
    case "request": {
      const request = doc as RequestModel;
      return {
        kind,
        path,
        original: request,
        draft: request,
        conflict: null,
        response: null,
        error: null,
        requestId: null,
      };
    }
    case "folder":
      return {
        kind,
        path,
        original: doc as FolderModel,
        draft: doc as FolderModel,
        conflict: null,
      };
    case "collection":
      return {
        kind,
        path,
        original: doc as CollectionModel,
        draft: doc as CollectionModel,
        conflict: null,
      };
  }
}

export const useTabs = create<TabsState>((set, get) => {
  const tabsOf = (dir: string) => get().byCollection[dir] ?? EMPTY;
  const findTab = (dir: string, path: string) => tabsOf(dir).tabs.find((t) => t.path === path);

  const setCollection = (dir: string, update: (c: CollectionTabs) => CollectionTabs) =>
    set((s) => ({
      byCollection: { ...s.byCollection, [dir]: update(s.byCollection[dir] ?? EMPTY) },
    }));

  /** Updates the document tab at `path` (the environments tab has no document). */
  const updateTab = (dir: string, path: string, update: (tab: DocumentTab) => DocumentTab) =>
    setCollection(dir, (c) => ({
      ...c,
      tabs: c.tabs.map((t) => (t.path === path && isDocument(t) ? update(t) : t)),
    }));

  const updateRequestTab = (dir: string, path: string, update: (tab: RequestTab) => RequestTab) =>
    updateTab(dir, path, (t) => (t.kind === "request" ? update(t) : t));

  /** Puts a freshly read document into a tab as both original and draft. */
  const withDoc = (tab: DocumentTab, doc: DocumentTab["draft"]): DocumentTab =>
    ({ ...tab, original: doc, draft: doc, conflict: null }) as DocumentTab;

  /** Creates the tab for `path`, reading its document from disk if it has one. */
  const loadTab = async (dir: string, path: string): Promise<Tab> => {
    const kind = kindOf(path);
    if (kind === "environments") return { kind, path: ENVIRONMENTS_TAB };
    if (kind === "history") {
      const id = Number(path.slice(HISTORY_TAB_PREFIX.length));
      const entry = await unwrap(commands.getHistoryEntry(id));
      return { kind, path, title: `${entry.summary.method} ${entry.summary.requestName}` };
    }
    return newTab(kind, path, await readDoc(dir, kind, path));
  };

  return {
    byCollection: {},

    openTab: async (dir, path, options) => {
      const preview = options?.preview === true;
      if (findTab(dir, path)) {
        get().setActive(dir, path);
        if (!preview) get().pin(dir, path);
        return;
      }
      try {
        const tab = await loadTab(dir, path);
        setCollection(dir, (c) => {
          const old = c.preview ?? null;
          const oldTab = c.tabs.find((t) => t.path === old);
          if (preview && old !== null && oldTab && !isDirty(oldTab)) {
            // The new preview takes the old preview's place in the tab bar.
            const tabs = c.tabs.map((t) => (t.path === old ? tab : t));
            return { ...c, tabs, active: path, preview: path };
          }
          const opened = activate({ ...c, tabs: [...c.tabs, tab] }, path);
          return { ...opened, preview: preview ? path : (opened.preview ?? null) };
        });
      } catch (e) {
        showError(e);
      }
    },

    pin: (dir, path) =>
      setCollection(dir, (c) => (c.preview === path ? { ...c, preview: null } : c)),

    closeTab: (dir, path) =>
      setCollection(dir, (c) => {
        const index = c.tabs.findIndex((t) => t.path === path);
        const tabs = c.tabs.filter((t) => t.path !== path);
        let active = c.active;
        if (active === path) {
          // Activate the neighbour, like browsers do.
          active = tabs[Math.min(index, tabs.length - 1)]?.path ?? null;
        }
        const preview = c.preview === path ? null : (c.preview ?? null);
        return { ...c, tabs, active, preview };
      }),

    closeTabs: (dir, paths) =>
      setCollection(dir, (c) => {
        const closing = new Set(paths);
        const tabs = c.tabs.filter((t) => !closing.has(t.path));
        // Keep the active tab if it survives; otherwise pick the nearest remaining one.
        let active = c.active;
        if (active !== null && closing.has(active)) {
          const index = c.tabs.findIndex((t) => t.path === active);
          const after = c.tabs.slice(index).find((t) => !closing.has(t.path));
          const before = c.tabs
            .slice(0, index)
            .reverse()
            .find((t) => !closing.has(t.path));
          active = (after ?? before)?.path ?? null;
        }
        const preview = c.preview && closing.has(c.preview) ? null : (c.preview ?? null);
        return { ...c, tabs, active, preview };
      }),

    applyRename: (dir, path, name) =>
      updateTab(
        dir,
        path,
        (t) =>
          ({
            ...t,
            original: { ...t.original, name },
            draft: { ...t.draft, name },
          }) as DocumentTab,
      ),

    revert: (dir, path) =>
      updateTab(dir, path, (t) => ({ ...t, draft: t.original, conflict: null }) as DocumentTab),

    setActive: (dir, path) => setCollection(dir, (c) => activate(c, path)),

    updateDraft: (dir, path, update) => {
      // `as never`: the caller chose `T` to match the tab at `path`; TS can't check that link.
      updateTab(dir, path, (t) => ({ ...t, draft: update(t.draft as never) }) as DocumentTab);
      // Editing a preview tab keeps it open.
      get().pin(dir, path);
    },

    save: async (dir, path) => {
      const tab = findTab(dir, path);
      if (!tab || !isDocument(tab)) return;
      try {
        await writeDoc(dir, tab);
        // Read back what was written (e.g. the method is normalized to upper case).
        const saved = await readDoc(dir, tab.kind, path);
        updateTab(dir, path, (t) => withDoc(t, saved));
        // The collection name is shown in the sidebar, which reads it from the workspace overview.
        if (tab.kind === "collection") await useWorkspace.getState().reloadWorkspace();
      } catch (e) {
        showError(e);
      }
    },

    send: async (dir, path) => {
      const tab = findTab(dir, path);
      if (tab?.kind !== "request" || tab.requestId) return;
      const requestId = crypto.randomUUID();
      updateRequestTab(dir, path, (t) => ({ ...t, requestId, error: null }));
      try {
        const response = await unwrap(commands.sendRequest(requestId, dir, path, tab.draft));
        updateRequestTab(dir, path, (t) => ({ ...t, response, error: null, requestId: null }));
      } catch (e) {
        updateRequestTab(dir, path, (t) => ({
          ...t,
          response: null,
          error: toApiError(e),
          requestId: null,
        }));
      }
      // A history entry was written; let the history list refresh.
      useDialogs.getState().bumpHistory();
    },

    cancel: async (dir, path) => {
      const tab = findTab(dir, path);
      if (tab?.kind === "request" && tab.requestId) await commands.cancelRequest(tab.requestId);
    },

    restore: async (dir, collectionId) => {
      if (get().byCollection[dir]) return; // already open in this session
      try {
        const state = fromRust(await unwrap(commands.getCollectionState(collectionId)));
        const tabs: Tab[] = [];
        for (const path of state.openTabs) {
          // Files may have been deleted since; skip those quietly.
          try {
            tabs.push(await loadTab(dir, path));
          } catch {
            // skip
          }
        }
        const active = tabs.some((t) => t.path === state.activeTab)
          ? state.activeTab
          : (tabs[0]?.path ?? null);
        setCollection(dir, () => ({
          tabs,
          active,
          expanded: state.expandedFolders,
          preview: null,
        }));
      } catch (e) {
        showError(e);
      }
    },

    renamePaths: (dir, from, to) =>
      setCollection(dir, (c) => {
        const rename = (p: string) => (isUnder(p, from) ? to + p.slice(from.length) : p);
        return {
          tabs: c.tabs.map((t) => (isDocument(t) ? { ...t, path: rename(t.path) } : t)),
          active: c.active !== null ? rename(c.active) : null,
          expanded: c.expanded.map(rename),
          preview: c.preview ? rename(c.preview) : null,
        };
      }),

    removePaths: (dir, path) => {
      for (const tab of tabsOf(dir).tabs.filter(
        (t) => t.kind !== "collection" && isUnder(t.path, path),
      )) {
        get().closeTab(dir, tab.path);
      }
    },

    syncFromDisk: async (dir) => {
      for (const tab of tabsOf(dir).tabs.filter(isDocument)) {
        let disk: DocumentTab["draft"];
        try {
          disk = await readDoc(dir, tab.kind, tab.path);
        } catch {
          continue; // deleted or broken: leave the tab as is
        }
        if (same(disk, tab.original)) continue; // our own save, or no real change
        updateTab(dir, tab.path, (t) =>
          isDirty(t) ? ({ ...t, conflict: disk } as DocumentTab) : withDoc(t, disk),
        );
      }
    },

    resolveConflict: (dir, path, keep) =>
      updateTab(dir, path, (t) => {
        if (!t.conflict) return t;
        return keep === "disk"
          ? withDoc(t, t.conflict)
          : // Keep my edits; the disk version becomes the new baseline, so saving overwrites it.
            ({ ...t, original: t.conflict, conflict: null } as DocumentTab);
      }),

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
  };
});
