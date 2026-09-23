/**
 * Smoke test: proves the built production artifact actually serves and renders.
 *
 * EP-009's execplan requires "offline launch" evidence, and HARNESS_LAWS.md law
 * 4 requires observed command evidence for a PASS. This is a Playwright spec
 * that runs against the production bundle with the network unavailable to the
 * page, so it demonstrates the offline claim rather than asserting it.
 */

import { expect, test } from "@playwright/test";

import { AR_ITEM, PC_ITEM, installStubBackend } from "./stubBackend";

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

  test("the plan's objective reaches the session it starts", async ({
    page,
  }) => {
    // The stub holds two Arithmetic Reasoning items in two different objectives,
    // and the one the plan did *not* name comes first. A session that ignored the
    // objective would serve it and this spec would see the wrong question.
    const other: typeof AR_ITEM = {
      ...AR_ITEM,
      id: "q-e2e-ar-0",
      objective_id: "OBJ-AR-PERCENT-01",
      stem: "What is 15 percent of 240?",
      options: ["24", "36", "48", "60"],
      correct_index: 1,
      explanation: "0.15 * 240 = 36",
    };
    await installStubBackend(page, {
      items: [other, AR_ITEM],
      drills: [
        {
          subtest: "AR",
          minutes: 10,
          reason: "weakness",
          objective_id: "OBJ-AR-RATE-01",
        },
      ],
    });
    await page.goto("/");

    // A plan belongs to a learner, so the session starts the way a real one does:
    // create the profile, then read the plan the backend produced for it.
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /getting started/i })
      .click();
    await page.getByLabel(/your name or initials/i).fill("E2E");
    await page.getByRole("button", { name: /create profile/i }).click();
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /today/i })
      .click();

    const start = page.getByTestId("drill-start-AR");
    await expect(start).toContainText("OBJ-AR-RATE-01");
    await start.click();

    // The session says which objective it is serving...
    await expect(page.getByTestId("practice-objective")).toContainText(
      "OBJ-AR-RATE-01",
    );
    // ...and the question is the item that objective holds, not the first item of
    // the subtest. The plan is an instruction, so the session has to follow it.
    await expect(page.getByText(AR_ITEM.stem)).toBeVisible();
    await expect(page.getByText(other.stem)).toHaveCount(0);
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
          objectives: [
            {
              objective_id: "OBJ-SI-TOOLS-01",
              subtest: "SI",
              title: "Shop Information: which tool performs a purpose",
              prerequisites: [],
              expected_correct: 0.53,
              responses: 0,
              basis: "declared from the items' difficulty scale",
            },
            {
              objective_id: "OBJ-AI-FUNCTION-01",
              subtest: "AI",
              title: "Auto Information: what a component is for",
              prerequisites: ["OBJ-SI-TOOLS-01"],
              expected_correct: 0.53,
              responses: 0,
              basis: "declared from the items' difficulty scale",
            },
          ],
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

    // What the pack teaches travels with it, including the prerequisite edge and the fact
    // that its difficulty figures rest on no responses.
    const objectives = page.getByTestId("pack-objectives-pack-core-asvab-1");
    await expect(objectives).toContainText("which tool performs a purpose");
    await expect(
      page.getByTestId("objective-after-OBJ-AI-FUNCTION-01"),
    ).toContainText("after OBJ-SI-TOOLS-01");
    await expect(
      page.getByTestId("objective-calibration-OBJ-AI-FUNCTION-01"),
    ).toContainText("declared, no responses recorded");

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
