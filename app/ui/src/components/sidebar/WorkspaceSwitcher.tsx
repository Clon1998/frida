import { open } from "@tauri-apps/plugin-dialog";
import {
  Briefcase,
  Check,
  ChevronsUpDown,
  FolderOpen,
  LogOut,
  Pin,
  PinOff,
  Plus,
  X,
} from "lucide-react";
import type { MouseEvent, ReactNode } from "react";
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
import { folderName, workspaceLists } from "@/lib/workspaces";
import { useWorkspace } from "@/stores/workspace";

/**
 * The current workspace, with a menu to switch to a pinned or recent one, open or create
 * another, or close it. Each collection keeps its open tabs, so switching back restores them.
 * Hovering an entry shows Pin/Unpin and "Remove from list".
 */
export function WorkspaceSwitcher() {
  const workspace = useWorkspace((s) => s.workspace);
  const settings = useWorkspace((s) => s.settings);
  const openWorkspace = useWorkspace((s) => s.openWorkspace);
  const createWorkspace = useWorkspace((s) => s.createWorkspace);
  const closeWorkspace = useWorkspace((s) => s.closeWorkspace);
  const [newDir, setNewDir] = useState<string | null>(null);

  if (!workspace) return null;
  const { pinned, recent } = workspaceLists(settings);

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
          {pinned.length > 0 && <MenuLabel>Pinned</MenuLabel>}
          {pinned.map((dir) => (
            <WorkspaceItem key={dir} dir={dir} current={dir === workspace.dir} pinned />
          ))}
          {recent.length > 0 && <MenuLabel>Recent</MenuLabel>}
          {recent.map((dir) => (
            <WorkspaceItem key={dir} dir={dir} current={dir === workspace.dir} pinned={false} />
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

function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <DropdownMenuLabel className="text-xs text-muted-foreground">{children}</DropdownMenuLabel>
  );
}

/** One workspace in the menu: open it, or (on hover) pin/unpin it or remove it from the list. */
function WorkspaceItem({
  dir,
  current,
  pinned,
}: {
  dir: string;
  current: boolean;
  pinned: boolean;
}) {
  const openWorkspace = useWorkspace((s) => s.openWorkspace);
  const pinWorkspace = useWorkspace((s) => s.pinWorkspace);
  const forgetWorkspace = useWorkspace((s) => s.forgetWorkspace);
  // The buttons sit inside the menu item: stop the click there, or it would also open the
  // workspace and close the menu.
  const action = (run: () => void) => (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    run();
  };
  return (
    <DropdownMenuItem
      title={dir}
      className="group"
      onSelect={() => !current && void openWorkspace(dir)}
    >
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate">{folderName(dir)}</span>
        <span className="truncate text-xs text-muted-foreground">{dir}</span>
      </span>
      <span className="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          className="hidden rounded p-0.5 text-muted-foreground group-hover:block group-focus:block hover:bg-muted hover:text-foreground"
          title={pinned ? "Unpin" : "Pin"}
          aria-label={pinned ? "Unpin workspace" : "Pin workspace"}
          onClick={action(() => void pinWorkspace(dir, !pinned))}
        >
          {pinned ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
        </button>
        {!current && (
          <button
            type="button"
            className="hidden rounded p-0.5 text-muted-foreground group-hover:block group-focus:block hover:bg-muted hover:text-foreground"
            title="Remove from list"
            aria-label="Remove from list"
            onClick={action(() => void forgetWorkspace(dir))}
          >
            <X className="size-3.5" />
          </button>
        )}
        {current && <Check className="size-4" />}
      </span>
    </DropdownMenuItem>
  );
}
