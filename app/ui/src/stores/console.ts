import { create } from "zustand";
import type { ConsoleEntry } from "@/bindings";

/** How many entries the console keeps (oldest are dropped). */
const MAX_ENTRIES = 300;

/** How an expanded entry is shown: sections and tables, or the raw HTTP text. */
export type ConsoleView = "pretty" | "raw";

interface ConsoleState {
  entries: ConsoleEntry[];
  open: boolean;
  view: ConsoleView;
  add: (entry: ConsoleEntry) => void;
  clear: () => void;
  toggle: () => void;
  setOpen: (open: boolean) => void;
  setView: (view: ConsoleView) => void;
}

/** The console panel's log, in memory for this session (like Postman's console). */
export const useConsole = create<ConsoleState>((set) => ({
  entries: [],
  open: false,
  view: "pretty",
  add: (entry) => set((s) => ({ entries: [...s.entries, entry].slice(-MAX_ENTRIES) })),
  clear: () => set({ entries: [] }),
  toggle: () => set((s) => ({ open: !s.open })),
  setOpen: (open) => set({ open }),
  setView: (view) => set({ view }),
}));
