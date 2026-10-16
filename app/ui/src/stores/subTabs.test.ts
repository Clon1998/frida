import { describe, expect, it } from "vitest";
import { useSubTabs } from "./subTabs";

describe("sub-tab memory", () => {
  it("remembers the open sub-tab per editor", () => {
    useSubTabs.getState().set("request:/c:a.yaml", "script");
    useSubTabs.getState().set("request:/c:b.yaml", "auth");
    expect(useSubTabs.getState().open["request:/c:a.yaml"]).toBe("script");
    expect(useSubTabs.getState().open["request:/c:b.yaml"]).toBe("auth");
  });
});
