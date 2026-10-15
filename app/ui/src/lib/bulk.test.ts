import { describe, expect, it } from "vitest";
import { bulkToRows, rowsToBulk } from "./bulk";

describe("bulk edit", () => {
  it("formats rows as lines, disabled ones commented out", () => {
    expect(
      rowsToBulk([
        { name: "Accept", value: "application/json", enabled: true },
        { name: "X-Debug", value: "1", enabled: false },
      ]),
    ).toBe("Accept: application/json\n//X-Debug: 1");
  });

  it("parses lines, splitting only on the first colon", () => {
    expect(bulkToRows("Referer: https://x.dev:8443/a\n\n// X-Off : yes\nflag\n:novalue")).toEqual([
      { name: "Referer", value: "https://x.dev:8443/a", enabled: true },
      { name: "X-Off", value: "yes", enabled: false },
      { name: "flag", value: "", enabled: true },
    ]);
  });

  it("round-trips", () => {
    const rows = [
      { name: "a", value: "{{var}}", enabled: true },
      { name: "b", value: "", enabled: false },
    ];
    expect(bulkToRows(rowsToBulk(rows))).toEqual(rows);
  });
});
