import {
  AlertTriangle,
  CheckCircle2,
  CircleDashed,
  Loader2,
  MinusCircle,
  Play,
  Square,
  XCircle,
} from "lucide-react";
import { useEffect } from "react";
import type { RunOutcome, RunProgress, RunStop, RunSummary } from "@/bindings";
import { ResponsePanel } from "@/components/response/ResponseViewer";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatDuration } from "@/lib/format";
import { methodColor } from "@/lib/methods";
import { isUnder } from "@/lib/tree";
import { runKey, useRunner } from "@/stores/runner";
import { isDirty, type RunnerTab, useTabs } from "@/stores/tabs";
import { cn } from "cn";

const DELAYS = [0, 100, 500, 1000, 2000];

/**
 * The collection runner (PLAN.md M11): sends the saved requests of a folder or the whole
 * collection in order and lists the results. Click a result to see its response.
 */
export function RunnerView({ dir, tab }: { dir: string; tab: RunnerTab }) {
  const { folder } = tab;
  const run = useRunner((s) => s.runs[runKey(dir, folder)]);
  const delayMs = useRunner((s) => s.delayMs);
  const { plan, start, stop, select, setDelay } = useRunner.getState();
  // Open request tabs in this folder with unsaved edits: the run uses the saved files.
  const unsaved = useTabs(
    (s) =>
      s.byCollection[dir]?.tabs.filter(
        (t) => t.kind === "request" && isDirty(t) && (folder === "" || isUnder(t.path, folder)),
      ).length ?? 0,
  );

  useEffect(() => {
    void plan(dir, folder);
  }, [dir, folder, plan]);

  const running = run?.runId != null;
  const results = run?.results ?? [];
  const selected = run?.selected != null ? results[run.selected] : undefined;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2 text-sm">
        {running ? (
          <Button size="sm" variant="outline" onClick={() => void stop(dir, folder)}>
            <Square /> Stop
          </Button>
        ) : (
          <Button size="sm" disabled={!run?.items.length} onClick={() => void start(dir, folder)}>
            <Play /> Run {run?.items.length ?? 0} requests
          </Button>
        )}
        <span className="text-xs text-muted-foreground">Delay</span>
        <Select value={String(delayMs)} onValueChange={(v) => setDelay(Number(v))}>
          <SelectTrigger size="sm" className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {DELAYS.map((d) => (
              <SelectItem key={d} value={String(d)}>
                {d === 0 ? "None" : formatDuration(d)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex-1" />
        {run?.summary && <Summary summary={run.summary} />}
      </div>

      {unsaved > 0 && (
        <p className="border-b bg-warning/10 px-3 py-1.5 text-xs">
          {unsaved === 1 ? "1 open request has" : `${unsaved} open requests have`} unsaved changes.
          The run uses the saved files.
        </p>
      )}
      {run?.error && (
        <p className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {run.error.message}
        </p>
      )}

      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel id="runner-list" defaultSize="40" minSize={220}>
          <div className="h-full overflow-auto py-1 text-sm">
            {results.length === 0 && !running
              ? run?.items.map((item) => (
                  <div
                    key={item.path}
                    className="flex items-center gap-2 px-3 py-1 text-muted-foreground"
                  >
                    <CircleDashed className="size-4 shrink-0" />
                    <Method method={item.method} />
                    <span className="truncate">{item.name}</span>
                  </div>
                ))
              : results.map((progress, index) => (
                  <ResultRow
                    key={progress.result.step}
                    progress={progress}
                    active={run?.selected === index}
                    onSelect={() => select(dir, folder, index)}
                  />
                ))}
            {running && (
              <div className="flex items-center gap-2 px-3 py-1 text-muted-foreground">
                <Loader2 className="size-4 animate-spin" /> Running…
              </div>
            )}
            {run?.items.length === 0 && (
              <p className="px-3 py-2 text-muted-foreground">No requests in here yet.</p>
            )}
          </div>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel id="runner-detail" minSize={240}>
          {selected ? (
            <ResultDetail progress={selected} viewKey={`runner:${dir}:${folder}`} />
          ) : (
            <div className="flex h-full items-center justify-center p-6 text-center text-sm text-muted-foreground">
              {results.length ? "Select a result to see its response." : "Run to see results here."}
            </div>
          )}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}

function Method({ method }: { method: string }) {
  return (
    <span className={cn("w-12 shrink-0 font-mono text-[10px] font-semibold", methodColor(method))}>
      {method}
    </span>
  );
}

function OutcomeIcon({ outcome }: { outcome: RunOutcome }) {
  switch (outcome) {
    case "passed":
      return <CheckCircle2 className="size-4 shrink-0 text-success" aria-label="passed" />;
    case "failed":
      return <XCircle className="size-4 shrink-0 text-destructive" aria-label="failed" />;
    case "error":
      return <AlertTriangle className="size-4 shrink-0 text-warning" aria-label="error" />;
    case "skipped":
      return <MinusCircle className="size-4 shrink-0 text-muted-foreground" aria-label="skipped" />;
  }
}

function ResultRow({
  progress,
  active,
  onSelect,
}: {
  progress: RunProgress;
  active: boolean;
  onSelect: () => void;
}) {
  const r = progress.result;
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-accent",
        active && "bg-accent",
      )}
    >
      <OutcomeIcon outcome={r.outcome} />
      <Method method={r.item.method} />
      <span className="min-w-0 flex-1 truncate">{r.item.name}</span>
      <span className="shrink-0 text-xs whitespace-nowrap text-muted-foreground">
        {r.status ?? (r.outcome === "skipped" ? "skipped" : "—")}
        {r.timeMs != null && ` · ${formatDuration(r.timeMs)}`}
        {r.testsTotal > 0 && ` · ${r.testsPassed}/${r.testsTotal}`}
      </span>
    </button>
  );
}

function ResultDetail({ progress, viewKey }: { progress: RunProgress; viewKey: string }) {
  const r = progress.result;
  return (
    <div className="flex h-full flex-col">
      {r.message && (
        <p
          className={cn(
            "border-b px-3 py-2 text-xs",
            r.outcome === "error" ? "text-warning" : "text-destructive",
          )}
        >
          {r.message}
        </p>
      )}
      {progress.response ? (
        <div className="min-h-0 flex-1">
          <ResponsePanel response={progress.response} viewKey={viewKey} />
        </div>
      ) : (
        <div className="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
          {r.outcome === "skipped" ? "Skipped by its pre-request script." : "Not sent."}
        </div>
      )}
    </div>
  );
}

function Summary({ summary }: { summary: RunSummary }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs whitespace-nowrap">
      <span className="text-success">{summary.passed} passed</span>
      {summary.failed > 0 && <span className="text-destructive">{summary.failed} failed</span>}
      {summary.errors > 0 && <span className="text-warning">{summary.errors} not sent</span>}
      {summary.skipped > 0 && (
        <span className="text-muted-foreground">{summary.skipped} skipped</span>
      )}
      <span className="text-muted-foreground">{formatDuration(summary.durationMs)}</span>
      {summary.stopped && (
        <span className="text-muted-foreground">· {stopText(summary.stopped)}</span>
      )}
    </div>
  );
}

function stopText(stop: RunStop): string {
  switch (stop.kind) {
    case "cancelled":
      return "stopped";
    case "script":
      return "stopped by a script";
    case "unknownNextRequest":
      return `stopped: no request named "${stop.name}"`;
    case "loopGuard":
      return "stopped: too many repeats (setNextRequest loop?)";
  }
}
