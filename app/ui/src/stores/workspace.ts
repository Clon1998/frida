import { create } from "zustand";
import { type AppSettings, commands, type TreeNode, type WorkspaceOverview } from "@/bindings";
import { showError, unwrap } from "@/lib/api";

interface WorkspaceState {
  settings: AppSettings | null;
  workspace: WorkspaceOverview | null;
  /** The collection whose tree and tabs are shown. Collections are keyed by their folder. */
  activeCollectionDir: string | null;
  trees: Record<string, TreeNode[]>;

  loadSettings: () => Promise<void>;
  openWorkspace: (dir: string) => Promise<void>;
  createWorkspace: (dir: string, name: string) => Promise<void>;
  closeWorkspace: () => Promise<void>;
  reloadWorkspace: () => Promise<void>;
  createCollection: (dir: string, name: string) => Promise<void>;
  addCollection: (dir: string) => Promise<void>;
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

    createCollection: async (dir, name) => {
      try {
        const overview = await unwrap(commands.createCollection(dir, name));
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
