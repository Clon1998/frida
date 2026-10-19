/** Copying environments through the clipboard, and copying/moving them between scopes. */
import type { Page } from "@playwright/test";
import { backend, expect, openWorkspace, test } from "./fixtures";

async function openEnvironments(page: Page) {
  await page.getByTitle("Environments", { exact: true }).click();
  await page.getByRole("menuitem", { name: /Configure collection environments/ }).click();
}

/** An environment in the editor's list. */
const entry = (page: Page, name: string) =>
  page.getByTitle(/Drag to reorder/).filter({ hasText: new RegExp(`^${name}`) });

test("copy an environment to the clipboard and paste it", async ({ app, browserName }) => {
  test.skip(browserName === "webkit", "Playwright's WebKit can't grant clipboard permissions");
  await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await openWorkspace(app);
  await openEnvironments(app);

  await entry(app, "local").click({ button: "right" });
  await app.getByRole("menuitem", { name: "Copy to clipboard" }).click();
  await expect(app.getByText(/Secret values are not included/)).toBeVisible();
  const copied = await app.evaluate<string>("navigator.clipboard.readText()");
  expect(copied).toContain("baseUrl");

  await app.getByRole("button", { name: "Paste…" }).click();
  await app.getByRole("dialog").getByRole("textbox").fill(copied);
  await app.getByRole("button", { name: "Add environment" }).click();
  // "local" exists, so the pasted one is "local 2", and it is selected.
  await expect(entry(app, "local 2")).toBeVisible();
  await expect(app.getByRole("main").getByRole("textbox", { name: "Name" }).first()).toHaveValue(
    "local 2",
  );

  // Something that isn't an environment is refused with a message.
  await app.getByRole("button", { name: "Paste…" }).click();
  await app.getByRole("dialog").getByRole("textbox").fill("hello");
  await app.getByRole("button", { name: "Add environment" }).click();
  await expect(app.getByText(/doesn't contain a Frida environment/)).toBeVisible();
});

test("move an environment to global and copy one to another collection", async ({ app }) => {
  await openWorkspace(app);
  await openEnvironments(app);

  await entry(app, "prod").click({ button: "right" });
  await app.getByRole("menuitem", { name: "Move to", exact: true }).click();
  await app.getByRole("menuitem", { name: "Global (workspace)" }).click();
  await expect(entry(app, "prod")).toHaveCount(0);
  // The toast leads to the moved environment, now on the Global tab.
  await app.getByRole("button", { name: "Show" }).click();
  await expect(app.getByRole("tab", { name: "Global (workspace)" })).toHaveAttribute(
    "data-state",
    "active",
  );
  await expect(entry(app, "prod")).toBeVisible();

  await app.getByRole("tab", { name: /^Collection/ }).click();
  await entry(app, "local").click({ button: "right" });
  await app.getByRole("menuitem", { name: "Copy to", exact: true }).click();
  await app.getByRole("menuitem", { name: "Billing" }).click();
  await expect(app.getByText('Copied "local" to Billing')).toBeVisible();
  const state = await backend(app);
  expect(state.otherEnvironments.map((e: { name: string }) => e.name)).toEqual(["local"]);
  expect(state.globalEnvironments.map((e: { name: string }) => e.name)).toEqual(["prod"]);
  expect(state.environments.map((e: { name: string }) => e.name)).toEqual(["local"]);
});

test("deleting the active environment selects no environment", async ({ app }) => {
  await openWorkspace(app);
  await app.getByTitle("Environments", { exact: true }).click();
  await app.getByRole("menuitem", { name: "local", exact: true }).click();
  await expect(app.getByTitle("Environments", { exact: true })).toHaveText(/local/);
  await expect(app.getByRole("menu")).toHaveCount(0);

  await openEnvironments(app);
  await entry(app, "local").click();
  await app.getByRole("button", { name: "Delete environment" }).click();
  await app.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(entry(app, "local")).toHaveCount(0);
  await expect(app.getByTitle("Environments", { exact: true })).toHaveText(/No environment/);
});
