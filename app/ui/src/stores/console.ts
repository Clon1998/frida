import { create } from "zustand";
import type { ConsoleEntry } from "@/bindings";

/** How many entries the console keeps (oldest are dropped). */
const MAX_ENTRIES = 300;

interface ConsoleState {
  entries: ConsoleEntry[];
  open: boolean;
  /** Entries already seen with the console open; the rest is the badge count. */
  seen: number;
  add: (entry: ConsoleEntry) => void;
  clear: () => void;
  toggle: () => void;
  setOpen: (open: boolean) => void;
}

/** The console panel's log, in memory for this session (like Postman's console). */
export const useConsole = create<ConsoleState>((set) => ({
  entries: [],
  open: false,
  seen: 0,
  add: (entry) =>
    set((s) => {
      const entries = [...s.entries, entry].slice(-MAX_ENTRIES);
      return { entries, seen: s.open ? entries.length : Math.min(s.seen, entries.length) };
    }),
  clear: () => set({ entries: [], seen: 0 }),
  toggle: () => set((s) => ({ open: !s.open, seen: s.entries.length })),
  setOpen: (open) => set((s) => ({ open, seen: s.entries.length })),
}));
