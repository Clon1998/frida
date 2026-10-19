/**
 * Response viewer details, history, files changed outside Frida, branches, keyboard
 * shortcuts, settings and folder settings.
 */
import {
  backend,
  changeOnDisk,
  expect,
  expectErrorToast,
  offerUpdate,
  openRequest,
  openWorkspace,
  setUrl,
  test,
  urlBar,
} from "./fixtures";

test("response: JSONPath filter, headers, save suggestion and cancelling a slow send", async ({
  app,
}) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await app.getByRole("button", { name: "Send" }).click();
  await expect(app.getByText("200 OK")).toBeVisible();

  await app.getByRole("button", { name: "Filter with JSONPath" }).click();
  await app.getByLabel("JSONPath filter").fill("$.method");
  await expect(app.locator("main .cm-content").last()).toHaveText('"GET"');
  await app.getByLabel("JSONPath filter").press("Escape");
  await expect(app.getByLabel("JSONPath filter")).toHaveCount(0);

  await app.getByRole("tab", { name: /^Headers \(/ }).click();
  await expect(app.getByText("x-request-id")).toBeVisible();

  // Feedback #6: the file name comes from the content type.
  await app.getByRole("button", { name: "Save", exact: true }).click();
  await expect
    .poll(async () => (await backend(app)).lastSaveDialog?.defaultPath)
    .toBe("response.json");
  expect(Object.keys((await backend(app)).savedBodies)).toEqual(["/fake/saved.zip"]);

  // Feedback #12: while sending, Cancel shows a spinner and its label, nothing else.
  await setUrl(app, "{{baseUrl}}/slow");
  await app.getByRole("button", { name: "Send" }).click();
  const cancel = app.getByRole("button", { name: "Cancel" });
  await expect(cancel).toBeVisible();
  await expect(cancel.locator("svg")).toHaveCount(1);
  await expect(cancel.locator("svg")).toHaveClass(/animate-spin/);
  // The response pane counts the time while waiting.
  await expect(app.getByLabel("Elapsed time")).toHaveText(/^[1-9]\d* ms$|s$/);
  await cancel.click();
  await expect(app.getByText("The request was cancelled.")).toBeVisible();
});

test("history: search, open an entry and restore it as a new request", async ({ app }) => {
  await openWorkspace(app);
  for (const name of ["Health", "Create order"]) {
    await openRequest(app, name);
    await app.getByRole("button", { name: "Send" }).click();
    await expect(app.getByText("200 OK")).toBeVisible();
  }
  await app.getByRole("tab", { name: "History" }).click();
  const list = app.locator("aside");
  await expect(list.getByText("Create order")).toBeVisible();
  await list.getByPlaceholder("Search history").fill("health");
  await expect(list.getByText("Create order")).toHaveCount(0);

  await list.getByText("Health", { exact: true }).click();
  await expect(app.getByRole("button", { name: "Restore as new request" })).toBeVisible();
  await expect(app.getByText("200 OK")).toBeVisible();
  await app.getByRole("button", { name: "Restore as new request" }).click();
  await expect(app.getByRole("button", { name: "Send" })).toBeVisible();
  expect(Object.keys((await backend(app)).requests)).toContain("health-2.yaml");
});

test("a file changed outside Frida: clean tabs reload, edited tabs ask", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");

  // No local edits: the tab just shows the new version.
  await changeOnDisk(app, "health.yaml", { url: "{{baseUrl}}/v2/health" });
  await expect(urlBar(app)).toHaveText("{{baseUrl}}/v2/health");

  // With local edits: keep or reload.
  await setUrl(app, "{{baseUrl}}/mine");
  await changeOnDisk(app, "health.yaml", { url: "{{baseUrl}}/v3/health" });
  await expect(app.getByText("This file changed on disk while you were editing it.")).toBeVisible();
  await app.getByRole("button", { name: "Reload from disk" }).click();
  await expect(urlBar(app)).toHaveText("{{baseUrl}}/v3/health");
});

test("git branches: switching is refused with changes, a new branch keeps them", async ({
  app,
}) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "{{baseUrl}}/changed");
  await app.keyboard.press("ControlOrMeta+s");

  await app.getByRole("tab", { name: "Git" }).click();
  const panel = app.locator("aside");
  await panel.getByRole("button", { name: "main" }).click();
  await app.getByRole("menuitem", { name: "feature/login" }).click();
  await expectErrorToast(app, "uncommitted changes");

  await panel.getByRole("button", { name: "main" }).click();
  await app.getByRole("menuitem", { name: "New branch…" }).click();
  await app.getByRole("dialog").getByRole("textbox").fill("feature/health");
  await app.getByRole("dialog").getByRole("button", { name: "Create and switch" }).click();
  await expect(panel.getByRole("button", { name: "feature/health" })).toBeVisible();
  await expect(panel.getByText("health.yaml")).toBeVisible();
});

test("quick open and the environment switcher from the keyboard", async ({ app }) => {
  await openWorkspace(app);
  await app.keyboard.press("ControlOrMeta+k");
  await app.getByPlaceholder(/Search requests in/).fill("create");
  await app.keyboard.press("Enter");
  await expect(app.locator("[data-tab-path='orders/create-order.yaml']")).toBeVisible();

  await app.keyboard.press("ControlOrMeta+e");
  await expect(app.getByRole("menuitem", { name: "local" })).toBeVisible();
  await app.getByRole("menuitem", { name: "local" }).click();
  await expect(app.getByTitle("Environments", { exact: true })).toContainText("local");
});

