import { json } from "@codemirror/lang-json";
import { yaml } from "@codemirror/lang-yaml";
import { MergeView, unifiedMergeView } from "@codemirror/merge";
import { searchKeymap } from "@codemirror/search";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap, lineNumbers } from "@codemirror/view";
import { minimalSetup } from "codemirror";
import { useEffect, useRef } from "react";
import { editorTheme } from "@/components/editor/theme";
import { searchExtension } from "@/components/editor/search";

export type DiffMode = "unified" | "split";

/** Syntax highlighting from the file extension (collections are YAML files). */
function languageFor(path: string): Extension {
  if (/\.ya?ml$/i.test(path)) return yaml();
  if (/\.json$/i.test(path)) return json();
  return [];
}

/** Added and removed lines in the app's success/destructive colors, light and dark. */
const diffTheme = EditorView.theme({
  ".cm-changedLine, &.cm-merge-b .cm-changedLine, .cm-insertedLine": {
    backgroundColor: "color-mix(in oklch, var(--success) 14%, transparent) !important",
  },
  "&.cm-merge-a .cm-changedLine, .cm-deletedChunk": {
    backgroundColor: "color-mix(in oklch, var(--destructive) 14%, transparent) !important",
  },
  "&.cm-merge-b .cm-changedText, .cm-insertedLine .cm-changedText": {
    background: "color-mix(in oklch, var(--success) 35%, transparent) !important",
  },
  "&.cm-merge-a .cm-changedText, .cm-deletedChunk .cm-deletedText": {
    background: "color-mix(in oklch, var(--destructive) 35%, transparent) !important",
  },
  ".cm-changeGutter": { width: "3px", paddingLeft: "0" },
  "&.cm-merge-b .cm-changedLineGutter, .cm-changedLineGutter": {
    background: "var(--success)",
  },
  "&.cm-merge-a .cm-changedLineGutter, .cm-deletedLineGutter": {
    background: "var(--destructive)",
  },
  ".cm-collapsedLines": {
    background: "var(--muted)",
    color: "var(--muted-foreground)",
    padding: "2px 8px",
    fontSize: "11px",
  },
});

/** Read-only, but focusable (selection, Cmd/Ctrl+F). */
const viewer = (path: string): Extension => [
  minimalSetup,
  lineNumbers(),
  keymap.of(searchKeymap),
  searchExtension(),
  languageFor(path),
  editorTheme,
  diffTheme,
  EditorState.readOnly.of(true),
];

/** Long unchanged stretches fold into "n unchanged lines" (click to expand). */
const collapse = { margin: 3, minSize: 6 };

/**
 * The diff of one file with `@codemirror/merge`: unified (deleted lines shown above the new
 * ones in one editor) or side by side (old left, new right, unchanged lines aligned). `old` is
 * `null` for a new file and `new` for a deleted one. Both editors are read-only.
 *
 * CodeMirror lives outside React, so the view is created in an effect and recreated when the
 * input changes (like `CodeEditor`, but it never needs to keep a cursor across changes here).
 */
export function DiffView({
  path,
  old,
  new: next,
  mode,
}: {
  path: string;
  old: string | null;
  new: string | null;
  mode: DiffMode;
}) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const parent = host.current;
    if (!parent) return;
    if (mode === "split") {
      const view = new MergeView({
        a: { doc: old ?? "", extensions: viewer(path) },
        b: { doc: next ?? "", extensions: viewer(path) },
        parent,
        gutter: true,
        collapseUnchanged: collapse,
      });
      return () => view.destroy();
    }
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: next ?? "",
        extensions: [
          viewer(path),
          unifiedMergeView({
            original: old ?? "",
            mergeControls: false,
            gutter: true,
            collapseUnchanged: collapse,
          }),
        ],
      }),
    });
    return () => view.destroy();
  }, [path, old, next, mode]);

  // `.cm-mergeView` isn't scrollable by itself; the host scrolls (split) or the editor does.
  return (
    <div
      ref={host}
      // `isolate`: see CodeEditor (CodeMirror's find bar must not paint over dialogs).
      className="isolate h-full overflow-auto [&_.cm-editor]:min-h-full [&>.cm-editor]:h-full"
      data-testid="diff-view"
    />
  );
}
