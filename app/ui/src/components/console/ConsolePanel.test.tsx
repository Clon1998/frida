import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ConsoleEntry } from "@/bindings";
import { useConsole } from "@/stores/console";
import { ConsolePanel } from "./ConsolePanel";

const entry: ConsoleEntry = {
  id: "1",
  timestamp: "2026-10-16T10:00:00.000Z",
  requestName: "List webcams",
  requestPath: "webcams.yaml",
  method: "GET",
  url: "http://192.168.20.32/server/webcams/list",
  requestHeaders: [
    { name: "host", value: "192.168.20.32", enabled: true },
    { name: "user-agent", value: "frida-http/0.1.0", enabled: true },
  ],
  requestBody: null,
  response: {
    httpVersion: "HTTP/1.1",
    status: 200,
    statusText: "OK",
    headers: [{ name: "content-encoding", value: "gzip", enabled: true }],
    body: { text: '{"result":{}}', truncated: false },
    timeMs: 12,
    sizeBytes: 13,
    finalUrl: "http://192.168.20.32/server/webcams/list",
  },
  error: null,
};

describe("ConsolePanel", () => {
  afterEach(() => {
    cleanup();
    useConsole.setState({ items: [], view: "pretty" });
  });

  it("expands an entry to sections, or to the raw exchange", () => {
    useConsole.setState({ items: [{ kind: "http", id: entry.id, entry }], view: "pretty" });
    render(<ConsolePanel />);
    fireEvent.click(screen.getByText(entry.url));
    expect(screen.getByText("Request Headers")).toBeTruthy();
    expect(screen.getByText("content-encoding")).toBeTruthy();

    fireEvent.click(screen.getByRole("radio", { name: "raw" }));
    const raw = document.querySelector("pre")?.textContent ?? "";
    expect(raw).toContain("GET /server/webcams/list HTTP/1.1\nhost: 192.168.20.32\n");
    expect(raw).toContain("HTTP/1.1 200 OK\ncontent-encoding: gzip\n\n{");
  });
});

describe("ConsolePanel script logs", () => {
  afterEach(() => {
    cleanup();
    useConsole.setState({ items: [] });
  });

  it("shows script logs between the sends", () => {
    useConsole.getState().addLog({
      timestamp: "2026-10-16T10:00:00.000Z",
      level: "warn",
      message: "token expired",
      location: 'request "Login", post-response script',
      requestName: "Login",
    });
    render(<ConsolePanel />);
    expect(screen.getByText("token expired")).toBeTruthy();
    expect(screen.getByText('request "Login", post-response script')).toBeTruthy();
  });
});
