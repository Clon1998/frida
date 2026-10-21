import { describe, expect, it } from "vitest";
import { expiryLabel, timeAgo } from "./time";

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

describe("expiryLabel", () => {
  const now = Date.parse("2026-10-15T12:00:00.000Z");
  it("says how long a token is valid, or since when it isn't", () => {
    expect(expiryLabel(null, now)).toBe("no expiry");
    expect(expiryLabel("2026-10-15T12:54:00.000Z", now)).toBe("expires in 54 min");
    expect(expiryLabel("2026-10-15T12:00:20.000Z", now)).toBe("expires in 1 min");
    expect(expiryLabel("2026-10-15T14:00:00.000Z", now)).toBe("expires in 2 h");
    expect(expiryLabel("2026-10-15T11:57:00.000Z", now)).toBe("expired 3 min ago");
  });
});
