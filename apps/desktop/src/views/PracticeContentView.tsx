/**
 * Container that loads real practice content and hands it to `PracticeView`.
 *
 * `PracticeView` renders a question and records the attempt; it does not know
 * where questions come from. Before this container existed, `App.tsx` passed it
 * `sampleQuestions` — three literals compiled into the bundle — so the view was
 * practising demonstration content and there was no way to notice.
 *
 * The four states are distinct on purpose. "Empty" in particular is not folded
 * into "ready with no items", because a learner looking at an empty practice
 * screen needs to know whether they have finished everything or whether there is
 * nothing to practise yet, and those need different words.
 */

import { useState } from "react";

import { useBackend } from "../ipc/Backend";
import { usePracticeItems, STARTER_BATCH } from "../state/usePracticeItems";
import { PracticeView } from "./PracticeView";

/**
 * Subtests the factory can generate for.
 *
 * Arithmetic Reasoning and Mathematics Knowledge are computable, so their items
 * carry deterministic proofs. Listing a subtest here that the factory cannot
 * generate for would offer a button that always fails.
 */
const GENERATABLE_SUBTESTS = ["AR", "MK"] as const;

export function PracticeContentView({ learnerId }: { learnerId?: string }) {
  const backend = useBackend();
  const [subtest, setSubtest] = useState<string>(GENERATABLE_SUBTESTS[0]);
  const [working, setWorking] = useState(false);
  const { state, generate } = usePracticeItems(backend.client, subtest);

  async function onGenerate() {
    setWorking(true);
    try {
      await generate(subtest, STARTER_BATCH);
    } finally {
      setWorking(false);
    }
  }

  return (
    <section aria-label="Practice">
      <div
        className="subtest-picker"
        role="group"
        aria-label="Choose a subtest"
      >
        {GENERATABLE_SUBTESTS.map((option) => (
          <button
            key={option}
            type="button"
            aria-pressed={option === subtest}
            onClick={() => setSubtest(option)}
          >
            {option}
          </button>
        ))}
      </div>

      {state.status === "loading" && (
        <p data-testid="practice-loading">Loading practice content…</p>
      )}

      {state.status === "error" && (
        <p className="error" data-testid="practice-error" role="alert">
          Practice content could not be loaded: {state.message}
        </p>
      )}

      {state.status === "empty" && (
        <div data-testid="practice-empty">
          <p>No {subtest} questions have been prepared on this device yet.</p>
          <p className="hint">
            {state.stats.sources === 0
              ? "The content library is empty."
              : `This device holds ${state.stats.total} question(s), none for ${subtest}.`}
          </p>
          <button type="button" onClick={onGenerate} disabled={working}>
            {working
              ? "Generating…"
              : `Prepare ${STARTER_BATCH} ${subtest} questions`}
          </button>
        </div>
      )}

      {state.status === "ready" && (
        <>
          <p className="hint" data-testid="practice-corpus-summary">
            {state.items.length} question(s) from a corpus of{" "}
            {state.stats.total}.
          </p>
          <PracticeView questions={state.items} learnerId={learnerId} />
        </>
      )}
    </section>
  );
}
