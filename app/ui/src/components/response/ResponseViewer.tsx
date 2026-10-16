import { save } from "@tauri-apps/plugin-dialog";
import { useSubTab } from "@/stores/subTabs";
import {
  AlertCircle,
  CheckCircle2,
  Copy,
  Download,
  Filter,
  Loader2,
  X,
  XCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { commands, type ResponseView, type ScriptOutcome } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { showError, unwrap } from "@/lib/api";
import { bodyKind, formatBytes, formatDuration, prettyJson } from "@/lib/format";
import { type RequestTab, useTabs } from "@/stores/tabs";
import { cn } from "cn";

function statusColor(status: number) {
  if (status < 300) return "bg-success/15 text-success";
  if (status < 400) return "bg-info/15 text-info";
  if (status < 500) return "bg-warning/15 text-warning";
  return "bg-destructive/15 text-destructive";
}

/** The response pane of a request tab. Only shown once there is something to show. */
export function ResponseViewer({ tab, dir }: { tab: RequestTab; dir: string }) {
  const close = () => useTabs.getState().setResponseHidden(dir, tab.path, true);
  if (tab.response && !tab.requestId && !tab.error) {
    return <ResponsePanel response={tab.response} viewKey={`${dir}:${tab.path}`} onClose={close} />;
  }
  return (
    <div className="relative h-full">
      <CloseButton className="absolute top-2 right-2" onClick={close} />
      {tab.requestId ? (
        <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" /> Waiting for response…
        </div>
      ) : (
        tab.error && (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            <AlertCircle className="size-6 text-destructive" />
            <p className="text-sm font-medium">{tab.error.message}</p>
            {tab.error.hint && <p className="text-xs text-muted-foreground">{tab.error.hint}</p>}
            {tab.error.aws?.fixCommand && <FixCommand command={tab.error.aws.fixCommand} />}
          </div>
        )
      )}
    </div>
  );
}

function CloseButton({ className, onClick }: { className?: string; onClick: () => void }) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className={cn("size-7", className)}
      title="Close the response (it opens again on the next send)"
      aria-label="Close response"
      onClick={onClick}
    >
      <X />
    </Button>
  );
}

/** How long to wait after the last keystroke before filtering (large bodies take a moment). */
const FILTER_DEBOUNCE_MS = 250;

/**
 * The JSONPath filter of a JSON body (filtered in Rust, `filter_json`). `result` is the
 * filtered JSON while a valid filter is set, else `null` (show the whole body). While a new
 * filter is being checked, the last result stays, so the body doesn't flicker.
 */
function useJsonFilter(response: ResponseView, enabled: boolean, uncached: boolean) {
  const [path, setPath] = useState("");
  const [outcome, setOutcome] = useState<{
    path: string;
    result: string | null;
    error: string | null;
  } | null>(null);
  const active = enabled && path.trim() !== "";

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      unwrap(commands.filterResponse(response.id, uncached ? response.bodyText : null, path)).then(
        (result) => !cancelled && setOutcome({ path, result, error: null }),
        (e: { message?: string }) =>
          !cancelled &&
          setOutcome((last) => ({
            path,
            result: last?.result ?? null,
            error: e.message ?? String(e),
          })),
      );
    }, FILTER_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [active, path, response.id, response.bodyText, uncached]);

  return {
    path,
    setPath,
    result: active ? (outcome?.result ?? null) : null,
    error: active && outcome?.path === path ? outcome.error : null,
  };
}

