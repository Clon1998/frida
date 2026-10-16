import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ResponseView } from "@/bindings";
import { ResponsePanel } from "./ResponseViewer";

const response: ResponseView = {
  id: "r1",
  status: 200,
  statusText: "OK",
  headers: [],
  contentType: "application/json",
  sizeBytes: 30,
  timeMs: 5,
  finalUrl: "http://x",
  bodyText: '{"items":[{"id":1},{"id":2}]}',
  bodyBase64: null,
  truncated: false,
  scripts: { tests: [], errors: [] },
};

const editorText = () => document.querySelector(".cm-content")?.textContent ?? "";

describe("JSONPath filter", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("shows the filtered result, and errors under the field", async () => {
    const calls: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "filter_response") return null;
      calls.push(args);
      const { path } = args as { path: string };
      if (path === "$.items[*].id") return "[\n  1,\n  2\n]";
      throw { code: "invalid_input", message: "invalid JSONPath: unexpected end", hint: null };
    });
    render(<ResponsePanel response={response} viewKey="test" />);
    expect(editorText()).toContain('"items"');
    // The field is folded away until "Filter" is clicked.
    expect(screen.queryByLabelText("JSONPath filter")).toBeNull();
    fireEvent.click(screen.getByLabelText("Filter with JSONPath"));

    fireEvent.change(screen.getByLabelText("JSONPath filter"), {
      target: { value: "$.items[*].id" },
    });
    await waitFor(() => expect(editorText()).toBe("[  1,  2]"));
    // Live responses are filtered from Rust's cache: the body isn't sent along.
    expect(calls[0]).toMatchObject({ responseId: "r1", bodyText: null });

    fireEvent.change(screen.getByLabelText("JSONPath filter"), { target: { value: "$.items[" } });
    await waitFor(() => expect(screen.getByText(/invalid JSONPath/)).toBeTruthy());
    // The last good result stays while the filter is invalid.
    expect(editorText()).toBe("[  1,  2]");

    fireEvent.change(screen.getByLabelText("JSONPath filter"), { target: { value: "" } });
    await waitFor(() => expect(editorText()).toContain('"items"'));
  });
});