test("settings: theme and editor font size apply at once", async ({ app }) => {
  await app.getByRole("button", { name: "Settings" }).click();
  const dialog = app.getByRole("dialog");
  await dialog.getByRole("group", { name: "Theme" }).getByRole("button", { name: "Dark" }).click();
  await expect(app.locator("html")).toHaveClass(/dark/);

  await dialog.getByRole("group", { name: "Editor font size" }).getByRole("combobox").click();
  await app.getByRole("option", { name: "16 px" }).click();
  await expect
    .poll(() =>
      app.evaluate("document.documentElement.style.getPropertyValue('--editor-font-size')"),
    )
    .toBe("16px");
  expect((await backend(app)).settings.preferences).toMatchObject({
    theme: "dark",
    editorFontSize: 16,
  });
});

test("updates: check by hand, save unsaved tabs, install", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "http://localhost:8787/changed");

  await app.keyboard.press("ControlOrMeta+,");
  await app.getByRole("button", { name: "Updates" }).click();
  await app.getByRole("button", { name: "Check for updates" }).click();
  await expect(app.getByText("Frida is up to date")).toBeVisible();

  await offerUpdate(app, "9.9.9");
  await app.getByRole("button", { name: "Check for updates" }).click();
  const dialog = app.getByRole("dialog", { name: "Frida 9.9.9 is available" });
  await expect(dialog).toContainText("feat: auto-update");
  await expect(dialog).toContainText("Unsaved changes in Health");

  await dialog.getByRole("button", { name: "Save and install" }).click();
  await expect.poll(async () => (await backend(app)).updateInstalls).toBe(1);
  const saved = Object.values((await backend(app)).requests) as { name: string; url: string }[];
  expect(saved.find((r) => r.name === "Health")?.url).toBe("http://localhost:8787/changed");
});

test("updates: switching off the automatic check is saved", async ({ app }) => {
  await app.getByRole("button", { name: "Settings" }).click();
  await app.getByRole("button", { name: "Updates" }).click();
  await app.getByRole("checkbox").click();
  await expect
    .poll(async () => (await backend(app)).settings.preferences.checkForUpdates)
    .toBe(false);
});

test("network: a manual proxy with login and bypass list is saved, a bad URL refused", async ({
  app,
}) => {
  await app.getByRole("button", { name: "Settings" }).click();
  await app.getByRole("button", { name: "Network" }).click();
  const dialog = app.getByRole("dialog");
  await dialog.getByRole("button", { name: "Manual" }).click();

  const url = dialog.getByRole("group", { name: "Proxy URL" }).getByRole("textbox");
  await url.fill("proxy.corp:3128");
  await url.press("Enter");
  await expectErrorToast(app, "is not valid");
  await expect.poll(async () => (await backend(app)).settings.preferences.proxy.url).toBe("");

  await url.fill("http://proxy.corp:3128");
  await url.press("Enter");
  const user = dialog.getByRole("group", { name: "Username" }).getByRole("textbox");
  await user.fill("ann");
  await user.press("Enter");
  const password = dialog.getByRole("group", { name: "Password" });
  await password.getByPlaceholder("Password").fill("s3cret");
  await password.getByRole("button", { name: "Save" }).click();
  await expect(password.getByPlaceholder("•••••••• (saved)")).toBeVisible();

  const bypass = dialog
    .getByRole("group", { name: "Don't use the proxy for" })
    .getByRole("textbox");
  await bypass.fill("localhost, *.internal");
  await url.click(); // leave the field

  const saved = await backend(app);
  expect(saved.settings.preferences.proxy).toEqual({
    mode: "manual",
    url: "http://proxy.corp:3128",
    username: "ann",
    bypass: "localhost, *.internal",
  });
  expect(saved.proxyPassword).toBe("s3cret");
  // The password never goes into the settings.
  expect(JSON.stringify(saved.settings)).not.toContain("s3cret");
});

test("collection settings: the proxy switch is saved on this computer and survives tab changes", async ({
  app,
}) => {
  await openWorkspace(app);
  await app.getByRole("button", { name: "Collection settings" }).click();
  await app.getByRole("tab", { name: "Settings" }).click();
  await app.getByRole("checkbox", { name: "Don't use the proxy" }).click();
  const bypass = async () =>
    Object.values(
      (await backend(app)).collectionStates as Record<string, { bypassProxy?: boolean }>,
    )[0]?.bypassProxy;
  await expect.poll(bypass).toBe(true);
  await openRequest(app, "Health");
  await expect.poll(bypass).toBe(true);
});

test("folder settings: headers and auth are saved to the folder", async ({ app }) => {
  await openWorkspace(app);
  await app.locator("aside").getByText("Orders", { exact: true }).click({ button: "right" });
  await app.getByRole("menuitem", { name: "Settings…" }).click();

  await app.getByRole("tab", { name: "Headers" }).click();
  const panel = app.getByRole("tabpanel");
  await panel.getByPlaceholder(/^Add /).click();
  await app.keyboard.type("X-Team");
  await app.getByRole("tab", { name: "Auth" }).click();
  await panel.getByRole("combobox").first().click();
  await app.getByRole("option", { name: "Bearer token" }).click();
  await panel.getByRole("group", { name: "Token" }).locator(".cm-content, input").first().click();
  await app.keyboard.type("{{teamToken}}");

  await app.keyboard.press("ControlOrMeta+s");
  await expect
    .poll(async () => (await backend(app)).folders.orders.headers?.[0]?.name)
    .toBe("X-Team");
  const folder = (await backend(app)).folders.orders;
  expect(folder.auth).toEqual({
    mode: "custom",
    schemes: [{ type: "bearer", token: "{{teamToken}}" }],
  });
});
