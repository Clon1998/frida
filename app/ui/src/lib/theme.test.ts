import { afterEach, describe, expect, it, vi } from "vitest";
import { followSystemTheme } from "./theme";

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
