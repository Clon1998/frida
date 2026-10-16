import type { AppSettings } from "@/bindings";

/** How many recent (unpinned) workspaces the menu and the start screen show. */
export const SHOWN_RECENT = 5;

/** The last folder name of a path, e.g. `/work/eip` → `eip`. */
export const folderName = (dir: string) => dir.split(/[\\/]/).filter(Boolean).pop() ?? dir;

/**
 * The workspaces to offer: the pinned ones, then the most recent others (at most
 * {@link SHOWN_RECENT}, without the pinned ones and without `current`).
 */
export function workspaceLists(
  settings: AppSettings | null,
  current: string | null = null,
): { pinned: string[]; recent: string[] } {
  const pinned = settings?.pinnedWorkspaces ?? [];
  const recent = (settings?.recentWorkspaces ?? [])
    .filter((dir) => !pinned.includes(dir) && dir !== current)
    .slice(0, SHOWN_RECENT);
  return { pinned, recent };
}
