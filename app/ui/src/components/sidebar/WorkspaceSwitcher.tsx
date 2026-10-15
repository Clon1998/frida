import { open } from "@tauri-apps/plugin-dialog";
import { Briefcase, Check, ChevronsUpDown, FolderOpen, LogOut, Plus } from "lucide-react";
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
import { NameDialog } from "@/components/workspace/NameDialog";
import { useWorkspace } from "@/stores/workspace";

const NO_RECENT: string[] = [];

/** The last folder name of a path, e.g. `/work/eip` → `eip`. */
const folderName = (dir: string) => dir.split(/[\\/]/).filter(Boolean).pop() ?? dir;

/**
 * The current workspace, with a menu to switch to a recent one, open or create another, or
 * close it. Each collection keeps its open tabs, so switching back restores them.
 */
export function WorkspaceSwitcher() {
  const workspace = useWorkspace((s) => s.workspace);
  const recent = useWorkspace((s) => s.settings?.recentWorkspaces) ?? NO_RECENT;
  const openWorkspace = useWorkspace((s) => s.openWorkspace);
  const createWorkspace = useWorkspace((s) => s.createWorkspace);
  const closeWorkspace = useWorkspace((s) => s.closeWorkspace);
  const [newDir, setNewDir] = useState<string | null>(null);

  if (!workspace) return null;

  const pickExisting = async () => {
    const dir = await open({ directory: true, title: "Open a Frida workspace folder" });
    if (typeof dir === "string") await openWorkspace(dir);
  };
  const pickNew = async () => {
    const dir = await open({ directory: true, title: "Choose a folder for the new workspace" });
    if (typeof dir === "string") setNewDir(dir);
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            className="h-7 w-full justify-between px-2 text-xs text-muted-foreground"
            title={workspace.dir}
          >
            <span className="flex min-w-0 items-center gap-1.5">
              <Briefcase className="size-3.5 shrink-0" />
              <span className="truncate">{workspace.name}</span>
            </span>
            <ChevronsUpDown className="size-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-72">
          <DropdownMenuLabel className="text-xs text-muted-foreground">
            Recent workspaces
          </DropdownMenuLabel>
          {recent.map((dir) => (
            <DropdownMenuItem
              key={dir}
              title={dir}
              onSelect={() => dir !== workspace.dir && void openWorkspace(dir)}
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate">{folderName(dir)}</span>
                <span className="truncate text-xs text-muted-foreground">{dir}</span>
              </span>
              {dir === workspace.dir && <Check className="size-4" />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => void pickExisting()}>
            <FolderOpen /> Open workspace…
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void pickNew()}>
            <Plus /> New workspace…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => void closeWorkspace()}>
            <LogOut /> Close workspace
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <NameDialog
        open={newDir !== null}
        title="New workspace"
        label="Workspace name"
        onOpenChange={(o) => !o && setNewDir(null)}
        onConfirm={(name) => {
          if (newDir) void createWorkspace(newDir, name);
        }}
      />
    </>
  );
}
