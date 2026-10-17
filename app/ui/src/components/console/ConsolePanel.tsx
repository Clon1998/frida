import { ChevronRight, Copy, Search, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import type { ConsoleBody, ConsoleEntry, ScriptLog } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatBytes, formatDuration } from "@/lib/format";
import { methodColor } from "@/lib/methods";
import { prettyBody, rawExchange } from "@/lib/rawHttp";
import { type ConsoleItem, type ConsoleView, useConsole } from "@/stores/console";
import { cn } from "cn";

/**
 * The console (like Postman's and Bruno's): every send of this session, newest at the bottom.
 * An entry expands to every detail of the exchange, either as sections ("Pretty") or as the
 * raw HTTP text ("Raw"). Secret values arrive masked.
 */
export function ConsolePanel() {
  const items = useConsole((s) => s.items);
  const clear = useConsole((s) => s.clear);
  const setOpen = useConsole((s) => s.setOpen);
  const view = useConsole((s) => s.view);
  const setView = useConsole((s) => s.setView);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const list = useRef<HTMLDivElement>(null);

  const needle = filter.trim().toLowerCase();
  const shown = needle ? items.filter((item) => matches(item, needle)) : items;

  // Follow new entries, like a terminal.
  useEffect(() => {
    const el = list.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [items.length]);

  return (
    <section className="flex h-full flex-col border-t bg-background">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <span className="font-medium">Console</span>
        <span className="truncate whitespace-nowrap text-muted-foreground">
          {items.filter((i) => i.kind === "http").length} requests
        </span>
        <div className="flex-1" />
        <ViewToggle view={view} onChange={setView} />
        {/* Shrinks (down to 6rem) before anything else in the bar. */}
        <div className="relative w-56 min-w-24 shrink">
          <Search className="absolute top-1.5 left-2 size-3.5 text-muted-foreground" />
          <Input
            className="h-6 pl-7 text-xs"
            placeholder="Filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          title="Clear console"
          aria-label="Clear console"
          onClick={clear}
        >
          <Trash2 />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          title="Close (Ctrl/Cmd+J)"
          aria-label="Close console"
          onClick={() => setOpen(false)}
        >
          <X />
        </Button>
      </div>
      <div ref={list} className="min-h-0 flex-1 overflow-auto font-mono text-xs">
        {shown.length === 0 && (
          <p className="p-3 font-sans text-muted-foreground">
            {items.length === 0
              ? "Send a request to see its raw request and response, and script logs, here."
              : "No entries match the filter."}
          </p>
        )}
        {shown.map((item) =>
          item.kind === "http" ? (
            <ConsoleRow
              key={item.id}
              entry={item.entry}
              view={view}
              open={expanded === item.id}
              onToggle={() => setExpanded(expanded === item.id ? null : item.id)}
            />
          ) : (
            <LogRow key={item.id} log={item.log} />
          ),
        )}
      </div>
    </section>
  );
}

function matches(item: ConsoleItem, needle: string): boolean {
  const fields =
    item.kind === "http"
      ? [
          item.entry.method,
          item.entry.url,
          item.entry.requestName,
          String(item.entry.response?.status ?? "error"),
        ]
      : [item.log.message, item.log.location, item.log.requestName, item.log.level];
  return fields.some((t) => t.toLowerCase().includes(needle));
}

const LOG_COLORS: Record<ScriptLog["level"], string> = {
  log: "",
  info: "text-info",
  debug: "text-muted-foreground",
  warn: "bg-warning/10 text-warning",
  error: "bg-destructive/10 text-destructive",
};

/** A script's `console.log` line, with where it came from. */
function LogRow({ log }: { log: ScriptLog }) {
  return (
    <div className={cn("flex gap-2 border-b px-3 py-1 pl-8", LOG_COLORS[log.level])}>
      <span className="shrink-0 text-muted-foreground">
        {new Date(log.timestamp).toLocaleTimeString()}
      </span>
      <pre className="min-w-0 flex-1 break-all whitespace-pre-wrap select-text">{log.message}</pre>
      <span className="shrink-0 truncate font-sans text-muted-foreground" title={log.location}>
        {log.location}
      </span>
    </div>
  );
}

function ConsoleRow({
  entry,
  view,
  open,
  onToggle,
}: {
  entry: ConsoleEntry;
  view: ConsoleView;
  open: boolean;
  onToggle: () => void;
}) {
  const r = entry.response;
  return (
    <div className="border-b">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-accent"
      >
        <ChevronRight className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")} />
        <span className="shrink-0 text-muted-foreground">
          {new Date(entry.timestamp).toLocaleTimeString()}
        </span>
        <span className={cn("w-14 shrink-0 font-semibold", methodColor(entry.method))}>
          {entry.method}
        </span>
        <span className="min-w-0 flex-1 truncate" title={entry.url}>
          {entry.url}
        </span>
        {r ? (
          <>
            <span className={r.status < 400 ? "text-success" : "text-warning"}>{r.status}</span>
            <span className="w-16 text-right text-muted-foreground">
              {formatDuration(r.timeMs)}
            </span>
            <span className="w-16 text-right text-muted-foreground">
              {formatBytes(r.sizeBytes)}
            </span>
          </>
        ) : (
          <span className="truncate text-destructive" title={entry.error ?? ""}>
            {entry.error}
          </span>
        )}
      </button>
      {open && (view === "raw" ? <RawView entry={entry} /> : <PrettyView entry={entry} />)}
    </div>
  );
}

function ViewToggle({
  view,
  onChange,
}: {
  view: ConsoleView;
  onChange: (view: ConsoleView) => void;
}) {
  return (
    <div className="flex rounded-md border p-px" role="radiogroup" aria-label="Entry view">
      {(["pretty", "raw"] as const).map((v) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={view === v}
          onClick={() => onChange(v)}
          className={cn(
            "rounded px-2 py-0.5 capitalize",
            view === v ? "bg-secondary text-secondary-foreground" : "text-muted-foreground",
          )}
        >
          {v}
        </button>
      ))}
    </div>
  );
}

