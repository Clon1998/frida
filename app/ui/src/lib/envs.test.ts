import { describe, expect, it } from "vitest";
import type { EnvModel, EnvVar } from "@/stores/envs";
import { forDisk, secretOps } from "./envs";

const v = (name: string, value: string | null, secret = false): EnvVar => ({
  name,
  value,
  secret,
  source: null,
  enabled: true,
});
const env = (...variables: EnvVar[]): EnvModel => ({
  version: 1,
  name: "dev",
  color: null,
  variables,
});

describe("secretOps", () => {
  it("writes newly typed secret values only", () => {
    const draft = env(v("token", null, true), v("other", null, true), v("plain", "x"));
    expect(secretOps(draft, draft, { token: "s3cr3t", plain: "ignored" })).toEqual({
      set: [{ variable: "token", value: "s3cr3t" }],
      delete: [],
    });
  });

  it("deletes secrets that were removed, renamed or made plain", () => {
    const original = env(v("a", null, true), v("b", null, true), v("c", null, true));
    const draft = env(v("a", null, true), v("b-renamed", null, true), v("c", "now plain"));
    expect(secretOps(original, draft, {}).delete).toEqual(["b", "c"]);
  });
});

describe("forDisk", () => {
  it("never keeps secret values", () => {
    const out = forDisk(env(v("token", "s3cr3t", true), v("plain", "x")));
    expect(out.variables).toEqual([v("token", null, true), v("plain", "x")]);
  });
});
