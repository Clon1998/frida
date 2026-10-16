import { open } from "@tauri-apps/plugin-dialog";
import { FileArchive, FileUp } from "lucide-react";
import { useState } from "react";
import { type ArchiveSummary, commands, type ImportTarget } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { toApiError, unwrap } from "@/lib/api";
import { useDialogs } from "@/stores/dialogs";
import { useTabs } from "@/stores/tabs";
import { useWorkspace } from "@/stores/workspace";

const KIND_LABEL: Record<ArchiveSummary["manifest"]["kind"], string> = {
  workspace: "workspace",
  collection: "collection",
  folder: "folder",
  request: "request",
};

/**
 * "Import Frida archive…" (PLAN.md M12): choose a `.zip`, see what it contains, then import it
 * where it fits: a workspace into an empty folder (then opened), a collection into this
 * workspace, a folder or request into the chosen folder of the active collection.
 */
export function ArchiveImportDialog() {
  const request = useDialogs((s) => s.archiveImport);
  const close = useDialogs((s) => s.close);
  return (
    <Dialog open={request !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">
        {/* Mounted only while open, so every import starts fresh. */}
        {request && <ImportFlow dir={request.dir} parent={request.parent} onClose={close} />}
      </DialogContent>
    </Dialog>
  );
}

function ImportFlow({
  dir,
  parent,
  onClose,
}: {
  dir: string | null;
  parent: string;
  onClose: () => void;
}) {
  const importArchive = useWorkspace((s) => s.importArchive);
  const hasWorkspace = useWorkspace((s) => s.workspace !== null);
  const [picked, setPicked] = useState<{ file: string; summary: ArchiveSummary } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const pick = async () => {
    const file = await open({
      title: "Choose a Frida archive",
      filters: [{ name: "Frida archive", extensions: ["zip"] }],
    });
    if (typeof file !== "string") return;
    setError(null);
    try {
      setPicked({ file, summary: await unwrap(commands.previewArchive(file)) });
    } catch (e) {
      setError(toApiError(e).message);
    }
  };

  const kind = picked?.summary.manifest.kind;
  const blocked =
    kind === "collection" && !hasWorkspace
      ? "Open a workspace first to import a collection into it."
      : (kind === "folder" || kind === "request") && !dir
        ? "Open a collection first to import into it."
        : null;
  const where =
    kind === "workspace"
      ? "You'll choose an empty folder for the new workspace; it is opened afterwards."
      : kind === "collection"
        ? "It becomes a new collection in this workspace (in its collections/ folder)."
        : `It goes into ${parent ? `the folder "${parent}"` : "the root"} of the active collection.`;

  const run = async () => {
    if (!picked || !kind) return;
    let target: ImportTarget;
    if (kind === "workspace") {
      const folder = await open({ directory: true, title: "Empty folder for the workspace" });
      if (typeof folder !== "string") return;
      target = { kind: "workspace", dir: folder };
    } else if (kind === "collection") {
      target = { kind: "collection", workspaceDir: "", dir: null };
    } else {
      target = { kind: "into", collectionDir: dir ?? "", parent };
    }
    setBusy(true);
    setError(null);
    try {
      const imported = await importArchive(picked.file, target);
      if (imported.path) {
        if (parent) useTabs.getState().toggleExpanded(imported.dir, parent, true);
        if (kind === "request") await useTabs.getState().openTab(imported.dir, imported.path);
        else useTabs.getState().toggleExpanded(imported.dir, imported.path, true);
      }
      onClose();
    } catch (e) {
      setError(toApiError(e).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>Import Frida archive</DialogTitle>
        <DialogDescription>
          A workspace, collection, folder or request exported from Frida (.zip). Secret values
          aren&apos;t part of archives; fill them in afterwards.
        </DialogDescription>
      </DialogHeader>

      {picked ? (
        <div className="space-y-2 text-sm">
          <p className="flex items-center gap-2">
            <FileArchive className="size-4 shrink-0 text-muted-foreground" />
            <span>
              A {KIND_LABEL[picked.summary.manifest.kind]}{" "}
              <span className="font-medium">&ldquo;{picked.summary.manifest.name}&rdquo;</span> with{" "}
              {picked.summary.files} files, exported{" "}
              {picked.summary.manifest.exportedAt.slice(0, 10)}.
            </span>
          </p>
          <p className="text-muted-foreground">{blocked ?? where}</p>
        </div>
      ) : (
        <Button variant="outline" className="w-full" onClick={() => void pick()}>
          <FileUp /> Choose archive…
        </Button>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}

      <DialogFooter>
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        {picked && (
          <Button disabled={busy || blocked !== null} onClick={() => void run()}>
            Import
          </Button>
        )}
      </DialogFooter>
    </>
  );
}
