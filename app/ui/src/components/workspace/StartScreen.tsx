import { open } from "@tauri-apps/plugin-dialog";
import { FileArchive, FolderOpen, Pin, PinOff, Plus, Settings, X } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { folderName, workspaceLists } from "@/lib/workspaces";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { useDialogs } from "@/stores/dialogs";
import { ArchiveImportDialog } from "./ArchiveImportDialog";
import { NameDialog } from "./NameDialog";

/** Shown when no workspace is open: create one, open one, or pick a recent one. */
export function StartScreen() {
  const { pinned, recent } = workspaceLists(useWorkspace((s) => s.settings));
  const openWorkspace = useWorkspace((s) => s.openWorkspace);
  const createWorkspace = useWorkspace((s) => s.createWorkspace);
  const [newDir, setNewDir] = useState<string | null>(null);

  const pickExisting = async () => {
    const dir = await open({ directory: true, title: "Open a Frida workspace folder" });
    if (typeof dir === "string") await openWorkspace(dir);
  };
  const pickNew = async () => {
    const dir = await open({ directory: true, title: "Choose a folder for the new workspace" });
    if (typeof dir === "string") setNewDir(dir);
  };

  return (
    <main className="relative flex h-full flex-col items-center justify-center gap-6 p-6">
      <Button
        variant="ghost"
        size="icon"
        className="absolute top-3 right-3"
        title="Settings"
        aria-label="Settings"
        onClick={() => useUi.getState().setSettingsOpen(true)}
      >
        <Settings />
      </Button>
      <div className="text-center">
        {/* The Westie from the app icon (public/westie.svg), in the text colour: a CSS mask, so it
            follows the light/dark theme. */}
        <div
          aria-hidden
          className="mx-auto mb-3 h-24 w-28 bg-foreground"
          style={{
            mask: "url(/westie.svg) center / contain no-repeat",
            WebkitMask: "url(/westie.svg) center / contain no-repeat",
          }}
        />
        <h1 className="text-3xl font-semibold tracking-tight">Frida</h1>
        <p className="text-muted-foreground">Fast Request Inspector for Developer APIs</p>
      </div>
      <div className="flex gap-2">
        <Button onClick={() => void pickNew()}>
          <Plus /> New workspace…
        </Button>
        <Button variant="outline" onClick={() => void pickExisting()}>
          <FolderOpen /> Open workspace…
        </Button>
        <Button variant="outline" onClick={() => useDialogs.getState().openArchiveImport(null, "")}>
          <FileArchive /> Import archive…
        </Button>
      </div>
      <ul className="max-w-md space-y-1.5 text-sm text-muted-foreground">
        <li>
          <span className="font-medium text-foreground">A workspace</span> is a folder that lists
          your collections and holds shared (global) environments.
        </li>
        <li>
          <span className="font-medium text-foreground">Collections</span> are folders of plain YAML
          files, one per request: commit them to git and review changes like code.
        </li>
        <li>
          <span className="font-medium text-foreground">Secrets</span> stay in your OS keychain (or
          AWS Secrets Manager), never in the files.
        </li>
        <li>
          Coming from Postman or Bruno? Import an OpenAPI file or a Bruno collection from the
          collection menu once a workspace is open.
        </li>
      </ul>
      <WorkspaceList title="Pinned" dirs={pinned} pinned />
      <WorkspaceList title="Recent" dirs={recent} pinned={false} />
      <ArchiveImportDialog />
      <NameDialog
        open={newDir !== null}
        title="New workspace"
        label="Workspace name"
        onOpenChange={(o) => !o && setNewDir(null)}
        onConfirm={(name) => {
          if (newDir) void createWorkspace(newDir, name);
        }}
      />
    </main>
  );
}

/** Pinned or recent workspaces. Right-click an entry to pin/unpin it or remove it. */
function WorkspaceList({
  title,
  dirs,
  pinned,
}: {
  title: string;
  dirs: string[];
  pinned: boolean;
}) {
  const openWorkspace = useWorkspace((s) => s.openWorkspace);
  const pinWorkspace = useWorkspace((s) => s.pinWorkspace);
  const forgetWorkspace = useWorkspace((s) => s.forgetWorkspace);
  if (dirs.length === 0) return null;
  return (
    <div className="w-full max-w-md">
      <h2 className="mb-1 flex items-center gap-1 text-xs font-medium text-muted-foreground">
        {pinned && <Pin className="size-3" />} {title}
      </h2>
      <ul className="rounded-md border">
        {dirs.map((dir) => (
          <li key={dir}>
            <ContextMenu>
              <ContextMenuTrigger asChild>
                <button
                  type="button"
                  title={`${dir}\nRight-click to pin or remove`}
                  className="flex w-full items-baseline gap-2 px-3 py-2 text-left hover:bg-accent"
                  onClick={() => void openWorkspace(dir)}
                >
                  <span className="shrink-0 text-sm text-foreground">{folderName(dir)}</span>
                  <span className="truncate font-mono text-xs text-muted-foreground">{dir}</span>
                </button>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem onSelect={() => void pinWorkspace(dir, !pinned)}>
                  {pinned ? (
                    <>
                      <PinOff /> Unpin
                    </>
                  ) : (
                    <>
                      <Pin /> Pin
                    </>
                  )}
                </ContextMenuItem>
                <ContextMenuItem onSelect={() => void forgetWorkspace(dir)}>
                  <X /> Remove from list
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          </li>
        ))}
      </ul>
    </div>
  );
}
