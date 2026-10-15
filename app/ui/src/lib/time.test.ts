import { describe, expect, it } from "vitest";
import { timeAgo } from "./time";

describe("timeAgo", () => {
  const now = Date.parse("2026-10-15T12:00:00.000Z");
  it("formats relative times", () => {
    expect(timeAgo("2026-10-15T11:59:40.000Z", now)).toBe("just now");
    expect(timeAgo("2026-10-15T11:55:00.000Z", now)).toBe("5 min ago");
    expect(timeAgo("2026-10-15T09:00:00.000Z", now)).toBe("3 h ago");
    expect(timeAgo("2026-10-13T12:00:00.000Z", now)).toBe("2 d ago");
    expect(timeAgo("2026-09-01T12:00:00.000Z", now)).toBe("2026-09-01");
  });
});
