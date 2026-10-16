/**
 * Fuzzy matching for quick open: every character of the query must appear in order. Matches at
 * word starts and consecutive characters score higher, shorter texts break ties.
 * Returns `null` when the query doesn't match.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const t = text.toLowerCase();
  let score = 0;
  let ti = 0;
  let previous = -2;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found === -1) return null;
    const atWordStart = found === 0 || /[\s/_\-.:]/.test(t[found - 1] ?? "");
    score += 1 + (found === previous + 1 ? 3 : 0) + (atWordStart ? 2 : 0);
    previous = found;
    ti = found + 1;
  }
  return score - t.length * 0.01;
}

/** `items` that match `query`, best first. */
export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  return items
    .map((item) => ({ item, score: fuzzyScore(query, text(item)) }))
    .filter((r): r is { item: T; score: number } => r.score !== null)
    .sort((a, b) => b.score - a.score)
    .map((r) => r.item);
}
