/** Working with tabs, the collection runner, Frida archives and curl. */
import { backend, expect, openRequest, openWorkspace, test, urlBar } from "./fixtures";

const tab = (page: import("@playwright/test").Page, path: string) =>
  page.locator(`[data-tab-path='${path}']`);

test("preview tabs, tab menu actions and tabs restored with the workspace", async ({ app }) => {
  await openWorkspace(app);
  const tree = app.locator("aside");

  // A single click opens a preview tab (italic) that the next single click replaces.
  await tree.getByText("Health", { exact: true }).click();
  await expect(tab(app, "health.yaml").locator(".italic")).toBeVisible();
  await app.getByLabel("Expand folder").click();
  await tree.getByText("Get order", { exact: true }).click();
  await expect(tab(app, "health.yaml")).toHaveCount(0);
  await expect(tab(app, "orders/get-order.yaml").locator(".italic")).toBeVisible();

  // Double-click keeps it.
  await tree.getByText("Get order", { exact: true }).dblclick();
  await expect(tab(app, "orders/get-order.yaml").locator(".italic")).toHaveCount(0);
  await openRequest(app, "Health");
  await openRequest(app, "Create order");
  await expect(app.locator("[data-tab-path]")).toHaveCount(3);

  await tab(app, "orders/get-order.yaml").click({ button: "right" });
  await app.getByRole("menuitem", { name: "Close to the right" }).click();
  await expect(app.locator("[data-tab-path]")).toHaveCount(1);
  await openRequest(app, "Health");

  // Closing and reopening the workspace brings the tabs back.
  await app.getByRole("button", { name: "Fake workspace" }).click();
  await app.getByRole("menuitem", { name: "Close workspace" }).click();
  await openWorkspace(app);
  await expect(tab(app, "orders/get-order.yaml")).toBeVisible();
  await expect(tab(app, "health.yaml")).toBeVisible();
});

test("run a folder, then stop a collection run", async ({ app }) => {
  await openWorkspace(app);
  await app.locator("aside").getByText("Orders", { exact: true }).click({ button: "right" });
  await app.getByRole("menuitem", { name: "Run folder…" }).click();

  await app.getByRole("button", { name: "Run 2 requests" }).click();
  await expect(app.getByText("2 passed")).toBeVisible();
  await expect(app.getByRole("main").getByLabel("passed")).toHaveCount(2);

  // The whole collection (3 requests), stopped after the first result.
  // The tree's empty space has the collection's menu.
  await app.getByTestId("request-tree").click({ button: "right", position: { x: 40, y: 300 } });
  await app.getByRole("menuitem", { name: "Run collection…" }).click();
  await app.getByRole("button", { name: "Run 3 requests" }).click();
  // Stop once the first result is in (one request every 400 ms in the fake).
  await expect(app.getByRole("main").getByLabel("passed").first()).toBeVisible();
  await app.getByRole("button", { name: "Stop" }).click();
  await expect(app.getByText(/stopped/)).toBeVisible();
  await expect(app.getByRole("main").getByLabel("passed")).not.toHaveCount(3);
});

test("import a Frida archive into a folder", async ({ app }) => {
  await openWorkspace(app);
  await app.locator("aside").getByText("Orders", { exact: true }).click({ button: "right" });
  await app.getByRole("menuitem", { name: "Import Frida archive here…" }).click();

  const dialog = app.getByRole("dialog");
  await dialog.getByRole("button", { name: "Choose archive…" }).click();
  await expect(dialog).toContainText("Imported request");
  await expect(dialog).toContainText('into the folder "orders"');
  await dialog.getByRole("button", { name: "Import" }).click();

  await expect(dialog).toHaveCount(0);
  // A request archive opens the new request.
  await expect(tab(app, "orders/imported-request.yaml")).toBeVisible();
  await expect(app.locator("aside").getByText("Imported request")).toBeVisible();
});

test("new request from curl, then copy it as curl", async ({ app }) => {
  await openWorkspace(app);
  await app.getByLabel("Add request or folder").first().click();
  await app.getByRole("menuitem", { name: "New request from curl…" }).click();

  const dialog = app.getByRole("dialog");
  await dialog
    .getByRole("textbox")
    .first()
    .fill(
      "curl -X POST 'https://api.example.com/pets' -H 'Accept: application/json' -d '{\"name\":\"Rex\"}'",
    );
  await expect(dialog).toContainText("POST https://api.example.com/pets");
  await dialog.getByLabel("Name").fill("Create pet");
  await dialog.getByRole("button", { name: "Create request" }).click();

  await expect(urlBar(app)).toHaveText("https://api.example.com/pets");
  const created = (await backend(app)).requests["create-pet.yaml"];
  expect(created).toMatchObject({ method: "POST", name: "Create pet" });
  expect(created.headers[0]).toMatchObject({ name: "Accept", value: "application/json" });

  await app.getByRole("button", { name: "Copy as curl" }).click();
  await expect(app.getByRole("dialog")).toContainText(
    "curl -X POST 'https://api.example.com/pets'",
  );
});
