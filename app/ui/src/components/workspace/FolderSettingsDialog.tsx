import { useEffect, useState } from "react";
import { commands, type Folder } from "@/bindings";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { showError, unwrap } from "@/lib/api";
import { type Complete, fromRust } from "@/lib/model";

interface FolderSettingsDialogProps {
  dir: string;
  /** The folder's path, or `null` when the dialog is closed. */
  path: string | null;
  onClose: () => void;
}

/**
 * Settings every request in a folder inherits: headers and timeout. Folder auth, variables and
 * scripts are added with their milestones (M2, M3, M5).
 */
export function FolderSettingsDialog({ dir, path, onClose }: FolderSettingsDialogProps) {
  const [folder, setFolder] = useState<Complete<Folder> | null>(null);

  useEffect(() => {
    if (path === null) return;
    let cancelled = false;
    unwrap(commands.readFolder(dir, path))
      .then((f) => {
        if (!cancelled) setFolder(fromRust(f));
      })
      .catch(showError);
    return () => {
      cancelled = true;
    };
  }, [dir, path]);

  const close = () => {
    setFolder(null);
    onClose();
  };

  const save = async () => {
    if (!folder || path === null) return;
    try {
      await unwrap(commands.saveFolder(dir, path, folder));
      close();
    } catch (e) {
      showError(e);
    }
  };

  return (
    <Dialog open={path !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Folder settings{folder ? `: ${folder.name}` : ""}</DialogTitle>
          <DialogDescription>
            Applied to every request in this folder and its subfolders. Requests can override them.
          </DialogDescription>
        </DialogHeader>

        {folder && (
          <div className="flex max-h-[60vh] flex-col gap-4 overflow-auto text-sm">
            <section className="flex flex-col gap-1.5">
              <span>Headers</span>
              <KeyValueTable
                rows={folder.headers}
                namePlaceholder="Header"
                onChange={(headers) => setFolder({ ...folder, headers })}
              />
            </section>
            <label className="flex max-w-xs flex-col gap-1.5">
              Timeout (ms)
              <Input
                type="number"
                min={1}
                placeholder="Inherit from collection"
                value={folder.settings.timeoutMs ?? ""}
                onChange={(e) =>
                  setFolder({
                    ...folder,
                    settings: {
                      timeoutMs: e.target.value === "" ? null : Math.max(1, Number(e.target.value)),
                    },
                  })
                }
              />
            </label>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            Cancel
          </Button>
          <Button onClick={() => void save()} disabled={!folder}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
