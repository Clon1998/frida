import { describe, expect, it } from "vitest";
import type { AppSettings } from "@/bindings";
import { folderName, SHOWN_RECENT, workspaceLists } from "./workspaces";

const settings = (recent: string[], pinned: string[]) =>
  ({ recentWorkspaces: recent, pinnedWorkspaces: pinned }) as unknown as AppSettings;

describe("workspace lists", () => {
  it("shows pinned workspaces first and only a few recent ones", () => {
    const recent = Array.from({ length: 10 }, (_, i) => `/ws${i}`);
    const lists = workspaceLists(settings(recent, ["/ws3", "/other"]), "/ws0");
    expect(lists.pinned).toEqual(["/ws3", "/other"]);
    expect(lists.recent).toHaveLength(SHOWN_RECENT);
    expect(lists.recent).not.toContain("/ws3");
    expect(lists.recent).not.toContain("/ws0");
    expect(lists.recent[0]).toBe("/ws1");
  });

  it("copes with no settings yet", () => {
    expect(workspaceLists(null)).toEqual({ pinned: [], recent: [] });
    expect(folderName("C:\\work\\eip\\")).toBe("eip");
  });
});
