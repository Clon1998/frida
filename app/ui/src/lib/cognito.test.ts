import { describe, expect, it } from "vitest";
import { cognitoUrls, parseCognito } from "./cognito";

describe("Cognito preset", () => {
  it("builds the hosted UI endpoints from a domain prefix and region", () => {
    expect(cognitoUrls({ domain: "my-app", region: "eu-central-1" })).toEqual({
      tokenUrl: "https://my-app.auth.eu-central-1.amazoncognito.com/oauth2/token",
      authUrl: "https://my-app.auth.eu-central-1.amazoncognito.com/oauth2/authorize",
    });
  });

  it("supports custom domains", () => {
    expect(cognitoUrls({ domain: "auth.example.com", region: "" }).tokenUrl).toBe(
      "https://auth.example.com/oauth2/token",
    );
  });

  it("reads domain and region back", () => {
    expect(parseCognito("https://my-app.auth.eu-central-1.amazoncognito.com/oauth2/token")).toEqual(
      { domain: "my-app", region: "eu-central-1" },
    );
    expect(parseCognito("https://auth.example.com/oauth2/token").domain).toBe("auth.example.com");
    expect(parseCognito("https://other/token")).toEqual({ domain: "", region: "" });
  });
});
