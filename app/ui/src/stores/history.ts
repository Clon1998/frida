import { create } from "zustand";
import { commands, type HistorySummary } from "@/bindings";
import { showError, unwrap } from "@/lib/api";

interface HistoryState {
  /** The last loaded list per collection folder (for the search it was loaded with). */
  entries: Record<string, HistorySummary[]>;
  /** The search the cached list of a collection belongs to. */
  searches: Record<string, string>;
  /** Collections with a load in flight. */
  loading: Record<string, boolean>;
  /**
   * Loads the history of `dir`. The previous list stays visible meanwhile, so switching back
   * to the history tab shows it right away and only refreshes it.
   */
  load: (dir: string, search: string) => Promise<void>;
}

/** Per load, so an older reply arriving late doesn't overwrite a newer one. */
const latest: Record<string, number> = {};

/** The history sidebar's lists, kept while the sidebar shows another tab. */
export const useHistory = create<HistoryState>((set) => ({
  entries: {},
  searches: {},
  loading: {},
  load: async (dir, search) => {
    const id = (latest[dir] ?? 0) + 1;
    latest[dir] = id;
    set((s) => ({ loading: { ...s.loading, [dir]: true } }));
    try {
      const list = await unwrap(commands.listHistory(dir, search));
      if (latest[dir] === id)
        set((s) => ({
          entries: { ...s.entries, [dir]: list },
          searches: { ...s.searches, [dir]: search },
        }));
    } catch (e) {
      showError(e);
    } finally {
      if (latest[dir] === id) set((s) => ({ loading: { ...s.loading, [dir]: false } }));
    }
  },
}));
