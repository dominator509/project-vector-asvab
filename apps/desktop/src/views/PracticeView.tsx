/**
 * Practice view (REQ-005, REQ-010): custom drills, worked solutions, distractor
 * rationales, confidence capture, the error notebook, and durable recording.
 *
 * When a learner is selected and the local backend is reachable, every answered
 * question is written to the database through `record_attempt`, and the stored
 * totals are then read back and shown. The readback is the point: a UI that
 * displayed only its own in-memory count could not distinguish "saved" from
 * "the write was rejected", and the whole product claim is that progress is
 * real and inspectable.
 *
 * The session counters stay session-local on purpose. They answer "how did this
 * sitting go", which is a different question from "what is stored".
 */

import { useCallback, useMemo, useRef, useState } from "react";

import { useBackend } from "../ipc/Backend";
import type { AnalyticsDto } from "../ipc/types";
import type { PracticeQuestion } from "../data/sample";

/** How confident the learner felt, captured per attempt. */
export type Confidence = "guessing" | "unsure" | "confident";

export interface AttemptRecord {
  questionId: string;
  chosenIndex: number | null;
  correct: boolean;
  confidence: Confidence;
}

export interface PracticeViewProps {
  questions: PracticeQuestion[];
  /** The learner whose attempts are recorded; omitted, nothing is persisted. */
  learnerId?: string;
}

/** What happened when the attempt was written to the database. */
type StorageState =
  | { status: "idle" }
  | { status: "saving" }
  | { status: "stored"; storedTotal: number }
  | { status: "duplicate"; storedTotal: number }
  | { status: "failed"; message: string };

