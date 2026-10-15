import { Loader2, Send, SquareTerminal, X } from "lucide-react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { showError, unwrap } from "@/lib/api";
import { fromRust } from "@/lib/model";
import { useDialogs } from "@/stores/dialogs";
import { useUi } from "@/stores/ui";
import { useEffect } from "react";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { InspectorPanel } from "@/components/inspector/InspectorPanel";
import { useInspection } from "@/components/inspector/useInspection";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { RequestModel as Request } from "@/lib/model";
import { queryParamsFromUrl, syncPathParams, urlWithQueryParams } from "@/lib/url";
import { type RequestTab, useTabs } from "@/stores/tabs";
import { ConflictBanner } from "@/components/doc/ConflictBanner";
import { AuthEditor } from "@/components/doc/AuthEditor";
import { ScriptsSection, TextSection, VarsSection } from "@/components/doc/sections";
import type { Auth } from "@/bindings";
import { BodyEditor } from "./BodyEditor";
import { KeyValueTable } from "./KeyValueTable";
import { MethodSelect } from "./MethodSelect";
import { PathParamsTable } from "./PathParamsTable";

interface RequestEditorProps {
  dir: string;
  tab: RequestTab;
}

export function RequestEditor({ dir, tab }: RequestEditorProps) {
  const updateDraft = useTabs((s) => s.updateDraft);
  const send = useTabs((s) => s.send);
  const cancel = useTabs((s) => s.cancel);
  const request = tab.draft;
  const sending = tab.requestId !== null;
  const inspectorOpen = useUi((s) => s.inspectorOpen);
  const setInspectorOpen = useUi((s) => s.setInspectorOpen);
  const { report, variables } = useInspection(dir, tab.path, request);
  const unresolved = report?.vars.filter((v) => !v.resolved).length ?? 0;
  // The inspector button lives in the header bar; tell it how many variables are unresolved.
  useEffect(() => {
    useUi.getState().setUnresolved(unresolved);
    return () => useUi.getState().setUnresolved(0);
  }, [unresolved]);
  const openCurlExport = useDialogs((s) => s.openCurlExport);

  /** Pasting a curl command into the URL bar turns this request into it (PLAN.md §11.3). */
  const pasteCurl = (text: string) => {
    if (!/^\s*curl(\.exe)?\s/.test(text)) return false;
    void unwrap(commands.importCurl(text)).then((result) => {
      const imported = fromRust(result.request);
      // Keep this request's identity (name, order, docs); take everything curl describes.
      update((r) => ({
        ...r,
        method: imported.method,
        url: imported.url,
        params: imported.params,
        headers: imported.headers,
        body: imported.body,
        auth: imported.auth.mode === "custom" ? imported.auth : r.auth,
        settings: imported.settings.timeoutMs ? imported.settings : r.settings,
      }));
      toast.success("Imported curl command", {
        description: result.warnings.length ? result.warnings.join("\n") : undefined,
      });
    }, showError);
    return true;
  };

  const update = (change: (r: Request) => Request) =>
    updateDraft<RequestTab>(dir, tab.path, change);

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
      <ConflictBanner dir={dir} tab={tab} />

      <div className="flex items-center gap-2 border-b p-2">
        <MethodSelect
          value={request.method}
          onChange={(method) => update((r) => ({ ...r, method }))}
        />
        {/* The editor floats over the content below, so it can grow downwards while editing
            without moving anything (see the single-line theme in CodeEditor). */}
        <div className="relative h-9 min-w-0 flex-1">
          <CodeEditor
            singleLine
            className="absolute inset-x-0 top-0 z-20 min-h-9 rounded-md border bg-background px-1 py-1.5 has-[.cm-focused]:shadow-md has-[.cm-focused]:ring-2 has-[.cm-focused]:ring-ring/40"
            placeholder="https://api.example.com/orders/:orderId"
            value={request.url}
            variables={variables}
            onChange={setUrl}
            onEnter={() => void send(dir, tab.path)}
            onPasteText={pasteCurl}
          />
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="size-9 shrink-0"
          title="Copy as curl"
          aria-label="Copy as curl"
          onClick={() => openCurlExport(dir, tab.path)}
        >
          <SquareTerminal />
        </Button>
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

      <div className="flex min-h-0 flex-1">
        <Tabs defaultValue="params" className="min-h-0 min-w-0 flex-1 gap-0">
          <TabsList className="mx-2 mt-2">
            <TabsTrigger value="params">Params</TabsTrigger>
            <TabsTrigger value="body">Body</TabsTrigger>
            <TabsTrigger value="headers">
              Headers
              {request.headers.length > 0 && (
                <span className="text-muted-foreground">({request.headers.length})</span>
              )}
            </TabsTrigger>
            <TabsTrigger value="auth">Auth</TabsTrigger>
            <TabsTrigger value="vars">Vars</TabsTrigger>
            <TabsTrigger value="script">Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="docs">Docs</TabsTrigger>
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
            <BodyEditor
              body={request.body}
              variables={variables}
              onChange={(body) => update((r) => ({ ...r, body }))}
            />
          </TabsContent>

          <TabsContent value="headers" className="overflow-auto p-3">
            <KeyValueTable
              rows={request.headers}
              onChange={(headers) => update((r) => ({ ...r, headers }))}
              namePlaceholder="Header"
            />
          </TabsContent>

          <TabsContent value="auth" className="overflow-auto p-3">
            <AuthEditor
              auth={request.auth as Auth}
              canInherit
              context={{ dir, path: tab.path, request }}
              // `as`: the editor works with the generated type; see AuthEditor for why.
              onChange={(auth) => update((r) => ({ ...r, auth: auth as Request["auth"] }))}
            />
          </TabsContent>

          <TabsContent value="vars" className="overflow-auto p-3">
            <VarsSection vars={request.vars} onChange={(vars) => update((r) => ({ ...r, vars }))} />
          </TabsContent>

          <TabsContent value="script" className="min-h-0 p-3">
            <ScriptsSection
              scripts={request.scripts}
              onChange={(scripts) => update((r) => ({ ...r, scripts }))}
            />
          </TabsContent>

          <TabsContent value="tests" className="min-h-0 p-3">
            <TextSection
              value={request.tests}
              note="Saved now; tests run once the scripting milestone is done."
              onChange={(tests) => update((r) => ({ ...r, tests }))}
            />
          </TabsContent>

          <TabsContent value="docs" className="min-h-0 p-3">
            <TextSection
              value={request.docs}
              onChange={(docs) => update((r) => ({ ...r, docs }))}
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
        {inspectorOpen && (
          <div className="w-80 shrink-0">
            <InspectorPanel dir={dir} report={report} onClose={() => setInspectorOpen(false)} />
          </div>
        )}
      </div>
    </div>
  );
}
