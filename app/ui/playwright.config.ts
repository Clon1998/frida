import { defineConfig, devices } from "@playwright/test";

/**
 * UI flow tests (PLAN.md M12): Playwright drives the Vite dev server in Chromium, with the fake
 * in-memory backend from `src/e2e/fakeBackend.ts` instead of Tauri (`VITE_E2E=1`). A separate
 * port keeps it apart from a running `pnpm tauri dev` (5173).
 */
const PORT = 5183;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.e2e.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "retain-on-failure",
  },
  // WebKit is what the app runs on (WKWebView on macOS, WebKitGTK on Linux); Chromium is
  // close to Windows' WebView2.
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 1400, height: 900 } },
    },
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1400, height: 900 } },
    },
  ],
  webServer: {
    command: `pnpm exec vite --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    env: { VITE_E2E: "1" },
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
