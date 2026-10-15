const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Keeps the `.dark` class on `root` in sync with the OS color scheme.
 * Returns a function that stops listening. A manual override comes in a later milestone.
 */
export function followSystemTheme(root: HTMLElement = document.documentElement): () => void {
  const media = window.matchMedia(DARK_QUERY);
  const apply = () => root.classList.toggle("dark", media.matches);
  apply();
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}
