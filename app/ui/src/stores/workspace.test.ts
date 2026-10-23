import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceOverview } from "@/bindings";

const overview: WorkspaceOverview = {
  dir: "/ws",
  name: "Workspace",
  collections: [
    { dir: "/ws/collections/a", id: "a", name: "A", error: null },
    { dir: "/ws/collections/b", id: "b", name: "B", error: null },
  ],
};

describe("workspace store start", () => {
  let lastOpen: string | null;
  let folderExists: boolean;
  const calls: string[] = [];

  beforeEach(() => {
    // `start` runs once per module; a fresh module per test.
    vi.resetModules();
    lastOpen = "/ws";
    folderExists = true;
    calls.length = 0;
    mockIPC((cmd) => {
      calls.push(cmd);
      switch (cmd) {
        case "get_settings":
          return {
            lastOpenWorkspace: lastOpen,
            activeCollections: { "/ws": "/ws/collections/b" },
          };
        case "open_workspace":
          if (!folderExists) throw { code: "not_found", message: "/ws was not found", hint: null };
          return overview;
        case "close_workspace":
          lastOpen = null;
          return null;
        case "get_tree":
          return [];
      }
    });
  });
  afterEach(() => clearMocks());

  const load = async () => (await import("./workspace")).useWorkspace;

  it("reopens the last workspace with its last collection", async () => {
    const store = await load();
    const reopening: boolean[] = [];
    store.subscribe((s, prev) => {
      if (s.reopening !== prev.reopening) reopening.push(s.reopening);
    });
    await store.getState().start();
    // The start screen was held back while the workspace opened.
    expect(reopening).toEqual([true, false]);
    expect(store.getState().workspace?.dir).toBe("/ws");
    expect(store.getState().activeCollectionDir).toBe("/ws/collections/b");
  });

  it("opens it only once, even if start runs twice", async () => {
    const store = await load();
    await Promise.all([store.getState().start(), store.getState().start()]);
    expect(calls.filter((c) => c === "open_workspace")).toHaveLength(1);
  });

  it("shows the start screen when nothing was open", async () => {
    lastOpen = null;
    const store = await load();
    await store.getState().start();
    expect(store.getState().workspace).toBeNull();
    expect(calls).not.toContain("open_workspace");
  });

  it("forgets a workspace that can no longer be opened", async () => {
    folderExists = false;
    const store = await load();
    await store.getState().start();
    expect(store.getState().workspace).toBeNull();
    expect(store.getState().reopening).toBe(false);
    expect(lastOpen).toBeNull();
  });
});
