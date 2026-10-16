import { type Extension, RangeSetBuilder } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";

const added = Decoration.line({ class: "cm-diff-added" });
const removed = Decoration.line({ class: "cm-diff-removed" });
const hunk = Decoration.line({ class: "cm-diff-hunk" });
const meta = Decoration.line({ class: "cm-diff-meta" });

/** The decoration for one line of a unified diff, if any. */
export function diffLineKind(text: string): "added" | "removed" | "hunk" | "meta" | null {
  if (text.startsWith("+++") || text.startsWith("---")) return "meta";
  if (/^(diff |index |new file|deleted file|similarity|rename |old mode|new mode)/.test(text)) {
    return "meta";
  }
  if (text.startsWith("@@")) return "hunk";
  if (text.startsWith("+")) return "added";
  if (text.startsWith("-")) return "removed";
  return null;
}

const DECORATIONS = { added, removed, hunk, meta };

function build(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to;) {
      const line = view.state.doc.lineAt(pos);
      const kind = diffLineKind(line.text);
      if (kind) builder.add(line.from, line.from, DECORATIONS[kind]);
      pos = line.to + 1;
    }
  }
  return builder.finish();
}

/** Colors a unified diff (`git diff` output): green additions, red removals. */
export function diffHighlight(): Extension {
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = build(view);
        }
        update(update: ViewUpdate) {
          if (update.docChanged || update.viewportChanged) this.decorations = build(update.view);
        }
      },
      { decorations: (plugin) => plugin.decorations },
    ),
    EditorView.baseTheme({
      ".cm-diff-added": {
        backgroundColor: "color-mix(in oklch, oklch(69.6% 0.17 162.48) 16%, transparent)",
      },
      ".cm-diff-removed": {
        backgroundColor: "color-mix(in oklch, oklch(63.7% 0.237 25.331) 16%, transparent)",
      },
      ".cm-diff-hunk": { color: "oklch(62.3% 0.214 259.815)" },
      ".cm-diff-meta": { opacity: "0.6" },
    }),
  ];
}
