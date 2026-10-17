import { expect, type Page, test as base } from "@playwright/test";

/**
 * Every test starts with a fresh fake backend (it lives in the page, so a new page is enough)
 * and fails on unhandled commands or uncaught errors, so the fake can't silently drift from the
 * real command set.
 */
/** Error toasts a test expects (see `expectErrorToast`), per page. */
const expectedToasts = new WeakMap<Page, string[]>();

/** Waits for an error toast containing `text`; it then doesn't fail the test. */
export async function expectErrorToast(page: Page, text: string) {
  expectedToasts.set(page, [...(expectedToasts.get(page) ?? []), text]);
  await expect(
    page.locator('[data-sonner-toast][data-type="error"]', { hasText: text }),
  ).toBeVisible();
}

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
    const allowed = expectedToasts.get(page) ?? [];
    for (const toast of await page.locator('[data-sonner-toast][data-type="error"]').all()) {
      const text = await toast.innerText();
      if (!allowed.some((a) => text.includes(a))) problems.push(`error toast: ${text}`);
    }
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
  // The closed menu hands the focus back to its button a moment later; wait for that, or it
  // could take the focus away from whatever the test types into next.
  await expect(picker).toBeFocused();
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

/** Makes the fake backend's "Check for updates" find `version`. */
export const offerUpdate = (page: Page, version: string) =>
  page.evaluate(
    (v) =>
      (
        globalThis as unknown as { __fridaE2E: { offerUpdate: (info: unknown) => void } }
      ).__fridaE2E.offerUpdate({
        version: v,
        currentVersion: "0.2.0",
        notes: "## What's Changed\n* feat: auto-update",
        date: "2026-10-20T10:00:00Z",
        installBlocker: null,
      }),
    version,
  );

/** Changes a request in the fake backend as if edited outside Frida, and sends `fs-changed`. */
export const changeOnDisk = (page: Page, path: string, patch: Record<string, unknown>) =>
  page.evaluate(
    ([p, change]) =>
      (
        globalThis as unknown as {
          __fridaE2E: { changeOnDisk: (p: string, c: unknown) => Promise<void> };
        }
      ).__fridaE2E.changeOnDisk(p, change),
    [path, patch] as const,
  );
