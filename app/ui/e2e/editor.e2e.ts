/**
 * The request editor: params, headers, body, variables and auth, and that edits survive saving
 * and reopening. Plus secrets in the environment editor.
 */
import {
  backend,
  expect,
  openRequest,
  openWorkspace,
  save,
  setUrl,
  test,
  urlBar,
  useEnvironment,
} from "./fixtures";

test("params, headers and body survive saving and reopening", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Get order");
  const panel = app.getByRole("tabpanel");

  // URL → params: the query and the `:orderId` path param show up in the tables.
  await setUrl(app, "{{baseUrl}}/orders/:orderId?expand=items");
  await expect(panel.locator('input[value="expand"]')).toBeVisible();
  const pathRow = panel.locator("tr", { has: app.locator('input[value="orderId"]') });
  await pathRow.locator(".cm-content").click();
  await app.keyboard.type("42");

  // Params → URL: a new query row is added to the URL.
  await panel.getByPlaceholder(/^Add /).first().click();
  await app.keyboard.type("limit");
  await expect(urlBar(app)).toHaveText("{{baseUrl}}/orders/:orderId?expand=items&limit");

  await app.getByRole("tab", { name: "Headers" }).click();
  await panel.getByPlaceholder(/^Add /).click();
  await app.keyboard.type("X-Trace");
  await panel
    .locator("tr", { has: app.locator('input[value="X-Trace"]') })
    .locator(".cm-content")
    .click();
  await app.keyboard.type("abc");

  await app.getByRole("tab", { name: "Body" }).click();
  await panel.getByRole("combobox").click();
  await app.getByRole("option", { name: "JSON" }).click();
  await panel.locator(".cm-content").click();
  await app.keyboard.type('{"amount":1');

  await save(app);
  const saved = (await backend(app)).requests["orders/get-order.yaml"];
  expect(saved.url).toBe("{{baseUrl}}/orders/:orderId?expand=items&limit");
  expect(saved.params.path).toEqual([{ name: "orderId", value: "42" }]);
  expect(saved.headers[0]).toMatchObject({ name: "X-Trace", value: "abc", enabled: true });
  expect(saved.body.mode).toBe("json");
  expect(JSON.parse(saved.body.json)).toEqual({ amount: 1 });

  // Reopened from the backend, the editor shows the same.
  await app.locator("[data-tab-path='orders/get-order.yaml']").getByLabel("Close tab").click();
  await openRequest(app, "Get order");
  await expect(urlBar(app)).toHaveText("{{baseUrl}}/orders/:orderId?expand=items&limit");
  await app.getByRole("tab", { name: "Headers" }).click();
  await expect(panel.locator('input[value="X-Trace"]')).toBeVisible();
});

test("variables are highlighted, explained on hover and completed with Tab", async ({ app }) => {
  await openWorkspace(app);
  await useEnvironment(app, "local");
  await openRequest(app, "Health");

  // Known variables are green, unknown ones red.
  await expect(urlBar(app).locator(".cm-var-resolved")).toHaveText("{{baseUrl}}");
  await setUrl(app, "{{baseUrl}}/{{nope}}");
  await expect(urlBar(app).locator(".cm-var-unresolved")).toHaveText("{{nope}}");

  // Hovering shows the value.
  await urlBar(app).locator(".cm-var-resolved").hover();
  await expect(app.locator(".cm-var-tooltip")).toContainText("http://localhost:8080");

  // Feedback #8: Tab accepts the highlighted suggestion.
  await setUrl(app, "{{bas");
  await expect(app.locator(".cm-tooltip-autocomplete")).toContainText("baseUrl");
  await app.keyboard.press("Tab");
  await expect(urlBar(app)).toHaveText("{{baseUrl}}");
});

test("OAuth2 plus an API key is saved as the request's auth", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await app.getByRole("tab", { name: "Auth" }).click();
  const panel = app.getByRole("tabpanel");

  await panel.getByRole("combobox").first().click();
  await app.getByRole("option", { name: "OAuth 2.0" }).click();
  await panel.getByRole("group", { name: "Token URL" }).locator(".cm-content").click();
  await app.keyboard.type("https://auth.example.com/token");
  await panel.getByRole("button", { name: "Add API key" }).click();
  const apiKey = panel.locator("section", { hasText: "Additional API key" });
  await apiKey.getByRole("group", { name: "Name" }).locator(".cm-content, input").first().click();
  await app.keyboard.press("ControlOrMeta+a");
  await app.keyboard.type("X-Api-Key");

  await save(app);
  const auth = (await backend(app)).requests["health.yaml"].auth;
  expect(auth.mode).toBe("custom");
  expect(auth.schemes.map((s: { type: string }) => s.type)).toEqual(["oauth2", "apikey"]);
  expect(auth.schemes[0].tokenUrl).toBe("https://auth.example.com/token");
  expect(auth.schemes[1].name).toBe("X-Api-Key");
});

test("a secret's value goes to the keychain, never into the environment file", async ({ app }) => {
  await openWorkspace(app);
  await app.getByTitle("Environments", { exact: true }).click();
  await app.getByRole("menuitem", { name: /Configure collection environments/ }).click();
  await app.getByRole("tab", { name: /^Secrets/ }).click();

  await app.getByPlaceholder("Add secret").click();
  await app.keyboard.type("apiToken");
  const value = app
    .locator("tr", { has: app.locator('input[value="apiToken"]') })
    .locator('input[type="password"]');
  await value.click();
  await app.keyboard.type("s3cr3t");
  const saveButton = app.getByRole("main").getByRole("button", { name: "Save", exact: true });
  await saveButton.click();
  await expect(saveButton).toBeDisabled();

  const state = await backend(app);
  const variable = state.environments[0].variables.find(
    (v: { name: string }) => v.name === "apiToken",
  );
  expect(variable).toMatchObject({ secret: true, value: null });
  expect(state.secrets["local/apiToken"]).toBe("s3cr3t");

  // Stored: masked, and it can be revealed.
  await app.getByRole("button", { name: "Reveal value" }).click();
  await expect(
    app
      .locator("tr", { has: app.locator('input[value="apiToken"]') })
      .locator('input[type="text"]'),
  ).toHaveValue("s3cr3t");
});
