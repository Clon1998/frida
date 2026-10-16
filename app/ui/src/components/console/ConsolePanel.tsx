import { ChevronRight, Copy, Search, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import type { ConsoleEntry } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatBytes, formatDuration } from "@/lib/format";
import { methodColor } from "@/lib/methods";
import { rawRequest, rawResponse } from "@/lib/rawHttp";
import { useConsole } from "@/stores/console";
import { cn } from "cn";

/**
 * The console (like Postman's and Bruno's): every send of this session, newest at the bottom,
 * expandable to the raw request and response. Secret values arrive masked.
 */
export function ConsolePanel() {
  const entries = useConsole((s) => s.entries);
  const clear = useConsole((s) => s.clear);
  const setOpen = useConsole((s) => s.setOpen);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const list = useRef<HTMLDivElement>(null);

  const needle = filter.trim().toLowerCase();
  const shown = needle
    ? entries.filter((e) =>
        [e.method, e.url, e.requestName, String(e.response?.status ?? "error")].some((t) =>
          t.toLowerCase().includes(needle),
        ),
      )
    : entries;

  // Follow new entries, like a terminal.
  useEffect(() => {
    const el = list.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries.length]);

  return (
    <section className="flex h-full flex-col border-t bg-background">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <span className="font-medium">Console</span>
        <span className="text-muted-foreground">{entries.length} requests</span>
        <div className="flex-1" />
        <div className="relative w-56">
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
            {entries.length === 0
              ? "Send a request to see its raw request and response here."
              : "No entries match the filter."}
          </p>
        )}
        {shown.map((e) => (
          <ConsoleRow
            key={e.id}
            entry={e}
            open={expanded === e.id}
            onToggle={() => setExpanded(expanded === e.id ? null : e.id)}
          />
        ))}
      </div>
    </section>
  );
}

function ConsoleRow({
  entry,
  open,
  onToggle,
}: {
  entry: ConsoleEntry;
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
            <span
              className={
                r.status < 400
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-amber-600 dark:text-amber-400"
              }
            >
              {r.status}
            </span>
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
      {open && (
        <div className="grid grid-cols-2 gap-px bg-border">
          <RawBlock title="Request" text={rawRequest(entry)} />
          <RawBlock title="Response" text={rawResponse(entry)} />
        </div>
      )}
    </div>
  );
}

function RawBlock({ title, text }: { title: string; text: string }) {
  const copy = async () => {
    await navigator.clipboard.writeText(text);
    toast.success(`${title} copied`);
  };
  return (
    <div className="min-w-0 bg-muted/30">
      <div className="flex items-center justify-between px-3 py-1 font-sans text-muted-foreground">
        <span>{title}</span>
        <Button
          variant="ghost"
          size="icon"
          className="size-5"
          aria-label={`Copy ${title}`}
          onClick={() => void copy()}
        >
          <Copy />
        </Button>
      </div>
      <pre className="max-h-80 overflow-auto px-3 pb-2 break-all whitespace-pre-wrap select-text">
        {text}
      </pre>
    </div>
  );
}
