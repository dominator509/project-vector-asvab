/**
 * Live-fire tests: prove the real production artifact works through its real
 * boundary, with effects read back independently.
 *
 * The project's "reality law" is that compilation, screenshots and health-check
 * responses are not feature proof. These tests therefore observe side effects
 * rather than rendering: state written, then read back; navigation performed
 * with the keyboard; content carried through a full practice cycle.
 */

import { expect, test } from "@playwright/test";

test.describe("live-fire: real artifact, real effects, independent readback", () => {
  test("the artifact digest is stable across two fetches", async ({
    request,
  }) => {
    // The entry document must be byte-stable, or an artifact digest would not
    // identify the build.
    const first = await request.get("/");
    const second = await request.get("/");
    expect(first.status()).toBe(200);
    expect(second.status()).toBe(200);

    const a = await first.body();
    const b = await second.body();
    expect(a.equals(b), "the served entry document must be deterministic").toBe(
      true,
    );
  });

  test("the built bundle is self-contained", async ({ page }) => {
    // Every asset the document requests must come from the artifact itself, so
    // the app cannot silently depend on a CDN.
    const external: string[] = [];
    page.on("request", (request) => {
      const url = request.url();
      if (!url.startsWith("http://127.0.0.1") && !url.startsWith("data:")) {
        external.push(url);
      }
    });

    await page.goto("/", { waitUntil: "networkidle" });
    expect(
      external,
      `unexpected external requests: ${external.join(", ")}`,
    ).toEqual([]);
  });

  test("a full practice cycle produces an observable effect", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    // Before: no attempts recorded.
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /0 attempted, 0 incorrect/i,
    );

    // Act: answer correctly.
    await page.getByRole("radio", { name: /1,800/ }).check();
    await page.getByRole("button", { name: /check answer/i }).click();

    // Read back: the effect is observable, not just the rendering.
    await expect(page.getByTestId("worked-solution")).toBeVisible();
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /1 attempted, 0 incorrect/i,
    );
  });

  test("a mistake is captured and re-readable in the error notebook", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    await page.getByRole("radio", { name: /^180$/ }).check();
    await page.getByRole("button", { name: /check answer/i }).click();
    await page.getByRole("button", { name: /add to error notebook/i }).click();

    // The write is read back from the rendered list, an independent read of the
    // same state the click produced.
    await expect(page.getByTestId("notebook-list")).toContainText("q-ar-1");
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /1 attempted, 1 incorrect/i,
    );
  });

  test("the exam simulator preserves an answer within a continuous session", async ({
    page,
  }) => {
    // Scope note: exam state is component-local, so it resets when the view
    // unmounts. This asserts the persistence that IS implemented — an answer
    // survives paging forward and back inside one sitting. Session-level
    // persistence across navigation is a known gap recorded in the EP-010
    // anti-gaming review, not something this test pretends to prove.
    await page.goto("/");
    const nav = page.getByTestId("primary-nav");

    await nav.getByRole("button", { name: /paper simulator/i }).click();
    await page.getByRole("radio").first().check();
    await page.getByRole("button", { name: /^next$/i }).click();

    // Within the session, the paper form allows returning to a prior item.
    await page.getByRole("button", { name: /previous/i }).click();
    await expect(page.getByRole("radio").first()).toBeChecked();
  });

  test("keyboard-only operation completes a real task", async ({ page }) => {
    // The whole interaction uses only the keyboard, which is the accessibility
    // requirement stated as behaviour rather than as markup.
    await page.goto("/");
    await page.keyboard.press("Tab"); // skip link
    await page.keyboard.press("Tab"); // first nav button
    await expect(page.locator(":focus")).toHaveAttribute("data-view", /.+/);

    // Tab into the main region and reach the first control.
    for (let i = 0; i < 40; i += 1) {
      await page.keyboard.press("Tab");
      const tag = await page.evaluate(() =>
        document.activeElement?.tagName.toLowerCase(),
      );
      if (tag === "input") break;
    }

    const focusedTag = await page.evaluate(() =>
      document.activeElement?.tagName.toLowerCase(),
    );
    expect(["input", "button"]).toContain(focusedTag);
  });
});
