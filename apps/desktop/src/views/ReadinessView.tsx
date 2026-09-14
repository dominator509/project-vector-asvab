/**
 * Readiness dashboard (REQ-011).
 *
 * ADR-010 and SCORING_AND_READINESS.md forbid presenting a precise predicted
 * official score before a validation cohort exists. This view therefore shows a
 * band plus calibration context, and it states plainly that the value is not an
 * official score prediction.
 *
 * The band is a prop rather than sample data: the figures a learner sees must
 * come from their own stored mastery estimates, and a component that could
 * supply its own numbers would eventually do so.
 */

import { useCallback, useState } from "react";

import { AsyncBoundary, useAsync, useBackend } from "../ipc/Backend";
import type { ReadinessDto } from "../ipc/types";
import { useActiveProfile } from "../state/ProfileContext";

export interface ReadinessViewProps {
  band: ReadinessDto;
  /** Whose estimate this is, for an unambiguous reading. */
  learnerName?: string;
  /** How many recorded attempts the estimate rests on, when known. */
  attemptCount?: number;
}

export function ReadinessView({
  band,
  learnerName,
  attemptCount,
}: ReadinessViewProps) {
  const {
    low,
    high,
    confidence,
    official_score_claim: officialScoreClaim,
  } = band;

  // Guard rather than trust: if the band were ever inverted or set to claim an
  // official score, the UI must not present it as a prediction.
  const inverted = low > high;

  return (
    <section aria-labelledby="readiness-heading">
      <h3 id="readiness-heading">Readiness estimate</h3>

      {learnerName && (
        <p data-testid="readiness-learner">Estimate for {learnerName}.</p>
      )}

      {inverted ? (
        <p role="alert" data-testid="readiness-error">
          The readiness estimate is unavailable because its range is invalid.
        </p>
      ) : (
        <>
          <p data-testid="readiness-band">
            Estimated readiness: {Math.round(low * 100)}%–
            {Math.round(high * 100)}%
          </p>

          {/*
            A meter communicates the band visually while remaining accessible;
            the numeric text above is the authoritative value.
          */}
          <div
            role="meter"
            aria-label="Readiness range"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(((low + high) / 2) * 100)}
            aria-valuetext={`${Math.round(low * 100)} to ${Math.round(high * 100)} percent`}
            className="band-meter"
          >
            <span
              className="band-fill"
              style={{
                marginLeft: `${low * 100}%`,
                width: `${(high - low) * 100}%`,
              }}
            />
          </div>

          <p data-testid="readiness-confidence">
            Calibration confidence: {Math.round(confidence * 100)}%
          </p>

          {/*
            A wide band is the honest reading of thin evidence, so say what it
            means instead of leaving the learner to guess.
          */}
          {high - low > 0.5 && (
            <p data-testid="readiness-wide">
              This range is wide because there is not much evidence behind it
              yet. Practice in more subtests to narrow it.
            </p>
          )}

          {typeof attemptCount === "number" && (
            <p data-testid="readiness-evidence">
              Based on {attemptCount} recorded attempt
              {attemptCount === 1 ? "" : "s"} on this computer.
            </p>
          )}

          <p className="disclaimer" data-testid="readiness-disclaimer">
            This is a practice estimate from your own study history. It is not
            an official ASVAB or AFQT score prediction, and it is shown as a
            range because VECTOR has not validated a score-prediction model.
          </p>

          {officialScoreClaim && (
            <p role="alert" data-testid="readiness-illegal-claim">
              Invalid state: an official score claim must never be displayed.
            </p>
          )}
        </>
      )}
    </section>
  );
}

/**
 * The readiness dashboard wired to the local database.
 *
 * Attempt totals are read alongside the band so the learner can see how much
 * evidence the estimate rests on, rather than reading a percentage with no
 * indication of whether it comes from three answers or three hundred.
 *
 * The recompute control exists because mastery is derived from the attempt
 * history: without a way to refresh it, a learner who has just practised would
 * still be shown the estimate from before they did.
 */
export function ReadinessPanel() {
  const backend = useBackend();
  const { profile, unavailable } = useActiveProfile();
  const [recomputed, setRecomputed] = useState<number | null>(null);
  const [recomputeError, setRecomputeError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const request = useAsync(
    async (client) => {
      if (!profile) throw new Error("no learner profile is selected");
      const [band, analytics] = await Promise.all([
        client.readiness(profile.id),
        client.analyticsAll(profile.id),
      ]);
      const attempts = analytics.reduce((sum, [, row]) => sum + row.total, 0);
      return { band, attempts };
    },
    [profile?.id],
    profile !== null && backend.available,
  );

  const { reload } = request;
  const runRecompute = useCallback(async () => {
    if (!profile) return;
    setBusy(true);
    setRecomputeError(null);
    try {
      const rows = await backend.client.recomputeMastery(profile.id);
      setRecomputed(rows);
      // Re-read rather than assuming: the estimate on screen must come from the
      // database, not from the belief that the recompute worked.
      reload();
    } catch (error) {
      setRecomputeError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [backend.client, profile, reload]);

  if (!backend.available || unavailable) {
    return (
      <section aria-labelledby="readiness-heading">
        <h3 id="readiness-heading">Readiness estimate</h3>
        <p
          className="status-notice"
          role="status"
          data-testid="backend-unavailable"
        >
          {backend.reason} A readiness estimate is computed from your own stored
          mastery estimates, so there is nothing to show until the application
          is running.
        </p>
      </section>
    );
  }

  if (!profile) {
    return (
      <section aria-labelledby="readiness-heading">
        <h3 id="readiness-heading">Readiness estimate</h3>
        <p data-testid="no-profile">
          No learner profile is selected, so there is no study history to
          estimate from.
        </p>
      </section>
    );
  }

  return (
    <>
      <AsyncBoundary state={request.state} reload={request.reload}>
        {({ band, attempts }) => (
          <ReadinessView
            band={band}
            learnerName={profile.name}
            attemptCount={attempts}
          />
        )}
      </AsyncBoundary>

      <section aria-labelledby="recompute-heading" className="diagnostics">
        <h4 id="recompute-heading">Recompute from your history</h4>
        <p>
          Mastery estimates are derived from your recorded attempts. Recompute
          them after practising so the estimate reflects what you have done.
        </p>
        <button type="button" onClick={runRecompute} disabled={busy}>
          {busy ? "Recomputing…" : "Recompute mastery"}
        </button>
        {recomputed !== null && (
          <p role="status" data-testid="recompute-status">
            Recomputed {recomputed} subtest estimate
            {recomputed === 1 ? "" : "s"} from your stored attempts.
          </p>
        )}
        {recomputeError && (
          <p role="alert" data-testid="recompute-error">
            The estimates were not recomputed: {recomputeError}
          </p>
        )}
      </section>
    </>
  );
}
