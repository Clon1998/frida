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

  it("hide literal secrets until the field is focused", () => {
    renderEditor(basic("alice", "hunter2"));
    const password = document.querySelector<HTMLInputElement>('input[type="password"]');
    expect(password?.value).toBe("hunter2");
    expect(document.querySelectorAll(".cm-editor")).toHaveLength(1);

    fireEvent.focus(password as HTMLInputElement);
    expect(document.querySelector('input[type="password"]')).toBeNull();
    expect(document.querySelectorAll(".cm-editor")).toHaveLength(2);
  });
});
