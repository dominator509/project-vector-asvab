/**
 * Practice view (REQ-005): custom drills, worked solutions, distractor
 * rationales, confidence capture and the error notebook.
 */

import { useMemo, useState } from "react";
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
}

export function PracticeView({ questions }: PracticeViewProps) {
  const [index, setIndex] = useState(0);
  const [chosen, setChosen] = useState<number | null>(null);
  const [confidence, setConfidence] = useState<Confidence>("unsure");
  const [revealed, setRevealed] = useState(false);
  const [attempts, setAttempts] = useState<AttemptRecord[]>([]);
  const [notebook, setNotebook] = useState<string[]>([]);

  const question = questions[index];

  /**
   * Questions the learner got wrong, surfaced as an error notebook so mistakes
   * become review material rather than being forgotten.
   */
  const errorEntries = useMemo(
    () => attempts.filter((a) => !a.correct),
    [attempts],
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
    setAttempts((prev) => [
      ...prev,
      { questionId: question.id, chosenIndex: chosen, correct, confidence },
    ]);
    setRevealed(true);
  };

  const next = () => {
    setChosen(null);
    setConfidence("unsure");
    setRevealed(false);
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
      <p data-testid="practice-prompt">{question.prompt}</p>

      {/*
        A fieldset with a legend groups the options so a screen reader announces
        them as one question rather than a bare list.
      */}
      <fieldset>
        <legend>Choose the best answer</legend>
        {question.options.map((option, optionIndex) => (
          <label key={option} className="option-row">
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
            Source: <code>{question.sourceId}</code>
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
      </section>
    </section>
  );
}
