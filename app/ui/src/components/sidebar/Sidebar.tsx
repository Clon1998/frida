import {
  DndContext,
  type DragEndEvent,
  type DragMoveEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronsUpDown,
  Download,
  FileArchive,
  FileUp,
  FolderPlus,
  Library,
  Plus,
  Settings,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { collectionMoveIndex, dropPosition } from "@/lib/tree";
import { exportArchive } from "@/stores/actions";
import { useDialogs } from "@/stores/dialogs";
import { useTabs } from "@/stores/tabs";
import { type ImportKind, useActiveCollection, useWorkspace } from "@/stores/workspace";
import { HistoryPanel } from "@/components/history/HistoryPanel";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CollectionTree } from "./CollectionTree";
import { ImportCollectionDialog } from "./ImportCollectionDialog";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";
import { GitPanel } from "@/components/git/GitPanel";
import { useHistory } from "@/stores/history";
import { useUi } from "@/stores/ui";

export function Sidebar() {
  const workspace = useWorkspace((s) => s.workspace);
  const active = useActiveCollection();
  const setActive = useWorkspace((s) => s.setActiveCollection);
  const addCollection = useWorkspace((s) => s.addCollection);
  const createCollection = useWorkspace((s) => s.createCollection);
  const removeCollection = useWorkspace((s) => s.removeCollection);
  const [creating, setCreating] = useState(false);
  const [importing, setImporting] = useState<ImportKind | null>(null);
  const view = useUi((s) => s.sidebarView);
  const setView = useUi((s) => s.setSidebarView);
  const [removing, setRemoving] = useState(false);
  const openTab = useTabs((s) => s.openTab);
  const showAll = useWorkspace((s) => s.settings?.showAllCollections ?? false);
  const setShowAll = useWorkspace((s) => s.setShowAllCollections);

  // Load the history in the background as soon as a collection is active, so the history tab
  // has its list ready when it is opened.
  const activeDir = active?.dir;
  useEffect(() => {
    if (activeDir && useHistory.getState().entries[activeDir] === undefined)
      void useHistory.getState().load(activeDir, "");
  }, [activeDir]);

  // In the "all collections" view the active collection is expanded, also after picking it in
  // the menu. Others stay as the user left them (collapsed at first).
  useEffect(() => {
    if (showAll && activeDir) useUi.getState().setCollectionOpen(activeDir, true);
  }, [showAll, activeDir]);

  if (!workspace) return null;

  const pickExistingCollection = async () => {
    const dir = await open({ directory: true, title: "Choose a collection folder" });
    if (typeof dir === "string") await addCollection(dir);
  };

  return (
    <aside className="flex h-full flex-col border-r bg-muted/30">
      <div className="border-b px-2 pt-2">
        <WorkspaceSwitcher />
      </div>
      <div className="flex items-center gap-1 border-b p-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" className="h-8 min-w-0 flex-1 justify-between px-2">
              <span className="truncate font-medium">{active?.name ?? "No collection"}</span>
              <ChevronsUpDown className="text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-64">
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              {workspace.name}
            </DropdownMenuLabel>
            {workspace.collections.map((c) => (
              <DropdownMenuItem
                key={c.dir}
                disabled={c.error !== null}
                title={c.error ?? c.dir}
                onSelect={() => setActive(c.dir)}
              >
                <span className="truncate">{c.name}</span>
                {c.error && <span className="ml-auto text-xs text-destructive">missing</span>}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setCreating(true)}>
              <Plus /> New collection…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void pickExistingCollection()}>
              <FolderPlus /> Add existing collection…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setImporting("openapi")}>
              <FileUp /> Import OpenAPI…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setImporting("bruno")}>
              <FileUp /> Import Bruno collection…
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => useDialogs.getState().openArchiveImport(active?.dir ?? null, "")}
            >
              <FileArchive /> Import Frida archive…
            </DropdownMenuItem>
            {active && (
              <DropdownMenuItem
                onSelect={() =>
                  void exportArchive({ kind: "collection", collectionDir: active.dir }, active.name)
                }
              >
                <Download /> Export “{active.name}”…
              </DropdownMenuItem>
            )}
            {active && (
              <DropdownMenuItem onSelect={() => void openTab(active.dir, "")}>
                <Settings /> Collection settings…
              </DropdownMenuItem>
            )}
            {active && (
              <DropdownMenuItem onSelect={() => setRemoving(true)}>
                Remove “{active.name}” from workspace
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        {active && (
          <Button
            variant={showAll ? "secondary" : "ghost"}
            size="icon"
            className="size-8"
            title={showAll ? "Show only this collection" : "Show all collections"}
            aria-label="Show all collections"
            aria-pressed={showAll}
            onClick={() => void setShowAll(!showAll)}
          >
            <Library />
          </Button>
        )}
        {active && !showAll && (
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            title="Collection settings"
            aria-label="Collection settings"
            onClick={() => void openTab(active.dir, "")}
          >
            <Settings />
          </Button>
        )}
      </div>

      {active && (
        <Tabs
          value={view}
          onValueChange={(v) => setView(v as "tree" | "history" | "git")}
          className="px-2 pt-2"
        >
          {/* One row even in a narrow sidebar: labels shrink (and cut off) instead of wrapping. */}
          <TabsList className="w-full flex-nowrap [&>button]:text-xs">
            <TabsTrigger value="tree" className="min-w-0 flex-1 px-1">
              <span className="truncate">Requests</span>
            </TabsTrigger>
            <TabsTrigger value="history" className="min-w-0 flex-1 px-1">
              <span className="truncate">History</span>
            </TabsTrigger>
            <TabsTrigger value="git" className="min-w-0 flex-1 px-1">
              <span className="truncate">Git</span>
            </TabsTrigger>
          </TabsList>
        </Tabs>
      )}
      {active && view === "git" ? (
        <div className="flex min-h-0 flex-1 flex-col pt-2">
          <GitPanel key={active.dir} dir={active.dir} />
        </div>
      ) : active && view === "history" ? (
        <div className="flex min-h-0 flex-1 flex-col pt-2">
          <HistoryPanel key={active.dir} dir={active.dir} />
        </div>
      ) : active && showAll ? (
        <AllCollections dirs={workspace.collections.map((c) => c.dir)} />
      ) : active ? (
        <CollectionTree key={active.dir} dir={active.dir} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-4 text-center">
          <p className="text-sm text-muted-foreground">This workspace has no collections yet.</p>
          <Button size="sm" onClick={() => setCreating(true)}>
            New collection…
          </Button>
          <Button size="sm" variant="outline" onClick={() => void pickExistingCollection()}>
            Add existing…
          </Button>
          <Button size="sm" variant="outline" onClick={() => setImporting("openapi")}>
            Import OpenAPI…
          </Button>
          <Button size="sm" variant="outline" onClick={() => setImporting("bruno")}>
            Import Bruno…
          </Button>
        </div>
      )}

      <ImportCollectionDialog kind={importing} onOpenChange={(o) => !o && setImporting(null)} />
      <NameDialog
        open={creating}
        title="New collection"
        label="Collection name"
        onOpenChange={setCreating}
        onConfirm={(name) => void createCollection(name)}
      />
      <ConfirmDialog
        open={removing}
        title={`Remove “${active?.name}” from the workspace?`}
        description="The collection's files stay on disk. You can add it again later."
        confirmLabel="Remove"
        onOpenChange={setRemoving}
        onConfirm={() => {
          if (active) void removeCollection(active.dir);
        }}
      />
    </aside>
  );
}

