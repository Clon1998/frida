import { open as openFile } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { commands } from "@/bindings";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import type { Collection } from "@/bindings";

interface CollectionSettingsDialogProps {
  dir: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Collection-wide settings: default timeout, TLS verification and extra CA certificates. */
export function CollectionSettingsDialog({
  dir,
  open,
  onOpenChange,
}: CollectionSettingsDialogProps) {
  const [collection, setCollection] = useState<Complete<Collection> | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    unwrap(commands.readCollection(dir))
      .then((c) => {
        if (!cancelled) setCollection(fromRust(c));
      })
      .catch(showError);
    return () => {
      cancelled = true;
    };
  }, [dir, open]);

  const close = () => {
    onOpenChange(false);
    setCollection(null);
  };

  const save = async () => {
    if (!collection) return;
    try {
      await unwrap(commands.saveCollection(dir, collection));
      close();
    } catch (e) {
      showError(e);
    }
  };

  const settings = collection?.settings;
  const patchTls = (tls: Partial<Complete<Collection>["settings"]["tls"]>) =>
    collection &&
    setCollection({
      ...collection,
      settings: { ...collection.settings, tls: { ...collection.settings.tls, ...tls } },
    });

  const addCaCert = async () => {
    const path = await openFile({
      multiple: false,
      directory: false,
      title: "Choose a CA certificate (PEM)",
      filters: [{ name: "PEM certificate", extensions: ["pem", "crt", "cer"] }],
    });
    if (typeof path === "string" && settings) {
      patchTls({ caCertPaths: [...settings.tls.caCertPaths, path] });
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => (o ? onOpenChange(true) : close())}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Collection settings</DialogTitle>
          <DialogDescription>
            Saved in <code>collection.yaml</code>, so they are shared through git.
          </DialogDescription>
        </DialogHeader>

        {collection && settings && (
          <div className="flex max-h-[60vh] flex-col gap-4 overflow-auto text-sm">
            <section className="flex flex-col gap-1.5">
              <span>Headers sent with every request</span>
              <KeyValueTable
                rows={collection.headers}
                namePlaceholder="Header"
                onChange={(headers) => setCollection({ ...collection, headers })}
              />
            </section>
            <label className="flex max-w-xs flex-col gap-1.5">
              Default timeout (ms)
              <Input
                type="number"
                min={1}
                placeholder="30000"
                value={settings.timeoutMs ?? ""}
                onChange={(e) =>
                  setCollection({
                    ...collection,
                    settings: {
                      ...settings,
                      timeoutMs: e.target.value === "" ? null : Math.max(1, Number(e.target.value)),
                    },
                  })
                }
              />
            </label>

            <label className="flex items-center gap-2">
              <Checkbox
                checked={settings.tls.verify}
                onCheckedChange={(checked) => patchTls({ verify: checked === true })}
              />
              Verify TLS certificates
            </label>
            {!settings.tls.verify && (
              <p className="-mt-2 text-xs text-amber-600 dark:text-amber-400">
                Any certificate is accepted, including invalid or self-signed ones. Only use this
                for local development.
              </p>
            )}

            <div className="flex flex-col gap-1.5">
              <span>Extra CA certificates</span>
              <span className="text-xs text-muted-foreground">
                Trusted in addition to the system certificates. Paths may be relative to the
                collection folder.
              </span>
              {settings.tls.caCertPaths.map((path, index) => (
                <div key={index} className="flex items-center gap-2">
                  <Input
                    className="h-8 font-mono text-xs"
                    value={path}
                    onChange={(e) =>
                      patchTls({
                        caCertPaths: settings.tls.caCertPaths.map((p, i) =>
                          i === index ? e.target.value : p,
                        ),
                      })
                    }
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    aria-label="Remove certificate"
                    onClick={() =>
                      patchTls({
                        caCertPaths: settings.tls.caCertPaths.filter((_, i) => i !== index),
                      })
                    }
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))}
              <Button
                variant="outline"
                size="sm"
                className="self-start"
                onClick={() => void addCaCert()}
              >
                <FolderOpen /> Add certificate…
              </Button>
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            Cancel
          </Button>
          <Button onClick={() => void save()} disabled={!collection}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
