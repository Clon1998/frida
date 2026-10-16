import { GitBranch } from "lucide-react";
import { useEffect } from "react";
import { events } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useGit, useGitInfo } from "@/stores/git";
import { useUi } from "@/stores/ui";

/**
 * The status bar's git indicator for the active collection: branch, number of changed files and
 * ahead/behind. Kept live by refreshing on file changes and when the window gets focus (the
 * file watcher ignores `.git`, so commits made in a terminal show up on focus). Clicking it
 * opens the git panel.
 */
export function GitStatusIndicator({ dir }: { dir: string }) {
  const info = useGitInfo(dir);
  const refresh = useGit((s) => s.refresh);
  const setSidebarView = useUi((s) => s.setSidebarView);

  useEffect(() => {
    void refresh(dir);
    let timer: ReturnType<typeof setTimeout> | undefined;
    const later = () => {
      clearTimeout(timer);
      timer = setTimeout(() => void refresh(dir), 300);
    };
    const unlisten = events.fsChanged.listen(({ payload }) => {
      if (payload.paths.some((p) => p.startsWith(dir))) later();
    });
    window.addEventListener("focus", later);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("focus", later);
      void unlisten.then((stop) => stop());
    };
  }, [dir, refresh]);

  if (!info || info.kind !== "repo") return null;
  const { branch, ahead, behind, files, upstream } = info.status;
  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-5 gap-1 px-1.5 text-xs"
      title="Open the git panel"
      onClick={() => setSidebarView("git")}
    >
      <GitBranch className="size-3.5" />
      {branch ?? "detached"}
      {files.length > 0 && <span>· {files.length} changed</span>}
      {upstream && (ahead > 0 || behind > 0) && (
        <span className="tabular-nums">
          · ↑{ahead} ↓{behind}
        </span>
      )}
    </Button>
  );
}
