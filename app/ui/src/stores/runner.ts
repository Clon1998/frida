import { create } from "zustand";
import {
  type ApiError,
  commands,
  events,
  type RunItem,
  type RunProgress,
  type RunSummary,
} from "@/bindings";
import { toApiError, unwrap } from "@/lib/api";

/** One collection or folder run (PLAN.md M11). */
export interface RunState {
  /** The requests the run will send, in order (shown before the first run). */
  items: RunItem[];
  /** One entry per finished request, in order. */
  results: RunProgress[];
  summary: RunSummary | null;
  /** Set while the run is going on; used to stop it. */
  runId: string | null;
  /** Index into `results` of the result shown on the right. */
  selected: number | null;
  error: ApiError | null;
}

const EMPTY: RunState = {
  items: [],
  results: [],
  summary: null,
  runId: null,
  selected: null,
  error: null,
};

interface RunnerStore {
  /** By {@link runKey}. */
  runs: Record<string, RunState>;
  /** Pause between requests, for every run. */
  delayMs: number;
  setDelay: (delayMs: number) => void;
  /** Loads the list of requests a run would send. */
  plan: (dir: string, folder: string) => Promise<void>;
  start: (dir: string, folder: string) => Promise<void>;
  stop: (dir: string, folder: string) => Promise<void>;
  select: (dir: string, folder: string, index: number) => void;
  /** A `RunProgress` event arrived. */
  addProgress: (progress: RunProgress) => void;
}

export const runKey = (dir: string, folder: string) => `${dir}\n${folder}`;

export const useRunner = create<RunnerStore>((set, get) => {
  const update = (key: string, change: (run: RunState) => RunState) =>
    set((s) => ({ runs: { ...s.runs, [key]: change(s.runs[key] ?? EMPTY) } }));

  return {
    runs: {},
    delayMs: 0,
    setDelay: (delayMs) => set({ delayMs }),

    plan: async (dir, folder) => {
      const key = runKey(dir, folder);
      try {
        const items = await unwrap(commands.planRun(dir, folder));
        update(key, (run) => ({ ...run, items, error: null }));
      } catch (e) {
        update(key, (run) => ({ ...run, error: toApiError(e) }));
      }
    },

    start: async (dir, folder) => {
      const key = runKey(dir, folder);
      if (get().runs[key]?.runId) return;
      const runId = crypto.randomUUID();
      update(key, (run) => ({
        ...run,
        runId,
        results: [],
        summary: null,
        selected: null,
        error: null,
      }));
      // Refresh the list too: requests may have been added since the tab was opened.
      void get().plan(dir, folder);
      try {
        const options = { delayMs: get().delayMs };
        const summary = await unwrap(commands.runCollection(runId, dir, folder, options));
        update(key, (run) => ({ ...run, summary, runId: null }));
      } catch (e) {
        update(key, (run) => ({ ...run, error: toApiError(e), runId: null }));
      }
    },

    stop: async (dir, folder) => {
      const runId = get().runs[runKey(dir, folder)]?.runId;
      if (runId) await commands.cancelRequest(runId);
    },

    select: (dir, folder, index) =>
      update(runKey(dir, folder), (run) => ({ ...run, selected: index })),

    addProgress: (progress) => {
      const entry = Object.entries(get().runs).find(([, run]) => run.runId === progress.runId);
      if (!entry) return;
      update(entry[0], (run) => ({
        ...run,
        results: [...run.results, progress],
        // Follow the run: show the newest result unless the user picked one.
        selected:
          run.selected === null || run.selected === run.results.length - 1
            ? run.results.length
            : run.selected,
      }));
    },
  };
});

/** Routes `RunProgress` events to the store. Returns a function that stops listening. */
export function listenToRunProgress(): () => void {
  const unlisten = events.runProgress.listen(({ payload }) =>
    useRunner.getState().addProgress(payload),
  );
  return () => void unlisten.then((stop) => stop());
}
