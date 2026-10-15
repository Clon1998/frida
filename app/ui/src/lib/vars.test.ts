import { describe, expect, it } from "vitest";
import { sourceLabel, usageLabel, variableAt } from "./vars";

describe("variable helpers", () => {
  it("finds the placeholder under a position", () => {
    const text = "{{baseUrl}}/orders/{{ id }}";
    expect(variableAt(text, 3)).toEqual({ name: "baseUrl", from: 0, to: 11 });
    expect(variableAt(text, 14)).toBeNull();
    expect(variableAt(text, 22)).toEqual({ name: "id", from: 19, to: 27 });
    expect(variableAt("{{}}", 1)).toBeNull();
  });

  it("labels sources and usages", () => {
    expect(sourceLabel({ kind: "collectionEnv", name: "dev" })).toBe("Collection env: dev");
    expect(sourceLabel(null)).toBe("not defined");
    expect(usageLabel({ kind: "header", name: "X-Id" })).toBe("Header X-Id");
  });
});
