import { ask } from "@tauri-apps/plugin-dialog";
import { Columns2, Rows2 } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, events } from "@/bindings";
import { CurlExportDialog } from "@/components/curl/CurlExportDialog";
import { CurlImportDialog } from "@/components/curl/CurlImportDialog";
import { CollectionEditor } from "@/components/doc/CollectionEditor";
import { FolderEditor } from "@/components/doc/FolderEditor";
import { EnvironmentsEditor } from "@/components/env/EnvironmentsEditor";
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

  useRestoreTabs();
  useLoadEnvs();
  usePersistTabs();
  useFileWatcher();
  useShortcuts();

  return (
    <div className="flex h-full flex-col">
      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel defaultSize="22" minSize={180} maxSize="45">
          <Sidebar />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel>
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
                  <ResponseViewer tab={tab} />
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
            {!tab && (
              <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
                Open a request from the sidebar, or right-click it to create one.
              </div>
            )}
          </main>
        </ResizablePanel>
      </ResizablePanelGroup>

      <CurlExportDialog />
      <CurlImportDialog />
      <footer className="flex h-7 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
        <span className="truncate">{collection ? collection.dir : "No collection"}</span>
        <div className="flex-1" />
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

/** Ctrl/Cmd+Enter sends, Ctrl/Cmd+S saves, Ctrl/Cmd+W closes the active tab. */
function useShortcuts() {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const dir = useWorkspace.getState().activeCollectionDir;
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
