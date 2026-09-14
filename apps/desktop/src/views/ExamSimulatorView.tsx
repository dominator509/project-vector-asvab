/**
 * Exam simulator view (REQ-006).
 *
 * Mirrors the domain rules in `vector-domain::exam`: the CAT form commits each
 * answer and hides backward navigation, while the paper form allows review.
 * The UI must not offer an affordance the domain would refuse — showing a
 * "Previous" button that errors on click would be worse than not showing it.
 */

import { useEffect, useState } from "react";

export interface ExamSimulatorViewProps {
  form: "cat" | "paper";
}

interface SimItem {
  id: string;
  prompt: string;
}

const CAT_ITEMS: SimItem[] = [
  {
    id: "c1",
    prompt: "A unit rate converts to a total by multiplying by what?",
  },
  { id: "c2", prompt: "Which word most nearly means CANDID?" },
  { id: "c3", prompt: "What is the main idea of the sleep passage?" },
];

const PAPER_ITEMS: SimItem[] = [
  {
    id: "p1",
    prompt: "A unit rate converts to a total by multiplying by what?",
  },
  { id: "p2", prompt: "Which word most nearly means CANDID?" },
  { id: "p3", prompt: "What is the main idea of the sleep passage?" },
];

export function ExamSimulatorView({ form }: ExamSimulatorViewProps) {
  const items = form === "cat" ? CAT_ITEMS : PAPER_ITEMS;
  const allowsBacktracking = form === "paper";

  const [cursor, setCursor] = useState(0);
  const [answers, setAnswers] = useState<Record<number, number | null>>({});
  const [locked, setLocked] = useState<number[]>([]);
  const [finished, setFinished] = useState(false);
  const [remaining, setRemaining] = useState(
    form === "cat" ? 39 * 60 : 36 * 60,
  );

  const item = items[cursor];
  const isLocked = locked.includes(cursor);

  /**
   * The countdown is driven by the same rules as the domain session: time never
   * goes below zero, and reaching zero submits the exam.
   */
  useEffect(() => {
    if (finished) return;
    const handle = window.setInterval(() => {
      setRemaining((prev) => {
        if (prev <= 1) {
          setFinished(true);
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => window.clearInterval(handle);
  }, [finished]);

  const answer = (option: number) => {
    if (isLocked) return;
    setAnswers((prev) => ({ ...prev, [cursor]: option }));
    if (!allowsBacktracking) {
      setLocked((prev) => [...prev, cursor]);
    }
  };

  const next = () => {
    if (cursor + 1 >= items.length) {
      setFinished(true);
      return;
    }
    setCursor((c) => c + 1);
  };

  const previous = () => {
    if (!allowsBacktracking) return;
    setCursor((c) => Math.max(0, c - 1));
  };

  if (finished) {
    const answered = Object.values(answers).filter((v) => v !== null).length;
    return (
      <section aria-labelledby="exam-done">
        <h3 id="exam-done">Exam submitted</h3>
        <p data-testid="exam-summary">
          {answered} of {items.length} questions answered.
        </p>
        <p>
          This is a practice simulation. VECTOR does not predict an official
          score.
        </p>
      </section>
    );
  }

  return (
    <section aria-labelledby="exam-heading" data-testid={`exam-${form}`}>
      <h3 id="exam-heading">
        {form === "cat" ? "CAT simulator" : "Paper simulator"}
      </h3>

      {/*
        The navigation constraint is stated in the UI, not only enforced in
        code, so the learner understands the real test's rules.
      */}
      <p className="exam-rule" data-testid="exam-rule">
        {allowsBacktracking
          ? "This form allows you to review and change earlier answers."
          : "This form does not allow returning to a previous question. Each answer is final."}
      </p>

      <p className="timer" data-testid="exam-timer">
        Time remaining: {Math.floor(remaining / 60)} minutes
      </p>

      <p>
        Question {cursor + 1} of {items.length}
      </p>
      <p data-testid="exam-prompt">{item.prompt}</p>

      <fieldset>
        <legend>Select an answer</legend>
        {[0, 1, 2, 3].map((option) => (
          <label key={option} className="option-row">
            <input
              type="radio"
              name={`exam-${form}-${cursor}`}
              checked={answers[cursor] === option}
              onChange={() => answer(option)}
              disabled={isLocked}
            />
            <span>Option {option + 1}</span>
          </label>
        ))}
      </fieldset>

      {isLocked && (
        <p className="locked-note" data-testid="answer-locked">
          Answer committed. It cannot be changed.
        </p>
      )}

      <div className="exam-controls">
        {/*
          Backward navigation is not rendered at all on the CAT form. A disabled
          button would imply the action is sometimes available.
        */}
        {allowsBacktracking && (
          <button type="button" onClick={previous} disabled={cursor === 0}>
            Previous
          </button>
        )}
        <button
          type="button"
          onClick={next}
          disabled={answers[cursor] === undefined || answers[cursor] === null}
        >
          {cursor + 1 >= items.length ? "Submit exam" : "Next"}
        </button>
      </div>
    </section>
  );
}
