import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { FolderModel } from "@/lib/model";
import { type FolderTab, isDirty, kindOf, useTabs } from "./tabs";

const folder: FolderModel = {
  version: 1,
  name: "Orders",
  seq: 1,
  vars: [],
  headers: [],
  auth: { mode: "inherit" },
  scripts: { pre: "", post: "" },
  tests: "",
  settings: { timeoutMs: null },
  docs: "",
};

describe("tabs store", () => {
  let saved: unknown = null;

  beforeEach(() => {
    saved = null;
    useTabs.setState({ byCollection: {} });
    mockIPC((cmd, args) => {
      if (cmd === "read_folder") return saved ?? folder;
      if (cmd === "save_folder") {
        saved = (args as { folder: unknown }).folder;
        return null;
      }
      return null;
    });
  });
  afterEach(() => clearMocks());

  it("derives the tab kind from the path", () => {
    expect(kindOf("")).toBe("collection");
    expect(kindOf("orders/get.yaml")).toBe("request");
    expect(kindOf("orders")).toBe("folder");
  });

  it("opens, edits and saves a folder tab through the folder commands", async () => {
    const tabs = useTabs.getState();
    await tabs.openTab("/col", "orders");
    const tab = () => useTabs.getState().byCollection["/col"]?.tabs[0] as FolderTab;
    expect(tab().kind).toBe("folder");

    useTabs.getState().updateDraft<FolderTab>("/col", "orders", (f) => ({
      ...f,
      headers: [{ name: "X-Team", value: "orders", enabled: true }],
    }));
    expect(isDirty(tab())).toBe(true);

    await useTabs.getState().save("/col", "orders");
    expect((saved as FolderModel).headers[0]?.name).toBe("X-Team");
    expect(isDirty(tab())).toBe(false);
  });
});

describe("closing several tabs", () => {
  beforeEach(() => {
    const tab = (path: string) => ({ kind: "environments" as const, path }) as never;
    useTabs.setState({
      byCollection: {
        "/col": { tabs: ["a", "b", "c", "d"].map(tab), active: "c", expanded: [] },
      },
    });
  });
  const state = () => useTabs.getState().byCollection["/col"];

  it("keeps the active tab when it survives", () => {
    useTabs.getState().closeTabs("/col", ["a", "d"]);
    expect(state()?.tabs.map((t) => t.path)).toEqual(["b", "c"]);
    expect(state()?.active).toBe("c");
  });

  it("moves to the nearest remaining tab when the active one closes", () => {
    useTabs.getState().closeTabs("/col", ["c", "d"]);
    expect(state()?.active).toBe("b");
    useTabs.getState().closeTabs("/col", ["a", "b"]);
    expect(state()?.active).toBeNull();
  });
});

describe("preview tabs", () => {
  beforeEach(() => {
    useTabs.setState({ byCollection: {} });
    // Every folder tab reads the same folder document; good enough for tab bookkeeping.
    mockIPC((cmd) => (cmd === "read_folder" ? folder : null));
  });
  afterEach(() => clearMocks());
  const state = () => useTabs.getState().byCollection["/col"];
  const paths = () => state()?.tabs.map((t) => t.path);

  it("replaces the preview tab on the next single click", async () => {
    await useTabs.getState().openTab("/col", "a", { preview: true });
    await useTabs.getState().openTab("/col", "b", { preview: true });
    expect(paths()).toEqual(["b"]);
    expect(state()?.preview).toBe("b");
  });

  it("keeps pinned tabs and pins on double click", async () => {
    await useTabs.getState().openTab("/col", "a");
    await useTabs.getState().openTab("/col", "b", { preview: true });
    await useTabs.getState().openTab("/col", "b"); // double click
    await useTabs.getState().openTab("/col", "c", { preview: true });
    expect(paths()).toEqual(["a", "b", "c"]);
  });

  it("closes an untouched preview when another tab is activated", async () => {
    await useTabs.getState().openTab("/col", "a");
    await useTabs.getState().openTab("/col", "b", { preview: true });
    useTabs.getState().setActive("/col", "a");
    expect(paths()).toEqual(["a"]);
  });

  it("editing a preview tab pins it", async () => {
    await useTabs.getState().openTab("/col", "a");
    await useTabs.getState().openTab("/col", "b", { preview: true });
    useTabs.getState().updateDraft<FolderTab>("/col", "b", (f) => ({ ...f, docs: "edited" }));
    useTabs.getState().setActive("/col", "a");
    expect(paths()).toEqual(["a", "b"]);
  });
});

describe("preview tabs and no-op updates", () => {
  beforeEach(() => {
    useTabs.setState({ byCollection: {} });
    mockIPC((cmd) => (cmd === "read_folder" ? folder : null));
  });
  afterEach(() => clearMocks());

  it("an update that changes nothing does not pin a preview tab", async () => {
    await useTabs.getState().openTab("/col", "a", { preview: true });
    useTabs.getState().updateDraft<FolderTab>("/col", "a", (f) => ({ ...f }));
    expect(useTabs.getState().byCollection["/col"]?.preview).toBe("a");
    await useTabs.getState().openTab("/col", "b", { preview: true });
    expect(useTabs.getState().byCollection["/col"]?.tabs.map((t) => t.path)).toEqual(["b"]);
  });
});
