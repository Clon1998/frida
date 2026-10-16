import { describe, expect, it } from "vitest";
import { extensionFor, suggestedFileName } from "./fileName";

describe("response file names", () => {
  it("maps content types, ignoring parameters and case", () => {
    expect(extensionFor("application/json; charset=utf-8")).toBe("json");
    expect(extensionFor("Image/PNG")).toBe("png");
    expect(extensionFor("application/vnd.github+json")).toBe("json");
    expect(extensionFor("text/x-unknown")).toBe("txt");
    expect(extensionFor("application/x-unknown")).toBeNull();
    expect(extensionFor(null)).toBeNull();
  });

  it("prefers the Content-Disposition name, without any folder", () => {
    const h = (value: string) => [{ name: "Content-Disposition", value }];
    expect(suggestedFileName("application/pdf", h('attachment; filename="report.pdf"'))).toBe(
      "report.pdf",
    );
    expect(suggestedFileName(null, h("attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"))).toBe(
      "résumé.pdf",
    );
    expect(suggestedFileName(null, h('attachment; filename="../../etc/passwd"'))).toBe("passwd");
    expect(suggestedFileName("text/csv", h("inline"))).toBe("response.csv");
    expect(suggestedFileName("application/x-unknown", [])).toBe("response");
  });
});
