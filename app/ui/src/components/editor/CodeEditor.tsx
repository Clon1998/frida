import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { Annotation, Compartment, EditorState, type Extension, Prec } from "@codemirror/state";
import { acceptCompletion, autocompletion, completionStatus } from "@codemirror/autocomplete";
import {
  EditorView,
  keymap,
  placeholder as placeholderExtension,
  tooltips,
} from "@codemirror/view";
import { openSearchPanel } from "@codemirror/search";
import { basicSetup, minimalSetup } from "codemirror";
import { Search } from "lucide-react";
import { cn } from "cn";
import { useContext, useEffect, useRef } from "react";
import { searchExtension } from "./search";
import { editorTheme } from "./theme";
import {
  setVariables,
  type VariableContext,
  VariablesContext,
  variablesExtension,
} from "./variables";

export type CodeLanguage = "json" | "xml" | "javascript" | "text";

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  language?: CodeLanguage;
  readOnly?: boolean;
  className?: string;
  /**
   * Variables for `{{var}}` highlighting, tooltips and autocompletion. Defaults to the
   * surrounding {@link VariablesContext} (the request editor provides one).
   */
  variables?: VariableContext | null;
  /** Also highlight `:pathParams` (the URL bar). */
  pathParams?: boolean;
  /** One line only (e.g. the URL bar): newlines are removed, Enter calls `onEnter`. */
  singleLine?: boolean;
  /** A single-line editor sized like a table cell input. */
  compact?: boolean;
  onEnter?: () => void;
  placeholder?: string;
  /** Called with pasted text; return `true` to handle it instead of inserting it. */
  onPasteText?: (text: string) => boolean;
  /**
   * Multi-line editors have a find bar (Cmd/Ctrl+F); this also shows a search button in the
   * top right corner. Default: on for multi-line editors.
   */
  searchButton?: boolean;
}

const languageExtension = (language: CodeLanguage): Extension => {
  switch (language) {
    case "json":
      return json();
    case "xml":
      return xml();
    case "javascript":
      return javascript();
    case "text":
      return [];
  }
};

/**
 * Single-line editors (the URL bar) behave like Postman's: one line ending in "…" while not
 * focused, and wrapped onto as many lines as needed (growing downwards, same width) while
 * editing. Wrapping is always on; the unfocused look overrides it with CSS.
 */
const singleLineTheme = EditorView.theme({
  ".cm-scroller": { overflow: "hidden", lineHeight: "1.5rem" },
  ".cm-content, .cm-line": { padding: "0 2px" },
  // URLs have no spaces, so allow breaks anywhere while editing.
  "&.cm-focused .cm-content": { wordBreak: "break-all" },
  "&:not(.cm-focused) .cm-content": { whiteSpace: "pre" },
  "&:not(.cm-focused) .cm-line": { overflow: "hidden", textOverflow: "ellipsis" },
});

/**
 * One zero-size element for the popups (autocompletion, variable tooltips) of every editor.
 *
 * Popups go outside the editors so containers that clip their content (table cells, the
 * floating URL bar) can't cut them off. CodeMirror wraps them in a container per editor that
 * carries the editor's style classes, including `height: 100%` from {@link theme}. Placed
 * directly in `<body>`, each of those containers was a full window height below the app, so
 * the page could be scrolled past the footer. In this zero-size, fixed host they take no space.
 * (The popups themselves are `position: fixed`, so they still show up next to the cursor.)
 */
function tooltipHost(): HTMLElement {
  const id = "cm-tooltip-host";
  let host = document.getElementById(id);
  if (!host) {
    host = document.createElement("div");
    host.id = id;
    // Above dialogs (z-50), so suggestions also show for editors inside a dialog.
    host.style.cssText = "position:fixed;top:0;left:0;width:0;height:0;z-index:60";
    document.body.appendChild(host);
  }
  return host;
}

/**
 * Marks text replaced from outside (switching tabs, reload from disk). Such changes are not user
 * edits, so they must not call `onChange`: that would mark tabs as edited (and pin preview tabs).
 */
const externalChange = Annotation.define<boolean>();

