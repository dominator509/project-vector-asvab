/**
 * Today's plan (REQ-003) and offline analytics (REQ-010), with the database
 * health and latency evidence that backs them (REQ-039, REQ-040).
 *
 * Four separate reads are issued in one request so the view has a single,
 * honest status: either every figure on screen came from the local database, or
 * the view says it could not read it. A per-panel boundary would let a learner
 * read a real plan beside an analytics panel that had simply failed, with no
 * way to tell which was which.
 *
 * The time budget is a real control, not a decoration: changing it re-plans.
 */

import { useState } from "react";

import { AsyncBoundary, useAsync, useBackend } from "../ipc/Backend";
import type {
  AnalyticsDto,
  HealthDto,
  LatencyDto,
  PlanDto,
} from "../ipc/types";
import { useActiveProfile } from "../state/ProfileContext";
import type { PracticeRequest } from "./PracticeContentView";

/** The planning horizons offered, in minutes. */
export const TIME_BUDGETS = [15, 30, 60, 90] as const;

export interface TodaySnapshot {
  plan: PlanDto;
  analytics: Array<[string, AnalyticsDto]>;
  health: HealthDto;
  latency: LatencyDto;
}

/**
 * `onPractise` is how a plan drill becomes a session.
 *
 * It is optional so the view can be rendered on its own in a test, and the
 * control is rendered only when it is supplied: a "Practise" button that went
 * nowhere would be worse than the label it replaces.
 */
export function TodayView({
  onPractise,
}: {
  onPractise?: (request: PracticeRequest) => void;
} = {}) {
  const backend = useBackend();
  const { profile, unavailable } = useActiveProfile();
  const [minutes, setMinutes] = useState<number>(30);

  const request = useAsync<TodaySnapshot>(
    async (client) => {
      if (!profile) throw new Error("no learner profile is selected");
      const [plan, analytics, health, latency] = await Promise.all([
        client.studyPlan(profile.id, profile.target_score, minutes),
        client.analyticsAll(profile.id),
        client.health(),
        client.latencyProbe(25),
      ]);
      return { plan, analytics, health, latency };
    },
    [profile?.id, profile?.target_score, minutes],
    profile !== null && backend.available,
  );

  if (!backend.available || unavailable) {
    return (
      <section aria-labelledby="today-heading">
        <h3 id="today-heading">Your plan for today</h3>
        <p
          className="status-notice"
          role="status"
          data-testid="backend-unavailable"
        >
          {backend.reason} Your plan is generated from your own study history on
          this computer, so there is nothing to show until the application is
          running.
        </p>
      </section>
    );
  }

  if (!profile) {
    return (
      <section aria-labelledby="today-heading">
        <h3 id="today-heading">Your plan for today</h3>
        <p data-testid="no-profile">
          No learner profile is selected. Create one in{" "}
          <strong>Getting started</strong> to generate a plan.
        </p>
      </section>
    );
  }

  return (
    <section aria-labelledby="today-heading">
      <h3 id="today-heading">Your plan for today</h3>
      <p data-testid="today-learner">
        Planning for <strong>{profile.name}</strong>, target AFQT{" "}
        {profile.target_score}.
      </p>

      <p>
        <label htmlFor="time-budget">Time available today</label>{" "}
        <select
          id="time-budget"
          value={minutes}
          onChange={(event) => setMinutes(Number(event.target.value))}
        >
          {TIME_BUDGETS.map((value) => (
            <option key={value} value={value}>
              {value} minutes
            </option>
          ))}
        </select>
      </p>

      <AsyncBoundary
        state={request.state}
        reload={request.reload}
        loading="Reading your study history…"
      >
        {(snapshot) => (
          <>
            <PlanSection
              plan={snapshot.plan}
              minutes={minutes}
              onPractise={onPractise}
            />
            <AnalyticsSection analytics={snapshot.analytics} />
            <DiagnosticsSection
              health={snapshot.health}
              latency={snapshot.latency}
            />
          </>
        )}
      </AsyncBoundary>
    </section>
  );
}

