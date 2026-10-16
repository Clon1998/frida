import { AlertTriangle } from "lucide-react";
import { useEffect, useState } from "react";
import { type CurlImport, commands } from "@/bindings";
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
import { Textarea } from "@/components/ui/textarea";
import { showError, toApiError, unwrap } from "@/lib/api";
import { useDialogs } from "@/stores/dialogs";
import { useTabs } from "@/stores/tabs";
import { useWorkspace } from "@/stores/workspace";

/** "New request from curl": paste a command, check the preview, create the request. */
export function CurlImportDialog() {
  const target = useDialogs((s) => s.curlImport);
  const close = useDialogs((s) => s.close);
  const [command, setCommand] = useState("");
  const [parsed, setParsed] = useState<CurlImport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");

  // Parse as the user types (it's cheap and local to Rust).
  useEffect(() => {
    if (!command.trim()) return; // cleared in the change handler
    let cancelled = false;
    void unwrap(commands.importCurl(command)).then(
      (result) => {
        if (cancelled) return;
        setParsed(result);
        setError(null);
        setName((n) => n || result.request.name);
      },
      (e) => {
        if (cancelled) return;
        setParsed(null);
        setError(toApiError(e).message);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [command]);

  const reset = () => {
    setCommand("");
    setParsed(null);
    setError(null);
    setName("");
    close();
  };

  const create = async () => {
    if (!target || !parsed) return;
    try {
      const request = { ...parsed.request, name: name.trim() || parsed.request.name };
      const path = await unwrap(commands.createRequestFrom(target.dir, target.parent, request));
      await useWorkspace.getState().reloadTree(target.dir);
      await useTabs.getState().openTab(target.dir, path);
      reset();
    } catch (e) {
      showError(e);
    }
  };

  return (
    <Dialog open={target !== null} onOpenChange={(open) => !open && reset()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>New request from curl</DialogTitle>
          <DialogDescription>
            Paste a curl command, e.g. from the browser's dev tools ("Copy as cURL").
          </DialogDescription>
        </DialogHeader>
        <Textarea
          autoFocus
          className="min-h-40 font-mono text-xs"
          spellCheck={false}
          placeholder="curl 'https://api.example.com/orders' -H 'Accept: application/json'"
          value={command}
          onChange={(e) => {
            setCommand(e.target.value);
            if (!e.target.value.trim()) {
              setParsed(null);
              setError(null);
            }
          }}
        />
        {error && <p className="text-xs text-destructive">{error}</p>}
        {parsed && (
          <div className="flex flex-col gap-2 text-sm">
            <label className="flex flex-col gap-1">
              Name
              <Input className="h-8" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <p className="font-mono text-xs">
              <span className="font-semibold">{parsed.request.method}</span> {parsed.request.url}
            </p>
            {parsed.warnings.map((w) => (
              <p key={w} className="flex items-center gap-1.5 text-xs text-warning">
                <AlertTriangle className="size-3.5 shrink-0" /> {w}
              </p>
            ))}
          </div>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={reset}>
            Cancel
          </Button>
          <Button disabled={!parsed} onClick={() => void create()}>
            Create request
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
