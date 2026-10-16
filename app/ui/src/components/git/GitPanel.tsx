import {
  ArrowDown,
  ArrowUp,
  Check,
  GitBranch,
  Loader2,
  Minus,
  Plus,
  GitCompare,
  RefreshCw,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { FileChange, Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Textarea } from "@/components/ui/textarea";
import { NameDialog } from "@/components/workspace/NameDialog";
import { openChanges } from "@/stores/actions";
import { useGit } from "@/stores/git";
import { cn } from "cn";

/**
 * The git panel of a collection (PLAN.md §12): branch, fetch/pull/push, staged and unstaged
 * changes with diffs, and committing. Only non-destructive git commands are available.
 */
export function GitPanel({ dir }: { dir: string }) {
  const git = useGit((s) => s.byCollection[dir]);
  const refresh = useGit((s) => s.refresh);
  const run = useGit((s) => s.run);

  useEffect(() => {
    void refresh(dir);
  }, [dir, refresh]);

  const info = git?.info ?? null;
  if (!info) {
    return (
      <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" /> Reading git status…
      </div>
    );
  }
  if (info.kind === "noGit") {
    return (
      <p className="p-4 text-sm text-muted-foreground">
        Git isn&apos;t installed (or not on the PATH). Install it from git-scm.com and restart
        Frida.
      </p>
    );
  }
  if (info.kind === "noRepo") {
    return (
      <div className="flex flex-col items-start gap-2 p-4 text-sm">
        <p className="text-muted-foreground">This collection isn&apos;t in a git repository.</p>
        <Button size="sm" onClick={() => void run(dir, { kind: "init" })}>
          Initialize repository
        </Button>
      </div>
    );
  }
  return <RepoView dir={dir} status={info.status} busy={git?.busy ?? null} />;
}

function RepoView({ dir, status, busy }: { dir: string; status: Status; busy: string | null }) {
  const run = useGit((s) => s.run);
  const branches = useGit((s) => s.byCollection[dir]?.branches ?? []);
  const [message, setMessage] = useState("");
  const [creatingBranch, setCreatingBranch] = useState(false);

  const staged = status.files.filter((f) => f.staged !== null);
  const unstaged = status.files.filter((f) => f.unstaged !== null);

  const selected = useGit((s) => s.selected[dir] ?? null);
  const showDiff = (file: FileChange, isStaged: boolean) =>
    openChanges(dir, { path: file.path, staged: isStaged });

  const commit = async () => {
    if (await run(dir, { kind: "commit", message })) setMessage("");
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col text-sm">
      <div className="flex items-center gap-1 border-b px-2 py-1.5">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="h-7 min-w-0 gap-1 px-1.5">
              <GitBranch className="size-3.5" />
              <span className="truncate">{status.branch ?? "detached HEAD"}</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-80 w-64 overflow-auto">
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              Switch branch
            </DropdownMenuLabel>
            {branches.map((b) => (
              <DropdownMenuItem
                key={b.name}
                disabled={b.current}
                onSelect={() => void run(dir, { kind: "switch", branch: b.name })}
              >
                {b.current ? <Check /> : <span className="size-4" />}
                <span className="truncate">{b.name}</span>
                {b.remote && <span className="ml-auto text-xs text-muted-foreground">remote</span>}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setCreatingBranch(true)}>
              <Plus /> New branch…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <div className="flex-1" />
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title="Open the changes tab"
          aria-label="Open changes"
          onClick={() => openChanges(dir)}
        >
          <GitCompare />
        </Button>
        <span
          className="text-xs text-muted-foreground tabular-nums"
          title={status.upstream ? `Compared with ${status.upstream} (as of the last fetch)` : ""}
        >
          {status.upstream ? `↑${status.ahead} ↓${status.behind}` : "no upstream"}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title="Fetch"
          aria-label="Fetch"
          disabled={busy !== null}
          onClick={() => void run(dir, { kind: "fetch" })}
        >
          <RefreshCw className={cn(busy === "fetch" && "animate-spin")} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title="Pull (fast-forward only)"
          aria-label="Pull"
          disabled={busy !== null || !status.upstream}
          onClick={() => void run(dir, { kind: "pull" })}
        >
          {busy === "pull" ? <Loader2 className="animate-spin" /> : <ArrowDown />}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          title="Push"
          aria-label="Push"
          disabled={busy !== null || status.noCommits}
          onClick={() => void run(dir, { kind: "push" })}
        >
          {busy === "push" ? <Loader2 className="animate-spin" /> : <ArrowUp />}
        </Button>
      </div>

      <div className="flex flex-col gap-2 border-b p-2">
        <Textarea
          className="min-h-16 text-xs"
          placeholder="Commit message (Ctrl/Cmd+Enter to commit)"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void commit();
            }
          }}
        />
        <Button
          size="sm"
          disabled={busy !== null || staged.length === 0 || message.trim() === ""}
          onClick={() => void commit()}
        >
          {busy === "commit" ? <Loader2 className="animate-spin" /> : <Check />}
          Commit {staged.length > 0 && `(${staged.length})`}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto py-1">
        <FileSection
          title="Staged changes"
          files={staged}
          letter={(f) => f.staged ?? ""}
          action={{ label: "Unstage", icon: <Minus /> }}
          onAction={(paths) => void run(dir, { kind: "unstage", paths })}
          onOpen={(f) => showDiff(f, true)}
          selected={selected?.staged ? selected.path : null}
        />
        <FileSection
          title="Changes"
          files={unstaged}
          letter={(f) => (f.untracked ? "U" : (f.unstaged ?? ""))}
          action={{ label: "Stage", icon: <Plus /> }}
          onAction={(paths) => void run(dir, { kind: "stage", paths })}
          onOpen={(f) => showDiff(f, false)}
          selected={selected && !selected.staged ? selected.path : null}
        />
        {status.files.length === 0 && (
          <p className="px-3 py-2 text-xs text-muted-foreground">No changes.</p>
        )}
      </div>

      <NameDialog
        open={creatingBranch}
        title="New branch"
        label="Branch name"
        confirmLabel="Create and switch"
        onOpenChange={setCreatingBranch}
        onConfirm={(name) => void run(dir, { kind: "createBranch", name })}
      />
    </div>
  );
}

