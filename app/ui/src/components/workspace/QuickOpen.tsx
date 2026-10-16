import { useMemo, useState } from "react";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { fuzzyFilter } from "@/lib/fuzzy";
import { methodColor } from "@/lib/methods";
import { type RequestEntry as Entry, flattenRequests } from "@/lib/tree";
import { useTabs } from "@/stores/tabs";
import { useUi } from "@/stores/ui";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { cn } from "cn";

const MAX_SHOWN = 50;

/** Ctrl/Cmd+K: fuzzy search the requests of the active collection and open one. */
export function QuickOpen() {
  const open = useUi((s) => s.quickOpen);
  const setOpen = useUi((s) => s.setQuickOpen);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="top-24 translate-y-0 gap-0 p-0 sm:max-w-xl" showCloseButton={false}>
        <DialogTitle className="sr-only">Open a request</DialogTitle>
        {open && <Search onDone={() => setOpen(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function Search({ onDone }: { onDone: () => void }) {
  const collection = useActiveCollection();
  const tree = useWorkspace((s) => (collection ? s.trees[collection.dir] : undefined));
  const openTab = useTabs((s) => s.openTab);
  const pin = useTabs((s) => s.pin);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);

  const entries = useMemo(() => flattenRequests(tree ?? []), [tree]);
  const shown = fuzzyFilter(entries, query, (e) => `${e.folder} ${e.name} ${e.method}`).slice(
    0,
    MAX_SHOWN,
  );

  const choose = async (entry: Entry | undefined) => {
    if (!entry || !collection) return;
    onDone();
    await openTab(collection.dir, entry.path);
    pin(collection.dir, entry.path);
  };

  return (
    <div className="flex flex-col">
      <input
        autoFocus
        className="h-11 border-b bg-transparent px-4 text-sm outline-none"
        placeholder={`Search requests in ${collection?.name ?? "the collection"}…`}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setSelected(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setSelected((i) => Math.min(i + 1, shown.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setSelected((i) => Math.max(i - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            void choose(shown[selected]);
          }
        }}
      />
      <ul className="max-h-80 overflow-auto py-1" role="listbox">
        {shown.map((entry, i) => (
          <li key={entry.path} role="option" aria-selected={i === selected}>
            <button
              type="button"
              onMouseMove={() => setSelected(i)}
              onClick={() => void choose(entry)}
              className={cn(
                "flex w-full items-center gap-3 px-4 py-1.5 text-left text-sm",
                i === selected && "bg-accent",
              )}
            >
              <span
                className={cn(
                  "w-14 shrink-0 font-mono text-[10px] font-semibold",
                  methodColor(entry.method),
                )}
              >
                {entry.method}
              </span>
              <span className="truncate">{entry.name}</span>
              <span className="ml-auto truncate text-xs text-muted-foreground">{entry.folder}</span>
            </button>
          </li>
        ))}
        {shown.length === 0 && (
          <li className="px-4 py-2 text-sm text-muted-foreground">No matching requests.</li>
        )}
      </ul>
    </div>
  );
}
