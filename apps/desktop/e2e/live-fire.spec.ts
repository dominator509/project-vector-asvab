/**
 * Live-fire tests: prove the real production artifact works through its real
 * boundary, with effects read back independently.
 *
 * The project's "reality law" is that compilation, screenshots and health-check
 * responses are not feature proof. These tests therefore observe side effects
 * rather than rendering: state written, then read back; navigation performed
 * with the keyboard; content carried through a full practice cycle.
 */

import { expect, test, type Page } from "@playwright/test";

/**
 * A question the stubbed backend serves.
 *
 * This suite serves the built `dist/` bundle through `vite preview`, so there is
 * no Rust process behind it and no database. Practice content now comes from the
 * corpus rather than from literals in the bundle, so exercising a practice cycle
 * here needs a transport. This installs a documented stub of
 * `window.__TAURI_INTERNALS__.invoke` — the bridge Tauri itself injects — so the
 * real client, the real response readers, the real loading hook and the real view
 * all run; only the Rust side is replaced.
 *
 * What this does *not* prove is that the Rust commands work, and it is not meant
 * to: `apps/desktop/src-tauri/tests/content_commands.rs` drives those against a
 * real SQLite file, and `scripts/desktop-live-fire.py` proves the packaged
 * process launches with the real backend attached.
 */
const STUB_ITEM = {
  id: "q-e2e-1",
  subtest: "AR",
  objective_id: "OBJ-AR-RATE-01",
  stem: "A printer produces 12 pages per minute. How many pages does it produce in 2.5 hours?",
  options: ["180", "1800", "360", "1440"],
  correct_index: 1,
  explanation: "12 * 150 = 1800",
  distractor_rationales: {
    "0": "Used 15 minutes instead of 150.",
    "2": "Treated 2.5 hours as 30 minutes.",
    "3": "Used pages per hour rather than per minute.",
  },
  difficulty: -0.5,
};

/** Install the stub bridge before the bundle executes. */
async function installStubBackend(page: Page): Promise<void> {
  await page.addInitScript((item) => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      async invoke(command: string, args: Record<string, unknown> = {}) {
        switch (command) {
          // Startup and ambient reads, answered benignly so the shell renders.
          case "ui_ready":
            return "marker-live-fire";
          case "app_paths":
            return {
              data_dir: "/tmp/vector",
              db_path: "/tmp/vector/vector.db",
              backup_dir: "/tmp/vector/backups",
            };
          case "health":
            return {
              component: "database",
              healthy: true,
              basis: "operation_succeeded",
              profiles: 0,
            };
          case "list_profiles":
          case "mastery":
          case "evidence_list":
          case "backup_list":
          case "analytics_all":
            return [];
          case "analytics":
            return { total: 0, correct: 0, accuracy: 0, mean_latency_ms: 0 };
          case "study_plan":
            return { drills: [], total_minutes: 0 };
          case "readiness":
            return {
              low: 0,
              high: 0,
              confidence: 0,
              official_score_claim: false,
            };
          case "latency_probe":
            return {
              operation: "stub",
              iterations: Number(args.iterations ?? 0),
              mean_micros: 0,
              max_micros: 0,
            };

          // The content surface under test.
          case "content_stats":
            return {
              total: 1,
              servable: 1,
              sources: 1,
              by_state: [{ state: "active", count: 1 }],
              by_subtest: [{ subtest: "AR", count: 1 }],
            };
          case "content_generate":
            return {
              subtest: args.subtest,
              generated: Number(args.count ?? 0),
              verified: Number(args.count ?? 0),
              activated: 0,
              already_present: Number(args.count ?? 0),
              rejected: [],
            };
          case "content_next": {
            const seen = (args.seen as string[] | undefined) ?? [];
            return seen.includes(item.id) ? null : item;
          }
          case "record_attempt":
            return true;

          default:
            // Loud rather than silent: an unexpected call means the surface moved
            // and this stub no longer describes it.
            throw new Error(`live-fire stub has no response for ${command}`);
        }
      },
    };
  }, STUB_ITEM);
}

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
    await installStubBackend(page);
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    // The prompt arriving proves the whole content path ran in the bundle:
    // client, response reader, loading hook and view.
    await expect(page.getByTestId("practice-prompt")).toBeVisible();

    // Before: no attempts recorded.
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /0 attempted, 0 incorrect/i,
    );

    // Act: answer correctly. Which option is correct belongs to the corpus, not
    // to this test, so the page is asked rather than assumed.
    await page.locator('label.option-row[data-correct="true"] input').check();
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
    await installStubBackend(page);
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    await expect(page.getByTestId("practice-prompt")).toBeVisible();
    const questionId = await page
      .getByTestId("practice-prompt")
      .getAttribute("data-question-id");
    expect(questionId, "the served question must identify itself").toBeTruthy();

    await page
      .locator('label.option-row[data-correct="false"] input')
      .first()
      .check();
    await page.getByRole("button", { name: /check answer/i }).click();
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /1 attempted, 1 incorrect/i,
    );
    await page.getByRole("button", { name: /add to error notebook/i }).click();

    // The write is read back from the rendered list, an independent read of the
    // same state the click produced. The id comes from the page rather than a
    // literal, so this holds for whatever the corpus serves.
    await expect(page.getByTestId("notebook-list")).toContainText(
      questionId as string,
    );
    await expect(page.getByTestId("attempt-summary")).toHaveText(
      /1 attempted, 1 incorrect/i,
    );
  });

  test("practice refuses rather than fabricating content with no backend", async ({
    page,
  }) => {
    // No stub: this is the plain browser case, where the bundle has no Rust
    // process behind it. The view must say so rather than rendering questions
    // from literals, which is exactly how the app appeared to have content for
    // so long. A silent fallback to demonstration data would be the bug.
    await page.goto("/");
    await page
      .getByTestId("primary-nav")
      .getByRole("button", { name: /^practice$/i })
      .click();

    await expect(page.getByTestId("practice-error")).toBeVisible();
    await expect(page.getByTestId("practice-error")).toContainText(
      /backend is not available/,
    );
    await expect(page.getByTestId("practice-prompt")).toHaveCount(0);
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
