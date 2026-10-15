import { Loader2, Send, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { RequestModel as Request } from "@/lib/model";
import { queryParamsFromUrl, syncPathParams, urlWithQueryParams } from "@/lib/url";
import { type Tab, useTabs } from "@/stores/tabs";
import { BodyEditor } from "./BodyEditor";
import { KeyValueTable } from "./KeyValueTable";
import { MethodSelect } from "./MethodSelect";
import { PathParamsTable } from "./PathParamsTable";

interface RequestEditorProps {
  dir: string;
  tab: Tab;
}

export function RequestEditor({ dir, tab }: RequestEditorProps) {
  const updateDraft = useTabs((s) => s.updateDraft);
  const send = useTabs((s) => s.send);
  const cancel = useTabs((s) => s.cancel);
  const resolveConflict = useTabs((s) => s.resolveConflict);
  const request = tab.draft;
  const sending = tab.requestId !== null;

  const update = (change: (r: Request) => Request) => updateDraft(dir, tab.path, change);

  // The URL is the source of truth for enabled query params and the list of path params.
  const setUrl = (url: string) =>
    update((r) => ({
      ...r,
      url,
      params: {
        query: queryParamsFromUrl(url, r.params.query),
        path: syncPathParams(url, r.params.path),
      },
    }));

  return (
    <div className="flex h-full flex-col">
      {tab.conflict && (
        <div className="flex items-center gap-2 border-b bg-amber-500/10 px-3 py-2 text-sm">
          <span className="flex-1">This request changed on disk while you were editing it.</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => resolveConflict(dir, tab.path, "disk")}
          >
            Reload from disk
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => resolveConflict(dir, tab.path, "mine")}
          >
            Keep my version
          </Button>
        </div>
      )}

      <div className="flex items-center gap-2 border-b p-2">
        <MethodSelect
          value={request.method}
          onChange={(method) => update((r) => ({ ...r, method }))}
        />
        <Input
          className="h-9 font-mono text-sm"
          placeholder="https://api.example.com/orders/:orderId"
          value={request.url}
          onChange={(e) => setUrl(e.target.value)}
          spellCheck={false}
        />
        {sending ? (
          <Button variant="outline" onClick={() => void cancel(dir, tab.path)}>
            <Loader2 className="animate-spin" />
            Cancel
            <X />
          </Button>
        ) : (
          <Button onClick={() => void send(dir, tab.path)} title="Send (Ctrl/Cmd+Enter)">
            <Send />
            Send
          </Button>
        )}
      </div>

      <Tabs defaultValue="params" className="min-h-0 flex-1 gap-0">
        <TabsList className="mx-2 mt-2">
          <TabsTrigger value="params">Params</TabsTrigger>
          <TabsTrigger value="body">Body</TabsTrigger>
          <TabsTrigger value="headers">
            Headers
            {request.headers.length > 0 && (
              <span className="text-muted-foreground">({request.headers.length})</span>
            )}
          </TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>

        <TabsContent value="params" className="overflow-auto p-3">
          <h3 className="mb-2 text-xs font-medium text-muted-foreground">Query</h3>
          <KeyValueTable
            rows={request.params.query}
            onChange={(query) =>
              update((r) => ({
                ...r,
                url: urlWithQueryParams(r.url, query),
                params: { ...r.params, query },
              }))
            }
          />
          <h3 className="mt-4 mb-2 text-xs font-medium text-muted-foreground">Path</h3>
          <PathParamsTable
            params={request.params.path}
            onChange={(path) => update((r) => ({ ...r, params: { ...r.params, path } }))}
          />
        </TabsContent>

        <TabsContent value="body" className="min-h-0 p-3">
          <BodyEditor body={request.body} onChange={(body) => update((r) => ({ ...r, body }))} />
        </TabsContent>

        <TabsContent value="headers" className="overflow-auto p-3">
          <KeyValueTable
            rows={request.headers}
            onChange={(headers) => update((r) => ({ ...r, headers }))}
            namePlaceholder="Header"
          />
        </TabsContent>

        <TabsContent value="settings" className="overflow-auto p-3">
          <label className="flex max-w-sm flex-col gap-1 text-sm">
            Timeout (ms)
            <Input
              type="number"
              min={1}
              className="h-8"
              placeholder="Inherit from folder / collection (default 30000)"
              value={request.settings.timeoutMs ?? ""}
              onChange={(e) => {
                const value = e.target.value === "" ? null : Math.max(1, Number(e.target.value));
                update((r) => ({ ...r, settings: { ...r.settings, timeoutMs: value } }));
              }}
            />
          </label>
        </TabsContent>
      </Tabs>
    </div>
  );
}
