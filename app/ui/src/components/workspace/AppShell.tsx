import { ask } from "@tauri-apps/plugin-dialog";
import { Columns2, Keyboard, Rows2, Settings, SquareChevronUp } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, events } from "@/bindings";
import { CurlExportDialog } from "@/components/curl/CurlExportDialog";
import { CurlImportDialog } from "@/components/curl/CurlImportDialog";
import { CollectionEditor } from "@/components/doc/CollectionEditor";
import { FolderEditor } from "@/components/doc/FolderEditor";
import { EnvironmentsEditor } from "@/components/env/EnvironmentsEditor";
import { ConsolePanel } from "@/components/console/ConsolePanel";
import { QuickOpen } from "./QuickOpen";
import { ShortcutsDialog } from "./ShortcutsDialog";
import { formatKeys, isMac } from "@/lib/shortcuts";
import * as actions from "@/stores/actions";
import { AwsProfileDialog } from "@/components/env/AwsProfileDialog";
import { GitStatusIndicator } from "@/components/git/GitStatusIndicator";
import { HistoryView } from "@/components/history/HistoryView";
import { useConsole } from "@/stores/console";
import { InspectorPanel } from "@/components/inspector/InspectorPanel";
import { useUi } from "@/stores/ui";
import { RequestEditor } from "@/components/request/RequestEditor";
import { ResponseViewer } from "@/components/response/ResponseViewer";
import { Sidebar } from "@/components/sidebar/Sidebar";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { useEnvs } from "@/stores/envs";
import { isDirty, tabTitle, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { RequestHeader } from "./RequestHeader";
import { TabBar } from "./TabBar";

export function AppShell() {
  const collection = useActiveCollection();
  const dir = collection?.dir ?? null;
  const tab = useTabs((s) => {
    if (!dir) return null;
    const c = s.byCollection[dir];
    return c?.tabs.find((t) => t.path === c.active) ?? null;
  });
  const [split, setSplit] = useState<"horizontal" | "vertical">("horizontal");
  const inspectorOpen = useUi((s) => s.inspectorOpen);
  const setInspectorOpen = useUi((s) => s.setInspectorOpen);
  const report = useUi((s) => s.report);
  const consoleOpen = useConsole((s) => s.open);
  const toggleConsole = useConsole((s) => s.toggle);
  const showInspector = inspectorOpen && dir !== null && tab?.kind === "request";

  useRestoreTabs();
  useConsoleEvents();
  useLoadEnvs();
  usePersistTabs();
  useFileWatcher();
  useShortcuts();

  return (
    <div className="flex h-full flex-col">
      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel id="sidebar" defaultSize="22" minSize={180} maxSize="45">
          <Sidebar />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel id="main">
          {/* The work area above, the console below (when open). */}
          <ResizablePanelGroup orientation="vertical" className="h-full">
            <ResizablePanel id="work" minSize={200}>
              <main className="flex h-full min-w-0 flex-col">
                {dir && <TabBar dir={dir} />}
                {dir && <RequestHeader dir={dir} tab={tab} />}
                {dir && tab?.kind === "request" && (
                  <ResizablePanelGroup key={split} orientation={split} className="min-h-0 flex-1">
                    <ResizablePanel defaultSize="50" minSize={200}>
                      <RequestEditor dir={dir} tab={tab} />
                    </ResizablePanel>
                    <ResizableHandle />
                    <ResizablePanel defaultSize="50" minSize={150}>
                      <ResponseViewer dir={dir} tab={tab} />
                    </ResizablePanel>
                  </ResizablePanelGroup>
                )}
                {dir && tab?.kind === "folder" && (
                  <div className="min-h-0 flex-1">
                    <FolderEditor dir={dir} tab={tab} />
                  </div>
                )}
                {dir && tab?.kind === "collection" && (
                  <div className="min-h-0 flex-1">
                    <CollectionEditor dir={dir} tab={tab} />
                  </div>
                )}
                {dir && tab?.kind === "environments" && (
                  <div className="min-h-0 flex-1">
                    <EnvironmentsEditor dir={dir} />
                  </div>
                )}
                {dir && tab?.kind === "history" && (
                  <div className="min-h-0 flex-1">
                    <HistoryView key={tab.path} dir={dir} path={tab.path} />
                  </div>
                )}
                {!tab && <EmptyState />}
              </main>
            </ResizablePanel>
            {consoleOpen && (
              <>
                <ResizableHandle />
                <ResizablePanel id="console" defaultSize="30" minSize={100}>
                  <ConsolePanel />
                </ResizablePanel>
              </>
            )}
          </ResizablePanelGroup>
        </ResizablePanel>
        {showInspector && (
          // The variables drawer: full height on the right of the window (PLAN.md §14).
          <>
            <ResizableHandle />
            <ResizablePanel id="inspector" defaultSize="22" minSize={240} maxSize="45">
              <InspectorPanel dir={dir} report={report} onClose={() => setInspectorOpen(false)} />
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>

      <CurlExportDialog />
      <CurlImportDialog />
      <QuickOpen />
      <ShortcutsDialog />
      <AwsProfileDialog />
      <footer className="flex h-7 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
        <span className="truncate">{collection ? collection.dir : "No collection"}</span>
        {collection && <GitStatusIndicator dir={collection.dir} />}
        <div className="flex-1" />
        <Button
          variant="ghost"
          size="icon"
          className="size-5"
          title="Keyboard shortcuts (Ctrl/Cmd+/)"
          aria-label="Keyboard shortcuts"
          onClick={() => useUi.getState().setShortcutsOpen(true)}
        >
          <Keyboard />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-5"
          title="Settings (Ctrl/Cmd+,)"
          aria-label="Settings"
          onClick={() => useUi.getState().setSettingsOpen(true)}
        >
          <Settings />
        </Button>
        <Button
          variant={consoleOpen ? "secondary" : "ghost"}
          size="sm"
          className="h-5 gap-1 px-1.5 text-xs"
          title="Console (Ctrl/Cmd+J)"
          onClick={toggleConsole}
        >
          <SquareChevronUp className="size-3.5" /> Console
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-5"
          title="Toggle request/response layout"
          onClick={() => setSplit((s) => (s === "horizontal" ? "vertical" : "horizontal"))}
        >
          {split === "horizontal" ? <Rows2 /> : <Columns2 />}
        </Button>
      </footer>
    </div>
  );
}

/** Collects what Rust emits for the console: every send, and every script `console.log`. */
function useConsoleEvents() {
  useEffect(() => {
    const unlisteners = [
      events.consoleLogged.listen(({ payload }) => useConsole.getState().add(payload.entry)),
      events.scriptLogged.listen(({ payload }) => useConsole.getState().addLog(payload.log)),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((stop) => stop());
    };
  }, []);
}

/** Loads the environments (and the active selection) when the active collection changes. */
function useLoadEnvs() {
  const dir = useActiveCollection()?.dir;
  const load = useEnvs((s) => s.load);
  useEffect(() => {
    if (dir) void load(dir);
  }, [dir, load]);
}

/** Restores the saved tabs the first time a collection becomes active. */
function useRestoreTabs() {
  const collection = useActiveCollection();
  const restore = useTabs((s) => s.restore);
  useEffect(() => {
    if (collection?.id) void restore(collection.dir, collection.id);
  }, [collection?.dir, collection?.id, restore]);
}

/** Saves open tabs and expanded folders to local state, shortly after they change. */
function usePersistTabs() {
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = useTabs.subscribe((state, previous) => {
      if (state.byCollection === previous.byCollection) return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        const collections = useWorkspace.getState().workspace?.collections ?? [];
        for (const [dir, c] of Object.entries(useTabs.getState().byCollection)) {
          const id = collections.find((col) => col.dir === dir)?.id;
          if (!id) continue;
          void commands.saveCollectionState(id, {
            openTabs: c.tabs.map((t) => t.path),
            activeTab: c.active,
            expandedFolders: c.expanded,
          });
        }
      }, 500);
    });
    return () => {
      clearTimeout(timer);
      unsubscribe();
    };
  }, []);
}

