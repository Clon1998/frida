import { Download, Loader2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { formatBytes } from "@/lib/format";
import { isDirty, isDocument, tabTitle, useTabs } from "@/stores/tabs";
import { useUpdate } from "@/stores/update";

/** Unsaved tabs in every collection, since the restart closes them all. */
function dirtyTabs() {
  return Object.entries(useTabs.getState().byCollection).flatMap(([dir, { tabs }]) =>
    tabs.filter(isDirty).map((tab) => ({ dir, tab })),
  );
}

/**
 * "Frida X is available" (PLAN.md M14): the release notes, then Install and restart or Later.
 * Unsaved tabs are asked about first, like closing them. While downloading, the dialog shows
 * the progress and a Cancel button and can't be closed.
 */
export function UpdateDialog() {
  const info = useUpdate((s) => s.available);
  const open = useUpdate((s) => s.dialogOpen);
  const setOpen = useUpdate((s) => s.setDialogOpen);
  const phase = useUpdate((s) => s.phase);
  const progress = useUpdate((s) => s.progress);
  const install = useUpdate((s) => s.install);
  const cancel = useUpdate((s) => s.cancel);
  // Read when the dialog renders; the tabs can't change while it is open (it's modal).
  const dirty = open ? dirtyTabs() : [];
  const [saving, setSaving] = useState(false);

  if (!info) return null;
  const busy = phase === "downloading" || phase === "installing";

  const saveAndInstall = async () => {
    setSaving(true);
    const store = useTabs.getState();
    for (const { dir, tab } of dirty) if (isDocument(tab)) await store.save(dir, tab.path);
    setSaving(false);
    // `save` shows its own error; anything still unsaved (or the environment editor) stops
    // here so nothing is lost.
    if (dirtyTabs().length === 0) void install();
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>Frida {info.version} is available</DialogTitle>
          <DialogDescription>
            You have {info.currentVersion}
            {info.date && `. Released ${new Date(info.date).toLocaleDateString()}`}.
          </DialogDescription>
        </DialogHeader>

        {info.notes && (
          <pre className="max-h-64 overflow-auto rounded-md border bg-muted/40 p-3 font-sans text-xs whitespace-pre-wrap">
            {info.notes}
          </pre>
        )}

        {info.installBlocker && (
          <p className="text-sm text-muted-foreground">
            Frida can&apos;t update itself here: {info.installBlocker}. Download the new version
            from GitHub Releases instead.
          </p>
        )}

        {!busy && !info.installBlocker && dirty.length > 0 && (
          <p className="rounded-md bg-warning/10 p-2 text-sm">
            Frida restarts to finish the update. Unsaved changes in{" "}
            {dirty.map(({ tab }) => tabTitle(tab)).join(", ")} would be lost.
          </p>
        )}

        {busy && progress && (
          <div className="flex flex-col gap-1.5" role="status">
            <div className="h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full bg-primary transition-[width]"
                style={{
                  width: progress.total
                    ? `${Math.min(100, (progress.downloaded / progress.total) * 100)}%`
                    : "100%",
                }}
              />
            </div>
            <p className="text-xs text-muted-foreground">
              {phase === "installing"
                ? "Installing… Frida restarts when it's done."
                : `Downloading ${formatBytes(progress.downloaded)}${progress.total ? ` of ${formatBytes(progress.total)}` : ""}…`}
            </p>
          </div>
        )}

        <DialogFooter>
          {busy ? (
            <Button variant="outline" disabled={phase === "installing"} onClick={cancel}>
              Cancel
            </Button>
          ) : (
            <>
              <Button variant="outline" className="sm:mr-auto" onClick={() => setOpen(false)}>
                Later
              </Button>
              {!info.installBlocker && dirty.length > 0 && (
                <Button variant="destructive" onClick={() => void install()}>
                  Install without saving
                </Button>
              )}
              {!info.installBlocker && (
                <Button
                  autoFocus
                  disabled={saving}
                  onClick={() => void (dirty.length > 0 ? saveAndInstall() : install())}
                >
                  {saving ? <Loader2 className="animate-spin" /> : <Download />}
                  {dirty.length > 0 ? "Save and install" : "Install and restart"}
                </Button>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
