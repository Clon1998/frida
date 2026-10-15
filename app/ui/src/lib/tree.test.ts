import { describe, expect, it } from "vitest";
import type { TreeNode } from "@/bindings";
import { childrenOf, dropPosition, moveTarget, parentOf } from "./tree";

const request = (path: string): TreeNode => ({
  kind: "request",
  path,
  name: path,
  seq: 0,
  method: "GET",
});
const folder = (path: string, children: TreeNode[]): TreeNode => ({
  kind: "folder",
  path,
  name: path,
  seq: 0,
  children,
});

// a.yaml, orders/ { orders/x.yaml, orders/eu/ { orders/eu/y.yaml } }, b.yaml
const tree = [
  request("a.yaml"),
  folder("orders", [request("orders/x.yaml"), folder("orders/eu", [request("orders/eu/y.yaml")])]),
  request("b.yaml"),
];

describe("tree helpers", () => {
  it("finds parents and children", () => {
    expect(parentOf("orders/eu/y.yaml")).toBe("orders/eu");
    expect(parentOf("a.yaml")).toBe("");
    expect(childrenOf(tree, "orders/eu").map((n) => n.path)).toEqual(["orders/eu/y.yaml"]);
    expect(childrenOf(tree, "")).toBe(tree);
  });

  it("splits rows into drop zones", () => {
    const rect = { top: 100, height: 20 };
    expect(dropPosition(102, rect, true)).toBe("before");
    expect(dropPosition(110, rect, true)).toBe("inside");
    expect(dropPosition(118, rect, true)).toBe("after");
    expect(dropPosition(108, rect, false)).toBe("before");
    expect(dropPosition(112, rect, false)).toBe("after");
  });

  it("computes the target folder and index", () => {
    // Reorder within the root: a.yaml after b.yaml.
    expect(moveTarget(tree, "a.yaml", "b.yaml", "after")).toEqual({ parent: "", index: 2 });
    // Into a folder: appended at the end.
    expect(moveTarget(tree, "b.yaml", "orders", "inside")).toEqual({ parent: "orders", index: 2 });
    // Before a request in a nested folder.
    expect(moveTarget(tree, "a.yaml", "orders/eu/y.yaml", "before")).toEqual({
      parent: "orders/eu",
      index: 0,
    });
  });

  it("refuses to drop a folder into itself", () => {
    expect(moveTarget(tree, "orders", "orders", "inside")).toBeNull();
    expect(moveTarget(tree, "orders", "orders/eu", "inside")).toBeNull();
    expect(moveTarget(tree, "orders", "orders/x.yaml", "after")).toBeNull();
  });
});
