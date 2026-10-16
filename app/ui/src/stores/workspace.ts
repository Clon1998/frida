import { create } from "zustand";
import {
  type AppSettings,
  commands,
  type ImportSummary,
  type Preferences,
  type TreeNode,
  type WorkspaceOverview,
} from "@/bindings";
import { showError, unwrap } from "@/lib/api";

/** What can be imported as a new collection. */
export type ImportKind = "openapi" | "bruno";

interface WorkspaceState {
  settings: AppSettings | null;
  workspace: WorkspaceOverview | null;
  /** The collection whose tree and tabs are shown. Collections are keyed by their folder. */
  activeCollectionDir: string | null;
  trees: Record<string, TreeNode[]>;

  loadSettings: () => Promise<void>;
  /**
   * Saves the Settings dialog. Shown right away; Rust corrects values out of range and applies
   * what isn't UI (git program, AWS).
   */
  savePreferences: (preferences: Preferences) => Promise<void>;
  openWorkspace: (dir: string) => Promise<void>;
  createWorkspace: (dir: string, name: string) => Promise<void>;
  closeWorkspace: () => Promise<void>;
  reloadWorkspace: () => Promise<void>;
  /** Creates a collection named `name` in the workspace's `collections/` folder. */
  createCollection: (name: string) => Promise<void>;
  addCollection: (dir: string) => Promise<void>;
  /**
   * Imports an OpenAPI file or a Bruno collection folder as a new collection (in `target`, or
   * the workspace's `collections/` folder) and selects it. Throws on failure, so the dialog
   * can show it.
   */
  importCollection: (
    kind: ImportKind,
    source: string,
    target: string | null,
  ) => Promise<ImportSummary>;
  removeCollection: (dir: string) => Promise<void>;
  setActiveCollection: (dir: string) => void;
  reloadTree: (dir: string) => Promise<void>;
}

export const useWorkspace = create<WorkspaceState>((set, get) => {
  /** Applies a fresh overview: keeps the active collection if it still exists. */
  const applyOverview = async (workspace: WorkspaceOverview) => {
    const usable = workspace.collections.filter((c) => c.error === null);
    const current = get().activeCollectionDir;
    const active = usable.find((c) => c.dir === current)?.dir ?? usable[0]?.dir ?? null;
    set({ workspace, activeCollectionDir: active });
    await Promise.all(usable.map((c) => get().reloadTree(c.dir)));
  };

  return {
    settings: null,
    workspace: null,
    activeCollectionDir: null,
    trees: {},

    loadSettings: async () => {
      set({ settings: await commands.getSettings() });
    },

    savePreferences: async (preferences) => {
      const withPreferences = (p: Preferences) => {
        const settings = get().settings;
        if (settings) set({ settings: { ...settings, preferences: p } });
      };
      withPreferences(preferences);
      try {
        withPreferences(await unwrap(commands.savePreferences(preferences)));
      } catch (e) {
        showError(e);
      }
    },

    openWorkspace: async (dir) => {
      try {
        await applyOverview(await unwrap(commands.openWorkspace(dir)));
        await get().loadSettings();
      } catch (e) {
        showError(e);
      }
    },

    createWorkspace: async (dir, name) => {
      try {
        await applyOverview(await unwrap(commands.createWorkspace(dir, name)));
        await get().loadSettings();
      } catch (e) {
        showError(e);
      }
    },

    closeWorkspace: async () => {
      await commands.closeWorkspace();
      set({ workspace: null, activeCollectionDir: null, trees: {} });
      await get().loadSettings();
    },

    reloadWorkspace: async () => {
      try {
        await applyOverview(await unwrap(commands.reloadWorkspace()));
      } catch (e) {
        showError(e);
      }
    },

    createCollection: async (name) => {
      try {
        const [overview, dir] = await unwrap(commands.createCollection(name));
        set({ activeCollectionDir: dir });
        await applyOverview(overview);
      } catch (e) {
        showError(e);
      }
    },

    addCollection: async (dir) => {
      try {
        await applyOverview(await unwrap(commands.addCollection(dir)));
      } catch (e) {
        showError(e);
      }
    },

    importCollection: async (kind, source, target) => {
      const result = await unwrap(
        kind === "openapi"
          ? commands.importOpenapi(source, target)
          : commands.importBruno(source, target),
      );
      set({ activeCollectionDir: result.dir });
      await applyOverview(result.overview);
      return result.summary;
    },

    removeCollection: async (dir) => {
      try {
        await applyOverview(await unwrap(commands.removeCollection(dir)));
      } catch (e) {
        showError(e);
      }
    },

    setActiveCollection: (dir) => set({ activeCollectionDir: dir }),

    reloadTree: async (dir) => {
      try {
        const tree = await unwrap(commands.getTree(dir));
        set((s) => ({ trees: { ...s.trees, [dir]: tree } }));
      } catch (e) {
        showError(e);
      }
    },
  };
});

/** The summary of the active collection, if any. */
export function useActiveCollection() {
  return useWorkspace((s) => s.workspace?.collections.find((c) => c.dir === s.activeCollectionDir));
}
