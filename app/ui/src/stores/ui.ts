import { create } from "zustand";
import type { ResolveReport } from "@/bindings";

/** Small bits of UI state shared between the header bar and the request editor. */
interface UiState {
  /** Whether the variables inspector panel is open. */
  inspectorOpen: boolean;
  /** Unresolved variables in the active request, for the badge on the inspector button. */
  unresolved: number;
  /** The active request's variable report, shown in the inspector drawer. */
  report: ResolveReport | null;
  setReport: (report: ResolveReport | null) => void;
  toggleInspector: () => void;
  setInspectorOpen: (open: boolean) => void;
  setUnresolved: (count: number) => void;
  /** What the sidebar shows below the collection picker. */
  sidebarView: "tree" | "history" | "git";
  setSidebarView: (view: "tree" | "history" | "git") => void;
}

export const useUi = create<UiState>((set) => ({
  inspectorOpen: false,
  unresolved: 0,
  report: null,
  setReport: (report) =>
    set({ report, unresolved: report?.vars.filter((v) => !v.resolved).length ?? 0 }),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setUnresolved: (unresolved) => set({ unresolved }),
  sidebarView: "tree",
  setSidebarView: (sidebarView) => set({ sidebarView }),
}));