/**
 * Every collection as a section (the "all collections" view). Dragging a section's header
 * moves the collection, which changes the order in `frida-workspace.yaml` for everyone.
 */
function AllCollections({ dirs }: { dirs: string[] }) {
  const collections = useWorkspace((s) => s.workspace?.collections ?? []);
  const moveCollection = useWorkspace((s) => s.moveCollection);
  const [drop, setDrop] = useState<{ over: string; position: "before" | "after" } | null>(null);
  // Like the request tree: dragging starts after a few pixels, so clicks still expand.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  const onDragMove = (event: DragMoveEvent) => {
    const { over, activatorEvent, delta } = event;
    if (!over || !(activatorEvent instanceof PointerEvent)) {
      setDrop(null);
      return;
    }
    const position = dropPosition(activatorEvent.clientY + delta.y, over.rect, false);
    setDrop({ over: String(over.id), position: position === "before" ? "before" : "after" });
  };

  const onDragEnd = (event: DragEndEvent) => {
    const dragged = String(event.active.id);
    const index = drop && collectionMoveIndex(dirs, dragged, drop.over, drop.position);
    setDrop(null);
    if (index !== null) void moveCollection(dragged, index);
  };

  return (
    <div className="mt-2 min-h-0 flex-1 overflow-auto border-t">
      <DndContext
        sensors={sensors}
        onDragMove={onDragMove}
        onDragEnd={onDragEnd}
        onDragCancel={() => setDrop(null)}
      >
        {collections
          .filter((c) => c.error === null)
          .map((c) => (
            <CollectionTree
              key={c.dir}
              dir={c.dir}
              section
              dropEdge={drop?.over === c.dir ? drop.position : null}
            />
          ))}
      </DndContext>
    </div>
  );
}
