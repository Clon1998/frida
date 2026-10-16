import { describe, expect, it } from "vitest";
import type { ConsoleEntry } from "@/bindings";
import { rawRequest, rawResponse } from "./rawHttp";

const entry: ConsoleEntry = {
  id: "1",
  timestamp: "2026-10-16T10:00:00.000Z",
  requestName: "Create",
  requestPath: "create.yaml",
  method: "POST",
  url: "https://api.example.com/orders",
  requestHeaders: [{ name: "Content-Type", value: "application/json", enabled: true }],
  requestBody: { text: '{"item":"book"}', truncated: false },
  response: {
    httpVersion: "HTTP/1.1",
    status: 201,
    statusText: "Created",
    headers: [{ name: "content-type", value: "application/json", enabled: true }],
    body: { text: '{"id":1}', truncated: true },
    timeMs: 12,
    sizeBytes: 8,
    finalUrl: "https://api.example.com/orders",
  },
  error: null,
};

describe("raw HTTP formatting", () => {
  it("formats request and response like an HTTP message", () => {
    expect(rawRequest(entry)).toBe(
      'POST https://api.example.com/orders\nContent-Type: application/json\n\n{"item":"book"}',
    );
    expect(rawResponse(entry)).toBe(
      'HTTP/1.1 201 Created\ncontent-type: application/json\n\n{"id":1}\n… (truncated)',
    );
  });

  it("shows the error when there is no response", () => {
    expect(rawResponse({ ...entry, response: null, error: "connection refused" })).toBe(
      "Error: connection refused",
    );
  });
});
