import type { Preferences, ThemePreference } from "@/bindings";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Keeps the `.dark` class on `root` in sync with the OS color scheme.
 * Returns a function that stops listening.
 */
export function followSystemTheme(root: HTMLElement = document.documentElement): () => void {
  const media = window.matchMedia(DARK_QUERY);
  const apply = () => root.classList.toggle("dark", media.matches);
  apply();
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}

/**
 * Applies the theme setting: "system" follows the OS (until the returned function is called),
 * "light" and "dark" are fixed.
 */
export function applyTheme(
  theme: ThemePreference,
  root: HTMLElement = document.documentElement,
): () => void {
  if (theme === "system") return followSystemTheme(root);
  root.classList.toggle("dark", theme === "dark");
  return () => {};
}

/**
 * Applies the appearance settings that aren't the theme:
 * - the accent color, as `data-accent` (colors in index.css),
 * - the UI size: Tailwind sizes are in `rem`, so the root font size scales the whole UI,
 * - the editor font size, as `--editor-font-size` (used by the code editors).
 */
export function applyAppearance(
  prefs: Pick<Preferences, "accent" | "uiScale" | "editorFontSize">,
  root: HTMLElement = document.documentElement,
) {
  root.dataset.accent = prefs.accent;
  root.style.fontSize = prefs.uiScale === 100 ? "" : `${prefs.uiScale}%`;
  root.style.setProperty("--editor-font-size", `${prefs.editorFontSize}px`);
}