export function PracticeView({ questions, learnerId }: PracticeViewProps) {
  const backend = useBackend();
  const [index, setIndex] = useState(0);
  const [chosen, setChosen] = useState<number | null>(null);
  const [confidence, setConfidence] = useState<Confidence>("unsure");
  const [revealed, setRevealed] = useState(false);
  const [attempts, setAttempts] = useState<AttemptRecord[]>([]);
  const [notebook, setNotebook] = useState<string[]>([]);
  const [storage, setStorage] = useState<StorageState>({ status: "idle" });
  const startedAt = useRef<number>(Date.now());

  const question = questions[index];

  /**
   * Questions the learner got wrong, surfaced as an error notebook so mistakes
   * become review material rather than being forgotten.
   */
  const errorEntries = useMemo(
    () => attempts.filter((a) => !a.correct),
    [attempts],
  );

  /**
   * Write the attempt and read the stored total back.
   *
   * The attempt id is derived from the learner, question and attempt number so
   * that a retry of the same submission is idempotent rather than producing
   * duplicate history — the same guarantee the database enforces.
   */
  const persist = useCallback(
    async (
      questionId: string,
      subtest: string,
      correct: boolean,
      latencyMs: number,
      ordinal: number,
    ) => {
      if (!learnerId || !backend.available || !question) return;
      setStorage({ status: "saving" });
      try {
        const inserted = await backend.client.recordAttempt({
          attemptId: `${learnerId}:${questionId}:${ordinal}`,
          learnerId,
          subtest,
          questionId,
          correct,
          latencyMs: Math.max(0, Math.round(latencyMs)),
        });
        const stored: AnalyticsDto = await backend.client.analytics(
          learnerId,
          subtest,
        );
        setStorage(
          inserted
            ? { status: "stored", storedTotal: stored.total }
            : { status: "duplicate", storedTotal: stored.total },
        );
      } catch (error) {
        setStorage({
          status: "failed",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [backend.available, backend.client, learnerId, question],
  );

  if (!question) {
    return (
      <section aria-label="Practice">
        <p>No practice questions are loaded.</p>
      </section>
    );
  }

  const submit = () => {
    if (chosen === null) return;
    const correct = chosen === question.correctIndex;
    const latency = Date.now() - startedAt.current;
    const ordinal = attempts.length + 1;
    setAttempts((prev) => [
      ...prev,
      { questionId: question.id, chosenIndex: chosen, correct, confidence },
    ]);
    setRevealed(true);
    void persist(question.id, question.subtest, correct, latency, ordinal);
  };

  const next = () => {
    setChosen(null);
    setConfidence("unsure");
    setRevealed(false);
    setStorage({ status: "idle" });
    startedAt.current = Date.now();
    setIndex((i) => Math.min(i + 1, questions.length - 1));
  };

  const addToNotebook = () => {
    setNotebook((prev) =>
      prev.includes(question.id) ? prev : [...prev, question.id],
    );
  };

  return (
    <section aria-labelledby="practice-heading">
      <h3 id="practice-heading">
        Question {index + 1} of {questions.length}
      </h3>

      <p className="subtest-tag">Subtest: {question.subtest}</p>
      <p data-testid="practice-prompt" data-question-id={question.id}>
        {question.prompt}
      </p>

      {/*
        A fieldset with a legend groups the options so a screen reader announces
        them as one question rather than a bare list.
      */}
      <fieldset>
        <legend>Choose the best answer</legend>
        {question.options.map((option, optionIndex) => (
          <label
            key={option}
            className="option-row"
            data-option-index={optionIndex}
            /*
             * Which option is correct is real state the view already holds, and
             * stating it on the element is what lets the packaged live-fire test
             * answer correctly or incorrectly on purpose. Without it that test
             * has to hard-code option text, which ties it to one particular
             * corpus and made it fail the moment real generated items replaced
             * the demonstration ones.
             */
            data-correct={optionIndex === question.correctIndex}
          >
            <input
              type="radio"
              name={`answer-${question.id}`}
              value={optionIndex}
              checked={chosen === optionIndex}
              onChange={() => setChosen(optionIndex)}
              disabled={revealed}
              aria-describedby={
                revealed ? `rationale-${optionIndex}` : undefined
              }
            />
            <span>{option}</span>
            {revealed && optionIndex === question.correctIndex && (
              <span className="correct-tag"> — correct</span>
            )}
            {revealed && (
              <span
                id={`rationale-${optionIndex}`}
                className="rationale"
                data-testid={`rationale-${optionIndex}`}
              >
                {optionIndex === question.correctIndex
                  ? "Correct."
                  : (question.distractorRationales[optionIndex] ??
                    "Not the best answer.")}
              </span>
            )}
          </label>
        ))}
      </fieldset>

      <fieldset>
        <legend>How confident were you?</legend>
        {(["guessing", "unsure", "confident"] as Confidence[]).map((level) => (
          <label key={level} className="option-row">
            <input
              type="radio"
              name={`confidence-${question.id}`}
              value={level}
              checked={confidence === level}
              onChange={() => setConfidence(level)}
              disabled={revealed}
            />
            <span>{level}</span>
          </label>
        ))}
      </fieldset>

      {!revealed ? (
        <button type="button" onClick={submit} disabled={chosen === null}>
          Check answer
        </button>
      ) : (
        <div className="solution" data-testid="worked-solution">
          <h4>Worked solution</h4>
          <p>{question.explanation}</p>
          <p className="source-line">
            {question.objectiveId ? (
              <>
                Objective: <code>{question.objectiveId}</code>
              </>
            ) : question.sourceId ? (
              <>
                Source: <code>{question.sourceId}</code>
              </>
            ) : null}
          </p>
          <button type="button" onClick={next}>
            Next question
          </button>
          {chosen !== question.correctIndex && (
            <button type="button" onClick={addToNotebook}>
              Add to error notebook
            </button>
          )}
        </div>
      )}

      <section aria-labelledby="notebook-heading" className="notebook">
        <h4 id="notebook-heading">Error notebook</h4>
        {notebook.length === 0 ? (
          <p data-testid="notebook-empty">
            No entries yet. Mistakes you save appear here.
          </p>
        ) : (
          <ul data-testid="notebook-list">
            {notebook.map((id) => (
              <li key={id}>{id}</li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="attempts-heading" className="attempts">
        <h4 id="attempts-heading">Attempts this session</h4>
        <p data-testid="attempt-summary">
          {attempts.length} attempted, {errorEntries.length} incorrect
        </p>

        {/*
          What the database holds, read back after the write. Absent when no
          learner is selected, because in that case nothing is being recorded
          and claiming otherwise would be the lie this line exists to prevent.
        */}
        {learnerId && <StorageStatus state={storage} />}
      </section>
    </section>
  );
}

function StorageStatus({ state }: { state: StorageState }) {
  switch (state.status) {
    case "idle":
      return null;
    case "saving":
      return (
        <p role="status" data-testid="attempt-persistence">
          Saving this attempt…
        </p>
      );
    case "stored":
      return (
        <p role="status" data-testid="attempt-persistence">
          Saved. Stored attempts for this subtest: {state.storedTotal}.
        </p>
      );
    case "duplicate":
      return (
        <p role="status" data-testid="attempt-persistence">
          Already recorded, so nothing was counted twice. Stored attempts for
          this subtest: {state.storedTotal}.
        </p>
      );
    case "failed":
      return (
        <p role="alert" data-testid="attempt-persistence">
          This attempt was not saved: {state.message}
        </p>
      );
  }
}
