import { RotateCcw } from "lucide-react";
import { useSubTab } from "@/stores/subTabs";
import { useEffect, useState } from "react";
import { commands, type HistoryEntry } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { ResponsePanel } from "@/components/response/ResponseViewer";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { showError, unwrap } from "@/lib/api";
import { methodColor } from "@/lib/methods";
import { useTabs } from "@/stores/tabs";
import { useWorkspace } from "@/stores/workspace";
import { cn } from "cn";

/** A history entry, read-only: the request as sent and the response (PLAN.md §13). */
export function HistoryView({ dir, path }: { dir: string; path: string }) {
  const [subTab, setSubTab] = useSubTab(`history:${dir}:${path}`, "headers");
  const id = Number(path.split("/").pop());
  const [entry, setEntry] = useState<HistoryEntry | null>(null);

  useEffect(() => {
    let cancelled = false;
    void unwrap(commands.getHistoryEntry(id)).then((e) => !cancelled && setEntry(e), showError);
    return () => {
      cancelled = true;
    };
  }, [id]);

  const restore = async () => {
    try {
      const newPath = await unwrap(commands.restoreHistoryEntry(dir, id));
      await useWorkspace.getState().reloadTree(dir);
      await useTabs.getState().openTab(dir, newPath);
    } catch (e) {
      showError(e);
    }
  };

  if (!entry) return null;
  const { summary } = entry;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b p-2 text-sm">
        <span className={cn("font-mono font-semibold", methodColor(summary.method))}>
          {summary.method}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono" title={summary.url}>
          {summary.url}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">
          {new Date(summary.timestamp).toLocaleString()}
        </span>
        <Button size="sm" variant="outline" onClick={() => void restore()}>
          <RotateCcw /> Restore as new request
        </Button>
      </div>
      <p className="border-b bg-muted/30 px-3 py-1 text-xs text-muted-foreground">
        Read-only. Secret values were replaced by *** before saving.
      </p>
      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel defaultSize="45" minSize={200}>
          <Tabs value={subTab} onValueChange={setSubTab} className="h-full gap-0">
            <TabsList className="mx-2 mt-2">
              <TabsTrigger value="headers">
                Request headers ({entry.requestHeaders.length})
              </TabsTrigger>
              <TabsTrigger value="body">Request body</TabsTrigger>
            </TabsList>
            <TabsContent value="headers" className="overflow-auto p-3">
              <table className="w-full text-xs">
                <tbody>
                  {entry.requestHeaders.map((h, i) => (
                    <tr key={i} className="border-b last:border-0">
                      <td className="py-1 pr-4 align-top font-mono font-medium">{h.name}</td>
                      <td className="py-1 font-mono break-all">{h.value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TabsContent>
            <TabsContent value="body" className="min-h-0 p-2">
              {entry.requestBody ? (
                <CodeEditor
                  className="h-full overflow-hidden rounded-md border"
                  value={entry.requestBody}
                  readOnly
                />
              ) : (
                <p className="p-2 text-xs text-muted-foreground">No body.</p>
              )}
              {entry.requestBodyTruncated && (
                <p className="p-2 text-xs text-warning">Only the first 1 MB was stored.</p>
              )}
            </TabsContent>
          </Tabs>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel defaultSize="55" minSize={200}>
          {entry.response ? (
            <ResponsePanel response={entry.response} viewKey={`history:${dir}:${path}`} />
          ) : (
            <div className="flex h-full items-center justify-center p-6 text-sm text-destructive">
              {summary.error ?? "No response."}
            </div>
          )}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
