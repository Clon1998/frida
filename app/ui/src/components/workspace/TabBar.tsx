import { Folder, GitCompare, History, Layers, Play, Settings, X } from "lucide-react";
import { useEffect, useRef } from "react";
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
import { useEnvs } from "@/stores/envs";
import { isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { cn } from "cn";

const NO_TABS: Tab[] = [];

/**
 * The tabs of one collection. Middle-click closes a tab; right-click offers the usual tab
 * operations (close others, to the right, saved, all, duplicate, revert, ...), like Bruno.
 */
export function TabBar({ dir }: { dir: string }) {
  const tabs = useTabs((s) => s.byCollection[dir]?.tabs) ?? NO_TABS;
  const active = useTabs((s) => s.byCollection[dir]?.active ?? null);
  const preview = useTabs((s) => s.byCollection[dir]?.preview ?? null);
  // Re-render when the environment editor's unsaved state changes (its dot reads the store).
  useEnvs((s) => s.editorDrafts);
  const pin = useTabs((s) => s.pin);
  const setActive = useTabs((s) => s.setActive);
  const closeTabs = useTabs((s) => s.closeTabs);
  const save = useTabs((s) => s.save);
  const revert = useTabs((s) => s.revert);
  const openCurlExport = useDialogs((s) => s.openCurlExport);
  const barRef = useRef<HTMLDivElement>(null);

  // Keep the active tab visible when it changes (e.g. opened from the sidebar).
  useEffect(() => {
    const bar = barRef.current;
    if (!bar || active === null) return;
    const el = [...bar.querySelectorAll<HTMLElement>("[data-tab-path]")].find(
      (node) => node.dataset.tabPath === active,
    );
    el?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [active, tabs.length]);

  const requestClose = (paths: string[]) => actions.closeTabsAsking(dir, paths);

  if (tabs.length === 0) return null;

  const paths = (list: Tab[]) => list.map((t) => t.path);

  return (
    <>
      <div
        ref={barRef}
        onWheel={(e) => {
          // A normal mouse wheel scrolls vertically; turn that into sideways scrolling here.
          if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) e.currentTarget.scrollLeft += e.deltaY;
        }}
        // No visible scrollbar (Windows draws a thick one): the wheel scrolls sideways and the
        // active tab is scrolled into view.
        className="flex h-9 shrink-0 items-end overflow-x-auto overflow-y-hidden border-b bg-muted/30 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {tabs.map((tab, index) => (
          <ContextMenu key={tab.path}>
            <ContextMenuTrigger asChild>
              <div
                title={tab.path}
                data-tab-path={tab.path}
                onClick={() => setActive(dir, tab.path)}
                onDoubleClick={() => pin(dir, tab.path)}
                // Middle-click closes. Not `onAuxClick`: on Windows a middle press on a
                // scrollable strip starts autoscroll and the click never arrives, and WebKit
                // fires it unreliably. Preventing the press stops autoscroll; the release closes.
                onMouseDown={(e) => {
                  if (e.button === 1) e.preventDefault();
                }}
                onMouseUp={(e) => {
                  if (e.button === 1) requestClose([tab.path]);
                }}
                className={cn(
                  // Fixed minimum width: tabs never shrink; the bar scrolls instead.
                  "group flex h-full w-auto max-w-56 min-w-36 shrink-0 cursor-default items-center gap-1.5 border-r px-3 text-sm select-none",
                  tab.path === active ? "bg-background" : "text-muted-foreground hover:bg-accent",
                )}
              >
                <TabIcon tab={tab} />
                <span
                  className={cn("truncate", tab.path === preview && "italic")}
                  title={
                    tab.path === preview
                      ? "Preview: double-click or edit to keep it open"
                      : undefined
                  }
                >
                  {tabTitle(tab)}
                </span>
                <button
                  type="button"
                  aria-label="Close tab"
                  onClick={(e) => {
                    e.stopPropagation();
                    requestClose([tab.path]);
                  }}
                  className="ml-auto flex size-4 shrink-0 items-center justify-center rounded hover:bg-muted"
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
    case "history":
      return <History className="size-3.5 shrink-0" />;
    case "runner":
      return <Play className="size-3.5 shrink-0" />;
    case "git":
      return <GitCompare className="size-3.5 shrink-0" />;
  }
}
