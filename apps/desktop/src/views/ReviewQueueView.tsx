/**
 * Review queue view (REQ-005 / REQ-043 surface): spaced-repetition cards with
 * their real FSRS state surfaced to the learner.
 */

import { useMemo, useState } from "react";
import type { ReviewCard } from "../data/sample";

export type Rating = "again" | "hard" | "good" | "easy";

export interface ReviewQueueViewProps {
  cards: ReviewCard[];
}

/** A card is due when its due date has arrived or passed. */
export function isDue(card: ReviewCard): boolean {
  return card.dueInDays <= 0;
}

export function ReviewQueueView({ cards }: ReviewQueueViewProps) {
  const [rated, setRated] = useState<Record<string, Rating>>({});

  const due = useMemo(() => cards.filter(isDue), [cards]);
  const upcoming = useMemo(
    () =>
      cards.filter((c) => !isDue(c)).sort((a, b) => a.dueInDays - b.dueInDays),
    [cards],
  );

  const current = due.find((c) => !(c.id in rated));

  const rate = (rating: Rating) => {
    if (!current) return;
    setRated((prev) => ({ ...prev, [current.id]: rating }));
  };

  return (
    <section aria-labelledby="review-heading">
      <h3 id="review-heading">Review queue</h3>

      <p data-testid="due-count">
        {due.length} card{due.length === 1 ? "" : "s"} due now.
      </p>

      {current ? (
        <article className="review-card" aria-labelledby="current-card">
          <h4 id="current-card">{current.subtest}</h4>
          <p data-testid="card-prompt">{current.prompt}</p>
          <dl className="card-stats">
            <dt>Stability</dt>
            <dd data-testid="card-stability">{current.stability} days</dd>
            <dt>Lapses</dt>
            <dd data-testid="card-lapses">{current.lapses}</dd>
          </dl>

          <fieldset>
            <legend>How well did you recall this?</legend>
            {(["again", "hard", "good", "easy"] as Rating[]).map((rating) => (
              <button key={rating} type="button" onClick={() => rate(rating)}>
                {rating}
              </button>
            ))}
          </fieldset>
        </article>
      ) : (
        <p data-testid="queue-empty">
          Nothing is due right now. The next review will appear here when it
          comes up.
        </p>
      )}

      <section aria-labelledby="upcoming-heading">
        <h4 id="upcoming-heading">Coming up</h4>
        {upcoming.length === 0 ? (
          <p data-testid="upcoming-empty">No scheduled reviews ahead.</p>
        ) : (
          <ul data-testid="upcoming-list">
            {upcoming.map((card) => (
              <li key={card.id}>
                {card.subtest}: {card.prompt} — in {card.dueInDays} days
              </li>
            ))}
          </ul>
        )}
      </section>
    </section>
  );
}
