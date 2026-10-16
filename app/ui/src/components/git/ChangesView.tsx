import { FileWarning, Loader2, Minus, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type FileChange, type FileVersions } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { toApiError, unwrap } from "@/lib/api";
import { useGit } from "@/stores/git";
import { useSubTab } from "@/stores/subTabs";
import { type DiffMode, DiffView } from "./DiffView";
import { FileSection } from "./GitPanel";

type Loaded =
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "ready"; versions: FileVersions };

/** One file's change: what a loaded diff belongs to. */
const changeKey = (c: { path: string; staged: boolean }) => `${c.staged ? "S" : "W"}:${c.path}`;

/**
 * The changes tab (PLAN.md M12): the collection's changed files on the left, the selected
 * file's diff on the right. Clicking another file only swaps the diff. Stage/unstage work from
 * here too; branch, commit and push stay in the sidebar's git panel.
 */
export function ChangesView({ dir }: { dir: string }) {
  const info = useGit((s) => s.byCollection[dir]?.info ?? null);
  const selected = useGit((s) => s.selected[dir] ?? null);
  const select = useGit((s) => s.select);
  const run = useGit((s) => s.run);
  const refresh = useGit((s) => s.refresh);
  const [mode, setMode] = useSubTab("git-diff-mode", "unified");
  // The last result and the change it belongs to; another file shows "loading" until its own
  // result arrives, while a reload of the same file keeps showing the old diff.
  const [result, setResult] = useState<{ key: string; loaded: Loaded } | null>(null);

  useEffect(() => {
    if (!info) void refresh(dir);
  }, [dir, info, refresh]);

  const status = info?.kind === "repo" ? info.status : null;
  const staged = status?.files.filter((f) => f.staged !== null) ?? [];
  const unstaged = status?.files.filter((f) => f.unstaged !== null) ?? [];
  // The selected change is gone (committed, staged, reverted outside Frida): show nothing.
  const current =
    selected && (selected.staged ? staged : unstaged).some((f) => f.path === selected.path)
      ? selected
      : null;

  const key = current ? changeKey(current) : null;
  const loaded: Loaded | null =
    key === null ? null : result?.key === key ? result.loaded : { state: "loading" };

  // Reload the diff when the selection or the status changes (e.g. the file was edited).
  useEffect(() => {
    if (!current) return;
    let cancelled = false;
    const key = changeKey(current);
    unwrap(commands.gitFileVersions(dir, current.path, current.staged)).then(
      (versions) => !cancelled && setResult({ key, loaded: { state: "ready", versions } }),
      (e) =>
        !cancelled &&
        setResult({ key, loaded: { state: "error", message: toApiError(e).message } }),
    );
    return () => {
      cancelled = true;
    };
  }, [dir, current?.path, current?.staged, status]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!info) {
    return (
      <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" /> Reading git status…
      </div>
    );
  }
  if (!status) {
    return (
      <p className="p-4 text-sm text-muted-foreground">
        This collection isn&apos;t in a git repository (see the git panel in the sidebar).
      </p>
    );
  }

  const open = (f: FileChange, isStaged: boolean) =>
    select(dir, { path: f.path, staged: isStaged });

  return (
    <ResizablePanelGroup orientation="horizontal" className="h-full">
      <ResizablePanel id="files" defaultSize="28" minSize={180}>
        <div className="h-full overflow-auto py-1 text-sm">
          <FileSection
            title="Staged changes"
            files={staged}
            letter={(f) => f.staged ?? ""}
            action={{ label: "Unstage", icon: <Minus /> }}
            onAction={(paths) => void run(dir, { kind: "unstage", paths })}
            onOpen={(f) => open(f, true)}
            selected={current?.staged ? current.path : null}
          />
          <FileSection
            title="Changes"
            files={unstaged}
            letter={(f) => (f.untracked ? "U" : (f.unstaged ?? ""))}
            action={{ label: "Stage", icon: <Plus /> }}
            onAction={(paths) => void run(dir, { kind: "stage", paths })}
            onOpen={(f) => open(f, false)}
            selected={current && !current.staged ? current.path : null}
          />
          {status.files.length === 0 && (
            <p className="px-3 py-2 text-xs text-muted-foreground">No changes.</p>
          )}
        </div>
      </ResizablePanel>
      <ResizableHandle />
      <ResizablePanel id="diff" minSize={240}>
        {current ? (
          <div className="flex h-full flex-col">
            <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs">
              <span className="min-w-0 truncate font-mono font-medium" title={current.path}>
                {loaded?.state === "ready" && loaded.versions.oldPath
                  ? `${loaded.versions.oldPath} → ${current.path}`
                  : current.path}
              </span>
              <span className="text-muted-foreground">
                {current.staged ? "staged" : "not staged"}
              </span>
              <div className="ml-auto flex">
                <Button
                  size="sm"
                  variant={mode === "unified" ? "secondary" : "ghost"}
                  className="h-6 text-xs"
                  onClick={() => setMode("unified")}
                >
                  Unified
                </Button>
                <Button
                  size="sm"
                  variant={mode === "split" ? "secondary" : "ghost"}
                  className="h-6 text-xs"
                  onClick={() => setMode("split")}
                >
                  Side by side
                </Button>
              </div>
            </div>
            <div className="min-h-0 flex-1">
              <DiffBody path={current.path} loaded={loaded} mode={mode as DiffMode} />
            </div>
          </div>
        ) : (
          <p className="p-4 text-sm text-muted-foreground">
            {status.files.length > 0 ? "Select a file to see its changes." : "Nothing changed."}
          </p>
        )}
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}

function DiffBody({ path, loaded, mode }: { path: string; loaded: Loaded | null; mode: DiffMode }) {
  if (!loaded || loaded.state === "loading") {
    return (
      <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" /> Loading diff…
      </div>
    );
  }
  if (loaded.state === "error") {
    return <p className="p-4 text-sm text-destructive">{loaded.message}</p>;
  }
  const v = loaded.versions;
  if (v.binary || v.tooLarge) {
    return (
      <p className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
        <FileWarning className="size-4" />
        {v.binary ? "Binary file: no text diff." : "The file is larger than 2 MB: no diff shown."}
      </p>
    );
  }
  return <DiffView path={path} old={v.old} new={v.new} mode={mode} />;
}
