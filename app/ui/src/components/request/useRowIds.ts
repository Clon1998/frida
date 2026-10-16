import { arrayMove } from "@dnd-kit/sortable";
import { useState } from "react";

let nextId = 0;
const newId = () => `row-${nextId++}`;

/**
 * Stable ids for the rows of a reorderable table (rows themselves have no id). Use them as React
 * `key`s and drag ids: when a row moves, its id moves with it, so React moves the row's element
 * instead of reusing elements by position (which made dropped rows jump back and animate again).
 *
 * `move` and `remove` must be called alongside the matching change to the rows.
 */
export function useRowIds(count: number) {
  const [ids, setIds] = useState<string[]>(() => Array.from({ length: count }, newId));
  let current = ids;
  if (ids.length !== count) {
    // Rows were added (typing into the "add" row) or the list was replaced: adjust the length.
    // Setting state while rendering is React's documented way to derive state from props.
    current =
      ids.length < count
        ? [...ids, ...Array.from({ length: count - ids.length }, newId)]
        : ids.slice(0, count);
    setIds(current);
  }
  return {
    ids: current,
    move: (from: number, to: number) => setIds(arrayMove(current, from, to)),
    remove: (index: number) => setIds(current.filter((_, i) => i !== index)),
  };
}
