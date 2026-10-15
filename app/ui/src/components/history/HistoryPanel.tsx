import { Search, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type HistorySummary } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { showError, unwrap } from "@/lib/api";
import { methodColor } from "@/lib/methods";
import { timeAgo } from "@/lib/time";
import { useDialogs } from "@/stores/dialogs";
import { HISTORY_TAB_PREFIX, useTabs } from "@/stores/tabs";
import { cn } from "cn";

/** The history sidebar tab (PLAN.md §13): this collection's sends, newest first. */
export function HistoryPanel({ dir }: { dir: string }) {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<HistorySummary[]>([]);
  const [clearing, setClearing] = useState(false);
  const version = useDialogs((s) => s.historyVersion);
  const openTab = useTabs((s) => s.openTab);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let cancelled = false;
    // Debounced, so typing in the search box doesn't query on every key.
    const timer = setTimeout(() => {
      void unwrap(commands.listHistory(dir, search)).then(
        (list) => !cancelled && setEntries(list),
        showError,
      );
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [dir, search, version, reload]);

  const clear = async () => {
    try {
      await unwrap(commands.clearHistory());
      setReload((r) => r + 1);
    } catch (e) {
      showError(e);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 px-2 pb-2">
        <div className="relative flex-1">
          <Search className="absolute top-2 left-2 size-3.5 text-muted-foreground" />
          <Input
            className="h-7 pl-7 text-xs"
            placeholder="Search history"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title="Clear all history"
          aria-label="Clear all history"
          disabled={entries.length === 0}
          onClick={() => setClearing(true)}
        >
          <Trash2 />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto text-sm">
        {entries.length === 0 && (
          <p className="px-3 py-2 text-xs text-muted-foreground">
            {search ? "No matches." : "Sent requests show up here."}
          </p>
        )}
        {entries.map((e) => (
          <button
            key={e.id}
            type="button"
            onClick={() => void openTab(dir, `${HISTORY_TAB_PREFIX}${e.id}`, { preview: true })}
            onDoubleClick={() => void openTab(dir, `${HISTORY_TAB_PREFIX}${e.id}`)}
            className="flex w-full flex-col gap-0.5 px-3 py-1.5 text-left hover:bg-accent"
          >
            <span className="flex items-center gap-2">
              <span
                className={cn(
                  "w-12 shrink-0 font-mono text-[10px] font-semibold",
                  methodColor(e.method),
                )}
              >
                {e.method}
              </span>
              <span className="flex-1 truncate">{e.requestName}</span>
              <span
                className={cn(
                  "font-mono text-[10px]",
                  e.status === null
                    ? "text-destructive"
                    : e.status < 400
                      ? "text-emerald-600 dark:text-emerald-400"
                      : "text-amber-600 dark:text-amber-400",
                )}
              >
                {e.status ?? "error"}
              </span>
            </span>
            <span className="flex gap-2 pl-14 text-xs text-muted-foreground">
              <span className="flex-1 truncate font-mono" title={e.url}>
                {e.url}
              </span>
              <span className="shrink-0">{timeAgo(e.timestamp)}</span>
            </span>
          </button>
        ))}
      </div>
      <ConfirmDialog
        open={clearing}
        title="Clear all history?"
        description="Every history entry, for all collections, will be deleted."
        confirmLabel="Clear history"
        destructive
        onOpenChange={setClearing}
        onConfirm={() => void clear()}
      />
    </div>
  );
}
