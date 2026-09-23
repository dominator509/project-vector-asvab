/**
 * Load practice items for a subtest from the command boundary.
 *
 * Before this existed the practice view rendered `sampleQuestions` — three
 * literals compiled into the frontend bundle — because nothing asked the backend
 * for a question. The corpus is now real, so the view loads from it.
 *
 * ## Why an empty corpus is a state and not a fallback
 *
 * When there is no content, this reports `empty` rather than quietly falling
 * back to the demonstration items. A silent fallback is precisely how the gap
 * stayed invisible: the app looked like it had questions, so nobody checked
 * whether it did. An empty corpus is a fact the learner is entitled to see, and
 * the surface offers to fix it.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { VectorClient } from "../ipc/client";
import type { ContentStatsDto } from "../ipc/types";
import type { PracticeQuestion } from "../data/sample";
import { practiceQuestionsFromItems } from "../data/fromBackend";

/** Items requested on a first run, matching the real per-call ceiling's spirit. */
export const STARTER_BATCH = 40;

/**
 * The subtests the item factory can generate for.
 *
 * Arithmetic Reasoning, Mathematics Knowledge and Mechanical Comprehension are
 * computable: a lever's effort, a gear train's turns and a piston's pressure are
 * arithmetic over the machine's own geometry, so those items carry deterministic
 * proofs. Every other subtest's items are ingested from a public-domain source
 * instead, which is why asking the factory for one is not a slow path but an
 * impossible one.
 */
export const GENERATABLE_SUBTESTS = ["AR", "MK", "MC"] as const;

/**
 * The ten subtests in the order the ASVAB presents them.
 *
 * Duplicated here rather than fetched because the picker must render before any
 * command returns, and it is a fixed fact about the test, not about this device.
 * `Subtest::ORDER` in `vector-domain` is the authority; this mirrors it.
 */
export const SUBTEST_ORDER = [
  "GS",
  "AR",
  "WK",
  "PC",
  "MK",
  "EI",
  "AI",
  "SI",
  "MC",
  "AO",
] as const;

/** Whether the factory can produce items for this subtest. */
export function isGeneratable(subtest: string): boolean {
  return (GENERATABLE_SUBTESTS as readonly string[]).includes(subtest);
}

/**
 * The subtests this device can actually serve a question for.
 *
 * A subtest is offered when the corpus holds items for it or the factory can make
 * them. Offering one that satisfies neither would be a button that always reports
 * an empty set, and hiding an ingested one would strand content the learner
 * already has on disk.
 */
export function useCorpusSubtests(client: VectorClient): string[] {
  const [held, setHeld] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const stats = await client.contentStats();
        if (cancelled) return;
        setHeld(
          stats.by_subtest
            .filter((row) => row.count > 0)
            .map((row) => row.subtest),
        );
      } catch {
        // The picker falls back to the generatable subtests. The practice load
        // itself surfaces the failure, so reporting it twice would be noise.
        if (!cancelled) setHeld([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  return SUBTEST_ORDER.filter(
    (subtest) => isGeneratable(subtest) || held.includes(subtest),
  );
}

/**
 * How many items a practice set holds.
 *
 * The view pages through an array, so the set is fetched up front. Ten is a
 * sitting's worth; `ARG_AR` gives the learner a way to extend it.
 */
export const PRACTICE_SET_SIZE = 10;

export type PracticeContentState =
  | { status: "loading" }
  | { status: "empty"; stats: ContentStatsDto }
  | { status: "ready"; items: PracticeQuestion[]; stats: ContentStatsDto }
  | { status: "error"; message: string };

export interface PracticeContent {
  state: PracticeContentState;
  /** Generate another batch for this subtest, then reload. */
  generate: (subtest: string, count?: number) => Promise<void>;
  reload: () => Promise<void>;
}

/**
 * Fetch `wanted` items for `subtest`, generating a starter batch if the corpus is
 * empty.
 *
 * ## Why the seed is derived from the corpus size
 *
 * The generator is deterministic, so a fixed seed would reproduce the same items
 * on every run and "generate more" would report them all as already present and
 * add nothing. Seeding from the current total means the first run is
 * reproducible and a later run extends the corpus instead of repeating it.
 *
 * ## Why the objective narrows the read
 *
 * A plan that names `OBJ-MK-ALGEBRA-02` and a session that then serves coordinate
 * geometry contradicts itself on the next screen, and the learner has no way to
 * tell which of the two to believe. The objective is passed to the backend, which
 * falls back to the whole subtest when the objective holds no servable item, so a
 * thin objective degrades into practice rather than into an empty screen.
 */
export function usePracticeItems(
  client: VectorClient,
  subtest: string,
  wanted: number = PRACTICE_SET_SIZE,
  canGenerate: boolean = isGeneratable(subtest),
  objectiveId: string | null = null,
): PracticeContent {
  const [state, setState] = useState<PracticeContentState>({
    status: "loading",
  });
  // Guards against a slower earlier load overwriting a newer one, which would
  // show items for a subtest the learner has already navigated away from.
  const run = useRef(0);

  const load = useCallback(async () => {
    const current = ++run.current;
    setState({ status: "loading" });
    try {
      let stats = await client.contentStats();
      if (current !== run.current) return;

      // Only the generatable subtests have anything to generate. Asking the
      // factory for an ingested subtest fails the whole load, which would report
      // "content could not be loaded" when the truth is "this subtest has no
      // items yet".
      if (stats.total === 0 && canGenerate) {
        await client.contentGenerate(subtest, STARTER_BATCH, 0);
        if (current !== run.current) return;
        stats = await client.contentStats();
        if (current !== run.current) return;
      }

      const seen: string[] = [];
      const collected = [];
      for (let i = 0; i < wanted; i += 1) {
        const item = await client.contentNext(subtest, seen, objectiveId);
        if (current !== run.current) return;
        // `null` means the corpus has nothing for this subtest; a repeated id
        // means it is exhausted and would loop.
        if (item === null || seen.includes(item.id)) break;
        seen.push(item.id);
        collected.push(item);
      }

      const items = practiceQuestionsFromItems(collected);
      setState(
        items.length === 0
          ? { status: "empty", stats }
          : { status: "ready", items, stats },
      );
    } catch (error) {
      if (current !== run.current) return;
      setState({
        status: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [client, subtest, wanted, canGenerate, objectiveId]);

  useEffect(() => {
    void load();
  }, [load]);

  const generate = useCallback(
    async (target: string, count: number = STARTER_BATCH) => {
      // Seeded from the current size so a second call extends rather than
      // repeats, and so the first call for a fresh installation is reproducible.
      const stats = await client.contentStats();
      await client.contentGenerate(target, count, stats.total);
      await load();
    },
    [client, load],
  );

  return { state, generate, reload: load };
}
