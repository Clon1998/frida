import { toast } from "sonner";
import { create } from "zustand";
import { commands, events, type UpdateInfo } from "@/bindings";
import { showError, toApiError, unwrap } from "@/lib/api";

/** Where the update is (PLAN.md M14). */
export type UpdatePhase = "idle" | "checking" | "downloading" | "installing";

interface UpdateState {
  /** The newer version found by the last check, if any. */
  available: UpdateInfo | null;
  phase: UpdatePhase;
  /** Download progress, while downloading. `total` is null when the server doesn't say. */
  progress: { downloaded: number; total: number | null } | null;
  dialogOpen: boolean;
  /** The check on start: quiet, at most once a day (Rust decides), never in dev builds. */
  checkOnStart: () => Promise<void>;
  /** "Check for updates" in the settings: says "up to date" or shows errors. */
  checkNow: () => Promise<void>;
  /** Downloads, installs and restarts. Resolves only if it failed or was cancelled. */
  install: () => Promise<void>;
  cancel: () => void;
  setDialogOpen: (open: boolean) => void;
}

/** The toast id, so a second check doesn't stack a second notice. */
const NOTICE_ID = "update-available";

export const useUpdate = create<UpdateState>((set, get) => {
  const found = (info: UpdateInfo) => {
    set({ available: info });
    toast(`Frida ${info.version} is available`, {
      id: NOTICE_ID,
      description: `You have ${info.currentVersion}.`,
      duration: Infinity,
      action: { label: "Details…", onClick: () => set({ dialogOpen: true }) },
      cancel: { label: "Later", onClick: () => toast.dismiss(NOTICE_ID) },
    });
  };

  return {
    available: null,
    phase: "idle",
    progress: null,
    dialogOpen: false,

    checkOnStart: async () => {
      // Errors stay quiet here (Rust already returns nothing when offline).
      const info = await unwrap(commands.checkForUpdate(false)).catch(() => null);
      if (info) found(info);
    },

    checkNow: async () => {
      if (get().phase !== "idle") return;
      set({ phase: "checking" });
      try {
        const info = await unwrap(commands.checkForUpdate(true));
        if (info) {
          found(info);
          set({ dialogOpen: true });
        } else {
          set({ available: null });
          toast.success("Frida is up to date");
        }
      } catch (e) {
        showError(e);
      } finally {
        set({ phase: "idle" });
      }
    },

    install: async () => {
      if (get().phase !== "idle") return;
      set({ phase: "downloading", progress: { downloaded: 0, total: null } });
      const unlisten = await events.updateProgress.listen(({ payload }) => {
        const done = payload.total !== null && payload.downloaded >= payload.total;
        set({ progress: payload, phase: done ? "installing" : "downloading" });
      });
      try {
        // On success the app restarts and this never returns.
        await unwrap(commands.installUpdate());
      } catch (e) {
        if (toApiError(e).code !== "cancelled") showError(e);
      } finally {
        unlisten();
        set({ phase: "idle", progress: null });
      }
    },

    cancel: () => void commands.cancelUpdate(),

    setDialogOpen: (dialogOpen) => {
      // Closing the dialog while downloading would hide the only cancel button.
      if (!dialogOpen && get().phase !== "idle") return;
      set({ dialogOpen });
      if (dialogOpen) toast.dismiss(NOTICE_ID);
    },
  };
});
