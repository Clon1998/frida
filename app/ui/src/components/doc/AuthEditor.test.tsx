import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Auth } from "@/bindings";
import { type VariableContext, VariablesContext } from "@/components/editor/variables";
import { AuthEditor } from "./AuthEditor";

const ctx: VariableContext = {
  used: new Map(),
  available: new Map([
    [
      "user",
      {
        name: "user",
        source: { kind: "collectionEnv", name: "dev" },
        value: "alice",
        isSecret: false,
        hasValue: true,
      },
    ],
  ]),
  pathParams: new Map(),
};

const basic = (username: string, password: string): Auth => ({
  mode: "custom",
  schemes: [{ type: "basic", username, password }],
});

const renderEditor = (auth: Auth) =>
  render(
    <VariablesContext.Provider value={ctx}>
      <AuthEditor
        auth={auth}
        onChange={() => {}}
        canInherit
        context={{ dir: "/c", path: "", request: null }}
      />
    </VariablesContext.Provider>,
  );

describe("auth fields", () => {
  afterEach(cleanup);

  it("highlight variables like the other editors", () => {
    renderEditor(basic("{{user}}", "{{missing}}"));
    const resolved = [...document.querySelectorAll(".cm-var-resolved")].map((e) => e.textContent);
    const missing = [...document.querySelectorAll(".cm-var-unresolved")].map((e) => e.textContent);
    expect(resolved).toEqual(["{{user}}"]);
    expect(missing).toEqual(["{{missing}}"]);
  });

  // Regression: fields were plain inputs until the variables arrived, then switched to editors.
  it("are editors right away, before the variables have loaded", () => {
    render(
      <VariablesContext.Provider value={null}>
        <AuthEditor
          auth={basic("{{user}}", "{{pw}}")}
          onChange={() => {}}
          canInherit
          context={{ dir: "/c", path: "", request: null }}
        />
      </VariablesContext.Provider>,
    );
    expect(document.querySelectorAll(".cm-editor")).toHaveLength(2);
    expect(document.querySelectorAll("input")).toHaveLength(0);
  });

  it("hide literal secrets until the field is focused, without swapping elements", () => {
    renderEditor(basic("alice", "hunter2"));
    const editors = document.querySelectorAll(".cm-editor");
    expect(editors).toHaveLength(2);
    expect(document.querySelectorAll("input")).toHaveLength(0);
    const password = document.querySelector<HTMLElement>("[data-masked]");
    expect(password?.textContent).toContain("hunter2");

    // Typing needs focus to stay: the same editor is unmasked, nothing is replaced.
    fireEvent.focus(editors[1]?.querySelector(".cm-content") as HTMLElement);
    expect(document.querySelector("[data-masked]")).toBeNull();
    expect(document.querySelectorAll(".cm-editor")[1]).toBe(editors[1]);
  });

  it("show variable references in secret fields", () => {
    renderEditor(basic("alice", "{{password}}"));
    expect(document.querySelector("[data-masked]")).toBeNull();
  });
});
