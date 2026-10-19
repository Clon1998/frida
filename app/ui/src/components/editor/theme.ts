import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

/** Uses the app's CSS variables so the editor matches the light and dark theme. */
const baseTheme = EditorView.theme({
  "&": {
    height: "100%",
    // Settings → Appearance → Editor font size (see lib/theme.ts).
    fontSize: "var(--editor-font-size, 13px)",
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
  },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" },
  ".cm-gutters": {
    backgroundColor: "var(--muted)",
    color: "var(--muted-foreground)",
    border: "none",
  },
  ".cm-activeLine, .cm-activeLineGutter": {
    backgroundColor: "color-mix(in oklch, var(--accent) 60%, transparent)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-cursor": { borderLeftColor: "var(--foreground)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "color-mix(in oklch, var(--ring) 35%, transparent)",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)",
    color: "var(--popover-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
  },
});

/**
 * Syntax colors for JSON, XML/HTML and JavaScript, after Bruno's palette. The colors are the
 * `--syntax-*` variables in `index.css`, so they follow the light/dark theme. Replaces
 * CodeMirror's default style, which is made for light backgrounds only.
 */
const highlightStyle = HighlightStyle.define([
  {
    tag: [t.propertyName, t.attributeName, t.definition(t.variableName)],
    color: "var(--syntax-property)",
  },
  {
    tag: [t.string, t.attributeValue, t.special(t.string), t.regexp],
    color: "var(--syntax-string)",
  },
  { tag: [t.number, t.variableName], color: "var(--syntax-number)" },
  { tag: [t.bool, t.null, t.atom, t.keyword, t.tagName, t.self], color: "var(--syntax-keyword)" },
  {
    tag: [t.operator, t.punctuation, t.separator, t.bracket, t.angleBracket],
    color: "var(--syntax-punctuation)",
  },
  {
    tag: [t.comment, t.meta, t.processingInstruction],
    color: "var(--syntax-comment)",
    fontStyle: "italic",
  },
  { tag: t.invalid, color: "var(--destructive)" },
]);

export const editorTheme: Extension = [baseTheme, syntaxHighlighting(highlightStyle)];
