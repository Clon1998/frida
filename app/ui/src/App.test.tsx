import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

describe("App", () => {
  beforeEach(() => {
    vi.stubGlobal("matchMedia", () => ({
      matches: false,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
    // Pretend to be the Rust side of the `get_about_info` command.
    mockIPC((cmd) => {
      if (cmd === "get_about_info") return { productName: "Frida", coreVersion: "9.9.9" };
    });
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.unstubAllGlobals();
  });

  it("shows the core version returned by the backend", async () => {
    render(<App />);
    const about = await screen.findByTestId("about");
    expect(about.textContent).toBe("Frida core v9.9.9");
  });
});