/** Reloads trees and open tabs when files change on disk (git pull, an editor, ...). */
function useFileWatcher() {
  useEffect(() => {
    const unlisten = events.fsChanged.listen(({ payload }) => {
      const { workspace, reloadTree, reloadWorkspace } = useWorkspace.getState();
      if (!workspace) return;
      if (payload.paths.some((p) => p.endsWith("frida-workspace.yaml"))) void reloadWorkspace();
      // Environment files changed (git pull, editor): reload the lists and selection.
      const envDir = useEnvs.getState().dir;
      if (envDir && payload.paths.some((p) => p.includes("/environments/"))) {
        void useEnvs.getState().load(envDir);
      }
      for (const collection of workspace.collections) {
        if (payload.paths.some((p) => p.startsWith(collection.dir))) {
          void reloadTree(collection.dir);
          void useTabs.getState().syncFromDisk(collection.dir);
        }
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);
}

/**
 * App-wide shortcuts (PLAN.md §14, listed in `lib/shortcuts.ts`): Ctrl/Cmd+Enter sends,
 * +S saves, +W closes the tab, +T new request, +K quick open, +E environments, +J console,
 * +/ shortcut list, +, settings.
 */
function useShortcuts() {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      // Cmd on macOS (where Ctrl+E/K/T are text-editing keys), Ctrl elsewhere.
      const mod = isMac() ? e.metaKey : e.ctrlKey;
      if (!mod || e.altKey) return;
      const ui = useUi.getState();
      const dir = useWorkspace.getState().activeCollectionDir;
      switch (e.key) {
        case "j":
          e.preventDefault();
          useConsole.getState().toggle();
          return;
        case "/":
          e.preventDefault();
          ui.setShortcutsOpen(true);
          return;
        case ",":
          e.preventDefault();
          ui.setSettingsOpen(true);
          return;
        case "k":
          if (!dir) return;
          e.preventDefault();
          ui.setQuickOpen(true);
          return;
        case "e":
          if (!dir) return;
          e.preventDefault();
          ui.setEnvPickerOpen(true);
          return;
        case "t":
          if (!dir) return;
          e.preventDefault();
          void actions.createRequest(dir, "", "New request");
          return;
      }
      if (!dir) return;
      const tabs = useTabs.getState();
      const active = tabs.byCollection[dir]?.active;
      if (!active) return;
      if (e.key === "Enter") {
        e.preventDefault();
        void tabs.send(dir, active);
      } else if (e.key === "s") {
        e.preventDefault();
        void tabs.save(dir, active);
      } else if (e.key === "w") {
        e.preventDefault();
        const tab = tabs.byCollection[dir]?.tabs.find((t) => t.path === active);
        if (tab && isDirty(tab)) {
          void ask(`"${tabTitle(tab)}" has unsaved changes that will be lost.`, {
            title: "Close without saving?",
            kind: "warning",
            okLabel: "Close",
          }).then((close) => close && tabs.closeTab(dir, active));
        } else {
          tabs.closeTab(dir, active);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

/** The main area when no tab is open: what to do next and the most useful shortcuts. */
function EmptyState() {
  const rows: [string, string][] = [
    ["Mod+K", "Search requests"],
    ["Mod+T", "New request"],
    ["Mod+E", "Switch environment"],
    ["Mod+J", "Console"],
    ["Mod+/", "All shortcuts"],
  ];
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 text-sm text-muted-foreground">
      <p>Open a request from the sidebar, or right-click a folder to create one.</p>
      <table>
        <tbody>
          {rows.map(([keys, action]) => (
            <tr key={keys}>
              <td className="py-1 pr-4 text-right">
                <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-xs">
                  {formatKeys(keys)}
                </kbd>
              </td>
              <td className="py-1">{action}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
