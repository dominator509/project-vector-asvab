/**
 * EP-005 acceptance: view behaviour.
 *
 * Covers REQ-005 (practice, distractors, error notebook), REQ-006 (exam
 * navigation constraints), REQ-011 (readiness band, no score claim),
 * REQ-029/035 (crash consent, export, delete) and REQ-044 (search).
 */

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { PracticeView } from "./PracticeView";
import { ExamSimulatorView } from "./ExamSimulatorView";
import { ReadinessView } from "./ReadinessView";
import { ReviewQueueView } from "./ReviewQueueView";
import { PrivacyView, DELETE_CONFIRMATION_PHRASE } from "./PrivacyView";
import { SearchView } from "./SearchView";
import { AccessibleSettings } from "./AccessibleSettings";
import { sampleQuestions, reviewCards } from "../data/sample";
import { DEFAULT_A11Y_SETTINGS } from "../accessibility/settings";

// ---------------------------------------------------------------------------
// REQ-005
// ---------------------------------------------------------------------------

describe("practice view", () => {
  it("shows a question with its answer options", () => {
    render(<PracticeView questions={sampleQuestions} />);
    expect(screen.getByTestId("practice-prompt")).toBeInTheDocument();
    expect(screen.getAllByRole("radio").length).toBeGreaterThan(0);
  });

  it("groups the options in a fieldset with a legend", () => {
    // Screen readers need the options announced as one question.
    render(<PracticeView questions={sampleQuestions} />);
    expect(
      screen.getByRole("group", { name: /choose the best answer/i }),
    ).toBeInTheDocument();
  });

  it("cannot be submitted before an answer is chosen", () => {
    render(<PracticeView questions={sampleQuestions} />);
    expect(
      screen.getByRole("button", { name: /check answer/i }),
    ).toBeDisabled();
  });

  it("reveals the worked solution after answering", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    const solution = screen.getByTestId("worked-solution");
    expect(solution).toBeInTheDocument();
    expect(within(solution).getByText(/source:/i)).toBeInTheDocument();
  });

  it("explains why each distractor is wrong", async () => {
    // REQ-005 requires distractor rationales, not just the correct answer.
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    const rationales = screen.getAllByTestId(/^rationale-\d+$/);
    expect(rationales).toHaveLength(sampleQuestions[0].options.length);
    for (const rationale of rationales) {
      expect(rationale.textContent?.trim().length ?? 0).toBeGreaterThan(0);
    }
  });

  it("captures confidence alongside the answer", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    const group = screen.getByRole("group", { name: /how confident/i });
    expect(group).toBeInTheDocument();

    // The confidence choice must actually be selectable, not merely present.
    const confident = within(group).getByRole("radio", { name: /confident/i });
    await user.click(confident);
    expect(confident).toBeChecked();
  });

  it("records an incorrect attempt in the session summary", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    // Option 0 is wrong for the first sample question.
    await user.click(screen.getAllByRole("radio")[0]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    expect(screen.getByTestId("attempt-summary")).toHaveTextContent(
      /1 attempted, 1 incorrect/i,
    );
  });

  it("starts with an empty error notebook", () => {
    render(<PracticeView questions={sampleQuestions} />);
    expect(screen.getByTestId("notebook-empty")).toBeInTheDocument();
  });

  it("adds a missed question to the error notebook", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    await user.click(screen.getAllByRole("radio")[0]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));
    await user.click(
      screen.getByRole("button", { name: /add to error notebook/i }),
    );

    expect(screen.getByTestId("notebook-list")).toHaveTextContent("q-ar-1");
  });

  it("does not offer the notebook action after a correct answer", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    const correct = sampleQuestions[0].correctIndex;
    await user.click(screen.getAllByRole("radio")[correct]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    expect(
      screen.queryByRole("button", { name: /add to error notebook/i }),
    ).not.toBeInTheDocument();
  });

  it("disables answer inputs once the solution is revealed", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    await user.click(screen.getAllByRole("radio")[0]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    for (const radio of screen.getAllByRole("radio")) {
      expect(radio).toBeDisabled();
    }
  });

  it("handles an empty question set without crashing", () => {
    render(<PracticeView questions={[]} />);
    expect(screen.getByText(/no practice questions/i)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-006
// ---------------------------------------------------------------------------

describe("exam simulator", () => {
  it("states the no-backtracking rule on the CAT form", () => {
    render(<ExamSimulatorView form="cat" />);
    expect(screen.getByTestId("exam-rule")).toHaveTextContent(
      /does not allow returning/i,
    );
  });

  it("does not render a Previous control on the CAT form", () => {
    // A disabled button would imply the action is sometimes available.
    render(<ExamSimulatorView form="cat" />);
    expect(
      screen.queryByRole("button", { name: /previous/i }),
    ).not.toBeInTheDocument();
  });

  it("offers Previous on the paper form after advancing", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="paper" />);

    const previous = screen.getByRole("button", { name: /previous/i });
    expect(previous).toBeInTheDocument();
    // At the first item there is nowhere to go back to.
    expect(previous).toBeDisabled();

    await user.click(screen.getAllByRole("radio")[0]);
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    // After advancing, going back is genuinely available.
    expect(screen.getByRole("button", { name: /previous/i })).toBeEnabled();
  });

  it("states the review rule on the paper form", () => {
    render(<ExamSimulatorView form="paper" />);
    expect(screen.getByTestId("exam-rule")).toHaveTextContent(
      /allows you to review/i,
    );
  });

  it("locks a CAT answer once given", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="cat" />);

    await user.click(screen.getAllByRole("radio")[0]);
    expect(screen.getByTestId("answer-locked")).toBeInTheDocument();

    for (const radio of screen.getAllByRole("radio")) {
      expect(radio).toBeDisabled();
    }
  });

  it("allows changing a paper answer before submitting", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="paper" />);

    const radios = screen.getAllByRole("radio");
    await user.click(radios[0]);
    expect(radios[0]).toBeChecked();

    await user.click(radios[2]);
    expect(radios[2]).toBeChecked();
    expect(screen.queryByTestId("answer-locked")).not.toBeInTheDocument();
  });

  it("disables Next until the current item is answered", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="cat" />);
    expect(screen.getByRole("button", { name: /^next$/i })).toBeDisabled();

    await user.click(screen.getAllByRole("radio")[0]);
    expect(screen.getByRole("button", { name: /^next$/i })).toBeEnabled();
  });

  it("shows a timer", () => {
    render(<ExamSimulatorView form="cat" />);
    expect(screen.getByTestId("exam-timer")).toHaveTextContent(
      /time remaining/i,
    );
  });

  it("summarises the exam on submission", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="paper" />);

    // Answer all three items.
    for (let i = 0; i < 3; i += 1) {
      await user.click(screen.getAllByRole("radio")[0]);
      const label = i === 2 ? /submit exam/i : /^next$/i;
      await user.click(screen.getByRole("button", { name: label }));
    }

    expect(screen.getByTestId("exam-summary")).toHaveTextContent(
      /3 of 3 questions answered/i,
    );
  });

  it("does not claim to predict an official score", async () => {
    const user = userEvent.setup();
    render(<ExamSimulatorView form="paper" />);
    for (let i = 0; i < 3; i += 1) {
      await user.click(screen.getAllByRole("radio")[0]);
      const label = i === 2 ? /submit exam/i : /^next$/i;
      await user.click(screen.getByRole("button", { name: label }));
    }
    expect(
      screen.getByText(/does not predict an official score/i),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-011
// ---------------------------------------------------------------------------

describe("readiness view", () => {
  it("shows a range rather than a point score", () => {
    render(<ReadinessView />);
    const band = screen.getByTestId("readiness-band");
    expect(band.textContent).toMatch(/%–\d+%/);
  });

  it("states that it is not an official score prediction", () => {
    // ADR-010 forbids a precise predicted score before calibration.
    render(<ReadinessView />);
    expect(screen.getByTestId("readiness-disclaimer")).toHaveTextContent(
      /not an official ASVAB or AFQT score prediction/i,
    );
  });

  it("shows calibration confidence", () => {
    render(<ReadinessView />);
    expect(screen.getByTestId("readiness-confidence")).toHaveTextContent(
      /calibration confidence/i,
    );
  });

  it("exposes the band to assistive technology as a meter", () => {
    render(<ReadinessView />);
    const meter = screen.getByRole("meter", { name: /readiness range/i });
    expect(meter).toHaveAttribute("aria-valuetext");
  });

  it("never renders an official-score claim in the shipped state", () => {
    render(<ReadinessView />);
    expect(
      screen.queryByTestId("readiness-illegal-claim"),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Review queue
// ---------------------------------------------------------------------------

describe("review queue", () => {
  it("counts due cards", () => {
    render(<ReviewQueueView cards={reviewCards} />);
    expect(screen.getByTestId("due-count")).toHaveTextContent(
      /2 cards due now/i,
    );
  });

  it("shows FSRS state for the current card", () => {
    render(<ReviewQueueView cards={reviewCards} />);
    expect(screen.getByTestId("card-stability")).toHaveTextContent(/days/);
    expect(screen.getByTestId("card-lapses")).toBeInTheDocument();
  });

  it("offers all four FSRS ratings", () => {
    render(<ReviewQueueView cards={reviewCards} />);
    const group = screen.getByRole("group", {
      name: /how well did you recall/i,
    });
    for (const rating of ["again", "hard", "good", "easy"]) {
      expect(
        within(group).getByRole("button", { name: rating }),
      ).toBeInTheDocument();
    }
  });

  it("lists upcoming reviews", () => {
    render(<ReviewQueueView cards={reviewCards} />);
    expect(screen.getByTestId("upcoming-list")).toHaveTextContent(/in 3 days/i);
  });

  it("reports an empty queue when nothing is due", () => {
    render(<ReviewQueueView cards={[]} />);
    expect(screen.getByTestId("queue-empty")).toBeInTheDocument();
    expect(screen.getByTestId("upcoming-empty")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-029 / REQ-035
// ---------------------------------------------------------------------------

describe("privacy view", () => {
  it("reports the redaction status of each crash report", () => {
    render(<PrivacyView />);
    expect(screen.getByTestId("redaction-crash-001")).toHaveTextContent(
      /verified \(2 passes\)/i,
    );
  });

  it("requires explicit per-report consent before sharing", async () => {
    const user = userEvent.setup();
    render(<PrivacyView />);
    const consent = screen.getByRole("checkbox", {
      name: /share this report/i,
    });
    expect(consent).not.toBeChecked();
    await user.click(consent);
    expect(consent).toBeChecked();
  });

  it("blocks sharing of a report that still contains secrets", () => {
    render(
      <PrivacyView
        reports={[
          {
            id: "crash-leak",
            summary: "Crash with a credential in memory",
            redactionPasses: 1,
            status: "pending",
            containsSecrets: true,
            sharedWithProvider: false,
          },
        ]}
      />,
    );
    expect(
      screen.getByRole("checkbox", { name: /share this report/i }),
    ).toBeDisabled();
    expect(screen.getByTestId("blocked-crash-leak")).toHaveTextContent(
      /blocked/i,
    );
  });

  it("requires a typed confirmation before deleting everything", async () => {
    const user = userEvent.setup();
    render(<PrivacyView />);

    const deleteButton = screen.getByRole("button", {
      name: /delete everything/i,
    });
    expect(deleteButton).toBeDisabled();

    // A near-miss phrase must not unlock the action.
    await user.type(screen.getByLabelText(/type delete to confirm/i), "delete");
    expect(deleteButton).toBeDisabled();

    await user.clear(screen.getByLabelText(/type delete to confirm/i));
    await user.type(
      screen.getByLabelText(/type delete to confirm/i),
      DELETE_CONFIRMATION_PHRASE,
    );
    expect(deleteButton).toBeEnabled();
  });

  it("reports deletion once confirmed", async () => {
    const user = userEvent.setup();
    render(<PrivacyView />);
    await user.type(
      screen.getByLabelText(/type delete to confirm/i),
      DELETE_CONFIRMATION_PHRASE,
    );
    await user.click(
      screen.getByRole("button", { name: /delete everything/i }),
    );
    expect(screen.getByTestId("delete-status")).toBeInTheDocument();
  });

  it("offers a local export", () => {
    render(<PrivacyView />);
    expect(
      screen.getByRole("button", { name: /export local data/i }),
    ).toBeInTheDocument();
  });

  it("handles having no crash reports", () => {
    render(<PrivacyView reports={[]} />);
    expect(screen.getByTestId("no-crashes")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-044
// ---------------------------------------------------------------------------

describe("search view", () => {
  it("shows nothing until a query is typed", () => {
    render(<SearchView />);
    expect(screen.getByTestId("result-count")).toHaveTextContent(
      /type to search/i,
    );
    expect(screen.queryByTestId("search-results")).not.toBeInTheDocument();
  });

  it("finds matching records as the learner types", async () => {
    const user = userEvent.setup();
    render(<SearchView />);
    await user.type(screen.getByLabelText(/search lessons/i), "rate");
    expect(screen.getByTestId("search-results")).toBeInTheDocument();
  });

  it("announces the result count politely", async () => {
    const user = userEvent.setup();
    render(<SearchView />);
    await user.type(screen.getByLabelText(/search lessons/i), "rate");
    expect(screen.getByTestId("result-count")).toHaveAttribute(
      "aria-live",
      "polite",
    );
  });

  it("filters by category", async () => {
    const user = userEvent.setup();
    render(<SearchView />);
    await user.type(screen.getByLabelText(/search lessons/i), "rate");
    await user.click(screen.getByRole("checkbox", { name: /error notebook/i }));

    const results = screen.getByTestId("search-results");
    expect(results).toBeInTheDocument();
    expect(within(results).queryByText(/^lesson$/i)).not.toBeInTheDocument();
  });

  it("reports no results for an unmatched query", async () => {
    const user = userEvent.setup();
    render(<SearchView />);
    await user.type(
      screen.getByLabelText(/search lessons/i),
      "quantum chromodynamics",
    );
    expect(screen.getByTestId("result-count")).toHaveTextContent(/0 results/i);
  });
});

// ---------------------------------------------------------------------------
// REQ-035
// ---------------------------------------------------------------------------

describe("accessibility settings view", () => {
  it("offers each documented setting with help text", () => {
    render(
      <AccessibleSettings
        settings={DEFAULT_A11Y_SETTINGS}
        onChange={() => {}}
      />,
    );
    for (const label of [
      /high contrast/i,
      /reduce motion/i,
      /always show focus outline/i,
      /verbose announcements/i,
      /relaxed text spacing/i,
    ]) {
      expect(screen.getByRole("checkbox", { name: label })).toBeInTheDocument();
    }
  });

  it("reports setting changes to the caller", async () => {
    const user = userEvent.setup();
    const seen: boolean[] = [];
    render(
      <AccessibleSettings
        settings={DEFAULT_A11Y_SETTINGS}
        onChange={(s) => seen.push(s.highContrast)}
      />,
    );
    await user.click(screen.getByRole("checkbox", { name: /high contrast/i }));
    expect(seen).toContain(true);
  });

  it("bounds the font scale control to the supported range", () => {
    render(
      <AccessibleSettings
        settings={DEFAULT_A11Y_SETTINGS}
        onChange={() => {}}
      />,
    );
    const slider = screen.getByLabelText(/font scale/i);
    expect(slider).toHaveAttribute("min", "1");
    expect(slider).toHaveAttribute("max", "2");
  });

  it("reports an out-of-range font scale change through onChange", async () => {
    const user = userEvent.setup();
    const values: number[] = [];
    render(
      <AccessibleSettings
        settings={DEFAULT_A11Y_SETTINGS}
        onChange={(s) => values.push(s.fontScale)}
      />,
    );
    // The range input itself clamps, so any reported value must be in range.
    await user.type(screen.getByLabelText(/font scale/i), "{arrowright}");
    for (const value of values) {
      expect(value).toBeGreaterThanOrEqual(1);
      expect(value).toBeLessThanOrEqual(2);
    }
  });
});
