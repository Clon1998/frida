import { ChevronRight } from "lucide-react";
import { folderTrail } from "@/lib/tree";
import { isDirty, type Tab } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";

/**
 * The bar above the request editor: where the request lives (collection › folders › request).
 * The environment selectors join it on the right in M2.
 */
export function RequestHeader({ dir, tab }: { dir: string; tab: Tab | null }) {
  const collection = useActiveCollection();
  const tree = useWorkspace((s) => s.trees[dir]);
  const trail = tab && tree ? folderTrail(tree, tab.path) : [];

  return (
    <div className="flex h-9 shrink-0 items-center gap-1 border-b px-3 text-sm">
      <span className="text-muted-foreground">{collection?.name}</span>
      {trail.map((name, i) => (
        <span key={i} className="flex items-center gap-1 text-muted-foreground">
          <ChevronRight className="size-3.5" />
          {name}
        </span>
      ))}
      {tab && (
        <span className="flex min-w-0 items-center gap-1">
          <ChevronRight className="size-3.5 text-muted-foreground" />
          <span className="truncate font-medium" title={tab.path}>
            {tab.draft.name}
          </span>
          {isDirty(tab) && <span className="text-xs text-muted-foreground">(unsaved)</span>}
        </span>
      )}
      <div className="flex-1" />
    </div>
  );
}
