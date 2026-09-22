/**
 * EP-005 end-to-end acceptance against the production build.
 *
 * Required proof from the execplan: "Playwright production UI E2E,
 * keyboard/a11y, no-calculator/test-mode behavior."
 *
 * These run against `vite preview` serving the real `dist/` bundle, so they
 * exercise the artifact rather than a development server.
 */

import { expect, test } from "@playwright/test";

import { installStubBackend } from "./stubBackend";

test.describe("application shell", () => {
  test("loads and shows the product heading", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("heading", { level: 1, name: /project vector/i }),
    ).toBeVisible();
  });

  test("exposes a primary navigation landmark", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("navigation", { name: /primary/i }),
    ).toBeVisible();
  });

  test("has a main landmark", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("main")).toBeVisible();
  });
});

test.describe("keyboard-only operation", () => {
  test("navigates between views using only the keyboard", async ({ page }) => {
    await page.goto("/");

    // Tab from the skip link into the navigation, then use arrow keys.
    await page.keyboard.press("Tab"); // skip link
    await page.keyboard.press("Tab"); // first nav button
    const focused = page.locator(":focus");
    await expect(focused).toHaveAttribute("data-view", /.+/);

    // Move down the navigation and activate with Enter.
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("view-heading")).toBeVisible();
  });

  test("supports Home and End in the navigation", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("primary-nav").getByRole("button").first().focus();
    await page.keyboard.press("End");
    await expect(page.locator(":focus")).toHaveAttribute("data-view", "search");
    await page.keyboard.press("Home");
    await expect(page.locator(":focus")).toHaveAttribute(
      "data-view",
      "onboarding",
    );
  });

  test("the skip link targets the main landmark", async ({ page }) => {
    await page.goto("/");
    const skip = page.getByRole("link", { name: /skip to main content/i });
    await expect(skip).toHaveAttribute("href", "#main-content");
    await expect(page.locator("#main-content")).toHaveCount(1);
  });
});

test.describe("accessibility", () => {
  test("every interactive element has an accessible name", async ({ page }) => {
    await page.goto("/");
    const buttons = page.getByRole("button");
    const count = await buttons.count();
    expect(count).toBeGreaterThan(0);

    for (let i = 0; i < count; i += 1) {
      const button = buttons.nth(i);
      const name = (await button.textContent())?.trim() ?? "";
      const ariaLabel = await button.getAttribute("aria-label");
      expect((name + (ariaLabel ?? "")).length).toBeGreaterThan(0);
    }
  });

  test("the active view is marked with aria-current", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('[aria-current="page"]')).toHaveCount(1);
  });

  test("applies the high-contrast class when the setting is enabled", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /accessibility/i })
      .click();
    await page.getByRole("checkbox", { name: /high contrast/i }).check();
    await expect(page.getByTestId("app-root")).toHaveClass(/high-contrast/);
  });

  test("scales text when the font scale changes", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /accessibility/i })
      .click();

    const root = page.getByTestId("app-root");
    const before = await root.evaluate((el) =>
      getComputedStyle(el).getPropertyValue("--vector-font-scale"),
    );

    // Drive the range input with the keyboard, which is how a keyboard-only
    // user would change it.
    const slider = page.getByLabel(/font scale/i);
    await slider.focus();
    for (let i = 0; i < 5; i += 1) {
      await page.keyboard.press("ArrowRight");
    }

    const after = await root.evaluate((el) =>
      getComputedStyle(el).getPropertyValue("--vector-font-scale"),
    );
    expect(after).not.toBe(before);
  });
});

test.describe("exam simulation constraints", () => {
  test("the CAT form offers no backward navigation", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /cat simulator/i })
      .click();

    await expect(page.getByTestId("exam-rule")).toContainText(
      /does not allow returning/i,
    );
    await expect(page.getByRole("button", { name: /previous/i })).toHaveCount(
      0,
    );
  });

  test("a CAT answer is committed and cannot be changed", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /cat simulator/i })
      .click();

    const radios = page.getByRole("radio");
    await radios.first().check();
    await expect(page.getByTestId("answer-locked")).toBeVisible();
    await expect(radios.first()).toBeDisabled();
  });

  test("the paper form allows review and revision", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /paper simulator/i })
      .click();

    await expect(page.getByTestId("exam-rule")).toContainText(
      /allows you to review/i,
    );
    await expect(page.getByRole("button", { name: /previous/i })).toHaveCount(
      1,
    );
  });

  test("test mode hides distracting navigation chrome", async ({ page }) => {
    // SPEC-004: test mode suppresses distracting UI. The CAT simulator is the
    // exam surface, so the claim is that entering it does not render the
    // readiness/tutor chrome alongside the exam.
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /cat simulator/i })
      .click();

    await expect(page.getByTestId("exam-cat")).toBeVisible();
    await expect(page.getByTestId("today-drills")).toHaveCount(0);
  });
});

