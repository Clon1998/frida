import { create } from "zustand";

/** Small bits of UI state shared between the header bar and the request editor. */
interface UiState {
  /** Whether the variables inspector panel is open. */
  inspectorOpen: boolean;
  /** Unresolved variables in the active request, for the badge on the inspector button. */
  unresolved: number;
  toggleInspector: () => void;
  setInspectorOpen: (open: boolean) => void;
  setUnresolved: (count: number) => void;
}

export const useUi = create<UiState>((set) => ({
  inspectorOpen: false,
  unresolved: 0,
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setUnresolved: (unresolved) => set({ unresolved }),
}));
