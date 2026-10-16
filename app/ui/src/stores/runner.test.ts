import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { RunProgress } from "@/bindings";
import { runKey, useRunner } from "./runner";

const item = (name: string) => ({ path: `${name}.yaml`, name, method: "GET" });
const progress = (runId: string, step: number): RunProgress => ({
  runId,
  result: {
    step,
    item: item(`r${step}`),
    outcome: "passed",
    status: 200,
    timeMs: 5,
    testsPassed: 0,
    testsTotal: 0,
    message: null,
  },
  response: null,
});

describe("runner store", () => {
  let finish: (() => void) | null = null;

  beforeEach(() => {
    useRunner.setState({ runs: {}, delayMs: 0 });
    mockIPC((cmd) => {
      if (cmd === "plan_run") return [item("r0"), item("r1")];
      if (cmd === "run_collection") {
        // Stays "running" until the test finishes it.
        return new Promise((resolve) => {
          finish = () =>
            resolve({ passed: 2, failed: 0, errors: 0, skipped: 0, durationMs: 9, stopped: null });
        });
      }
      return null;
    });
  });
  afterEach(() => clearMocks());

  it("collects the progress of its own run and follows the newest result", async () => {
    const key = runKey("/col", "orders");
    const started = useRunner.getState().start("/col", "orders");
    await new Promise((r) => setTimeout(r, 0));
    const runId = useRunner.getState().runs[key]?.runId;
    expect(runId).toBeTruthy();

    useRunner.getState().addProgress(progress("someone else's run", 0));
    useRunner.getState().addProgress(progress(runId as string, 0));
    useRunner.getState().addProgress(progress(runId as string, 1));
    let run = useRunner.getState().runs[key];
    expect(run?.results.map((p) => p.result.step)).toEqual([0, 1]);
    expect(run?.selected).toBe(1);

    // A result the user picked stays selected while the run goes on.
    useRunner.getState().select("/col", "orders", 0);
    useRunner.getState().addProgress(progress(runId as string, 2));
    expect(useRunner.getState().runs[key]?.selected).toBe(0);

    finish?.();
    await started;
    run = useRunner.getState().runs[key];
    expect(run?.runId).toBeNull();
    expect(run?.summary?.passed).toBe(2);
    expect(run?.items).toHaveLength(2);
  });
});
