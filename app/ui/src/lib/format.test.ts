import { describe, expect, it } from "vitest";
import { bodyKind, formatBytes, formatDuration, prettyJson } from "./format";

describe("format helpers", () => {
  it("formats sizes and durations", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatDuration(850)).toBe("850 ms");
    expect(formatDuration(2300)).toBe("2.30 s");
  });

  it("pretty-prints JSON and leaves other text alone", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("not json")).toBe("not json");
  });

  it("detects body kinds", () => {
    expect(bodyKind("application/vnd.api+json", "{}")).toBe("json");
    expect(bodyKind("image/png", null)).toBe("image");
    expect(bodyKind("text/html; charset=utf-8", "<html>")).toBe("html");
    expect(bodyKind(null, "  [1,2]")).toBe("json");
    expect(bodyKind(null, "<a/>")).toBe("xml");
    expect(bodyKind("application/octet-stream", null)).toBe("binary");
  });
});
