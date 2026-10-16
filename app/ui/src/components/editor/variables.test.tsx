import { CompletionContext, startCompletion } from "@codemirror/autocomplete";
import { EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { AvailableVar } from "@/bindings";
import { CodeEditor } from "./CodeEditor";
import {
  setVariables,
  type VariableContext,
  VariablesContext,
  variableCompletions,
  variablesExtension,
  variableState,
} from "./variables";

const available = (name: string, extra: Partial<AvailableVar> = {}): AvailableVar => ({
  name,
  source: { kind: "collectionEnv", name: "dev" },
  value: "x",
  isSecret: false,
  hasValue: true,
  ...extra,
});

const ctx: VariableContext = {
  used: new Map(),
  available: new Map([
    ["baseUrl", available("baseUrl")],
    ["token", available("token", { isSecret: true, value: null })],
    ["unsetSecret", available("unsetSecret", { isSecret: true, value: null, hasValue: false })],
    ["$uuid", available("$uuid", { source: { kind: "dynamic" }, value: null })],
  ]),
  pathParams: new Map([
    ["id", "42"],
    ["empty", ""],
  ]),
};

describe("variable states", () => {
  it("is green with a value and red without one", () => {
    expect(variableState(ctx, "baseUrl")).toBe("resolved");
    expect(variableState(ctx, "token")).toBe("secret");
    expect(variableState(ctx, "unsetSecret")).toBe("unresolved");
    expect(variableState(ctx, "nope")).toBe("unresolved");
    expect(variableState(ctx, "process.env.HOME")).toBe("neutral");
    expect(variableState(null, "baseUrl")).toBe("neutral");
  });
});

describe("autocompletion", () => {
  const complete = (doc: string) => {
    const state = EditorState.create({ doc, extensions: variablesExtension() });
    const withVars = state.update({ effects: setVariables.of(ctx) }).state;
    return variableCompletions(new CompletionContext(withVars, doc.length, true));
  };

  it("offers every variable after {{ and closes the braces", () => {
    const result = complete("{{base");
    expect(result?.from).toBe(2);
    const base = result?.options.find((o) => o.label === "baseUrl");
    expect(base?.apply).toBe("baseUrl}}");
    expect(result?.options.map((o) => o.label)).toContain("$uuid");
    expect(result?.options.map((o) => o.label)).toContain("process.env.");
  });

  it("stays quiet outside of {{", () => {
    expect(complete("base")).toBeNull();
  });
});

describe("highlighting in the editor", () => {
  afterEach(cleanup);

  it("colors variables and path params from the context", () => {
    render(
      <VariablesContext.Provider value={ctx}>
        <CodeEditor singleLine pathParams value="{{baseUrl}}/orders/:id/:empty?x={{nope}}" />
      </VariablesContext.Provider>,
    );
    const resolved = [...document.querySelectorAll(".cm-var-resolved")].map((e) => e.textContent);
    const missing = [...document.querySelectorAll(".cm-var-unresolved")].map((e) => e.textContent);
    expect(resolved).toEqual(["{{baseUrl}}", ":id"]);
    expect(missing).toEqual([":empty", "{{nope}}"]);
  });
});

describe("autocompletion popup", () => {
  afterEach(cleanup);

  it("renders outside of clipping containers", async () => {
    const { container } = render(
      <div style={{ overflow: "hidden", height: 32 }}>
        <VariablesContext.Provider value={ctx}>
          <CodeEditor singleLine compact value="" />
        </VariablesContext.Provider>
      </div>,
    );
    const view = EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement);
    if (!view) throw new Error("no editor");
    view.dispatch({ changes: { from: 0, insert: "{{ba" }, selection: { anchor: 4 } });
    startCompletion(view);
    await waitFor(() => expect(document.querySelector(".cm-tooltip-autocomplete")).not.toBeNull());
    const popup = document.querySelector(".cm-tooltip-autocomplete") as HTMLElement;
    // Not inside the clipped container, and it lists the variable.
    expect(container.contains(popup)).toBe(false);
    expect(popup.textContent).toContain("baseUrl");
  });
});
