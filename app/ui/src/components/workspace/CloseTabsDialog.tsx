import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useDialogs } from "@/stores/dialogs";
import { isDirty, isDocument, tabTitle, useTabs } from "@/stores/tabs";

/**
 * "Save changes before closing?" with Save, Don't save and Cancel, like an IDE. Opened by
 * `closeTabsAsking` (tab close button, context menu, Cmd/Ctrl+W).
 */
export function CloseTabsDialog() {
  const request = useDialogs((s) => s.closeTabs);
  const closeDialog = useDialogs((s) => s.close);
  const allTabs = useTabs((s) => (request ? s.byCollection[request.dir]?.tabs : undefined));
  const [saving, setSaving] = useState(false);

  const dirty = (allTabs ?? []).filter((t) => request?.paths.includes(t.path) && isDirty(t));
  // The environment editor keeps its edits in its own component, so it is saved there.
  const envDirty = dirty.some((t) => t.kind === "environments");

  const discard = () => {
    if (request) useTabs.getState().closeTabs(request.dir, request.paths);
    closeDialog();
  };

  const saveAndClose = async () => {
    if (!request) return;
    setSaving(true);
    const store = useTabs.getState();
    for (const tab of dirty) if (isDocument(tab)) await store.save(request.dir, tab.path);
    setSaving(false);
    // `save` shows its own error; tabs that are still unsaved (failed, or the environment
    // editor) stay open so nothing is lost.
    const after = useTabs.getState().byCollection[request.dir]?.tabs ?? [];
    const closable = request.paths.filter((p) => {
      const tab = after.find((t) => t.path === p);
      return !tab || !isDirty(tab);
    });
    useTabs.getState().closeTabs(request.dir, closable);
    closeDialog();
  };

  const names = dirty.map(tabTitle);
  return (
    <AlertDialog open={request !== null} onOpenChange={(open) => !open && closeDialog()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {names.length === 1 ? `Save changes to "${names[0]}"?` : "Save changes?"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {names.length === 1
              ? "Your changes will be lost if you don't save them."
              : `${names.length} tabs have unsaved changes: ${names.join(", ")}.`}
            {envDirty &&
              " Environment changes are saved in the environment editor; that tab stays open."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <Button variant="outline" className="sm:mr-auto" onClick={closeDialog}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={discard}>
            Don&apos;t save
          </Button>
          <Button autoFocus disabled={saving} onClick={() => void saveAndClose()}>
            Save
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
