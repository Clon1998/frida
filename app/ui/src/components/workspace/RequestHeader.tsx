import { ChevronRight, Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import { folderTrail } from "@/lib/tree";
import { isDirty, type Tab, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";

/**
 * The bar above the editor: where the open item lives (collection › folders › item) and a Save
 * button while it has unsaved changes. The environment selectors join it on the right in M2.
 */
export function RequestHeader({ dir, tab }: { dir: string; tab: Tab | null }) {
  const collection = useActiveCollection();
  const tree = useWorkspace((s) => s.trees[dir]);
  const save = useTabs((s) => s.save);
  // A folder's own name is the last crumb, so its trail stops at its parent.
  const trail = tab && tab.kind !== "collection" && tree ? folderTrail(tree, tab.path) : [];
  const dirty = tab !== null && isDirty(tab);

  return (
    <div className="flex h-9 shrink-0 items-center gap-1 border-b px-3 text-sm">
      <span className={tab?.kind === "collection" ? "font-medium" : "text-muted-foreground"}>
        {tab?.kind === "collection" ? tab.draft.name : collection?.name}
        {tab?.kind === "collection" && " settings"}
      </span>
      {trail.map((name, i) => (
        <span key={i} className="flex items-center gap-1 text-muted-foreground">
          <ChevronRight className="size-3.5" />
          {name}
        </span>
      ))}
      {tab && tab.kind !== "collection" && (
        <span className="flex min-w-0 items-center gap-1">
          <ChevronRight className="size-3.5 text-muted-foreground" />
          <span className="truncate font-medium" title={tab.path}>
            {tab.draft.name}
            {tab.kind === "folder" && " settings"}
          </span>
        </span>
      )}
      <div className="flex-1" />
      {dirty && (
        <Button size="sm" variant="ghost" className="h-7" onClick={() => void save(dir, tab.path)}>
          <Save /> Save
        </Button>
      )}
    </div>
  );
}
