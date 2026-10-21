/** OAuth2 in the auth editor (feedback round 4) and renaming a request from its tab. */
import {
  backend,
  expect,
  openRequest,
  openWorkspace,
  save,
  test,
  useEnvironment,
} from "./fixtures";

const DEV_TOKEN_URL = "https://login.dev.example.com/oauth2/token";

test("Get new token uses the auth as edited; the token list shows where tokens come from", async ({
  app,
  browserName,
}) => {
  if (browserName === "chromium") {
    await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  }
  await openWorkspace(app);
  await useEnvironment(app, "local");
  await app.getByRole("button", { name: "Collection settings" }).click();
  await app.getByRole("tab", { name: "Auth" }).click();
  const panel = app.getByRole("tabpanel");
  await panel.getByRole("combobox").first().click();
  await app.getByRole("option", { name: "OAuth 2.0" }).click();
  await panel.getByRole("group", { name: "Token URL" }).locator(".cm-content").click();
  await app.keyboard.type(DEV_TOKEN_URL);

  // Not saved yet: the token request still uses what was typed.
  await panel.getByRole("button", { name: "Get new token" }).click();
  await expect(app.getByText("New token received")).toBeVisible();
  const fetched = (await backend(app)).lastTokenFetch;
  expect(fetched.path).toBe("");
  expect(fetched.auth.schemes[0].tokenUrl).toBe(DEV_TOKEN_URL);

  // The collection's token first, with its environment and token URL; others are folded away.
  const cards = panel.getByTestId("oauth-token");
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText("local");
  await expect(cards.first()).toContainText(DEV_TOKEN_URL);
  await expect(cards.first()).toContainText(/expires in/);
  await panel.getByRole("button", { name: "Other tokens (1)" }).click();
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(1)).toContainText("prod");

  // Decoding is an explicit button, not a click on the cut-off token.
  await cards.first().getByRole("button", { name: "Decode" }).click();
  await expect(cards.first().getByLabel("JWT claims")).toContainText('"sub": "frida"');

  if (browserName === "chromium") {
    await cards.first().getByRole("button", { name: "Copy" }).click();
    await expect(app.getByText("Token copied")).toBeVisible();
    expect(await app.evaluate<string>("navigator.clipboard.readText()")).toMatch(/^token-\d+-/);
  }
});

test("renaming a request in its tab is an unsaved change until saved", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await app.getByTitle(/health\.yaml · click to rename/).click();
  await app.keyboard.press("ControlOrMeta+a");
  await app.keyboard.type("Health check");
  await app.keyboard.press("Enter");

  // Only the tab changed: it is unsaved, the file is untouched.
  await expect(
    app.locator("main").getByRole("button", { name: "Save", exact: true }),
  ).toBeVisible();
  let requests = (await backend(app)).requests;
  expect(requests["health.yaml"].name).toBe("Health");

  await save(app);
  requests = (await backend(app)).requests;
  expect(requests["health.yaml"]).toBeUndefined();
  const renamed = Object.entries(requests).find(
    ([, r]) => (r as { name: string }).name === "Health check",
  );
  expect(renamed?.[0]).toMatch(/health-check\.yaml$/);
  await expect(app.locator(`[data-tab-path='${renamed?.[0]}']`)).toBeVisible();
  await expect(app.locator("aside").getByText("Health check", { exact: true })).toBeVisible();
});
