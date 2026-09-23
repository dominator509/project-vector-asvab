/**
 * A stub of the Tauri bridge, for E2E specs that run against the production bundle.
 *
 * ## Why a stub exists at all
 *
 * These specs serve the built `dist/` output through `vite preview`: there is no
 * Rust process behind the page and no database. Practice content is served by the
 * backend, so exercising a practice cycle here needs *some* transport. This installs
 * `window.__TAURI_INTERNALS__.invoke` — the bridge Tauri itself injects — so the
 * real client, the real response readers, the real loading hook and the real views
 * all run; only the Rust side is replaced.
 *
 * ## What it does not prove
 *
 * Nothing about the Rust commands. Those are driven against a real SQLite file by
 * `apps/desktop/src-tauri/tests/content_commands.rs`, and the packaged process is
 * launched for real by `scripts/desktop-live-fire.py`.
 *
 * ## Why one copy
 *
 * It used to exist three times: a richer one in `live-fire.spec.ts`, and nothing at
 * all in the other two specs, whose practice tests went on asserting option text
 * from `sample.ts` long after the view had stopped serving it. Two of those tests
 * had been failing since the corpus replaced the literals. Sharing the stub keeps
 * the served shape in one place, and it is checked against the real contract by the
 * same readers the application uses: a field the backend sends that this stub omits
 * makes the page throw, loudly.
 */

import type { Page } from "@playwright/test";

/** An item as the command boundary serves it. */
export interface StubItem {
  id: string;
  subtest: string;
  objective_id: string;
  stem: string;
  /** `null` for every subtest but Paragraph Comprehension. */
  passage: string | null;
  options: string[];
  correct_index: number;
  explanation: string;
  distractor_rationales: Record<string, string>;
  difficulty: number;
}

/**
 * An Arithmetic Reasoning item: the shape a generated item has.
 */
