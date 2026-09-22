/**
 * Smoke test: proves the built production artifact actually serves and renders.
 *
 * EP-009's execplan requires "offline launch" evidence, and HARNESS_LAWS.md law
 * 4 requires observed command evidence for a PASS. This is a Playwright spec
 * that runs against the production bundle with the network unavailable to the
 * page, so it demonstrates the offline claim rather than asserting it.
 */

import { expect, test } from "@playwright/test";

import { PC_ITEM, installStubBackend } from "./stubBackend";

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

    // The bridge is stubbed in the page, not fetched, so it is available with the
    // network blocked -- which is the situation a learner on a disconnected
    // machine is in. Practice content is served by the local backend, never by a
    // request, and this is what shows it.
    await installStubBackend(page);
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

    await installStubBackend(page);
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

  test("a comprehension passage reaches the learner with its question", async ({
    page,
  }) => {
    // A Paragraph Comprehension item is a passage plus a question about it. The
    // passage travels through the command contract, the response reader and the
    // view; if any of the three drops it, the item is unanswerable and this fails.
    await installStubBackend(page, { items: [PC_ITEM] });
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    // The picker opens on a generatable subtest, so the learner chooses Paragraph
    // Comprehension. That the subtest is offered at all is itself evidence that the
    // corpus list reached the interface.
    await page
      .getByRole("group", { name: /choose a subtest/i })
      .getByRole("button", { name: "PC" })
      .click();

    const passage = page.getByTestId("practice-passage");
    await expect(passage).toBeVisible();
    await expect(passage).toContainText(
      "The cooling tower removes 1897 litres",
    );

    // Read the passage, then answer from it.
    await page.locator('label.option-row[data-correct="true"] input').check();
    await page.getByRole("button", { name: /check answer/i }).click();
    await expect(page.getByTestId("worked-solution")).toBeVisible();
  });
});

test.describe("content packs in the built bundle", () => {
  test("the content manager shows an installed pack and its signature state", async ({
    page,
  }) => {
    await installStubBackend(page, {
      packs: [
        {
          id: "pack-core-asvab-1",
          name: "core-asvab",
          version: 1,
          status: "active",
          signer: "86b0ed0e",
          content_hash: "sha256:abc",
          schema_version: 1,
          item_count: 1234,
          created_at: "2026-09-22T00:00:00Z",
          signature_valid: true,
        },
      ],
    });
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /content/i })
      .click();

    const table = page.getByTestId("packs-table");
    await expect(table).toBeVisible();
    await expect(table).toContainText("core-asvab");
    await expect(table).toContainText("1234");
    await expect(table).toContainText("verified");

    // Installing with no path is refused by the interface rather than by the
    // backend, and a pack this stub cannot verify is refused by the backend with the
    // reason shown.
    await expect(page.getByTestId("pack-install")).toBeDisabled();
    await page.getByTestId("pack-path-input").fill("/tmp/some.vpack");
    await page.getByTestId("pack-install").click();
    await expect(page.getByTestId("action-error")).toContainText(
      /not signed by the key this installation trusts/,
    );
  });

  test("a fresh installation says there are no packs", async ({ page }) => {
    await installStubBackend(page);
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /content/i })
      .click();
    await expect(page.getByTestId("packs-empty")).toBeVisible();
  });
});
