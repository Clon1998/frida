import { create } from "zustand";
import { type Branch, commands, type GitInfo } from "@/bindings";
import { showError, toApiError, unwrap } from "@/lib/api";
import { toast } from "sonner";

/** What the git panel and the status bar show for one collection. */
interface CollectionGit {
  info: GitInfo | null;
  branches: Branch[];
  /** The operation running right now ("push", "pull", ...), to disable buttons. */
  busy: string | null;
}

interface GitState {
  byCollection: Record<string, CollectionGit>;
  /** Re-reads the status (and branch list) of a collection. Quiet on errors. */
  refresh: (dir: string) => Promise<void>;
  /** Runs a git operation, then refreshes. Shows git's message on failure. */
  run: (dir: string, op: GitOp) => Promise<boolean>;
}

/** Every operation the panel can trigger. */
export type GitOp =
  | { kind: "init" }
  | { kind: "stage"; paths: string[] }
  | { kind: "unstage"; paths: string[] }
  | { kind: "commit"; message: string }
  | { kind: "fetch" }
  | { kind: "pull" }
  | { kind: "push" }
  | { kind: "switch"; branch: string }
  | { kind: "createBranch"; name: string };

const EMPTY: CollectionGit = { info: null, branches: [], busy: null };

const SUCCESS: Partial<Record<GitOp["kind"], string>> = {
  commit: "Committed",
  push: "Pushed",
  pull: "Pulled",
  fetch: "Fetched",
  init: "Git repository created",
};

function call(dir: string, op: GitOp) {
  switch (op.kind) {
    case "init":
      return commands.gitInit(dir);
    case "stage":
      return commands.gitStage(dir, op.paths);
    case "unstage":
      return commands.gitUnstage(dir, op.paths);
    case "commit":
      return commands.gitCommit(dir, op.message);
    case "fetch":
      return commands.gitFetch(dir);
    case "pull":
      return commands.gitPull(dir);
    case "push":
      return commands.gitPush(dir);
    case "switch":
      return commands.gitSwitch(dir, op.branch);
    case "createBranch":
      return commands.gitCreateBranch(dir, op.name);
  }
}

/** Git state per collection (PLAN.md §12). Each collection may live in its own repository. */
export const useGit = create<GitState>((set, get) => {
  const patch = (dir: string, p: Partial<CollectionGit>) =>
    set((s) => ({
      byCollection: { ...s.byCollection, [dir]: { ...(s.byCollection[dir] ?? EMPTY), ...p } },
    }));

  return {
    byCollection: {},

    refresh: async (dir) => {
      try {
        const info = await unwrap(commands.gitInfo(dir));
        const branches = info.kind === "repo" ? await unwrap(commands.gitBranches(dir)) : [];
        patch(dir, { info, branches });
      } catch (e) {
        // The status bar refreshes in the background; don't toast for it.
        console.warn("git status failed", toApiError(e).message);
      }
    },

    run: async (dir, op) => {
      patch(dir, { busy: op.kind });
      try {
        await unwrap(call(dir, op));
        const message = SUCCESS[op.kind];
        if (message) toast.success(message);
        return true;
      } catch (e) {
        showError(e);
        return false;
      } finally {
        patch(dir, { busy: null });
        await get().refresh(dir);
      }
    },
  };
});

/** The git info of a collection, or `null` before the first refresh. */
export const useGitInfo = (dir: string | null) =>
  useGit((s) => (dir ? (s.byCollection[dir]?.info ?? null) : null));
