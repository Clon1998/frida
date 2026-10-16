import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { FolderModel } from "@/lib/model";
import { useEnvs } from "./envs";
import {
  type CollectionTab,
  ENVIRONMENTS_TAB,
  type FolderTab,
  isDirty,
  kindOf,
  type RequestTab,
  showsResponse,
  useTabs,
} from "./tabs";

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

describe("unsaved indicators", () => {
  afterEach(() => {
    clearMocks();
    useEnvs.setState({ editorDrafts: {} });
  });

  it("marks the collection tab dirty after an edit", async () => {
    const collection = {
      version: 1,
      id: "0d6c2a4e-8f1b-4c3d-9e5a-7b8c9d0e1f2a",
      name: "API",
      vars: [],
      headers: [],
      auth: { mode: "none" },
      scripts: { pre: "", post: "" },
      tests: "",
      settings: { timeoutMs: 30000, tls: { verify: true, caCertPaths: [] } },
      docs: "",
    };
    mockIPC((cmd) => (cmd === "read_collection" ? collection : null));
    useTabs.setState({ byCollection: {} });
    await useTabs.getState().openTab("/col", "");
    const tab = () => useTabs.getState().byCollection["/col"]?.tabs[0] as CollectionTab;
    expect(isDirty(tab())).toBe(false);
    useTabs.getState().updateDraft<CollectionTab>("/col", "", (c) => ({ ...c, docs: "x" }));
    expect(isDirty(tab())).toBe(true);
  });

  it("marks the environments tab dirty while the editor has unsaved edits", () => {
    const envTab = { kind: "environments", path: ENVIRONMENTS_TAB } as const;
    expect(isDirty(envTab)).toBe(false);
    useEnvs.getState().setEditorDraft("k", {
      draft: { version: 1, name: "dev", color: null, variables: [] },
      pending: {},
    });
    expect(isDirty(envTab)).toBe(true);
    useTabs.setState({
      byCollection: {
        "/col": { tabs: [envTab], active: envTab.path, expanded: [], preview: null },
      },
    });
    useTabs.getState().closeTabs("/col", [ENVIRONMENTS_TAB]);
    expect(isDirty(envTab)).toBe(false);
  });
});

describe("response pane", () => {
  const request = {
    version: 1,
    name: "Get",
    seq: 1,
    method: "GET",
    url: "http://x",
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
  const response = {
    id: "r",
    status: 200,
    statusText: "OK",
    headers: [],
    contentType: null,
    sizeBytes: 0,
    timeMs: 1,
    finalUrl: "http://x",
    bodyText: "",
    bodyBase64: null,
    truncated: false,
    scripts: { tests: [], errors: [] },
  };
  const tab = () => useTabs.getState().byCollection["/col"]?.tabs[0] as RequestTab;

  beforeEach(async () => {
    useTabs.setState({ byCollection: {} });
    mockIPC((cmd) => {
      if (cmd === "read_request") return request;
      if (cmd === "send_request") return response;
      return null;
    });
    await useTabs.getState().openTab("/col", "get.yaml");
  });
  afterEach(() => clearMocks());

  it("is hidden until the first send, then shown", async () => {
    expect(showsResponse(tab())).toBe(false);
    await useTabs.getState().send("/col", "get.yaml");
    expect(tab().response?.status).toBe(200);
    expect(showsResponse(tab())).toBe(true);
  });

  it("stays closed when closed, and the next send opens it again", async () => {
    await useTabs.getState().send("/col", "get.yaml");
    useTabs.getState().setResponseHidden("/col", "get.yaml", true);
    expect(showsResponse(tab())).toBe(false);

    await useTabs.getState().send("/col", "get.yaml");
    expect(showsResponse(tab())).toBe(true);
  });
});