function PlanSection({
  plan,
  minutes,
  onPractise,
}: {
  plan: PlanDto;
  minutes: number;
  onPractise?: (request: PracticeRequest) => void;
}) {
  const allocated = plan.drills.reduce((sum, d) => sum + d.minutes, 0);

  if (plan.drills.length === 0) {
    return (
      <section aria-labelledby="plan-heading">
        <h4 id="plan-heading">Plan</h4>
        <p data-testid="plan-empty">
          Nothing needs work right now at this target. Every skill is at or
          above your goal with nothing due for review.
        </p>
      </section>
    );
  }

  return (
    <section aria-labelledby="plan-heading">
      <h4 id="plan-heading">Plan</h4>
      <ol data-testid="today-drills">
        {plan.drills.map((drill) => (
          <li key={`${drill.subtest}-${drill.objective_id ?? "none"}`}>
            <strong>{drill.subtest}</strong> — {drill.minutes} minutes{" "}
            <span className="reason">({drill.reason})</span>
            {/* The objective comes from the installed pack's curriculum, and the reason says
                why it was chosen; a plan with no pack names none, and this renders nothing
                rather than an empty label. */}
            {drill.objective_id !== null && (
              <span
                className="objective"
                data-testid={`drill-objective-${drill.subtest}`}
              >
                {" "}
                {drill.objective_id}
              </span>
            )}
            {/* The plan is a set of instructions, so each drill is the control that
                starts it. The objective travels with the request, which is the
                whole point: a session that ignored it would contradict the plan. */}
            {onPractise && (
              <button
                type="button"
                className="drill-start"
                data-testid={`drill-start-${drill.subtest}`}
                onClick={() =>
                  onPractise({
                    subtest: drill.subtest,
                    objectiveId: drill.objective_id,
                  })
                }
              >
                Practise {drill.subtest}
                {drill.objective_id !== null ? ` — ${drill.objective_id}` : ""}
              </button>
            )}
          </li>
        ))}
      </ol>
      <p>
        Total: <span data-testid="today-total">{plan.total_minutes}</span>{" "}
        minutes of {minutes} available.
      </p>
      {allocated !== plan.total_minutes && (
        <p role="alert" data-testid="plan-total-mismatch">
          The plan's total does not match its drills; treat these figures as
          unreliable.
        </p>
      )}
    </section>
  );
}

function AnalyticsSection({
  analytics,
}: {
  analytics: Array<[string, AnalyticsDto]>;
}) {
  if (analytics.length === 0) {
    return (
      <section aria-labelledby="analytics-heading">
        <h4 id="analytics-heading">Progress by subtest</h4>
        <p data-testid="analytics-empty">
          No practice attempts recorded yet, so there is nothing to measure.
        </p>
      </section>
    );
  }

  return (
    <section aria-labelledby="analytics-heading">
      <h4 id="analytics-heading">Progress by subtest</h4>
      <table data-testid="analytics-table">
        <caption>
          Accuracy and speed from your own recorded attempts. Confidence in
          these figures grows with the number of attempts.
        </caption>
        <thead>
          <tr>
            <th scope="col">Subtest</th>
            <th scope="col">Attempts</th>
            <th scope="col">Correct</th>
            <th scope="col">Accuracy</th>
            <th scope="col">Mean time</th>
          </tr>
        </thead>
        <tbody>
          {analytics.map(([code, row]) => (
            <tr key={code}>
              <th scope="row">{code}</th>
              <td>{row.total}</td>
              <td>{row.correct}</td>
              <td>{Math.round(row.accuracy * 100)}%</td>
              <td>{(row.mean_latency_ms / 1000).toFixed(1)} s</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function DiagnosticsSection({
  health,
  latency,
}: {
  health: HealthDto;
  latency: LatencyDto;
}) {
  return (
    <section aria-labelledby="diagnostics-heading" className="diagnostics">
      <h4 id="diagnostics-heading">Local database</h4>
      <p data-testid="health-status">
        {health.healthy ? "Healthy" : "Degraded"} — {health.component}, verified
        by {health.basis.replace(/_/g, " ")}.
      </p>
      <p data-testid="latency-status">
        Read latency: {latency.mean_micros} µs mean across {latency.iterations}{" "}
        reads (worst {latency.max_micros} µs).
      </p>
    </section>
  );
}
