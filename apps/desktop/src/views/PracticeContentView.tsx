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
import {
  GENERATABLE_SUBTESTS,
  isGeneratable,
  useCorpusSubtests,
  usePracticeItems,
  STARTER_BATCH,
} from "../state/usePracticeItems";
import { PracticeView } from "./PracticeView";

export function PracticeContentView({ learnerId }: { learnerId?: string }) {
  const backend = useBackend();
  const [subtest, setSubtest] = useState<string>(GENERATABLE_SUBTESTS[0]);
  const [working, setWorking] = useState(false);
  const offered = useCorpusSubtests(backend.client);
  const canGenerate = isGeneratable(subtest);
  const { state, generate } = usePracticeItems(
    backend.client,
    subtest,
    undefined,
    canGenerate,
  );

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
        {offered.map((option) => (
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
          <p>No {subtest} questions are available to practise yet.</p>
          <p className="hint">
            {state.stats.sources === 0
              ? "The content library is empty."
              : `This device holds ${state.stats.total} question(s), ${state.stats.servable} of them available.`}
          </p>
          {/*
            Only a generatable subtest gets a button. Every other subtest's items
            are ingested from a public-domain source at pack-build time, so a
            "prepare questions" button here would always fail -- which is exactly
            what the previous version of this surface did for Word Knowledge.
          */}
          {canGenerate ? (
            <button type="button" onClick={onGenerate} disabled={working}>
              {working
                ? "Generating…"
                : `Prepare ${STARTER_BATCH} ${subtest} questions`}
            </button>
          ) : (
            <p className="hint" data-testid="practice-no-generator">
              {subtest} questions come from the study content pack rather than
              from the generator, so they cannot be created here.
            </p>
          )}
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
