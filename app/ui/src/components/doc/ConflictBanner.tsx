import { Button } from "@/components/ui/button";
import { type DocumentTab, useTabs } from "@/stores/tabs";

/** Shown when the tab's file changed on disk while it had unsaved edits. */
export function ConflictBanner({ dir, tab }: { dir: string; tab: DocumentTab }) {
  const resolveConflict = useTabs((s) => s.resolveConflict);
  if (!tab.conflict) return null;
  return (
    <div className="flex items-center gap-2 border-b bg-warning/10 px-3 py-2 text-sm">
      <span className="flex-1">This file changed on disk while you were editing it.</span>
      <Button size="sm" variant="outline" onClick={() => resolveConflict(dir, tab.path, "disk")}>
        Reload from disk
      </Button>
      <Button size="sm" variant="outline" onClick={() => resolveConflict(dir, tab.path, "mine")}>
        Keep my version
      </Button>
    </div>
  );
}