/** A response: status chips, body (pretty/raw/preview), headers, copy and save. */
export function ResponsePanel({
  response,
  viewKey,
  onClose,
  uncached = false,
}: {
  response: ResponseView;
  /** Remembers the open sub-tab under this key (e.g. per request tab). */
  viewKey: string;
  /** Shows a close button (request tabs; not the read-only history view). */
  onClose?: () => void;
  /** The body isn't in Rust's response cache (history): the filter sends the text along. */
  uncached?: boolean;
}) {
  const [pretty, setPretty] = useState(true);
  const kind = bodyKind(response.contentType, response.bodyText);
  const text = response.bodyText ?? "";
  const jsonFilter = useJsonFilter(response, kind === "json", uncached);
  const shown = jsonFilter.result ?? (pretty && kind === "json" ? prettyJson(text) : text);
  const hasPreview = kind === "image" || kind === "html";
  const [stored, setSubTab] = useSubTab(`response:${viewKey}`, hasPreview ? "preview" : "body");
  const subTab = stored === "preview" && !hasPreview ? "body" : stored;
  const { tests, errors } = response.scripts;
  const passed = tests.filter((t) => t.passed).length;
  const testsFailed = passed < tests.length || errors.length > 0;

  const copy = async () => {
    await navigator.clipboard.writeText(shown);
    toast.success("Response body copied");
  };

  const saveToFile = async () => {
    const target = await save({ title: "Save response body" });
    if (!target) return;
    try {
      await unwrap(commands.saveResponseBody(response.id, target));
      toast.success("Response body saved");
    } catch (e) {
      showError(e);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Wraps as whole items in a narrow pane (never inside "87 ms" or a button). */}
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b px-3 py-2 text-xs whitespace-nowrap">
        <Badge className={statusColor(response.status)}>
          {response.status} {response.statusText}
        </Badge>
        <span className="text-muted-foreground">{formatDuration(response.timeMs)}</span>
        <span className="text-muted-foreground">{formatBytes(response.sizeBytes)}</span>
        {(tests.length > 0 || errors.length > 0) && (
          <span className={cn("font-medium", testsFailed ? "text-destructive" : "text-success")}>
            Tests {passed}/{tests.length}
          </span>
        )}
        <div className="ml-auto flex">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void copy()}
            disabled={!response.bodyText}
          >
            <Copy /> Copy
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void saveToFile()}>
            <Download /> Save
          </Button>
          {onClose && <CloseButton onClick={onClose} />}
        </div>
      </div>

      {response.truncated && (
        <p className="border-b bg-warning/10 px-3 py-1.5 text-xs">
          The body is larger than 5 MB, so only the beginning is shown. Use Save to get all of it.
        </p>
      )}

      <Tabs value={subTab} onValueChange={setSubTab} className="min-h-0 flex-1 gap-0">
        <TabsList className="mx-2 mt-2">
          <TabsTrigger value="body">Body</TabsTrigger>
          {hasPreview && <TabsTrigger value="preview">Preview</TabsTrigger>}
          <TabsTrigger value="headers">Headers ({response.headers.length})</TabsTrigger>
          <TabsTrigger value="tests" className={cn(testsFailed && "text-destructive")}>
            Tests{tests.length > 0 && ` (${passed}/${tests.length})`}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="body" className="flex min-h-0 flex-col p-2">
          {kind === "json" && (
            <div className="mb-2 flex flex-wrap items-center gap-1">
              <Button
                size="sm"
                variant={pretty ? "secondary" : "ghost"}
                disabled={jsonFilter.result !== null}
                onClick={() => setPretty(true)}
              >
                Pretty
              </Button>
              <Button
                size="sm"
                variant={pretty ? "ghost" : "secondary"}
                disabled={jsonFilter.result !== null}
                onClick={() => setPretty(false)}
              >
                Raw
              </Button>
              <div className="relative min-w-40 flex-1">
                <Filter className="absolute top-2 left-2 size-3.5 text-muted-foreground" />
                <Input
                  className="h-8 pl-7 font-mono text-xs md:text-xs"
                  placeholder="Filter: $.items[*].id"
                  aria-label="JSONPath filter"
                  spellCheck={false}
                  value={jsonFilter.path}
                  onChange={(e) => jsonFilter.setPath(e.target.value)}
                />
              </div>
              {jsonFilter.error && (
                <p className="w-full text-xs text-destructive">{jsonFilter.error}</p>
              )}
            </div>
          )}
          {response.bodyText !== null ? (
            <CodeEditor
              className="min-h-0 flex-1 overflow-hidden rounded-md border"
              value={shown}
              language={
                kind === "json" ? "json" : kind === "xml" || kind === "html" ? "xml" : "text"
              }
              readOnly
            />
          ) : (
            <p className="p-2 text-sm text-muted-foreground">
              Binary body ({formatBytes(response.sizeBytes)}). Use Save to write it to a file.
            </p>
          )}
        </TabsContent>

        {hasPreview && (
          <TabsContent value="preview" className="min-h-0 p-2">
            {kind === "image" && response.bodyBase64 && (
              <img
                alt="Response preview"
                className="max-h-full max-w-full object-contain"
                src={`data:${response.contentType};base64,${response.bodyBase64}`}
              />
            )}
            {kind === "html" && (
              // `sandbox=""` applies every restriction: no scripts, forms, popups or same-origin
              // access, so a malicious page can't reach the app (CLAUDE.md security rules).
              // White on purpose, also in dark mode: pages expect a white page background.
              <iframe
                title="HTML preview"
                sandbox=""
                className={cn("h-full w-full rounded-md border bg-white")}
                srcDoc={text}
              />
            )}
          </TabsContent>
        )}

        <TabsContent value="headers" className="overflow-auto p-3">
          <table className="w-full text-xs">
            <tbody>
              {response.headers.map((h, i) => (
                <tr key={i} className="border-b last:border-0">
                  <td className="py-1 pr-4 align-top font-mono font-medium">{h.name}</td>
                  <td className="py-1 font-mono break-all">{h.value}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {response.finalUrl && (
            <p className="mt-3 text-xs text-muted-foreground">
              Final URL: <span className="font-mono break-all">{response.finalUrl}</span>
            </p>
          )}
        </TabsContent>
        <TabsContent value="tests" className="overflow-auto p-3">
          <TestResults scripts={response.scripts} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

/** A shell command that fixes the error (e.g. `aws sso login`), with a copy button. */
function FixCommand({ command }: { command: string }) {
  return (
    <div className="mt-1 flex items-center gap-1 rounded-md border bg-muted/40 py-1 pr-1 pl-3">
      <code className="text-xs">{command}</code>
      <Button
        variant="ghost"
        size="icon"
        className="size-6"
        aria-label="Copy command"
        title="Copy command"
        onClick={() =>
          void navigator.clipboard.writeText(command).then(() => toast.success("Command copied"))
        }
      >
        <Copy />
      </Button>
    </div>
  );
}

/** Test results and post-response script errors. */
export function TestResults({ scripts }: { scripts: ScriptOutcome }) {
  if (scripts.tests.length === 0 && scripts.errors.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No tests ran. Add some in the Tests tab of the request, its folders or the collection.
      </p>
    );
  }
  return (
    <div className="space-y-3 text-sm">
      {scripts.errors.map((e, i) => (
        <div key={i} className="rounded-md border border-destructive/40 bg-destructive/5 p-2">
          <p className="font-medium text-destructive">
            Error in {e.location}
            {e.line !== null && `, line ${e.line}`}
          </p>
          <p className="font-mono text-xs break-all">{e.message}</p>
        </div>
      ))}
      <ul className="space-y-1">
        {scripts.tests.map((t, i) => (
          <li key={i} className="flex items-start gap-2">
            {t.passed ? (
              <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" />
            ) : (
              <XCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
            )}
            <div className="min-w-0">
              <p>{t.name}</p>
              {t.error && (
                <p className="font-mono text-xs break-all text-muted-foreground">{t.error}</p>
              )}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
