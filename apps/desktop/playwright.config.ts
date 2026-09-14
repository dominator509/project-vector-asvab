import { defineConfig, devices } from "@playwright/test";

/** Host and port the preview server binds to. */
const HOST = "127.0.0.1";
const PORT = 4173;
const BASE_URL = `http://${HOST}:${PORT}`;

/**
 * Playwright configuration for the VECTOR desktop UI.
 *
 * EP-005 requires E2E against the production build, not a dev server, so the
 * webServer command serves the real `dist/` output via `vite preview`.
 *
 * The host is pinned to 127.0.0.1. Vite preview otherwise binds to `localhost`,
 * which on this platform resolves to IPv6 `::1` only, while Playwright probes
 * IPv4 — producing a startup timeout rather than a test failure.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [["list"], ["json", { outputFile: "e2e-results.json" }]],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `pnpm run preview --host ${HOST} --port ${PORT} --strictPort`,
    url: BASE_URL,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
