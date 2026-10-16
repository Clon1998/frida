import {
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  search,
  SearchQuery,
  setSearchQuery,
} from "@codemirror/search";
import { EditorSelection, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, type Panel, type ViewUpdate } from "@codemirror/view";

/** Counting stops here, so a one-letter search in a huge body stays fast ("1000+"). */
const MAX_COUNT = 1000;

/** How many matches `query` has, and which one (1-based) is selected, if any. */
export function countMatches(
  state: EditorState,
  query: SearchQuery,
): { total: number; current: number | null; capped: boolean } {
  if (!query.valid) return { total: 0, current: null, capped: false };
  const selection = state.selection.main;
  const cursor = query.getCursor(state);
  let total = 0;
  let current: number | null = null;
  for (let next = cursor.next(); !next.done; next = cursor.next()) {
    total++;
    if (next.value.from === selection.from && next.value.to === selection.to) current = total;
    if (total >= MAX_COUNT) return { total, current, capped: true };
  }
  return { total, current, capped: false };
}

function button(label: string, title: string, onClick: () => void): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  el.title = title;
  el.setAttribute("aria-label", title);
  // Keep the focus in the search field while clicking the buttons.
  el.addEventListener("mousedown", (e) => e.preventDefault());
  el.addEventListener("click", onClick);
  return el;
}

/**
 * The find bar (Cmd/Ctrl+F): a search field with "3 of 17", previous/next, case-sensitive and
 * regex toggles. Replaces CodeMirror's default panel, which has no match count and doesn't
 * fit the app's look. Plain DOM, because CodeMirror panels live outside React.
 */
function createPanel(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "cm-frida-search";

  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = "Find";
  input.spellcheck = false;
  input.setAttribute("aria-label", "Find");
  // CodeMirror focuses the field marked `main-field` when the panel opens.
  input.setAttribute("main-field", "true");

  const count = document.createElement("span");
  count.className = "cm-frida-search-count";
  count.setAttribute("aria-live", "polite");

  let query = getSearchQuery(view.state);
  input.value = query.search;

  const toggle = (label: string, title: string, get: () => boolean, set: (on: boolean) => void) => {
    const el = button(label, title, () => {
      set(!get());
      el.setAttribute("aria-pressed", String(get()));
      commit(false);
    });
    el.setAttribute("aria-pressed", String(get()));
    return el;
  };
  let caseSensitive = query.caseSensitive;
  let regexp = query.regexp;
  const caseButton = toggle(
    "Aa",
    "Match case",
    () => caseSensitive,
    (on) => (caseSensitive = on),
  );
  const regexButton = toggle(
    ".*",
    "Regular expression",
    () => regexp,
    (on) => (regexp = on),
  );

  /** Applies the field and toggles; while typing, jumps to the first match from the cursor. */
  const commit = (jump: boolean) => {
    const next = new SearchQuery({ search: input.value, caseSensitive, regexp });
    if (!next.eq(getSearchQuery(view.state))) view.dispatch({ effects: setSearchQuery.of(next) });
    if (jump && next.valid) {
      // Search again from the start of the current match, so typing more letters keeps the
      // same match selected instead of skipping to the one after it.
      const from = view.state.selection.main.from;
      view.dispatch({ selection: EditorSelection.cursor(from) });
      findNext(view);
    }
  };

  const showCount = () => {
    if (!query.search) {
      count.textContent = "";
      return;
    }
    if (!query.valid) {
      count.textContent = "invalid";
      return;
    }
    const { total, current, capped } = countMatches(view.state, query);
    const shownTotal = capped ? `${total}+` : String(total);
    count.textContent =
      total === 0 ? "No results" : current ? `${current} of ${shownTotal}` : shownTotal;
  };

  input.addEventListener("input", () => commit(true));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) findPrevious(view);
      else findNext(view);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeSearchPanel(view);
      view.focus();
    }
  });

  dom.append(
    input,
    count,
    caseButton,
    regexButton,
    button("↑", "Previous match (Shift+Enter)", () => findPrevious(view)),
    button("↓", "Next match (Enter)", () => findNext(view)),
    button("✕", "Close (Escape)", () => {
      closeSearchPanel(view);
      view.focus();
    }),
  );
  showCount();

  return {
    dom,
    top: true,
    // Focus the field when the bar opens (Cmd/Ctrl+F or the search button), so typing searches.
    mount() {
      input.select();
    },
    update(update: ViewUpdate) {
      const latest = getSearchQuery(update.state);
      if (!latest.eq(query)) {
        query = latest;
        if (input.value !== latest.search) input.value = latest.search;
      }
      if (update.docChanged || update.selectionSet || update.transactions.length > 0) showCount();
    },
  };
}

/** Styles for the find bar and the match highlights, from the app's CSS variables. */
const searchTheme = EditorView.theme({
  ".cm-panels": {
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
  },
  ".cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-frida-search": {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    padding: "4px 6px",
    fontFamily: "inherit",
    fontSize: "12px",
  },
  ".cm-frida-search input": {
    flex: "1",
    minWidth: "6rem",
    height: "24px",
    padding: "0 6px",
    border: "1px solid var(--input)",
    borderRadius: "4px",
    backgroundColor: "transparent",
    color: "inherit",
    outline: "none",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  },
  ".cm-frida-search input:focus": { borderColor: "var(--ring)" },
  ".cm-frida-search-count": {
    minWidth: "4.5rem",
    textAlign: "right",
    color: "var(--muted-foreground)",
    whiteSpace: "nowrap",
  },
  ".cm-frida-search button": {
    height: "24px",
    minWidth: "24px",
    padding: "0 4px",
    border: "none",
    borderRadius: "4px",
    background: "transparent",
    color: "var(--muted-foreground)",
    cursor: "pointer",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  },
  ".cm-frida-search button:hover": {
    backgroundColor: "var(--accent)",
    color: "var(--foreground)",
  },
  ".cm-frida-search button[aria-pressed=true]": {
    backgroundColor: "var(--accent)",
    color: "var(--foreground)",
    boxShadow: "inset 0 0 0 1px var(--ring)",
  },
  ".cm-searchMatch": {
    backgroundColor: "color-mix(in oklch, var(--warning) 30%, transparent)",
    borderRadius: "2px",
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "color-mix(in oklch, var(--warning) 65%, transparent)",
  },
});

/** Find bar for multi-line editors: Cmd/Ctrl+F (from basicSetup's keymap) opens it. */
export function searchExtension(): Extension {
  return [search({ top: true, createPanel }), searchTheme];
}
