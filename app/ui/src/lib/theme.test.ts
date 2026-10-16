import { afterEach, describe, expect, it, vi } from "vitest";
import { applyAppearance, applyTheme, followSystemTheme } from "./theme";

function mockMatchMedia(matches: boolean) {
  const listeners: (() => void)[] = [];
  const media = {
    matches,
    addEventListener: (_: string, fn: () => void) => listeners.push(fn),
    removeEventListener: vi.fn(),
  };
  vi.stubGlobal("matchMedia", () => media);
  return {
    media,
    change(next: boolean) {
      media.matches = next;
      listeners.forEach((fn) => fn());
    },
  };
}

describe("followSystemTheme", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("applies the current OS scheme and follows changes", () => {
    const os = mockMatchMedia(true);
    const root = document.createElement("html");

    followSystemTheme(root);
    expect(root.classList.contains("dark")).toBe(true);

    os.change(false);
    expect(root.classList.contains("dark")).toBe(false);
  });

  it("stops listening when disposed", () => {
    const os = mockMatchMedia(false);
    const stop = followSystemTheme(document.createElement("html"));
    stop();
    expect(os.media.removeEventListener).toHaveBeenCalledOnce();
  });
});

describe("theme and appearance settings", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("fixes light or dark, and follows the OS for system", () => {
    mockMatchMedia(true);
    const root = document.createElement("html");
    applyTheme("light", root);
    expect(root.classList.contains("dark")).toBe(false);
    applyTheme("dark", root);
    expect(root.classList.contains("dark")).toBe(true);
    applyTheme("light", root);
    applyTheme("system", root);
    expect(root.classList.contains("dark")).toBe(true);
  });

  it("sets the accent, UI size and editor font size on the root", () => {
    const root = document.createElement("html");
    applyAppearance({ accent: "blue", uiScale: 125, editorFontSize: 15 }, root);
    expect(root.dataset.accent).toBe("blue");
    expect(root.style.fontSize).toBe("125%");
    expect(root.style.getPropertyValue("--editor-font-size")).toBe("15px");

    applyAppearance({ accent: "neutral", uiScale: 100, editorFontSize: 13 }, root);
    expect(root.style.fontSize).toBe("");
  });
});
