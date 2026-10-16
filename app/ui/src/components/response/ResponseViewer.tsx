import { save } from "@tauri-apps/plugin-dialog";
import { useSubTab } from "@/stores/subTabs";
import { AlertCircle, CheckCircle2, Copy, Download, Loader2, XCircle } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { commands, type ResponseView, type ScriptOutcome } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { showError, unwrap } from "@/lib/api";
import { bodyKind, formatBytes, formatDuration, prettyJson } from "@/lib/format";
import type { RequestTab } from "@/stores/tabs";
import { cn } from "cn";

function statusColor(status: number) {
  if (status < 300) return "bg-success/15 text-success";
  if (status < 400) return "bg-info/15 text-info";
  if (status < 500) return "bg-warning/15 text-warning";
  return "bg-destructive/15 text-destructive";
}

export function ResponseViewer({ tab, dir }: { tab: RequestTab; dir: string }) {
  if (tab.requestId) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" /> Waiting for response…
      </div>
    );
  }
  if (tab.error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
        <AlertCircle className="size-6 text-destructive" />
        <p className="text-sm font-medium">{tab.error.message}</p>
        {tab.error.hint && <p className="text-xs text-muted-foreground">{tab.error.hint}</p>}
        {tab.error.aws?.fixCommand && <FixCommand command={tab.error.aws.fixCommand} />}
      </div>
    );
  }
  if (!tab.response) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Send the request to see the response.
      </div>
    );
  }
  return <ResponsePanel response={tab.response} viewKey={`${dir}:${tab.path}`} />;
}

/** A response: status chips, body (pretty/raw/preview), headers, copy and save. */
export function ResponsePanel({
  response,
  viewKey,
}: {
  response: ResponseView;
  /** Remembers the open sub-tab under this key (e.g. per request tab). */
  viewKey: string;
}) {
  const [pretty, setPretty] = useState(true);
  const kind = bodyKind(response.contentType, response.bodyText);
  const text = response.bodyText ?? "";
  const shown = pretty && kind === "json" ? prettyJson(text) : text;
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
            <div className="mb-2 flex gap-1">
              <Button
                size="sm"
                variant={pretty ? "secondary" : "ghost"}
                onClick={() => setPretty(true)}
              >
                Pretty
              </Button>
              <Button
                size="sm"
                variant={pretty ? "ghost" : "secondary"}
                onClick={() => setPretty(false)}
              >
                Raw
              </Button>
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
