/**
 * The core UI flows (PLAN.md M12), against the fake backend. Several of them pin down bugs from
 * the second feedback round (the comments say which).
 */
import { expect, openRequest, openWorkspace, setUrl, test, useEnvironment } from "./fixtures";

test("send a request with an environment and find it in the history", async ({ app }) => {
  await openWorkspace(app);
  await useEnvironment(app, "local");
  await openRequest(app, "Get order");
  await setUrl(app, "{{baseUrl}}/orders/7");
  await app.getByRole("button", { name: "Send" }).click();

  await expect(app.getByText("200 OK")).toBeVisible();
  await expect(app.getByText('"http://localhost:8080/orders/7"')).toBeVisible();

  await app.getByRole("tab", { name: "History" }).click();
  await expect(app.getByText("http://localhost:8080/orders/7")).toBeVisible();
});

test("find bar counts matches in the response body", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await app.getByRole("button", { name: "Send" }).click();
  await expect(app.getByText("200 OK")).toBeVisible();

  // The response body is the last editor on the page; its search button opens the find bar.
  await app.getByRole("button", { name: "Find" }).last().click();
  await app.keyboard.type("e");
  await expect(app.locator(".cm-frida-search-count")).toHaveText(/^\d+ of \d+$/);
  await app.keyboard.press("Escape");
  await expect(app.locator(".cm-frida-search")).toHaveCount(0);
});

test("create, rename and delete a request", async ({ app }) => {
  await openWorkspace(app);
  await app.getByLabel("Add request or folder").first().click();
  await app.getByRole("menuitem", { name: "New request", exact: true }).click();
  await app.getByRole("dialog").getByRole("textbox").fill("List pets");
  await app.getByRole("dialog").getByRole("button", { name: "Create" }).click();
  // The new request opens in a tab.
  await expect(app.getByRole("button", { name: "Send" })).toBeVisible();
  const item = app.locator("aside").getByText("List pets", { exact: true });
  await expect(item).toBeVisible();

  await item.click({ button: "right" });
  await app.getByRole("menuitem", { name: "Rename" }).click();
  await app.getByRole("dialog").getByRole("textbox").fill("List all pets");
  await app
    .getByRole("dialog")
    .getByRole("button", { name: /Rename|Save/ })
    .click();
  const renamed = app.locator("aside").getByText("List all pets", { exact: true });
  await expect(renamed).toBeVisible();

  await renamed.click({ button: "right" });
  await app.getByRole("menuitem", { name: "Delete" }).click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click();
  await expect(renamed).toHaveCount(0);
});

test("add an environment variable by typing and mark the active environment", async ({ app }) => {
  await openWorkspace(app);
  await app.getByTitle("Environments", { exact: true }).click();
  await app.getByRole("menuitem", { name: /Configure collection environments/ }).click();

  // Feedback #1: typing into the "add" row used to create one variable per keystroke.
  const add = app.getByPlaceholder("Add variable");
  await add.click();
  await app.keyboard.type("apiVersion");
  await expect(app.getByPlaceholder("Add variable")).toHaveValue("");
  await expect(app.locator('input[value="apiVersion"]')).toHaveCount(1);
  await expect(app.getByRole("tab", { name: "Variables (2)" })).toBeVisible();

  await app.keyboard.press("Tab");
  await app.keyboard.type("v2");
  const save = app.getByRole("main").getByRole("button", { name: "Save", exact: true });
  await save.click();
  await expect(save).toBeDisabled();

  // Feedback #7: the list shows which environment is active; right-click sets it.
  const prod = app.getByTitle(/Drag to reorder/).filter({ hasText: "prod" });
  await prod.click({ button: "right" });
  await app.getByRole("menuitem", { name: "Set as active" }).click();
  await expect(prod).toContainText("active");
  await expect(app.getByTitle("Environments", { exact: true })).toContainText("prod");
});

