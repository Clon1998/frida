// The callback of `page.evaluate` below runs in the browser.
/// <reference lib="dom" />
/**
 * Screenshots for the website (`site/`), taken from the real UI against the fake backend.
 * Skipped in the normal e2e run; regenerate them after visible UI changes with
 * `pnpm screenshots` (writes `site/src/assets/screenshots/<name>-<light|dark>.png`).
 */
import type { Page } from "@playwright/test";
import { expect, openRequest, openWorkspace, setUrl, test, useEnvironment } from "./fixtures";

const OUT = "../../site/src/assets/screenshots";

test.skip(!process.env.SCREENSHOTS, "only with SCREENSHOTS=1 (pnpm screenshots)");
// One browser is enough: the pictures should look the same every time.
test.skip(({ browserName }) => browserName !== "chromium", "Chromium only");
test.use({ viewport: { width: 1280, height: 760 }, deviceScaleFactor: 2 });

/** The fake backend's placeholder names, and what the website shows instead. */
const SHOWCASE_NAMES: [string, string][] = [
  ["/fake/workspace", "~/code/shop"],
  ["Fake workspace", "shop"],
];

/**
 * Takes the shot twice, in light and dark mode (the app follows the OS by default). The
 * placeholder names are replaced in the DOM right before, so the fake (and the tests that
 * use its names) stay as they are.
 */
async function shoot(page: Page, name: string) {
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.mouse.move(0, 0);
    // Let the theme switch and any transitions settle.
    await page.waitForTimeout(300);
    await page.evaluate((names) => {
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        for (const [from, to] of names)
          node.textContent = (node.textContent ?? "").replaceAll(from, to);
      }
    }, SHOWCASE_NAMES);
    await page.screenshot({ path: `${OUT}/${name}-${scheme}.png` });
  }
}

/** Opens "Create order" with a JSON body, ready to send. */
async function createOrder(page: Page) {
  await openWorkspace(page);
  await useEnvironment(page, "local");
  await openRequest(page, "Create order");
  await setUrl(page, "{{baseUrl}}/orders?notify=true");
  const panel = page.getByRole("tabpanel");
  await page.getByRole("tab", { name: "Body" }).click();
  await panel.getByRole("combobox").click();
  await page.getByRole("option", { name: "JSON" }).click();
  await panel.locator(".cm-content").click();
  await page.keyboard.insertText(
    '{\n  "customerId": "cus_4711",\n  "items": [\n    { "sku": "WESTIE-01", "qty": 2 }\n  ]\n}',
  );
}

test("request and response", async ({ app }) => {
  await createOrder(app);
  await app.getByRole("button", { name: "Send" }).click();
  await expect(app.getByText("200 OK")).toBeVisible();
  await shoot(app, "request");
});

test("environments", async ({ app }) => {
  await openWorkspace(app);
  await useEnvironment(app, "local");
  await app.getByTitle("Environments", { exact: true }).click();
  await app.getByRole("menuitem", { name: /Configure collection environments/ }).click();
  const add = app.getByPlaceholder("Add variable");
  for (const [name, value] of [
    ["apiVersion", "v2"],
    ["customerId", "cus_4711"],
  ]) {
    await add.click();
    await app.keyboard.type(name);
    await app.keyboard.press("Tab");
    await app.keyboard.type(value);
  }
  await app.getByRole("main").getByRole("button", { name: "Save", exact: true }).click();
  // Wait for the "Saved" toast to come and go.
  await expect(app.getByText('Saved "local"')).toBeVisible();
  await expect(app.locator("[data-sonner-toast]")).toHaveCount(0, { timeout: 10_000 });
  await shoot(app, "environments");
});

test("collection runner", async ({ app }) => {
  await openWorkspace(app);
  await useEnvironment(app, "local");
  await app.locator("aside").getByText("Orders", { exact: true }).click({ button: "right" });
  await app.getByRole("menuitem", { name: "Run folder…" }).click();
  await app.getByRole("button", { name: "Run 2 requests" }).click();
  await expect(app.getByText("2 passed")).toBeVisible();
  await shoot(app, "runner");
});

test("git diff", async ({ app }) => {
  await openWorkspace(app);
  await openRequest(app, "Health");
  await setUrl(app, "{{baseUrl}}/healthz?verbose=true");
  await app.keyboard.press("ControlOrMeta+s");
  await app.getByRole("tab", { name: "Git" }).click();
  await app.getByText("health.yaml").click();
  await app.getByRole("button", { name: "Side by side" }).click();
  await expect(app.locator(".cm-mergeView")).toBeVisible();
  await shoot(app, "git");
});
