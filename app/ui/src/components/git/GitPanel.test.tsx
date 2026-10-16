import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { GitInfo } from "@/bindings";
import { diffLineKind } from "@/components/editor/diff";
import { useGit } from "@/stores/git";
import { GitPanel } from "./GitPanel";

const info: GitInfo = {
  kind: "repo",
  status: {
    branch: "main",
    upstream: "origin/main",
    ahead: 1,
    behind: 0,
    noCommits: false,
    files: [
      {
        path: "orders/get.yaml",
        origPath: null,
        staged: "M",
        unstaged: null,
        untracked: false,
        conflicted: false,
      },
      {
        path: "new.yaml",
        origPath: null,
        staged: null,
        unstaged: "?",
        untracked: true,
        conflicted: false,
      },
    ],
  },
};

describe("git panel", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    useGit.setState({ byCollection: {} });
  });

  it("lists staged and unstaged changes and stages a file", async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "git_info") return info;
      if (cmd === "git_branches") return [{ name: "main", current: true, remote: false }];
      return null;
    });
    render(<GitPanel dir="/col" />);
    await screen.findByText("Staged changes (1)");
    expect(screen.getByText("Changes (1)")).toBeTruthy();
    expect(screen.getByText("↑1 ↓0")).toBeTruthy();

    fireEvent.click(screen.getByLabelText("Stage new.yaml"));
    await waitFor(() =>
      expect(calls).toContainEqual(["git_stage", { dir: "/col", paths: ["new.yaml"] }]),
    );
  });

  it("colors diff lines", () => {
    expect(diffLineKind("+added")).toBe("added");
    expect(diffLineKind("-removed")).toBe("removed");
    expect(diffLineKind("+++ b/file")).toBe("meta");
    expect(diffLineKind("@@ -1 +1 @@")).toBe("hunk");
    expect(diffLineKind(" context")).toBeNull();
  });
});
