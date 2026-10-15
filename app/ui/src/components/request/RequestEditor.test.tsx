import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { EditorView } from "@codemirror/view";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { RequestModel } from "@/lib/model";
import { useTabs } from "@/stores/tabs";
import { RequestEditor } from "./RequestEditor";

const DIR = "/col";

const emptyRequest: RequestModel = {
  version: 1,
  name: "Test",
  seq: 1,
  method: "GET",
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

/** Renders the editor for a tab held in the real store, re-rendering on store changes. */
function Harness() {
  const tab = useTabs((s) => s.byCollection[DIR]?.tabs[0]);
  return tab?.kind === "request" ? <RequestEditor dir={DIR} tab={tab} /> : null;
}

/** The URL bar is a single-line CodeMirror editor; type into it through its API. */
function urlEditor(): EditorView {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const view = dom && EditorView.findFromDOM(dom);
  if (!view) throw new Error("URL editor not found");
  return view;
}
function typeUrl(text: string) {
  const view = urlEditor();
  act(() => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }));
}
function draft() {
  const tab = useTabs.getState().byCollection[DIR]?.tabs[0];
  if (tab?.kind !== "request") throw new Error("no request tab open");
  return tab.draft;
}

describe("RequestEditor params", () => {
  beforeEach(async () => {
    mockIPC((cmd) => (cmd === "read_request" ? emptyRequest : null));
    useTabs.setState({ byCollection: {} });
    await useTabs.getState().openTab(DIR, "test.yaml");
  });
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("fills query and path params from the URL", () => {
    render(<Harness />);
    typeUrl("http://x/orders/:id?expand=items");

    expect(draft().params.query).toEqual([{ name: "expand", value: "items", enabled: true }]);
    expect(draft().params.path).toEqual([{ name: "id", value: "" }]);
    expect(screen.getByDisplayValue("expand")).toBeTruthy();
    expect(screen.getByDisplayValue("id")).toBeTruthy();
  });

  it("writes edited query params back into the URL", () => {
    render(<Harness />);
    typeUrl("http://x/a");
    // The query table's empty "add" row is the first Name/Value pair on the Params tab.
    const [name] = screen.getAllByPlaceholderText("Name");
    if (!name) throw new Error("no name input");
    fireEvent.change(name, { target: { value: "q" } });
    const [value] = screen.getAllByPlaceholderText("Value");
    if (!value) throw new Error("no value input");
    fireEvent.change(value, { target: { value: "1" } });

    expect(draft().url).toBe("http://x/a?q=1");
    expect(urlEditor().state.doc.toString()).toBe("http://x/a?q=1");
  });
});
