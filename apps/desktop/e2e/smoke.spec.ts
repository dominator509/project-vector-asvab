/**
 * Smoke test: proves the built production artifact actually serves and renders.
 *
 * EP-009's execplan requires "offline launch" evidence, and HARNESS_LAWS.md law
 * 4 requires observed command evidence for a PASS. This is a Playwright spec
 * that runs against the production bundle with the network unavailable to the
 * page, so it demonstrates the offline claim rather than asserting it.
 */

import { expect, test } from "@playwright/test";

test.describe("production artifact smoke", () => {
  test("the built bundle serves and renders the shell", async ({ page }) => {
    const response = await page.goto("/");
    expect(
      response?.status(),
      "the artifact must serve its entry document",
    ).toBe(200);

    await expect(
      page.getByRole("heading", { level: 1, name: /project vector/i }),
    ).toBeVisible();
  });

  test("the artifact works with the network blocked", async ({
    page,
    context,
  }) => {
    // Offline is the product's core promise (ADR-003). Blocking every request
    // that is not the local origin proves the shell does not depend on a CDN,
    // a font host, or a telemetry endpoint to render.
    await context.route("**/*", (route) => {
      const url = route.request().url();
      if (url.startsWith("http://127.0.0.1") || url.startsWith("data:")) {
        return route.continue();
      }
      return route.abort();
    });

    await page.goto("/");
    await expect(
      page.getByRole("heading", { level: 1, name: /project vector/i }),
    ).toBeVisible();

    // Core study surfaces must be reachable offline.
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();
    await expect(page.getByTestId("practice-prompt")).toBeVisible();
  });

  test("no external requests are attempted during a study session", async ({
    page,
  }) => {
    const external: string[] = [];
    page.on("request", (request) => {
      const url = request.url();
      if (!url.startsWith("http://127.0.0.1") && !url.startsWith("data:")) {
        external.push(url);
      }
    });

    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();
    await page.getByRole("radio").first().check();
    await page.getByRole("button", { name: /check answer/i }).click();

    expect(
      external,
      `the app must not contact external hosts, saw: ${external.join(", ")}`,
    ).toEqual([]);
  });
});
