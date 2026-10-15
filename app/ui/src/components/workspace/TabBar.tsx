import { Folder, Layers, Settings, X } from "lucide-react";
import { useState } from "react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { methodColor } from "@/lib/methods";
import * as actions from "@/stores/actions";
import { useDialogs } from "@/stores/dialogs";
import { isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { cn } from "cn";
import { ConfirmDialog } from "./ConfirmDialog";

const NO_TABS: Tab[] = [];

/**
 * The tabs of one collection. Middle-click closes a tab; right-click offers the usual tab
 * operations (close others, to the right, saved, all, duplicate, revert, ...), like Bruno.
 */
export function TabBar({ dir }: { dir: string }) {
  const tabs = useTabs((s) => s.byCollection[dir]?.tabs) ?? NO_TABS;
  const active = useTabs((s) => s.byCollection[dir]?.active ?? null);
  const setActive = useTabs((s) => s.setActive);
  const closeTabs = useTabs((s) => s.closeTabs);
  const save = useTabs((s) => s.save);
  const revert = useTabs((s) => s.revert);
  const openCurlExport = useDialogs((s) => s.openCurlExport);
  /** Tabs waiting for "close without saving?" confirmation. */
  const [confirmClose, setConfirmClose] = useState<string[] | null>(null);

  /** Closes `paths`, asking first if any of them has unsaved changes. */
  const requestClose = (paths: string[]) => {
    const dirty = tabs.filter((t) => paths.includes(t.path) && isDirty(t));
    if (dirty.length > 0) setConfirmClose(paths);
    else closeTabs(dir, paths);
  };

  if (tabs.length === 0) return null;

  const dirtyInConfirm = confirmClose
    ? tabs.filter((t) => confirmClose.includes(t.path) && isDirty(t))
    : [];
  const paths = (list: Tab[]) => list.map((t) => t.path);

  return (
    <>
      <div className="flex h-9 shrink-0 items-end overflow-x-auto border-b bg-muted/30">
        {tabs.map((tab, index) => (
          <ContextMenu key={tab.path}>
            <ContextMenuTrigger asChild>
              <div
                title={tab.path}
                onClick={() => setActive(dir, tab.path)}
                onAuxClick={(e) => {
                  if (e.button === 1) requestClose([tab.path]);
                }}
                className={cn(
                  "group flex h-full max-w-56 min-w-0 cursor-default items-center gap-1.5 border-r px-3 text-sm select-none",
                  tab.path === active ? "bg-background" : "text-muted-foreground hover:bg-accent",
                )}
              >
                <TabIcon tab={tab} />
                <span className="truncate">{tabTitle(tab)}</span>
                <button
                  type="button"
                  aria-label="Close tab"
                  onClick={(e) => {
                    e.stopPropagation();
                    requestClose([tab.path]);
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
            </ContextMenuTrigger>
            <ContextMenuContent className="w-56">
              <ContextMenuItem disabled={!isDirty(tab)} onSelect={() => void save(dir, tab.path)}>
                Save <ContextMenuShortcut>⌘S</ContextMenuShortcut>
              </ContextMenuItem>
              <ContextMenuItem disabled={!isDirty(tab)} onSelect={() => revert(dir, tab.path)}>
                Revert changes
              </ContextMenuItem>
              {tab.kind === "request" && (
                <ContextMenuItem onSelect={() => openCurlExport(dir, tab.path)}>
                  Copy as curl
                </ContextMenuItem>
              )}
              {tab.kind === "request" && (
                <ContextMenuItem onSelect={() => void actions.duplicateRequest(dir, tab.path)}>
                  Duplicate request
                </ContextMenuItem>
              )}
              {(tab.kind === "request" || tab.kind === "folder") && (
                <ContextMenuItem onSelect={() => void actions.revealItem(dir, tab.path)}>
                  Reveal in file manager
                </ContextMenuItem>
              )}
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={() => requestClose([tab.path])}>
                Close <ContextMenuShortcut>⌘W</ContextMenuShortcut>
              </ContextMenuItem>
              <ContextMenuItem
                disabled={tabs.length === 1}
                onSelect={() => requestClose(paths(tabs.filter((t) => t.path !== tab.path)))}
              >
                Close others
              </ContextMenuItem>
              <ContextMenuItem
                disabled={index === 0}
                onSelect={() => requestClose(paths(tabs.slice(0, index)))}
              >
                Close to the left
              </ContextMenuItem>
              <ContextMenuItem
                disabled={index === tabs.length - 1}
                onSelect={() => requestClose(paths(tabs.slice(index + 1)))}
              >
                Close to the right
              </ContextMenuItem>
              <ContextMenuItem
                disabled={!tabs.some((t) => !isDirty(t))}
                onSelect={() => closeTabs(dir, paths(tabs.filter((t) => !isDirty(t))))}
              >
                Close saved
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => requestClose(paths(tabs))}>
                Close all
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        ))}
      </div>
      <ConfirmDialog
        open={confirmClose !== null}
        title="Close without saving?"
        description={
          dirtyInConfirm.length === 1 && dirtyInConfirm[0]
            ? `"${tabTitle(dirtyInConfirm[0])}" has unsaved changes that will be lost.`
            : `${dirtyInConfirm.length} tabs have unsaved changes that will be lost: ${dirtyInConfirm
                .map(tabTitle)
                .join(", ")}.`
        }
        confirmLabel="Close"
        destructive
        onOpenChange={(o) => !o && setConfirmClose(null)}
        onConfirm={() => {
          if (confirmClose) closeTabs(dir, confirmClose);
        }}
      />
    </>
  );
}

/** Method for requests, a folder icon for folders, a gear for the collection settings. */
function TabIcon({ tab }: { tab: Tab }) {
  switch (tab.kind) {
    case "request":
      return (
        <span className={cn("font-mono text-[10px] font-semibold", methodColor(tab.draft.method))}>
          {tab.draft.method}
        </span>
      );
    case "folder":
      return <Folder className="size-3.5 shrink-0" />;
    case "collection":
      return <Settings className="size-3.5 shrink-0" />;
    case "environments":
      return <Layers className="size-3.5 shrink-0" />;
  }
}
