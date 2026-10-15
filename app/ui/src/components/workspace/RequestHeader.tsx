import { ChevronRight, Save } from "lucide-react";
import { EnvironmentPicker } from "@/components/env/EnvironmentPicker";
import { Button } from "@/components/ui/button";
import { folderTrail } from "@/lib/tree";
import * as actions from "@/stores/actions";
import { isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { EditableCrumb } from "./EditableCrumb";

/**
 * The bar above the editor: where the open item lives (collection › folders › item), each part
 * renamable by clicking it, a Save button while there are unsaved changes, and the environment
 * picker.
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
    <div className="flex h-10 shrink-0 items-center gap-0.5 border-b px-3 text-sm">
      {collection && (
        <EditableCrumb
          className="text-muted-foreground"
          value={collection.name}
          onRename={(name) => void actions.renameCollection(dir, name)}
        />
      )}
      {trail.map((folder) => (
        <span key={folder.path} className="flex min-w-0 items-center gap-0.5">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          <EditableCrumb
            className="text-muted-foreground"
            value={folder.name}
            onRename={(name) => void actions.renameItem(dir, folder.path, name)}
          />
        </span>
      ))}
      {tab && tab.kind !== "collection" && (
        <span className="flex min-w-0 items-center gap-0.5">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          {tab.kind === "environments" ? (
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
          <span className="px-1 font-medium">Settings</span>
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