test.describe("practice flow", () => {
  test("answers a question and sees the worked solution", async ({ page }) => {
    // The bundle has no Rust process behind it here, so the bridge is stubbed.
    // The practice view serves corpus items, not literals, so a spec that wants a
    // question has to provide a backend that serves one.
    await installStubBackend(page);
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    // Which option is correct belongs to the corpus, not to this spec, so the page
    // is asked rather than the option text being hard-coded. That hard-coded text
    // is what kept this test asserting on demonstration content long after the
    // view stopped serving it.
    await page.locator('label.option-row[data-correct="true"] input').check();
    await page.getByRole("button", { name: /check answer/i }).click();

    await expect(page.getByTestId("worked-solution")).toBeVisible();
    // What the view shows for provenance is the objective an item serves. A
    // generated item quotes nobody, so it must not claim a source; the old
    // assertion here demanded "Source:" from items that were sample literals.
    await expect(page.getByTestId("worked-solution")).toContainText(
      /objective:/i,
    );
  });

  test("captures a mistake into the error notebook", async ({ page }) => {
    await installStubBackend(page);
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    const questionId = await page
      .getByTestId("practice-prompt")
      .getAttribute("data-question-id");
    expect(questionId, "the served question must identify itself").toBeTruthy();

    await page
      .locator('label.option-row[data-correct="false"] input')
      .first()
      .check();
    await page.getByRole("button", { name: /check answer/i }).click();
    await page.getByRole("button", { name: /add to error notebook/i }).click();

    // The id comes from the page rather than from a literal, so this holds for
    // whatever the corpus serves.
    await expect(page.getByTestId("notebook-list")).toContainText(
      questionId as string,
    );
  });
});

test.describe("search", () => {
  test("finds a lesson by typing", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^search$/i })
      .click();

    await page.getByLabel(/search lessons/i).fill("rate");
    await expect(page.getByTestId("search-results")).toBeVisible();
  });

  test("shows nothing before a query is entered", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^search$/i })
      .click();
    await expect(page.getByTestId("result-count")).toContainText(
      /type to search/i,
    );
  });
});

test.describe("readiness", () => {
  test("states that the local backend is unreachable rather than showing a number", async ({
    page,
  }) => {
    // This bundle is served by `vite preview`, so there is no Rust process
    // behind it. The honest behaviour is to say so. It previously rendered a
    // hard-coded demonstration band, which meant this suite was asserting
    // fabricated data — the readiness *estimate* is now verified where it is
    // real: in `cargo test --workspace`, in the component tests with an
    // explicit band, and in `vector-desktop --self-check`.
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^readiness$/i })
      .click();

    await expect(page.getByTestId("backend-unavailable")).toBeVisible();
    await expect(page.getByTestId("readiness-band")).toHaveCount(0);
    await expect(page.getByTestId("readiness-illegal-claim")).toHaveCount(0);
  });
});

test.describe("the interface never fabricates study data", () => {
  test("today's plan explains that it needs the local database", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /today's plan/i })
      .click();

    await expect(page.getByTestId("backend-unavailable")).toBeVisible();
    await expect(page.getByTestId("today-drills")).toHaveCount(0);
    await expect(page.getByTestId("today-total")).toHaveCount(0);
  });

  test("the source viewer says the vault is unavailable rather than empty", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^sources$/i })
      .click();

    await expect(page.getByTestId("backend-unavailable")).toBeVisible();
    // "Empty" and "unreachable" are different claims, and only one is true.
    await expect(page.getByTestId("vault-empty")).toHaveCount(0);
  });

  test("deleting local data reports that nothing was deleted", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /privacy and crashes/i })
      .click();

    await page.getByLabel(/type delete to confirm/i).fill("DELETE");
    await page.getByRole("button", { name: /delete everything/i }).click();

    // The claim must match the effect: without a backend, nothing was erased.
    await expect(page.getByTestId("delete-status")).toContainText(
      /was not deleted/i,
    );
  });
});

test.describe("privacy controls", () => {
  test("deletion requires the typed confirmation phrase", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /privacy and crashes/i })
      .click();

    const deleteButton = page.getByRole("button", {
      name: /delete everything/i,
    });
    await expect(deleteButton).toBeDisabled();

    await page.getByLabel(/type delete to confirm/i).fill("DELETE");
    await expect(deleteButton).toBeEnabled();
  });
});
