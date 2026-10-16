import { ChevronDown, ChevronUp, FileWarning, Loader2, Minus, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type FileVersions } from "@/bindings";
import { Button } from "@/components/ui/button";
import { toApiError, unwrap } from "@/lib/api";
import { useGit } from "@/stores/git";
import { useSubTab } from "@/stores/subTabs";
import { type DiffMode, DiffView } from "./DiffView";

type Loaded =
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "ready"; versions: FileVersions };

/** One file's change: what a loaded diff belongs to. */
const changeKey = (c: { path: string; staged: boolean }) => `${c.staged ? "S" : "W"}:${c.path}`;

/**
 * The changes tab (PLAN.md M12): the diff of the file selected in the sidebar's git panel
 * (the one file list). Clicking another file there only swaps the diff. The header steps
 * through the changed files and stages or unstages the shown one.
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

  // Every change in the order of the git panel: staged first, then the rest.
  const changes = [
    ...staged.map((f) => ({ path: f.path, staged: true })),
    ...unstaged.map((f) => ({ path: f.path, staged: false })),
  ];
  const index = current
    ? changes.findIndex((c) => c.path === current.path && c.staged === current.staged)
    : -1;
  const go = (offset: number) => {
    const next = changes[index + offset];
    if (next) select(dir, next);
  };
  /** Stage or unstage the shown file, then keep showing it on its new side. */
  const toggleStaged = async () => {
    if (!current) return;
    const kind = current.staged ? "unstage" : "stage";
    if (await run(dir, { kind, paths: [current.path] }))
      select(dir, { path: current.path, staged: !current.staged });
  };

  if (!current) {
    return (
      <p className="p-4 text-sm text-muted-foreground">
        {status.files.length > 0
          ? "Select a file in the git panel to see its changes."
          : "Nothing changed."}
      </p>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs">
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          title="Previous file"
          aria-label="Previous file"
          disabled={index <= 0}
          onClick={() => go(-1)}
        >
          <ChevronUp />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          title="Next file"
          aria-label="Next file"
          disabled={index < 0 || index >= changes.length - 1}
          onClick={() => go(1)}
        >
          <ChevronDown />
        </Button>
        <span className="text-muted-foreground tabular-nums">
          {index + 1}/{changes.length}
        </span>
        <span className="min-w-0 truncate font-mono font-medium" title={current.path}>
          {loaded?.state === "ready" && loaded.versions.oldPath
            ? `${loaded.versions.oldPath} → ${current.path}`
            : current.path}
        </span>
        <span className="text-muted-foreground">{current.staged ? "staged" : "not staged"}</span>
        <div className="ml-auto flex items-center gap-1">
          <Button
            size="sm"
            variant="outline"
            className="h-6 text-xs"
            onClick={() => void toggleStaged()}
          >
            {current.staged ? <Minus /> : <Plus />}
            {current.staged ? "Unstage" : "Stage"}
          </Button>
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
