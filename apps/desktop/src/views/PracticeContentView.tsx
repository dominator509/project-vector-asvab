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

import { useEffect, useState } from "react";

import { useBackend } from "../ipc/Backend";
import {
  GENERATABLE_SUBTESTS,
  isGeneratable,
  useCorpusSubtests,
  usePracticeItems,
  STARTER_BATCH,
} from "../state/usePracticeItems";
import { PracticeView } from "./PracticeView";

/**
 * What the learner asked to practise.
 *
 * `objectiveId` is what the plan named, and it survives the navigation from the
 * plan into this view. Without it the plan's objective is a label that nothing
 * acts on, which is what it was before: the plan said `OBJ-WK-SYNONYM-01` and the
 * session served whatever word-knowledge item came first.
 */
export interface PracticeRequest {
  subtest: string;
  objectiveId: string | null;
}

export function PracticeContentView({
  learnerId,
  request,
}: {
  learnerId?: string;
  request?: PracticeRequest | null;
}) {
  const backend = useBackend();
  const [selection, setSelection] = useState<PracticeRequest>({
    subtest: request?.subtest ?? GENERATABLE_SUBTESTS[0],
    objectiveId: request?.objectiveId ?? null,
  });

  // A plan drill pressed after the view already rendered has to re-point the
  // session. The shell holds the request in state, so a new drill arrives as a new
  // object: comparing the object catches two drills of one subtest, which differ
  // only by objective and would be invisible to a subtest-only comparison.
  useEffect(() => {
    if (request) setSelection(request);
  }, [request]);

  const { subtest, objectiveId } = selection;
  const [working, setWorking] = useState(false);
  const offered = useCorpusSubtests(backend.client);
  const canGenerate = isGeneratable(subtest);
  const { state, generate } = usePracticeItems(
    backend.client,
    subtest,
    undefined,
    canGenerate,
    objectiveId,
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
            // Choosing a subtest by hand clears the objective: keeping another
            // subtest's objective would narrow the new subtest's read to an
            // objective it cannot hold.
            onClick={() => setSelection({ subtest: option, objectiveId: null })}
          >
            {option}
          </button>
        ))}
      </div>

      {objectiveId !== null && (
        <p className="hint" data-testid="practice-objective">
          Working on <strong>{objectiveId}</strong>, the objective your plan
          named for {subtest}.
        </p>
      )}

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
          {objectiveId !== null && (
            // The backend falls back to the whole subtest, so reaching this state
            // with an objective set means the *subtest* is empty, not the
            // objective. Saying only "no questions" would leave the learner
            // wondering whether the plan's objective was the problem.
            <p className="hint" data-testid="practice-empty-objective">
              Your plan named {objectiveId}; there is nothing to serve for it or
              for {subtest} as a whole.
            </p>
          )}
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
