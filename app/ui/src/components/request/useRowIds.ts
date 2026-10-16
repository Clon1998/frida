import { arrayMove } from "@dnd-kit/sortable";
import { useState } from "react";

let nextId = 0;
const newId = () => `row-${nextId++}`;

/**
 * Stable ids for the rows of a reorderable table (rows themselves have no id). Use them as React
 * `key`s and drag ids: when a row moves, its id moves with it, so React moves the row's element
 * instead of reusing elements by position (which made dropped rows jump back and animate again).
 *
 * `addId` is the key for the trailing "add" row. It is the id that row keeps once typing turns it
 * into a real row, so React keeps the same element (and the focused input) instead of mounting a
 * new one; the next "add" row gets a fresh id.
 *
 * `move` and `remove` must be called alongside the matching change to the rows.
 */
export function useRowIds(count: number) {
  // One id per row plus one for the "add" row at the end.
  const [all, setAll] = useState<string[]>(() => Array.from({ length: count + 1 }, newId));
  let current = all;
  if (all.length !== count + 1) {
    // Rows were added (typing into the "add" row) or the list was replaced: adjust the length.
    // New rows take over the reserved "add" id first. Setting state while rendering is React's
    // documented way to derive state from props.
    current =
      all.length < count + 1
        ? [...all, ...Array.from({ length: count + 1 - all.length }, newId)]
        : [...all.slice(0, count), all[all.length - 1] ?? newId()];
    setAll(current);
  }
  const ids = current.slice(0, count);
  const addId = current[count] ?? "add";
  return {
    ids,
    addId,
    move: (from: number, to: number) => setAll([...arrayMove(ids, from, to), addId]),
    remove: (index: number) => setAll([...ids.filter((_, i) => i !== index), addId]),
  };
}
