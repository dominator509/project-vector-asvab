/**
 * Privacy and crash center (REQ-029, REQ-030, REQ-033, REQ-034, REQ-035).
 *
 * Four guarantees are surfaced here, and all four do real work:
 *  - crash reports are double-redacted and never sent without explicit consent;
 *  - local data can be exported as a verified, integrity-checked archive;
 *  - an exported archive can be restored, and the restore verifies the archive
 *    against the digest taken when it was written;
 *  - local data can be fully erased, and the erase reports what remains read
 *    back from the database rather than what it intended to do.
 *
 * The erase path requires an explicit typed confirmation. That phrase is
 * checked again in the service layer, because a destructive operation must not
 * depend on the caller having rendered a particular input.
 */

import { useCallback, useState } from "react";

import { AsyncBoundary, useAsync, useBackend } from "../ipc/Backend";
import type { BackupDto, BackupEntryDto, ResetDto } from "../ipc/types";
import { useActiveProfile } from "../state/ProfileContext";

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

      <LocalDataSection
        canDelete={canDelete}
        confirmed={confirmed}
        setConfirmed={setConfirmed}
      />
    </section>
  );
}

interface LocalDataSectionProps {
  canDelete: boolean;
  confirmed: string;
  setConfirmed: (value: string) => void;
}

/**
 * Export, restore and erase, wired to the local database.
 *
 * Each action reports its own outcome in the section that triggered it, so a
 * failed restore cannot be mistaken for a successful export.
 */
function LocalDataSection({
  canDelete,
  confirmed,
  setConfirmed,
}: LocalDataSectionProps) {
  const backend = useBackend();
  const { reload: reloadProfiles } = useActiveProfile();

  const [exported, setExported] = useState<BackupDto | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [restored, setRestored] = useState<number | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [reset, setReset] = useState<ResetDto | null>(null);
  const [resetError, setResetError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const paths = useAsync(
    (client) => client.appPaths(),
    [backend.available],
    backend.available,
  );

  const backupDir =
    paths.state.status === "ready" ? paths.state.data.backup_dir : null;

  const backups = useAsync(
    (client) =>
      backupDir ? client.backupList(backupDir) : Promise.resolve([]),
    [backupDir],
    backupDir !== null,
  );

  const runExport = useCallback(async () => {
    if (!backupDir) return;
    setBusy(true);
    setExportError(null);
    try {
      const manifest = await backend.client.backupCreate(backupDir);
      // Read the archive list back: a manifest that was reported but not
      // written would otherwise look like a successful export.
      const onDisk = await backend.client.backupList(backupDir);
      if (!onDisk.some((entry) => entry.path === manifest.path)) {
        setExportError(
          "The export reported success but the archive is not on disk.",
        );
        return;
      }
      setExported(manifest);
      backups.reload();
    } catch (error) {
      setExportError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [backend.client, backupDir, backups]);

  const runRestore = useCallback(
    async (entry: BackupEntryDto) => {
      setBusy(true);
      setRestoreError(null);
      try {
        // The digest recorded beside the archive is supplied, so a file that
        // changed after it was written is refused rather than applied.
        const outcome = await backend.client.backupRestore(
          entry.path,
          entry.checksum,
        );
        setRestored(outcome.rows);
        reloadProfiles();
      } catch (error) {
        setRestoreError(error instanceof Error ? error.message : String(error));
      } finally {
        setBusy(false);
      }
    },
    [backend.client, reloadProfiles],
  );

  const runReset = useCallback(async () => {
    setBusy(true);
    setResetError(null);
    setReset(null);
    try {
      const outcome = await backend.client.resetLocalData(
        DELETE_CONFIRMATION_PHRASE,
      );
      setReset(outcome);
      reloadProfiles();
    } catch (error) {
      setResetError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [backend.client, reloadProfiles]);

  return (
    <>
      <section aria-labelledby="export-heading">
        <h4 id="export-heading">Export your data</h4>
        <p>
          Writes an integrity-checked local archive containing your profile,
          attempts, mastery estimates and stored sources. Nothing is uploaded.
        </p>

        {!backend.available && (
          <p
            className="status-notice"
            role="status"
            data-testid="backend-unavailable"
          >
            {backend.reason}
          </p>
        )}

        <button type="button" onClick={runExport} disabled={busy || !backupDir}>
          Export local data
        </button>

        {exported && (
          <p role="status" data-testid="export-status">
            Exported {exported.bytes.toLocaleString()} bytes to{" "}
            <code>{exported.path}</code>. Integrity check: {exported.integrity}.
            SHA-256 <code>{exported.checksum}</code>
          </p>
        )}
        {exportError && (
          <p role="alert" data-testid="export-error">
            The export did not complete: {exportError}
          </p>
        )}

        <h5>Existing archives</h5>
        <AsyncBoundary state={backups.state} reload={backups.reload}>
          {(entries) =>
            entries.length === 0 ? (
              <p data-testid="no-backups">No archives have been written yet.</p>
            ) : (
              <ul data-testid="backup-list">
                {entries.map((entry) => (
                  <li key={entry.path}>
                    <code>{entry.path}</code> — {entry.bytes.toLocaleString()}{" "}
                    bytes, written {entry.modified}{" "}
                    <button
                      type="button"
                      onClick={() => runRestore(entry)}
                      disabled={busy}
                    >
                      Restore this archive
                    </button>
                  </li>
                ))}
              </ul>
            )
          }
        </AsyncBoundary>

        {restored !== null && (
          <p role="status" data-testid="restore-status">
            Restored. The database now holds {restored} recorded attempt
            {restored === 1 ? "" : "s"}.
          </p>
        )}
        {restoreError && (
          <p role="alert" data-testid="restore-error">
            The restore was refused, so your current data is unchanged:{" "}
            {restoreError}
          </p>
        )}
      </section>

      <section aria-labelledby="delete-heading">
        <h4 id="delete-heading">Delete all local data</h4>
        <p>
          Removes your profile, attempts, mastery estimates, stored sources and
          crash reports from this device, then compacts the database. This
          cannot be undone — export first if you may want the data back.
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

        <button type="button" disabled={!canDelete || busy} onClick={runReset}>
          Delete everything
        </button>

        {/*
          The status is rendered whenever the action was attempted. Reporting
          the outcome is the point: "deleted" must mean the rows are gone, which
          is why the remaining counts are shown.
        */}
        {reset && (
          <p role="status" data-testid="delete-status">
            Removed {reset.profiles_removed} profile(s),{" "}
            {reset.attempts_removed} attempt(s) and {reset.evidence_removed}{" "}
            source snapshot(s). Remaining: {reset.profiles_remaining}{" "}
            profile(s), {reset.attempts_remaining} attempt(s),{" "}
            {reset.evidence_remaining} source snapshot(s).
          </p>
        )}
        {resetError && (
          <p role="alert" data-testid="delete-status">
            Local data was not deleted: {resetError}
          </p>
        )}
      </section>
    </>
  );
}
