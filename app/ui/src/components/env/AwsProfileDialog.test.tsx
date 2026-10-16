import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { RequestModel } from "@/lib/model";
import { useDialogs } from "@/stores/dialogs";
import { useTabs } from "@/stores/tabs";
import { AwsProfileDialog } from "./AwsProfileDialog";

const request: RequestModel = {
  version: 1,
  name: "Get secret thing",
  seq: 1,
  method: "GET",
  url: "{{baseUrl}}/x",
  params: { query: [], path: [] },
  headers: [],
  body: {
    mode: "none",
    json: "",
    xml: "",
    text: "",
    formUrlEncoded: [],
    multipart: [],
    binaryPath: "",
  },
  auth: { mode: "inherit" },
  vars: [],
  scripts: { pre: "", post: "" },
  tests: "",
  settings: { timeoutMs: null },
  docs: "",
};

describe("AWS profile prompt (PLAN.md §15, M8)", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    useDialogs.setState({ awsProfile: null });
    useTabs.setState({ byCollection: {} });
  });

  it("asks for a profile on first use, saves it and sends again", async () => {
    let profile: string | null = null;
    const calls: string[] = [];
    mockIPC((cmd, args) => {
      calls.push(cmd);
      if (cmd === "read_request") return request;
      if (cmd === "send_request") {
        if (profile === null) {
          throw {
            code: "aws_profile_required",
            message: "choose an AWS profile",
            hint: null,
            aws: { environment: "prod", global: false, fixCommand: null },
          };
        }
        return null;
      }
      if (cmd === "list_aws_profiles") return [{ name: "work", sso: true, region: "eu-west-1" }];
      if (cmd === "set_aws_profile") {
        profile = (args as { profile: string }).profile;
        return null;
      }
      return null;
    });

    await useTabs.getState().openTab("/col", "get.yaml");
    await useTabs.getState().send("/col", "get.yaml");
    expect(useDialogs.getState().awsProfile?.environment).toBe("prod");

    render(<AwsProfileDialog />);
    fireEvent.click(await screen.findByText("work"));
    fireEvent.click(screen.getByText("Use profile and send"));

    await waitFor(() => expect(profile).toBe("work"));
    await waitFor(() => expect(calls.filter((c) => c === "send_request")).toHaveLength(2));
    expect(useDialogs.getState().awsProfile).toBeNull();
  });
});
