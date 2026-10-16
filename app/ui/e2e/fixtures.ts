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
  await expect(page.getByText("Health")).toBeVisible();
}
