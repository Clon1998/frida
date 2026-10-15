import { ChevronRight, Save } from "lucide-react";
import { EnvironmentPicker } from "@/components/env/EnvironmentPicker";
import { Button } from "@/components/ui/button";
import { envColor } from "@/lib/envColors";
import { folderTrail } from "@/lib/tree";
import { useEnvs } from "@/stores/envs";
import { cn } from "cn";
import * as actions from "@/stores/actions";
import { isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { EditableCrumb } from "./EditableCrumb";

/**
 * The bar above the editor: where the open item lives (collection › folders › item), with the
 * open item renamable by clicking its name, a Save button while there are unsaved changes, and
 * the environment picker.
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
  const color = useActiveEnvColor();

  return (
    <div
      className={cn(
        "flex h-10 shrink-0 items-center gap-0.5 border-b px-3 text-sm",
        // A colored line in the active environment's color, e.g. red for production.
        color && ["border-b-2", color.line],
      )}
    >
      {/* Only the open entry can be renamed: the collection only from its settings tab. */}
      {collection && tab?.kind === "collection" ? (
        <EditableCrumb
          className="font-medium"
          value={collection.name}
          onRename={(name) => void actions.renameCollection(dir, name)}
        />
      ) : (
        <span className="truncate px-1 text-muted-foreground">{collection?.name}</span>
      )}
      {trail.map((folder) => (
        <span key={folder.path} className="flex min-w-0 items-center gap-0.5">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate px-1 text-muted-foreground">{folder.name}</span>
        </span>
      ))}
      {tab && tab.kind !== "collection" && (
        <span className="flex min-w-0 items-center gap-0.5">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          {tab.kind === "environments" || tab.kind === "history" ? (
            <span className="px-1 font-medium">{tabTitle(tab)}</span>
          ) : (
            <EditableCrumb
              className="font-medium"
              title={`${tab.path} · click to rename`}
              value={tab.draft.name}
              onRename={(name) => void actions.renameItem(dir, tab.path, name)}
            />
          )}
          {tab.kind === "folder" && <span className="text-muted-foreground">settings</span>}
        </span>
      )}
      {tab?.kind === "collection" && (
        <span className="flex items-center gap-0.5">
          <ChevronRight className="size-3.5 text-muted-foreground" />
          <span className="px-1 text-muted-foreground">Settings</span>
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

/** The color of the active environment (collection env first, then global), if any. */
function useActiveEnvColor() {
  return useEnvs((s) => {
    const find = (list: typeof s.collection, name: string | null) =>
      envColor(list.find((e) => e.environment.name === name)?.environment.color);
    return find(s.collection, s.activeCollection) ?? find(s.global, s.activeGlobal);
  });
}
