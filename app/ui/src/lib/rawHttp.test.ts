import { describe, expect, it } from "vitest";
import type { ConsoleEntry } from "@/bindings";
import { prettyBody, rawExchange, rawResponse, requestTarget } from "./rawHttp";

const entry: ConsoleEntry = {
  id: "1",
  timestamp: "2026-10-16T10:00:00.000Z",
  requestName: "Create",
  requestPath: "create.yaml",
  method: "POST",
  url: "https://api.example.com/orders?draft=1",
  requestHeaders: [
    { name: "host", value: "api.example.com", enabled: true },
    { name: "content-type", value: "application/json", enabled: true },
  ],
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
  it("formats the exchange like Postman's raw console view", () => {
    expect(rawExchange(entry)).toBe(
      [
        "POST /orders?draft=1 HTTP/1.1",
        "host: api.example.com",
        "content-type: application/json",
        "",
        '{"item":"book"}',
        "",
        "HTTP/1.1 201 Created",
        "content-type: application/json",
        "",
        '{"id":1}',
        "… (truncated)",
      ].join("\n"),
    );
  });

  it("shows the error when there is no response", () => {
    expect(rawResponse({ ...entry, response: null, error: "connection refused" })).toBe(
      "Error: connection refused",
    );
  });

  it("keeps unparsable URLs (e.g. with placeholders) as they are", () => {
    expect(requestTarget("{{base}}/orders")).toBe("{{base}}/orders");
    expect(requestTarget("http://x:8787/")).toBe("/");
  });

  it("indents JSON bodies only", () => {
    expect(prettyBody('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyBody("plain {text}")).toBe("plain {text}");
    expect(prettyBody("{broken")).toBe("{broken");
  });
});
