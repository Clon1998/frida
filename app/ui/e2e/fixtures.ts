import { expect, type Page, test as base } from "@playwright/test";

/**
 * Every test starts with a fresh fake backend (it lives in the page, so a new page is enough)
 * and fails on unhandled commands or uncaught errors, so the fake can't silently drift from the
 * real command set.
 */
export const test = base.extend<{ app: Page }>({
  app: async ({ page }, use) => {
    const problems: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "warning" && msg.text().includes("unhandled command"))
        problems.push(msg.text());
    });
    page.on("pageerror", (err) => problems.push(err.message));
    await page.goto("/");
    await use(page);
    // Error toasts (`showError`) mean a command failed or returned something the UI can't use.
    for (const toast of await page.locator('[data-sonner-toast][data-type="error"]').all())
      problems.push(`error toast: ${await toast.innerText()}`);
    expect(problems, "unhandled commands, page errors or error toasts").toEqual([]);
  },
});

export { expect };

/** From the start screen into the fake workspace, with the request tree loaded. */
export async function openWorkspace(page: Page) {
  await page
    .getByRole("button", { name: /workspace$/ })
    .first()
    .waitFor();
  await page.getByText("/fake/workspace").click();
  await expect(page.locator("aside").getByText("Health", { exact: true })).toBeVisible();
}

/** Opens a request of the fake collection as a normal (not preview) tab. */
export async function openRequest(page: Page, name: string) {
  const item = page.locator("aside").getByText(name, { exact: true });
  if ((await item.count()) === 0) await page.getByLabel("Expand folder").first().click();
  await item.dblclick();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
}

/** The URL bar's editor (a one-line CodeMirror editor). */
export const urlBar = (page: Page) => page.locator("main .cm-content").first();

/** Replaces the text of the URL bar. */
export async function setUrl(page: Page, url: string) {
  await urlBar(page).click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.type(url);
}

/** Picks a collection environment in the header's environment menu. */
export async function useEnvironment(page: Page, name: string) {
  const picker = page.getByTitle("Environments", { exact: true });
  await picker.click();
  await page.getByRole("menuitem", { name }).first().click();
  await expect(picker).toContainText(name);
}

/** Saves the active tab (Ctrl/Cmd+S) and waits until it no longer shows unsaved changes. */
export async function save(page: Page) {
  await page.keyboard.press("ControlOrMeta+s");
  await expect(page.locator("main").getByRole("button", { name: "Save", exact: true })).toHaveCount(
    0,
  );
}

/** Reads the fake backend's state (see src/e2e/fakeBackend.ts). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const backend = (page: Page): Promise<any> =>
  page.evaluate(() =>
    (globalThis as unknown as { __fridaE2E: { state: () => unknown } }).__fridaE2E.state(),
  );
