import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { TreeNode } from "@/bindings";
import { useTabs } from "@/stores/tabs";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { QuickOpen } from "./QuickOpen";

const tree: TreeNode[] = [
  {
    kind: "folder",
    path: "orders",
    name: "Orders",
    seq: 1,
    children: [
      { kind: "request", path: "orders/list.yaml", name: "List orders", seq: 1, method: "GET" },
      { kind: "request", path: "orders/create.yaml", name: "Create order", seq: 2, method: "POST" },
    ],
  },
  { kind: "request", path: "login.yaml", name: "Login", seq: 2, method: "POST" },
];

describe("quick open", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    useUi.setState({ quickOpen: false });
  });

  it("finds a request by a fuzzy query and opens it", async () => {
    const opened: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "read_request") {
        opened.push((args as { path: string }).path);
        return {
          version: 1,
          name: "Create order",
          seq: 2,
          method: "POST",
          url: "",
          params: { query: [], path: [] },
          headers: [],
          body: {
            mode: "none",
            json: "",
            xml: "",
            text: "",
            formUrlEncoded: [],
            multipart: [],
            binaryPath: "",
          },
          auth: { mode: "inherit" },
          vars: [],
          scripts: { pre: "", post: "" },
          tests: "",
          settings: { timeoutMs: null },
          docs: "",
        };
      }
      return null;
    });
    useWorkspace.setState({
      workspace: {
        dir: "/ws",
        name: "WS",
        collections: [{ dir: "/col", name: "Shop", id: "x", error: null }],
      },
      activeCollectionDir: "/col",
      trees: { "/col": tree },
    } as never);
    useTabs.setState({ byCollection: {} });
    useUi.setState({ quickOpen: true });

    render(<QuickOpen />);
    const input = screen.getByPlaceholderText(/Search requests in Shop/);
    fireEvent.change(input, { target: { value: "cror" } });
    expect(screen.getAllByRole("option")[0]?.textContent).toContain("Create order");
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(opened).toEqual(["orders/create.yaml"]));
    expect(useUi.getState().quickOpen).toBe(false);
  });
});
