import { type Extension, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  hoverTooltip,
  MatchDecorator,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import type { ResolvedVar } from "@/bindings";
import { displayValue, sourceLabel, variableAt } from "@/lib/vars";

/** Variables known for the current request, by name. */
export type VariableMap = Map<string, ResolvedVar>;

/** Replaces the known variables (sent when a new inspect report arrives). */
export const setVariables = StateEffect.define<VariableMap>();

const variablesField = StateField.define<VariableMap>({
  create: () => new Map(),
  update(value, tr) {
    for (const effect of tr.effects) if (effect.is(setVariables)) return effect.value;
    return value;
  },
});

function classFor(view: EditorView, name: string): string {
  const v = view.state.field(variablesField).get(name);
  if (!v) return "cm-var";
  if (!v.resolved) return "cm-var cm-var-unresolved";
  return v.isSecret ? "cm-var cm-var-secret" : "cm-var cm-var-resolved";
}

const decorator = new MatchDecorator({
  regexp: /\{\{([^{}]*?)\}\}/g,
  decoration: (match, view) => Decoration.mark({ class: classFor(view, (match[1] ?? "").trim()) }),
});

/** Re-decorates when the text changes or a new variable report arrives. */
const highlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorator.createDeco(view);
    }
    update(update: ViewUpdate) {
      const newVariables = update.transactions.some((tr) =>
        tr.effects.some((e) => e.is(setVariables)),
      );
      this.decorations = newVariables
        ? decorator.createDeco(update.view)
        : decorator.updateDeco(update, this.decorations);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const tooltip = hoverTooltip((view, pos) => {
  const line = view.state.doc.lineAt(pos);
  const found = variableAt(line.text, pos - line.from);
  if (!found) return null;
  const v = view.state.field(variablesField).get(found.name);
  return {
    pos: line.from + found.from,
    end: line.from + found.to,
    above: true,
    create: () => {
      const dom = document.createElement("div");
      dom.className = "cm-var-tooltip";
      const value = v ? displayValue(v) : "not inspected yet";
      const source = v ? sourceLabel(v.source) : "";
      const name = document.createElement("div");
      name.className = "cm-var-tooltip-name";
      name.textContent = found.name;
      const val = document.createElement("div");
      val.className = "cm-var-tooltip-value";
      val.textContent = value;
      dom.append(name, val);
      if (source) {
        const src = document.createElement("div");
        src.className = "cm-var-tooltip-source";
        src.textContent = source;
        dom.append(src);
      }
      return { dom };
    },
  };
});

const theme = EditorView.baseTheme({
  ".cm-var": { borderRadius: "3px", padding: "0 1px" },
  ".cm-var-resolved": {
    backgroundColor: "color-mix(in oklch, var(--color-emerald-500, #10b981) 18%, transparent)",
  },
  ".cm-var-secret": {
    backgroundColor: "color-mix(in oklch, var(--color-amber-500, #f59e0b) 20%, transparent)",
  },
  ".cm-var-unresolved": {
    textDecoration: "underline wavy var(--destructive)",
    textUnderlineOffset: "3px",
  },
  ".cm-var-tooltip": { padding: "4px 8px", fontSize: "12px", maxWidth: "420px" },
  ".cm-var-tooltip-name": { fontWeight: "600" },
  ".cm-var-tooltip-value": { fontFamily: "ui-monospace, monospace", wordBreak: "break-all" },
  ".cm-var-tooltip-source": { opacity: "0.7" },
});

/** Highlighting, hover tooltips and red underlines for `{{variables}}` (PLAN.md §5.4). */
export function variablesExtension(): Extension {
  return [variablesField, highlighter, tooltip, theme];
}
