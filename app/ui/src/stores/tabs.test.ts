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