export const AR_ITEM: StubItem = {
  id: "q-e2e-ar-1",
  subtest: "AR",
  objective_id: "OBJ-AR-RATE-01",
  stem: "A printer produces 12 pages per minute. How many pages does it produce in 2.5 hours?",
  passage: null,
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

/**
 * A Paragraph Comprehension item: a passage, and a question about a detail in it.
 *
 * The passage is here because an item of this subtest is unanswerable without one,
 * and the reader refuses a response that omits the field. This is what lets a spec
 * drive the passage through the client, the view and the renderer.
 */
export const PC_ITEM: StubItem = {
  id: "q-e2e-pc-1",
  subtest: "PC",
  objective_id: "OBJ-PC-DETAIL-01",
  stem: "According to the passage, which of the following is stated?",
  passage:
    "The cooling tower removes 1897 litres of water from the air each working day. " +
    "The process works best when the outside air is dry, below 24 percent humidity. " +
    "Operators check the gauge every morning before the shift, and keep 365 days of " +
    "readings on file.",
  options: [
    "The cooling tower removes 1897 litres of water from the air each working day.",
    "The cooling tower removes 1898 litres of water from the air each working day.",
    "The cooling tower removes 1904 litres of water from the air each working day.",
    "The cooling tower removes 1908 litres of water from the air each working day.",
  ],
  correct_index: 0,
  explanation:
    'The passage states: "The cooling tower removes 1897 litres of water from the air each working day."',
  distractor_rationales: {
    "1": 'The passage states "1897" here; it does not state "1898".',
    "2": 'The passage states "1897" here; it does not state "1904".',
    "3": 'The passage states "1897" here; it does not state "1908".',
  },
  difficulty: 0.4,
};

export interface StubOptions {
  /** Items `content_next` may serve. Defaults to the Arithmetic Reasoning item. */
  items?: StubItem[];
  /** Packs `content_packs` reports. Empty by default, which is a fresh install. */
  packs?: StubPack[];
  /** Value `ui_ready` returns, for specs that read the marker back. */
  marker?: string;
}

/**
 * A pack as the registry reports it.
 *
 * `signature_valid` is here because the content manager shows it, and a stub that
 * omitted it would let that column render `undefined` without failing.
 */
export interface StubPack {
  id: string;
  name: string;
  version: number;
  status: string;
  signer: string;
  content_hash: string;
  schema_version: number;
  item_count: number;
  created_at: string;
  signature_valid: boolean;
  /** What the pack teaches, as the content manager lists it. */
  objectives?: StubObjective[];
}

/** One objective a pack declares, with what it claims about its difficulty. */
export interface StubObjective {
  objective_id: string;
  subtest: string;
  title: string;
  prerequisites: string[];
  expected_correct: number | null;
  responses: number | null;
  basis: string | null;
}

/**
 * Install the stub bridge before the bundle executes.
 *
 * Every command the shell issues during a study session is answered. The `default`
 * arm throws rather than returning undefined, so a surface that starts calling a
 * command this stub does not describe fails loudly instead of rendering blank.
 */
export async function installStubBackend(
  page: Page,
  options: StubOptions = {},
): Promise<void> {
  const items = options.items ?? [AR_ITEM];
  const packs = options.packs ?? [];
  const marker = options.marker ?? "marker-e2e";

  await page.addInitScript(
    ({ items, packs, marker }) => {
      const subtests = [...new Set(items.map((item) => item.subtest))].sort();
      const w = window as unknown as Record<string, unknown>;
      w.__TAURI_INTERNALS__ = {
        async invoke(command: string, args: Record<string, unknown> = {}) {
          switch (command) {
            // Startup and ambient reads, answered benignly so the shell renders.
            case "ui_ready":
              return marker;
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

            // The content surface under test. `by_subtest` is derived from the
            // items rather than hard-coded, so a spec that serves PC sees PC
            // offered in the picker and a spec that serves AR does not.
            case "content_stats":
              return {
                total: items.length,
                servable: items.length,
                sources: items.length === 0 ? 0 : 1,
                by_state:
                  items.length === 0
                    ? []
                    : [{ state: "active", count: items.length }],
                by_subtest: subtests.map((subtest) => ({
                  subtest,
                  count: items.filter((item) => item.subtest === subtest)
                    .length,
                })),
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
              const candidates = items.filter(
                (item) => item.subtest === args.subtest,
              );
              return (
                candidates.find((item) => !seen.includes(item.id)) ??
                candidates[0] ??
                null
              );
            }
            case "record_attempt":
              return true;

            // The pack registry, and the two actions the content manager offers.
            // Installation is refused here for the same reason the backend refuses
            // it: this stub trusts no signing key, so it cannot verify a pack.
            case "content_packs":
              return packs;
            case "content_pack_install":
              throw new Error(
                "the pack is not signed by the key this installation trusts",
              );
            case "content_pack_rollback":
              return packs.find((pack) => pack.name === args.name) ?? null;

            // The corpus half of the content manager. It has to answer as well as the
            // pack half, or the view sits in its error state and the packs panel is
            // never reached -- which is exactly how these tests failed the first time.
            case "content_manager": {
              const sourceId = "ev-e2e-source";
              return {
                stats: {
                  total: items.length,
                  servable: items.length,
                  sources: items.length === 0 ? 0 : 1,
                  by_state:
                    items.length === 0
                      ? []
                      : [{ state: "active", count: items.length }],
                  by_subtest: subtests.map((subtest) => ({
                    subtest,
                    count: items.filter((item) => item.subtest === subtest)
                      .length,
                  })),
                },
                sources:
                  items.length === 0
                    ? []
                    : [
                        {
                          id: sourceId,
                          title: "Test source",
                          url: "https://example.invalid/source",
                          licence: "Public domain in the USA",
                          trust: 0.9,
                          item_count: items.length,
                        },
                      ],
                items: items.map((item) => ({
                  id: item.id,
                  subtest: item.subtest,
                  state: "active",
                  objective_id: item.objective_id,
                  preview: item.stem.slice(0, 120),
                  correct_answer: item.options[item.correct_index] ?? "",
                  reviewer: "content-reviewer",
                  content_hash: `sha256:${item.id}`,
                  sources: [sourceId],
                })),
              };
            }

            default:
              // Loud rather than silent: an unexpected call means the surface moved
              // and this stub no longer describes it.
              throw new Error(`the E2E stub has no response for ${command}`);
          }
        },
      };
    },
    { items, packs, marker },
  );
}
