import { ChevronRight, Save } from "lucide-react";
import { EnvironmentPicker } from "@/components/env/EnvironmentPicker";
import { Button } from "@/components/ui/button";
import { folderTrail } from "@/lib/tree";
import { isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";

/**
 * The bar above the editor: where the open item lives (collection › folders › item), a Save
 * button while it has unsaved changes, and the environment selectors.
 */
export function RequestHeader({ dir, tab }: { dir: string; tab: Tab | null }) {
  const collection = useActiveCollection();
  const tree = useWorkspace((s) => s.trees[dir]);
  const save = useTabs((s) => s.save);
  const trail =
    tab && (tab.kind === "request" || tab.kind === "folder") && tree
      ? folderTrail(tree, tab.path)
      : [];
  const dirty = tab !== null && isDirty(tab);

  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b px-3 text-sm">
      <span className="truncate text-muted-foreground">{collection?.name}</span>
      {trail.map((name, i) => (
        <span key={i} className="flex items-center gap-1 text-muted-foreground">
          <ChevronRight className="size-3.5" />
          {name}
        </span>
      ))}
      {tab && (
        <span className="flex min-w-0 items-center gap-1">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate font-medium" title={tab.path}>
            {tab.kind === "collection" ? "Settings" : tabTitle(tab)}
            {tab.kind === "folder" && " settings"}
          </span>
        </span>
      )}
      {dirty && (
        <Button size="sm" variant="ghost" className="h-7" onClick={() => void save(dir, tab.path)}>
          <Save /> Save
        </Button>
      )}
      <div className="flex-1" />
      <EnvironmentPicker dir={dir} />
    </div>
  );
}
