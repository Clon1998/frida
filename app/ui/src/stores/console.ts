import { create } from "zustand";
import type { ConsoleEntry, ScriptLog } from "@/bindings";

/** How many items the console keeps (oldest are dropped). */
const MAX_ITEMS = 500;

/** How an expanded entry is shown: sections and tables, or the raw HTTP text. */
export type ConsoleView = "pretty" | "raw";

/** One line of the console: a send (request and response) or a script's `console.log`. */
export type ConsoleItem =
  { kind: "http"; id: string; entry: ConsoleEntry } | { kind: "log"; id: string; log: ScriptLog };

interface ConsoleState {
  items: ConsoleItem[];
  open: boolean;
  view: ConsoleView;
  add: (entry: ConsoleEntry) => void;
  addLog: (log: ScriptLog) => void;
  clear: () => void;
  toggle: () => void;
  setOpen: (open: boolean) => void;
  setView: (view: ConsoleView) => void;
}

let nextLogId = 0;

/** The console panel's log, in memory for this session (like Postman's console). */
export const useConsole = create<ConsoleState>((set) => ({
  items: [],
  open: false,
  view: "pretty",
  add: (entry) =>
    set((s) => ({
      items: [...s.items, { kind: "http" as const, id: entry.id, entry }].slice(-MAX_ITEMS),
    })),
  addLog: (log) =>
    set((s) => ({
      items: [...s.items, { kind: "log" as const, id: `log-${nextLogId++}`, log }].slice(
        -MAX_ITEMS,
      ),
    })),
  clear: () => set({ items: [] }),
  toggle: () => set((s) => ({ open: !s.open })),
  setOpen: (open) => set({ open }),
  setView: (view) => set({ view }),
}));
