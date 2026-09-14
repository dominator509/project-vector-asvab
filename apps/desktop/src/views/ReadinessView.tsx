/**
 * Readiness dashboard (REQ-011).
 *
 * ADR-010 and SCORING_AND_READINESS.md forbid presenting a precise predicted
 * official score before a validation cohort exists. This view therefore shows a
 * band plus calibration context, and it states plainly that the value is not an
 * official score prediction.
 */

import { readinessBand } from "../data/sample";

export function ReadinessView() {
  const { low, high, confidence, officialScoreClaim } = readinessBand;

  // Guard rather than trust: if the band were ever inverted or set to claim an
  // official score, the UI must not present it as a prediction.
  const inverted = low > high;

  return (
    <section aria-labelledby="readiness-heading">
      <h3 id="readiness-heading">Readiness estimate</h3>

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
