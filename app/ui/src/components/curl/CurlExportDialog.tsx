import { ask } from "@tauri-apps/plugin-dialog";
import { Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { commands, type Request, type Shell } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toApiError, unwrap } from "@/lib/api";
import { useDialogs } from "@/stores/dialogs";
import { useTabs } from "@/stores/tabs";

/** "Copy as curl" (PLAN.md §11.3): options, a preview and a copy button. */
export function CurlExportDialog() {
  const target = useDialogs((s) => s.curlExport);
  const close = useDialogs((s) => s.close);
  const [shell, setShell] = useState<Shell>("bash");
  const [resolve, setResolve] = useState(true);
  const [includeSecrets, setIncludeSecrets] = useState(false);
  const [output, setOutput] = useState("");

  useEffect(() => {
    if (!target) return;
    let cancelled = false;
    void (async () => {
      try {
        // Prefer the open tab's (possibly unsaved) state, else the file on disk.
        const tab = useTabs
          .getState()
          .byCollection[target.dir]?.tabs.find((t) => t.path === target.path);
        const request: Request =
          tab?.kind === "request"
            ? tab.draft
            : await unwrap(commands.readRequest(target.dir, target.path));
        const curl = await unwrap(
          commands.exportCurl(target.dir, target.path, request, {
            shell,
            resolveVariables: resolve,
            includeSecrets,
          }),
        );
        if (!cancelled) setOutput(curl);
      } catch (e) {
        if (!cancelled) setOutput(`# ${toApiError(e).message}`);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [target, shell, resolve, includeSecrets]);

  const toggleSecrets = async (on: boolean) => {
    if (on) {
      const ok = await ask(
        "The command will contain secret values in plain text. Only share it with people who may see them.",
        { title: "Include secrets?", kind: "warning", okLabel: "Include secrets" },
      );
      if (!ok) return;
    }
    setIncludeSecrets(on);
  };

  const copy = async () => {
    await navigator.clipboard.writeText(output);
    toast.success("curl command copied");
  };

  return (
    <Dialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) {
          close();
          setIncludeSecrets(false);
        }
      }}
    >
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>Copy as curl</DialogTitle>
          <DialogDescription>
            Secret values are replaced by *** unless you include them.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap items-center gap-4 text-sm">
          <Tabs value={shell} onValueChange={(v) => setShell(v as Shell)}>
            <TabsList>
              <TabsTrigger value="bash">bash / zsh</TabsTrigger>
              <TabsTrigger value="powershell">PowerShell</TabsTrigger>
            </TabsList>
          </Tabs>
          <label className="flex items-center gap-2">
            <Checkbox checked={resolve} onCheckedChange={(c) => setResolve(c === true)} />
            Resolve variables
          </label>
          <label className="flex items-center gap-2">
            <Checkbox
              checked={includeSecrets}
              onCheckedChange={(c) => void toggleSecrets(c === true)}
            />
            Include secrets
          </label>
        </div>
        <CodeEditor className="h-72 overflow-hidden rounded-md border" value={output} readOnly />
        <div className="flex justify-end">
          <Button onClick={() => void copy()} disabled={!output}>
            <Copy /> Copy
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