/** The whole exchange as raw HTTP text, like Postman's "raw" log. */
function RawView({ entry }: { entry: ConsoleEntry }) {
  const text = rawExchange(entry);
  return (
    <div className="relative bg-muted/30">
      <CopyButton label="raw log" text={text} className="absolute top-1 right-3" />
      <pre className="max-h-96 overflow-auto px-3 py-2 pr-10 break-all whitespace-pre-wrap select-text">
        {text}
      </pre>
    </div>
  );
}

/** Every detail in sections, like Postman's expanded console entry. */
function PrettyView({ entry }: { entry: ConsoleEntry }) {
  const r = entry.response;
  const general: [string, string][] = [
    ["Request URL", entry.url],
    ["Request Method", entry.method],
  ];
  if (r) {
    general.push(["Status", `${r.status} ${r.statusText}`.trim()], ["Protocol", r.httpVersion]);
    if (r.finalUrl && r.finalUrl !== entry.url) general.push(["Final URL", r.finalUrl]);
    general.push(["Time", formatDuration(r.timeMs)], ["Size", formatBytes(r.sizeBytes)]);
  } else {
    general.push(["Error", entry.error ?? "no response"]);
  }
  if (entry.proxy) general.push(["Proxy", entry.proxy]);
  general.push(["Request", `${entry.requestName} (${entry.requestPath})`]);
  return (
    <div className="max-h-96 overflow-auto bg-muted/30 pb-1 select-text">
      <Section title="General">
        <Pairs pairs={general} />
      </Section>
      <Section title="Request Headers" count={entry.requestHeaders.length}>
        <Pairs pairs={entry.requestHeaders.map((h) => [h.name, h.value ?? ""])} />
      </Section>
      {entry.requestBody && (
        <Section title="Request Body">
          <Body body={entry.requestBody} />
        </Section>
      )}
      {r && (
        <Section title="Response Headers" count={r.headers.length}>
          <Pairs pairs={r.headers.map((h) => [h.name, h.value ?? ""])} />
        </Section>
      )}
      {r?.body && (
        <Section title="Response Body">
          <Body body={r.body} />
        </Section>
      )}
    </div>
  );
}

function Section({
  title,
  count,
  children,
}: {
  title: string;
  count?: number;
  children: React.ReactNode;
}) {
  return (
    <details open className="group px-3 pt-1.5">
      <summary className="flex cursor-default list-none items-center gap-1 font-sans font-medium select-none">
        <ChevronRight className="size-3 transition-transform group-open:rotate-90" />
        {title}
        {count !== undefined && <span className="text-muted-foreground">({count})</span>}
      </summary>
      <div className="pt-1 pl-4">{children}</div>
    </details>
  );
}

function Pairs({ pairs }: { pairs: [string, string][] }) {
  if (pairs.length === 0) return <p className="text-muted-foreground">None</p>;
  return (
    <table className="w-full">
      <tbody>
        {pairs.map(([name, value], i) => (
          <tr key={i} className="align-top">
            <td className="w-1/4 py-px pr-4 whitespace-nowrap text-muted-foreground">{name}</td>
            <td className="py-px break-all">{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Body({ body }: { body: ConsoleBody }) {
  const text = prettyBody(body.text);
  return (
    <div className="relative">
      <CopyButton label="body" text={body.text} className="absolute top-0 right-0" />
      <pre className="pr-8 break-all whitespace-pre-wrap">{text}</pre>
      {body.truncated && <p className="text-muted-foreground">… (truncated)</p>}
    </div>
  );
}

function CopyButton({
  label,
  text,
  className,
}: {
  label: string;
  text: string;
  className?: string;
}) {
  const copy = async () => {
    await navigator.clipboard.writeText(text);
    toast.success(`Copied ${label}`);
  };
  return (
    <Button
      variant="ghost"
      size="icon"
      className={cn("size-5", className)}
      title={`Copy ${label}`}
      aria-label={`Copy ${label}`}
      onClick={() => void copy()}
    >
      <Copy />
    </Button>
  );
}
