import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { FileVersions, GitInfo } from "@/bindings";
import { useGit } from "@/stores/git";
import { ChangesView } from "./ChangesView";

const file = (path: string, staged: string | null, unstaged: string | null) => ({
  path,
  origPath: null,
  staged,
  unstaged,
  untracked: false,
  conflicted: false,
});

const info: GitInfo = {
  kind: "repo",
  status: {
    branch: "main",
    upstream: null,
    ahead: 0,
    behind: 0,
    noCommits: false,
    files: [file("orders/get.yaml", null, "M"), file("logo.png", null, "M")],
  },
};

const versions = (v: Partial<FileVersions>): FileVersions => ({
  old: null,
  new: null,
  oldPath: null,
  binary: false,
  tooLarge: false,
  ...v,
});

// jsdom has no ResizeObserver; the resizable panels need one (they only measure, nothing here
// depends on the sizes).
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

describe("changes tab", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    useGit.setState({ byCollection: {}, selected: {} });
  });

  it("shows the selected file's diff and swaps it when another file is clicked", async () => {
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "git_file_versions") return null;
      asked.push(args);
      const { path } = args as { path: string };
      return path === "logo.png"
        ? versions({ binary: true })
        : versions({ old: "url: /a\nname: Get\n", new: "url: /b\nname: Get\n" });
    });
    useGit.setState({
      byCollection: { "/col": { info, branches: [], busy: null } },
      selected: { "/col": { path: "orders/get.yaml", staged: false } },
    });
    render(<ChangesView dir="/col" />);

    const diff = await screen.findByTestId("diff-view");
    await waitFor(() => expect(diff.textContent).toContain("url: /b"));
    // The unified view also shows the deleted line.
    expect(diff.textContent).toContain("url: /a");
    expect(asked[0]).toEqual({ dir: "/col", path: "orders/get.yaml", staged: false });

    fireEvent.click(screen.getByText("Side by side"));
    await waitFor(() => expect(document.querySelector(".cm-mergeView")).not.toBeNull());

    fireEvent.click(screen.getByText("logo.png"));
    await screen.findByText(/Binary file/);
    expect(useGit.getState().selected["/col"]).toEqual({ path: "logo.png", staged: false });
  });

  it("asks to pick a file when nothing is selected", () => {
    useGit.setState({ byCollection: { "/col": { info, branches: [], busy: null } }, selected: {} });
    render(<ChangesView dir="/col" />);
    expect(screen.getByText("Select a file to see its changes.")).toBeTruthy();
  });
});
