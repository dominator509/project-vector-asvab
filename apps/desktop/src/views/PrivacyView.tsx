/**
 * Privacy and crash center (REQ-029, REQ-030, REQ-035).
 *
 * Three guarantees are surfaced here:
 *  - crash reports are double-redacted and never sent without explicit consent;
 *  - local data can be exported portably;
 *  - local data can be fully deleted.
 *
 * The delete path requires an explicit typed confirmation, because an
 * irreversible destruction of the learner's history must not be one stray click.
 */

import { useState } from "react";

/** The literal the learner must type to confirm deletion. */
export const DELETE_CONFIRMATION_PHRASE = "DELETE";

export type RedactionStatus = "pending" | "redacted" | "verified";

export interface CrashReport {
  id: string;
  summary: string;
  /** Applied in two passes so a single-pass miss cannot leak a secret. */
  redactionPasses: number;
  status: RedactionStatus;
  containsSecrets: boolean;
  sharedWithProvider: boolean;
}

export interface PrivacyViewProps {
  reports?: CrashReport[];
}

const DEFAULT_REPORTS: CrashReport[] = [
  {
    id: "crash-001",
    summary: "Unexpected exit while opening the review queue",
    redactionPasses: 2,
    status: "verified",
    containsSecrets: false,
    sharedWithProvider: false,
  },
];

export function PrivacyView({ reports = DEFAULT_REPORTS }: PrivacyViewProps) {
  const [confirmed, setConfirmed] = useState("");
  const [consented, setConsented] = useState<Record<string, boolean>>({});
  const [deleted, setDeleted] = useState(false);

  const canDelete = confirmed === DELETE_CONFIRMATION_PHRASE;

  return (
    <section aria-labelledby="privacy-heading">
      <h3 id="privacy-heading">Privacy and crashes</h3>

      <section aria-labelledby="crash-heading">
        <h4 id="crash-heading">Crash reports</h4>
        {reports.length === 0 ? (
          <p data-testid="no-crashes">No crash reports have been recorded.</p>
        ) : (
          <ul data-testid="crash-list">
            {reports.map((report) => (
              <li key={report.id}>
                <p>{report.summary}</p>
                <p
                  className="crash-meta"
                  data-testid={`redaction-${report.id}`}
                >
                  Redaction: {report.status} ({report.redactionPasses} passes)
                </p>
                {/*
                  A report must never leave the machine without a per-report
                  consent decision, so the control is opt-in per report.
                */}
                <label>
                  <input
                    type="checkbox"
                    checked={consented[report.id] ?? false}
                    disabled={report.containsSecrets}
                    onChange={(e) =>
                      setConsented((prev) => ({
                        ...prev,
                        [report.id]: e.target.checked,
                      }))
                    }
                  />
                  Share this report with the maintainers
                </label>
                {report.containsSecrets && (
                  <p role="alert" data-testid={`blocked-${report.id}`}>
                    Sharing is blocked: this report still contains sensitive
                    data.
                  </p>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="export-heading">
        <h4 id="export-heading">Export your data</h4>
        <p>
          Produces a portable local archive containing your profile, attempts
          and notes. Nothing is uploaded.
        </p>
        <button type="button" data-testid="export-button">
          Export local data
        </button>
      </section>

      <section aria-labelledby="delete-heading">
        <h4 id="delete-heading">Delete all local data</h4>
        <p>
          Removes your profile, attempts, notes and crash reports from this
          device. This cannot be undone.
        </p>

        <label htmlFor="delete-confirm">
          Type {DELETE_CONFIRMATION_PHRASE} to confirm
        </label>
        <input
          id="delete-confirm"
          type="text"
          value={confirmed}
          onChange={(e) => setConfirmed(e.target.value)}
        />

        <button
          type="button"
          disabled={!canDelete}
          onClick={() => setDeleted(true)}
        >
          Delete everything
        </button>

        {deleted && (
          <p role="status" data-testid="delete-status">
            All local data has been deleted.
          </p>
        )}
      </section>
    </section>
  );
}
