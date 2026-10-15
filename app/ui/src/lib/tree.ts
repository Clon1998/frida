import type { TreeNode } from "@/bindings";

export type DropPosition = "before" | "after" | "inside";

/** `"orders/eu/get.yaml"` → `"orders/eu"`, `"get.yaml"` → `""`. */
export function parentOf(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

/** `path` itself, or anything inside it when `path` is a folder. */
export function isUnder(candidate: string, path: string): boolean {
  return candidate === path || candidate.startsWith(`${path}/`);
}

/** The children of folder `parent` (`""` = collection root). */
export function childrenOf(tree: TreeNode[], parent: string): TreeNode[] {
  if (parent === "") return tree;
  for (const node of tree) {
    if (node.kind !== "folder") continue;
    if (node.path === parent) return node.children;
    if (isUnder(parent, node.path)) return childrenOf(node.children, parent);
  }
  return [];
}

/**
 * Where a hovered row splits into drop zones: the top quarter means "before", the bottom
 * quarter "after", and the middle "inside" (folders only; requests split in half).
 */
export function dropPosition(
  pointerY: number,
  rect: { top: number; height: number },
  isFolder: boolean,
): DropPosition {
  const ratio = (pointerY - rect.top) / rect.height;
  if (!isFolder) return ratio < 0.5 ? "before" : "after";
  if (ratio < 0.25) return "before";
  if (ratio > 0.75) return "after";
  return "inside";
}

/**
 * The folder and position to move `dragged` to when it is dropped on `over`, or `null` when
 * the drop isn't allowed (onto itself, or a folder into its own subfolder).
 */
export function moveTarget(
  tree: TreeNode[],
  dragged: string,
  over: string,
  position: DropPosition,
): { parent: string; index: number } | null {
  if (isUnder(over, dragged)) return null;

  if (position === "inside") {
    const siblings = childrenOf(tree, over).filter((n) => n.path !== dragged);
    return { parent: over, index: siblings.length };
  }

  const parent = parentOf(over);
  const siblings = childrenOf(tree, parent).filter((n) => n.path !== dragged);
  const index = siblings.findIndex((n) => n.path === over);
  if (index === -1) return null;
  return { parent, index: position === "after" ? index + 1 : index };
}
