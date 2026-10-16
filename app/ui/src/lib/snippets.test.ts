import { describe, expect, it } from "vitest";
import { appendSnippet } from "./snippets";

describe("appendSnippet", () => {
  it("fills an empty editor and appends after existing code", () => {
    expect(appendSnippet("", "a();")).toBe("a();\n");
    expect(appendSnippet("  \n", "a();")).toBe("a();\n");
    expect(appendSnippet("x();\n\n\n", "a();")).toBe("x();\n\na();\n");
  });
});
