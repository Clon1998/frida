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
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { HistoryPanel } from "@/components/history/HistoryPanel";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CollectionTree } from "./CollectionTree";

export function Sidebar() {
  const workspace = useWorkspace((s) => s.workspace);
  const active = useActiveCollection();
  const setActive = useWorkspace((s) => s.setActiveCollection);
  const addCollection = useWorkspace((s) => s.addCollection);
  const createCollection = useWorkspace((s) => s.createCollection);
  const removeCollection = useWorkspace((s) => s.removeCollection);
  const closeWorkspace = useWorkspace((s) => s.closeWorkspace);
  const [creating, setCreating] = useState(false);
  const [view, setView] = useState<"tree" | "history">("tree");
  const [removing, setRemoving] = useState(false);
  const openTab = useTabs((s) => s.openTab);

  if (!workspace) return null;

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
            <DropdownMenuItem onSelect={() => setCreating(true)}>
              <Plus /> New collection…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void pickExistingCollection()}>
              <FolderPlus /> Add existing collection…
            </DropdownMenuItem>
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
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void closeWorkspace()}>
              <LogOut /> Close workspace
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        {active && (
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
          onValueChange={(v) => setView(v as "tree" | "history")}
          className="px-2 pt-2"
        >
          <TabsList className="w-full">
            <TabsTrigger value="tree">Requests</TabsTrigger>
            <TabsTrigger value="history">History</TabsTrigger>
          </TabsList>
        </Tabs>
      )}
      {active && view === "history" ? (
        <div className="flex min-h-0 flex-1 flex-col pt-2">
          <HistoryPanel key={active.dir} dir={active.dir} />
        </div>
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
        </div>
      )}

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
