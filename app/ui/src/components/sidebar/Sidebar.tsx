import { open } from "@tauri-apps/plugin-dialog";
import { ChevronsUpDown, FolderPlus, LogOut, Plus, Settings } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { CollectionSettingsDialog } from "@/components/workspace/CollectionSettingsDialog";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { CollectionTree } from "./CollectionTree";

export function Sidebar() {
  const workspace = useWorkspace((s) => s.workspace);
  const active = useActiveCollection();
  const setActive = useWorkspace((s) => s.setActiveCollection);
  const addCollection = useWorkspace((s) => s.addCollection);
  const createCollection = useWorkspace((s) => s.createCollection);
  const removeCollection = useWorkspace((s) => s.removeCollection);
  const closeWorkspace = useWorkspace((s) => s.closeWorkspace);
  const [newCollectionDir, setNewCollectionDir] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  if (!workspace) return null;

  const pickNewCollectionFolder = async () => {
    const dir = await open({ directory: true, title: "Choose an empty folder for the collection" });
    if (typeof dir === "string") setNewCollectionDir(dir);
  };
  const pickExistingCollection = async () => {
    const dir = await open({ directory: true, title: "Choose a collection folder" });
    if (typeof dir === "string") await addCollection(dir);
  };

  return (
    <aside className="flex h-full flex-col border-r bg-muted/30">
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
            <DropdownMenuItem onSelect={() => void pickNewCollectionFolder()}>
              <Plus /> New collection…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void pickExistingCollection()}>
              <FolderPlus /> Add existing collection…
            </DropdownMenuItem>
            {active && (
              <DropdownMenuItem onSelect={() => setSettingsOpen(true)}>
                <Settings /> Collection settings…
              </DropdownMenuItem>
            )}
            {active && (
              <DropdownMenuItem onSelect={() => setRemoving(true)}>
                Remove “{active.name}” from workspace
              </DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void closeWorkspace()}>
              <LogOut /> Close workspace
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {active ? (
        <CollectionTree key={active.dir} dir={active.dir} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-4 text-center">
          <p className="text-sm text-muted-foreground">This workspace has no collections yet.</p>
          <Button size="sm" onClick={() => void pickNewCollectionFolder()}>
            New collection…
          </Button>
          <Button size="sm" variant="outline" onClick={() => void pickExistingCollection()}>
            Add existing…
          </Button>
        </div>
      )}

      <NameDialog
        open={newCollectionDir !== null}
        title="New collection"
        label="Collection name"
        onOpenChange={(o) => !o && setNewCollectionDir(null)}
        onConfirm={(name) => {
          if (newCollectionDir) void createCollection(newCollectionDir, name);
        }}
      />
      {active && (
        <CollectionSettingsDialog
          dir={active.dir}
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
        />
      )}
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
