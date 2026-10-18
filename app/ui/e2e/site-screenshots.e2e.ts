// The callback of `page.evaluate` below runs in the browser.
/// <reference lib="dom" />
/**
 * Screenshots for the website (`site/`), taken from the real UI against the fake backend.
 * Skipped in the normal e2e run; regenerate them after visible UI changes with
 * `pnpm screenshots` (writes `site/src/assets/screenshots/<name>-<light|dark>.png` and the
 * social preview card `site/public/og.png`).
 */
import { readFileSync } from "node:fs";
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
 * Switches to the color scheme and gets the page ready for a picture: transitions settled, no
 * hover, and the placeholder names replaced in the DOM (so the fake, and the tests that use
 * its names, stay as they are).
 */
async function prepare(page: Page, scheme: "light" | "dark") {
  await page.emulateMedia({ colorScheme: scheme });
  await page.mouse.move(0, 0);
  await page.waitForTimeout(300);
  await page.evaluate((names) => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      for (const [from, to] of names)
        node.textContent = (node.textContent ?? "").replaceAll(from, to);
    }
  }, SHOWCASE_NAMES);
}

/** Takes the shot twice, in light and dark mode (the app follows the OS by default). */
async function shoot(page: Page, name: string) {
  for (const scheme of ["light", "dark"] as const) {
    await prepare(page, scheme);
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

/**
 * The picture link previews show (Slack, LinkedIn, X…): 1200×630, the logo and claim next to a
 * crop of the app in dark mode.
 */
test("social preview card", async ({ app }) => {
  await createOrder(app);
  await app.getByRole("button", { name: "Send" }).click();
  await expect(app.getByText("200 OK")).toBeVisible();
  await prepare(app, "dark");
  const shot = (await app.screenshot()).toString("base64");
  const logo = readFileSync("../../assets/app-icon.svg").toString("base64");

  await app.setViewportSize({ width: 1200, height: 630 });
  await app.setContent(`
    <style>
      body { margin: 0; width: 1200px; height: 630px; overflow: hidden; color: #fff;
        font-family: -apple-system, "Segoe UI", Inter, sans-serif;
        background: radial-gradient(ellipse at 20% 0%, #312e81, #0f0e1f 70%); }
      .text { position: absolute; left: 72px; top: 96px; width: 470px; }
      .brand { display: flex; align-items: center; gap: 18px; font-size: 44px; font-weight: 700; }
      .brand img { width: 84px; height: 84px; margin: -10px; }
      h1 { margin: 40px 0 0; font-size: 50px; line-height: 1.1; letter-spacing: -0.02em; }
      h1 span { color: #c7d2fe; }
      p { margin: 28px 0 0; font-size: 24px; line-height: 1.4; color: #c7c9d9; }
      .shot { position: absolute; left: 600px; top: 120px; width: 900px; border-radius: 14px;
        border: 1px solid #3f3d6b; box-shadow: 0 30px 80px rgb(0 0 0 / 0.5); }
    </style>
    <div class="text">
      <div class="brand"><img src="data:image/svg+xml;base64,${logo}">Frida</div>
      <h1>Your API requests, <span>as files in your repo.</span></h1>
      <p>Local-first HTTP client for macOS, Windows and Linux. Free and open source.</p>
    </div>
    <img class="shot" src="data:image/png;base64,${shot}">`);
  await app.screenshot({ path: "../../site/public/og.png" });
});
