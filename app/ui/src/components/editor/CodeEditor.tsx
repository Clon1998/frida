import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { Compartment, EditorState, type Extension, Prec } from "@codemirror/state";
import { EditorView, keymap, placeholder as placeholderExtension } from "@codemirror/view";
import { basicSetup, minimalSetup } from "codemirror";
import { useEffect, useRef } from "react";
import { setVariables, type VariableMap, variablesExtension } from "./variables";

export type CodeLanguage = "json" | "xml" | "text";

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  language?: CodeLanguage;
  readOnly?: boolean;
  className?: string;
  /** Variables of the current request, for `{{var}}` highlighting and tooltips. */
  variables?: VariableMap;
  /** One line only (e.g. the URL bar): newlines are removed, Enter calls `onEnter`. */
  singleLine?: boolean;
  onEnter?: () => void;
  placeholder?: string;
}

const languageExtension = (language: CodeLanguage): Extension =>
  language === "json" ? json() : language === "xml" ? xml() : [];

/** Uses the app's CSS variables so the editor matches the light and dark theme. */
const theme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "13px",
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

/** Keeps a single-line editor on one line: pasted newlines become spaces. */
const singleLineFilter = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || tr.newDoc.lines === 1) return tr;
  const text = tr.newDoc.toString().replace(/\r?\n/g, " ");
  return [tr, { changes: { from: 0, to: tr.newDoc.length, insert: text }, sequential: true }];
});

/**
 * A thin React wrapper around a CodeMirror 6 editor. The editor owns its text while the user
 * types; `value` changes from outside (e.g. switching tabs, reload from disk) replace it.
 */
export function CodeEditor({
  value,
  onChange,
  language = "text",
  readOnly = false,
  className,
  variables,
  singleLine = false,
  onEnter,
  placeholder,
}: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onEnterRef = useRef(onEnter);
  const languageSlot = useRef(new Compartment());
  const readOnlySlot = useRef(new Compartment());

  useEffect(() => {
    onChangeRef.current = onChange;
    onEnterRef.current = onEnter;
  }, [onChange, onEnter]);

  // Create the editor once.
  useEffect(() => {
    if (!host.current) return;
    const lineMode: Extension = singleLine
      ? [
          minimalSetup,
          singleLineFilter,
          Prec.highest(keymap.of([{ key: "Enter", run: () => (onEnterRef.current?.(), true) }])),
        ]
      : [basicSetup, EditorView.lineWrapping];
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineMode,
          theme,
          variablesExtension(),
          placeholder ? placeholderExtension(placeholder) : [],
          languageSlot.current.of(languageExtension(language)),
          readOnlySlot.current.of([
            EditorState.readOnly.of(readOnly),
            EditorView.editable.of(!readOnly),
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChangeRef.current?.(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    return () => editor.destroy();
    // Only on mount: later changes are applied by the effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Replace the text when `value` changes from outside.
  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  useEffect(() => {
    view.current?.dispatch({
      effects: languageSlot.current.reconfigure(languageExtension(language)),
    });
  }, [language]);

  useEffect(() => {
    view.current?.dispatch({
      effects: readOnlySlot.current.reconfigure([
        EditorState.readOnly.of(readOnly),
        EditorView.editable.of(!readOnly),
      ]),
    });
  }, [readOnly]);

  useEffect(() => {
    if (variables) view.current?.dispatch({ effects: setVariables.of(variables) });
  }, [variables]);

  return <div ref={host} className={className} />;
}
