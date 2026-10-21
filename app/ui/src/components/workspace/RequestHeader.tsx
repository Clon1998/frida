import { Braces, ChevronRight, Play, Save } from "lucide-react";
import { EnvironmentPicker } from "@/components/env/EnvironmentPicker";
import { Button } from "@/components/ui/button";
import { envColor } from "@/lib/envColors";
import { folderTrail } from "@/lib/tree";
import { useEnvs } from "@/stores/envs";
import { useUi } from "@/stores/ui";
import { cn } from "cn";
import * as actions from "@/stores/actions";
import { isDirty, isDocument, runnerTabPath, type Tab, tabTitle, useTabs } from "@/stores/tabs";
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
  const updateDraft = useTabs((s) => s.updateDraft);
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
          {!isDocument(tab) ? (
            <span className="px-1 font-medium">{tabTitle(tab)}</span>
          ) : (
            <EditableCrumb
              className="font-medium"
              title={`${tab.path} · click to rename (saved with the tab)`}
              value={tab.draft.name}
              // An edit like any other: saving writes it and renames the file to match.
              onRename={(name) =>
                updateDraft(dir, tab.path, (doc) => ({ ...doc, name: name.trim() }))
              }
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
      {(tab?.kind === "folder" || tab?.kind === "collection") && (
        <Button
          size="sm"
          variant="ghost"
          className="h-7"
          title={`Run every request in this ${tab.kind}`}
          onClick={() => void useTabs.getState().openTab(dir, runnerTabPath(tab.path))}
        >
          <Play /> Run
        </Button>
      )}
      {tab?.kind === "request" && <InspectorButton />}
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

/** Opens the variables inspector of the active request; the badge counts unresolved variables. */
function InspectorButton() {
  const open = useUi((s) => s.inspectorOpen);
  const toggle = useUi((s) => s.toggleInspector);
  const unresolved = useUi((s) => s.unresolved);
  return (
    <Button
      variant={open ? "secondary" : "ghost"}
      size="icon"
      className="relative mr-1 size-7"
      title="Variables inspector"
      aria-label="Variables inspector"
      onClick={toggle}
    >
      <Braces />
      {unresolved > 0 && (
        <span className="absolute -top-1 -right-1 flex size-4 items-center justify-center rounded-full bg-destructive text-[10px] text-white">
          {unresolved}
        </span>
      )}
    </Button>
  );
}
