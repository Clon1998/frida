import { X } from "lucide-react";
import { useState } from "react";
import { methodColor } from "@/lib/methods";
import { isDirty, type Tab, useTabs } from "@/stores/tabs";
import { cn } from "cn";
import { ConfirmDialog } from "./ConfirmDialog";

const NO_TABS: Tab[] = [];

/** The request tabs of one collection. Middle-click closes a tab. */
export function TabBar({ dir }: { dir: string }) {
  const tabs = useTabs((s) => s.byCollection[dir]?.tabs) ?? NO_TABS;
  const active = useTabs((s) => s.byCollection[dir]?.active ?? null);
  const setActive = useTabs((s) => s.setActive);
  const closeTab = useTabs((s) => s.closeTab);
  const [confirmClose, setConfirmClose] = useState<string | null>(null);

  const requestClose = (tab: Tab) => {
    if (isDirty(tab)) setConfirmClose(tab.path);
    else closeTab(dir, tab.path);
  };

  if (tabs.length === 0) return null;

  return (
    <>
      <div className="flex h-9 shrink-0 items-end overflow-x-auto border-b bg-muted/30">
        {tabs.map((tab) => (
          <div
            key={tab.path}
            title={tab.path}
            onClick={() => setActive(dir, tab.path)}
            onAuxClick={(e) => {
              if (e.button === 1) requestClose(tab);
            }}
            className={cn(
              "group flex h-full max-w-56 min-w-0 cursor-default items-center gap-1.5 border-r px-3 text-sm select-none",
              tab.path === active ? "bg-background" : "text-muted-foreground hover:bg-accent",
            )}
          >
            <span
              className={cn("font-mono text-[10px] font-semibold", methodColor(tab.draft.method))}
            >
              {tab.draft.method}
            </span>
            <span className="truncate">{tab.draft.name}</span>
            <button
              type="button"
              aria-label="Close tab"
              onClick={(e) => {
                e.stopPropagation();
                requestClose(tab);
              }}
              className="ml-1 flex size-4 shrink-0 items-center justify-center rounded hover:bg-muted"
            >
              {/* Unsaved tabs show a dot that turns into the close button on hover. */}
              {isDirty(tab) ? (
                <>
                  <span className="size-2 rounded-full bg-foreground group-hover:hidden" />
                  <X className="hidden size-3 group-hover:block" />
                </>
              ) : (
                <X className="size-3 opacity-0 group-hover:opacity-100" />
              )}
            </button>
          </div>
        ))}
      </div>
      <ConfirmDialog
        open={confirmClose !== null}
        title="Close without saving?"
        description="Your unsaved changes to this request will be lost."
        confirmLabel="Close"
        destructive
        onOpenChange={(o) => !o && setConfirmClose(null)}
        onConfirm={() => {
          if (confirmClose) closeTab(dir, confirmClose);
        }}
      />
    </>
  );
}