const LETTER_COLORS: Record<string, string> = {
  M: "text-warning",
  A: "text-success",
  U: "text-success",
  D: "text-destructive",
  R: "text-info",
};

function FileSection({
  title,
  files,
  letter,
  action,
  onAction,
  onOpen,
  selected = null,
}: {
  title: string;
  files: FileChange[];
  letter: (f: FileChange) => string;
  action: { label: string; icon: React.ReactNode };
  /** Called with the files to act on; `[]` means all. */
  onAction: (paths: string[]) => void;
  onOpen: (f: FileChange) => void;
  /** The path shown in the changes tab, highlighted. */
  selected?: string | null;
}) {
  if (files.length === 0) return null;
  return (
    <section className="mb-2">
      <div className="group flex items-center px-3 py-1 text-xs font-medium text-muted-foreground">
        <span>
          {title} ({files.length})
        </span>
        <div className="flex-1" />
        <Button
          variant="ghost"
          size="sm"
          className="h-5 px-1 text-xs"
          onClick={() => onAction([])}
          title={`${action.label} all`}
        >
          {action.label} all
        </Button>
      </div>
      {files.map((f) => {
        const l = letter(f);
        const slash = f.path.lastIndexOf("/");
        return (
          <div
            key={f.path}
            className={cn(
              "group flex cursor-default items-center gap-2 px-3 py-0.5 hover:bg-accent",
              f.path === selected && "bg-accent",
            )}
            aria-selected={f.path === selected}
            onClick={() => onOpen(f)}
            title={f.origPath ? `${f.origPath} → ${f.path}` : f.path}
          >
            <span className="min-w-0 flex-1 truncate">
              {f.path.slice(slash + 1)}
              {slash > 0 && (
                <span className="ml-1.5 text-xs text-muted-foreground">
                  {f.path.slice(0, slash)}
                </span>
              )}
            </span>
            <Button
              variant="ghost"
              size="icon"
              className="size-5 opacity-0 group-hover:opacity-100"
              aria-label={`${action.label} ${f.path}`}
              title={action.label}
              onClick={(e) => {
                e.stopPropagation();
                onAction([f.path]);
              }}
            >
              {action.icon}
            </Button>
            <span
              className={cn(
                "w-3 shrink-0 text-center font-mono text-xs font-semibold",
                f.conflicted ? "text-destructive" : (LETTER_COLORS[l] ?? "text-muted-foreground"),
              )}
            >
              {f.conflicted ? "!" : l}
            </span>
          </div>
        );
      })}
    </section>
  );
}
