import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { HistorySummary } from "@/bindings";
import { useHistory } from "./history";

const entry = (id: number): HistorySummary => ({
  id,
  timestamp: "2026-10-16T00:00:00Z",
  collectionId: "c",
  requestPath: "r.yaml",
  requestName: `r${id}`,
  method: "GET",
  url: "/",
  status: 200,
  timeMs: 1,
  error: null,
});

describe("history store", () => {
  const replies: ((list: HistorySummary[]) => void)[] = [];

  beforeEach(() => {
    useHistory.setState({ entries: {}, searches: {}, loading: {} });
    mockIPC((cmd) => {
      if (cmd === "list_history") return new Promise((resolve) => replies.push(resolve));
    });
  });
  afterEach(() => {
    clearMocks();
    replies.length = 0;
  });

  it("keeps the list across loads and ignores a reply that arrives after a newer one", async () => {
    const first = useHistory.getState().load("/c", "");
    const second = useHistory.getState().load("/c", "get");
    await Promise.resolve();
    expect(useHistory.getState().loading["/c"]).toBe(true);

    replies[1]?.([entry(2)]);
    await second;
    replies[0]?.([entry(1)]);
    await first;

    const s = useHistory.getState();
    expect(s.entries["/c"]?.map((e) => e.id)).toEqual([2]);
    expect(s.searches["/c"]).toBe("get");
    expect(s.loading["/c"]).toBe(false);
  });
});
