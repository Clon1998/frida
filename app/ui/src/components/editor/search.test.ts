import { SearchQuery } from "@codemirror/search";
import { EditorSelection, EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { countMatches } from "./search";

const state = (doc: string, from = 0, to = from) =>
  EditorState.create({ doc, selection: EditorSelection.single(from, to) });

describe("find bar match count", () => {
  it("counts matches and knows which one is selected", () => {
    const doc = '{"id": 1, "items": [{"id": 2}, {"id": 3}]}';
    const second = doc.indexOf('"id"', 3);
    const s = state(doc, second, second + 4);
    expect(countMatches(s, new SearchQuery({ search: '"id"' }))).toEqual({
      total: 3,
      current: 2,
      capped: false,
    });
  });

  it("respects case and regex options and caps huge counts", () => {
    expect(countMatches(state("Id id ID"), new SearchQuery({ search: "id" })).total).toBe(3);
    expect(
      countMatches(state("Id id ID"), new SearchQuery({ search: "id", caseSensitive: true })).total,
    ).toBe(1);
    expect(
      countMatches(state("a1 b22 c333"), new SearchQuery({ search: "\\d+", regexp: true })).total,
    ).toBe(3);
    const many = countMatches(state("a".repeat(5000)), new SearchQuery({ search: "a" }));
    expect(many).toMatchObject({ total: 1000, capped: true });
    expect(countMatches(state("x"), new SearchQuery({ search: "(", regexp: true })).total).toBe(0);
  });
});