/** Single-line editors in table cells: the height and padding of a plain cell input. */
const compactTheme = EditorView.theme({
  "&": { fontSize: "calc(var(--editor-font-size, 13px) - 1px)" },
  ".cm-scroller": { lineHeight: "2rem" },
  ".cm-content, .cm-line": { padding: "0 4px" },
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
  variables: variablesProp,
  pathParams = false,
  singleLine = false,
  compact = false,
  onEnter,
  placeholder,
  onPasteText,
  searchButton = !singleLine,
}: CodeEditorProps) {
  const contextVariables = useContext(VariablesContext) ?? null;
  const variables = variablesProp === undefined ? contextVariables : variablesProp;
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onEnterRef = useRef(onEnter);
  const onPasteRef = useRef(onPasteText);
  const languageSlot = useRef(new Compartment());
  const readOnlySlot = useRef(new Compartment());

  useEffect(() => {
    onChangeRef.current = onChange;
    onEnterRef.current = onEnter;
    onPasteRef.current = onPasteText;
  }, [onChange, onEnter, onPasteText]);

  // Create the editor once.
  useEffect(() => {
    if (!host.current) return;
    const lineMode: Extension = singleLine
      ? [
          minimalSetup,
          singleLineFilter,
          singleLineTheme,
          compact ? compactTheme : [],
          EditorView.lineWrapping,
          // minimalSetup has no autocompletion; `{{` suggestions need it.
          autocompletion({ icons: false }),
          Prec.highest(
            keymap.of([
              {
                key: "Enter",
                // With the suggestion list open, Enter picks a suggestion instead of sending.
                run: (view) => {
                  if (completionStatus(view.state) === "active") return false;
                  onEnterRef.current?.();
                  return true;
                },
              },
            ]),
          ),
        ]
      : [basicSetup, searchExtension(), EditorView.lineWrapping];
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineMode,
          // Tab accepts the highlighted suggestion (CodeMirror's default keymap only binds Enter).
          // Without an open suggestion list `acceptCompletion` returns false, so Tab still moves
          // the focus to the next field.
          Prec.highest(keymap.of([{ key: "Tab", run: acceptCompletion }])),
          editorTheme,
          tooltips({ parent: tooltipHost() }),
          variablesExtension({ pathParams }),
          placeholder ? placeholderExtension(placeholder) : [],
          languageSlot.current.of(languageExtension(language)),
          readOnlySlot.current.of(readOnlyExtension(readOnly, singleLine)),
          EditorView.domEventHandlers({
            paste: (event) => {
              const text = event.clipboardData?.getData("text/plain");
              if (text && onPasteRef.current?.(text)) {
                event.preventDefault();
                return true;
              }
              return false;
            },
          }),
          EditorView.updateListener.of((update) => {
            const fromOutside = update.transactions.some((tr) => tr.annotation(externalChange));
            if (update.docChanged && !fromOutside) {
              onChangeRef.current?.(update.state.doc.toString());
            }
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
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: value },
        annotations: externalChange.of(true),
      });
    }
  }, [value]);

  useEffect(() => {
    view.current?.dispatch({
      effects: languageSlot.current.reconfigure(languageExtension(language)),
    });
  }, [language]);

  useEffect(() => {
    view.current?.dispatch({
      effects: readOnlySlot.current.reconfigure(readOnlyExtension(readOnly, singleLine)),
    });
  }, [readOnly, singleLine]);

  useEffect(() => {
    view.current?.dispatch({ effects: setVariables.of(variables ?? null) });
  }, [variables]);

  // CodeMirror appends its editor to `host` after React's children (the button), so the two
  // don't get in each other's way.
  return (
    <div
      ref={host}
      className={cn(
        searchButton && "group/editor relative [&:has(.cm-panels-top)>button]:hidden",
        className,
      )}
    >
      {searchButton && (
        <button
          type="button"
          title="Find (Ctrl/Cmd+F)"
          aria-label="Find"
          className="absolute top-1.5 right-1.5 z-10 flex size-6 items-center justify-center rounded border bg-background/90 text-muted-foreground opacity-60 group-hover/editor:opacity-100 hover:text-foreground focus-visible:opacity-100"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            if (view.current) openSearchPanel(view.current);
          }}
        >
          <Search className="size-3.5" />
        </button>
      )}
    </div>
  );
}

/**
 * Read-only multi-line editors stay focusable (`editable`), so text can be selected and
 * Cmd/Ctrl+F reaches the find bar; only edits are blocked (`readOnly`). Read-only single-line
 * editors aren't focusable at all.
 */
function readOnlyExtension(readOnly: boolean, singleLine: boolean): Extension {
  return [EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly || !singleLine)];
}