test("closing an unsaved tab offers Save, Don't save and Cancel", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "{{baseUrl}}/health?full=true");

  const tab = app.locator("[data-tab-path='health.yaml']");
  // Feedback #10: the dialog used to offer only "Close" (discard) and Cancel.
  await tab.getByLabel("Close tab").click();
  const dialog = app.getByRole("alertdialog");
  await expect(dialog).toContainText('Save changes to "Health"?');
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(tab).toBeVisible();

  await tab.getByLabel("Close tab").click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Save", exact: true }).click();
  await expect(tab).toHaveCount(0);

  // Saved: opening it again shows the new URL.
  await openRequest(app, "Health");
  await expect(app.locator(".cm-content").first()).toHaveText("{{baseUrl}}/health?full=true");
});

test("middle-click closes a tab", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  const tab = app.locator("[data-tab-path='health.yaml']");
  await expect(tab).toBeVisible();
  await tab.click({ button: "middle" });
  await expect(tab).toHaveCount(0);
});

test("a dialog reopened while it is closing can still be used", async ({ app, browserName }) => {
  // CI found this: Radix gave the overlay and the content their own portals, so reopening
  // during the closing animation could put the overlay on top of the content. It only shows
  // on a slow machine, so the CPU is slowed down (a Chromium feature).
  test.skip(browserName !== "chromium", "CPU throttling needs Chromium");
  // Ten times slower, on CI machines that are slow already.
  test.setTimeout(120_000);
  const cdp = await app.context().newCDPSession(app);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 10 });
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "{{baseUrl}}/health?x=1");
  const tab = app.locator("[data-tab-path='health.yaml']");
  const dialog = app.getByRole("alertdialog");
  for (let i = 0; i < 3; i++) {
    await tab.getByLabel("Close tab").click();
    await dialog.getByRole("button", { name: "Cancel" }).click();
  }
  await tab.getByLabel("Close tab").click();
  await dialog.getByRole("button", { name: "Save", exact: true }).click({ timeout: 5000 });
  await expect(tab).toHaveCount(0);
});

test("git: a saved change shows up in the changes tab and can be committed", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "{{baseUrl}}/healthz");
  await app.keyboard.press("ControlOrMeta+s");

  await app.getByRole("tab", { name: "Git" }).click();
  await app.getByText("health.yaml").click();
  // Feedback #14/#15: the diff opens in a tab, with old and new lines.
  const diff = app.getByTestId("diff-view");
  await expect(diff).toContainText("url: {{baseUrl}}/healthz");
  await expect(diff).toContainText("url: {{baseUrl}}/health");
  await app.getByRole("button", { name: "Side by side" }).click();
  await expect(app.locator(".cm-mergeView")).toBeVisible();

  // Feedback: one file list (the sidebar's); the tab stages the file it shows.
  await expect(app.getByRole("main").getByText("health.yaml", { exact: true })).toHaveCount(1);
  await app.getByRole("main").getByRole("button", { name: "Stage", exact: true }).click();
  await expect(app.getByRole("main").getByText("staged", { exact: true })).toBeVisible();
  await app.getByPlaceholder(/Commit message/).fill("Use /healthz");
  await app.getByRole("button", { name: /^Commit/ }).click();
  await expect(app.locator("aside").getByText("No changes.")).toBeVisible();
});

test("OpenAPI import shows a preview with the folder grouping", async ({ app }) => {
  await openWorkspace(app);
  await app.getByRole("button", { name: "Echo API" }).click();
  await app.getByRole("menuitem", { name: "Import OpenAPI…" }).click();
  await app.getByRole("button", { name: "Choose file…" }).click();
  const dialog = app.getByRole("dialog");
  await expect(dialog).toContainText("Petstore");
  await expect(dialog).toContainText("3 requests in 2 folders");
  // Feedback #4: folders from URL paths are the default.
  await expect(dialog.getByLabel("Folders from")).toContainText("URL paths");
});

test("pin a workspace on the start screen", async ({ app }) => {
  const recent = app.getByRole("button", { name: /fake\/workspace/ });
  await recent.click({ button: "right" });
  await app.getByRole("menuitem", { name: "Pin" }).click();
  await expect(app.getByRole("heading", { name: "Pinned" })).toBeVisible();
  await expect(app.getByRole("heading", { name: "Recent" })).toHaveCount(0);
});
