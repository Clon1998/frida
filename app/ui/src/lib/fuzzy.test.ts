import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("fuzzy matching", () => {
  it("needs every character in order", () => {
    expect(fuzzyScore("gord", "orders/Get order")).not.toBeNull();
    expect(fuzzyScore("xyz", "orders/Get order")).toBeNull();
  });

  it("ranks word starts and runs higher", () => {
    const items = ["auth/Login", "orders/Get order", "orders/List orders", "misc/Logout"];
    expect(fuzzyFilter(items, "login", (i) => i)[0]).toBe("auth/Login");
    expect(fuzzyFilter(items, "list", (i) => i)[0]).toBe("orders/List orders");
    expect(fuzzyFilter(items, "", (i) => i)).toHaveLength(4);
  });
});
