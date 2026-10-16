import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, Settings } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { NameDialog } from "./NameDialog";

/** Stable empty value: a selector returning a fresh `[]` each time would re-render forever. */
const NO_RECENT: string[] = [];

/** Shown when no workspace is open: create one, open one, or pick a recent one. */
export function StartScreen() {
  const recent = useWorkspace((s) => s.settings?.recentWorkspaces) ?? NO_RECENT;
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
      {recent.length > 0 && (
        <div className="w-full max-w-md">
          <h2 className="mb-1 text-xs font-medium text-muted-foreground">Recent</h2>
          <ul className="rounded-md border">
            {recent.map((dir) => (
              <li key={dir}>
                <button
                  type="button"
                  className="w-full truncate px-3 py-2 text-left font-mono text-xs hover:bg-accent"
                  onClick={() => void openWorkspace(dir)}
                >
                  {dir}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
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
