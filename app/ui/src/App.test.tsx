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
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.unstubAllGlobals();
  });

  it("shows the start screen with recent workspaces when none is open", async () => {
    // Pretend to be the Rust side of the `get_settings` command.
    mockIPC((cmd) => {
      if (cmd === "get_settings") return { recentWorkspaces: ["/work/eip"], theme: "system" };
    });
    render(<App />);
    expect(screen.getByText("New workspace…")).toBeTruthy();
    expect(await screen.findByText("/work/eip")).toBeTruthy();
  });

  it("renders before settings have loaded", () => {
    mockIPC(() => new Promise(() => {})); // never resolves
    render(<App />);
    expect(screen.getByText("Open workspace…")).toBeTruthy();
  });
});
