import {
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { EditorState, type Extension, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  hoverTooltip,
  MatchDecorator,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { createContext } from "react";
import type { AvailableVar, ResolvedVar } from "@/bindings";
import { displayValue, sourceLabel, variableAt } from "@/lib/vars";

/**
 * What the editors know about variables for the request being edited (from the inspector):
 * the variables it uses, every variable it *could* use, and its path params.
 */
export interface VariableContext {
  used: Map<string, ResolvedVar>;
  available: Map<string, AvailableVar>;
  /** Path param name → value, for `:name` in the URL. */
  pathParams: Map<string, string>;
}

/**
 * Provides the variable context to every editor below (URL, body, scripts, key/value cells),
 * so each one doesn't need it passed down by hand.
 */
export const VariablesContext = createContext<VariableContext | null>(null);

/** Replaces the known variables (sent when a new inspect report arrives). */
export const setVariables = StateEffect.define<VariableContext | null>();

const variablesField = StateField.define<VariableContext | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) if (effect.is(setVariables)) return effect.value;
    return value;
  },
});

type VarState = "neutral" | "resolved" | "secret" | "unresolved";

/** Like Bruno: green when the variable has a value, red when it doesn't. */
export function variableState(ctx: VariableContext | null, name: string): VarState {
  if (!ctx) return "neutral";
  const used = ctx.used.get(name);
  if (used) return !used.resolved ? "unresolved" : used.isSecret ? "secret" : "resolved";
  const available = ctx.available.get(name);
  if (available) {
    if (!available.hasValue) return "unresolved";
    return available.isSecret ? "secret" : "resolved";
  }
  // Read from the OS environment when sending; not known here.
  if (name.startsWith("process.env.")) return "neutral";
  return "unresolved";
}

const classOf = (state: VarState) => (state === "neutral" ? "cm-var" : `cm-var cm-var-${state}`);

const variableDecorator = new MatchDecorator({
  regexp: /\{\{([^{}]*?)\}\}/g,
  decoration: (match, view) =>
    Decoration.mark({
      class: classOf(variableState(view.state.field(variablesField), (match[1] ?? "").trim())),
    }),
});

/** `/:orderId` in a URL path: green when the path param has a value. */
const pathParamDecorator = new MatchDecorator({
  regexp: /(?<=\/):([A-Za-z_][\w-]*)/g,
  decoration: (match, view) => {
    const ctx = view.state.field(variablesField);
    const value = ctx?.pathParams.get(match[1] ?? "");
    const state: VarState = !ctx ? "neutral" : value ? "resolved" : "unresolved";
    return Decoration.mark({ class: classOf(state) });
  },
});

/** Re-decorates when the text changes or a new variable report arrives. */
function highlighter(decorator: MatchDecorator) {
  return ViewPlugin.fromClass(
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
}

const tooltip = hoverTooltip((view, pos) => {
  const line = view.state.doc.lineAt(pos);
  const found = variableAt(line.text, pos - line.from);
  if (!found) return null;
  const ctx = view.state.field(variablesField);
  const used = ctx?.used.get(found.name);
  const available = ctx?.available.get(found.name);
  let value = "not inspected yet";
  let source = "";
  if (used) {
    value = displayValue(used);
    source = sourceLabel(used.source);
  } else if (available) {
    value = available.isSecret
      ? available.hasValue
        ? "•••••••• (secret)"
        : "unset secret"
      : (available.value ?? "");
    if (available.source.kind === "dynamic") value = "generated on every send";
    source = sourceLabel(available.source);
  } else if (ctx) {
    value = "not defined";
  }
  return {
    pos: line.from + found.from,
    end: line.from + found.to,
    above: true,
    create: () => {
      const dom = document.createElement("div");
      dom.className = "cm-var-tooltip";
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

/**
 * Suggests variable names after `{{` (like Bruno): every variable in scope, the dynamic ones
 * and `process.env.`. Picking one completes the closing `}}` too, unless it's already there.
 */
export function variableCompletions(context: CompletionContext): CompletionResult | null {
  const before = context.matchBefore(/\{\{\s*[\w.$-]*$/);
  if (!before) return null;
  const typedFrom = before.from + before.text.search(/[^{\s]|$/);
  const ctx = context.state.field(variablesField, false) ?? null;
  const after = context.state.sliceDoc(context.pos, context.pos + 2);
  const closing = after === "}}" ? "" : "}}";

  const options: Completion[] = [];
  for (const v of ctx?.available.values() ?? []) {
    const dynamic = v.source.kind === "dynamic";
    options.push({
      label: v.name,
      detail: dynamic ? "dynamic" : sourceLabel(v.source),
      info: dynamic
        ? "Generated on every send"
        : v.isSecret
          ? v.hasValue
            ? "Secret"
            : "Secret without a value"
          : (v.value ?? ""),
      type: dynamic ? "function" : "variable",
      boost: dynamic ? -1 : 0,
      apply: `${v.name}${closing}`,
    });
  }
  options.push({
    label: "process.env.",
    detail: "OS environment",
    type: "namespace",
    boost: -2,
    apply: "process.env.",
  });
  return { from: typedFrom, options, validFor: /^[\w.$-]*$/ };
}

const theme = EditorView.baseTheme({
  ".cm-var": { borderRadius: "3px", padding: "0 1px" },
  ".cm-var-tooltip": { padding: "4px 8px", fontSize: "12px", maxWidth: "420px" },
  ".cm-var-tooltip-name": { fontWeight: "600" },
  ".cm-var-tooltip-value": { fontFamily: "ui-monospace, monospace", wordBreak: "break-all" },
  ".cm-var-tooltip-source": { opacity: "0.7" },
});

/**
 * Highlighting (green = has a value, red = missing), hover tooltips and `{{` autocompletion
 * for variables (PLAN.md §5.4). With `pathParams`, `:name` path segments are highlighted too.
 * The colors are in `index.css` so they can follow the light/dark theme.
 */
export function variablesExtension(options: { pathParams?: boolean } = {}): Extension {
  return [
    variablesField,
    highlighter(variableDecorator),
    options.pathParams ? highlighter(pathParamDecorator) : [],
    tooltip,
    theme,
    // Offered to whichever autocompletion is active (basicSetup's, or the one added for
    // single-line editors).
    EditorState.languageData.of(() => [{ autocomplete: variableCompletions }]),
  ];
}
