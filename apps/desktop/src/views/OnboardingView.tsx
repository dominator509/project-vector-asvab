/**
 * Onboarding (REQ-001): a local, privacy-minimal learner profile.
 *
 * There is no account, no email and no network call. The only thing created is
 * a row in the local database, and the view says so rather than implying a
 * sign-up. The target score is bounded to the 1..99 reporting scale here *and*
 * in the service layer, because the UI bounds are a convenience and the service
 * bound is the one that protects the data.
 */

import { useState, type FormEvent } from "react";

import { AsyncBoundary, useAsync, useBackend } from "../ipc/Backend";
import { useActiveProfile } from "../state/ProfileContext";

export function OnboardingView() {
  const backend = useBackend();
  const { profiles, select, reload, profile } = useActiveProfile();

  const [name, setName] = useState("");
  const [target, setTarget] = useState("50");
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const parsed = Number.parseInt(target, 10);
  const targetValid = Number.isInteger(parsed) && parsed >= 1 && parsed <= 99;
  const nameValid = name.trim().length > 0;
  const canSubmit = nameValid && targetValid && !busy && backend.available;

  const list = useAsync(
    async (client) => client.listProfiles(),
    [profiles.length],
    backend.available,
  );

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;

    setBusy(true);
    setFailure(null);
    try {
      const created = await backend.client.createProfile(name.trim(), parsed);
      // Read the new row back before trusting it: the insert could have been
      // accepted while the record differs from what was asked for.
      const readback = await backend.client.getProfile(created.id);
      if (readback.name !== name.trim() || readback.target_score !== parsed) {
        setFailure(
          "The stored profile does not match what was submitted, so it was not selected.",
        );
        return;
      }
      select(readback.id);
      reload();
      list.reload();
      setName("");
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-labelledby="onboarding-heading">
      <h3 id="onboarding-heading">Set up your local profile</h3>

      <p data-testid="onboarding-privacy">
        Everything you do in VECTOR stays on this computer. There is no account,
        no email address and no sign-in, and nothing here is sent anywhere.
      </p>

      {!backend.available && (
        <p
          className="status-notice"
          role="status"
          data-testid="backend-unavailable"
        >
          {backend.reason} Open Project VECTOR as the desktop application to
          create a profile.
        </p>
      )}

      {backend.available && (
        <>
          <form onSubmit={submit} aria-labelledby="new-profile-heading">
            <h4 id="new-profile-heading">Create a profile</h4>

            <p>
              <label htmlFor="learner-name">Your name or initials</label>
              <input
                id="learner-name"
                name="learnerName"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoComplete="off"
                aria-describedby="learner-name-help"
                required
              />
              <span id="learner-name-help" className="field-help">
                Used only to label your progress on this device.
              </span>
            </p>

            <p>
              <label htmlFor="target-score">Target AFQT score</label>
              <input
                id="target-score"
                name="targetScore"
                type="number"
                min={1}
                max={99}
                step={1}
                value={target}
                onChange={(e) => setTarget(e.target.value)}
                aria-describedby="target-score-help"
                aria-invalid={!targetValid}
                required
              />
              <span id="target-score-help" className="field-help">
                The AFQT is reported on a 1–99 scale. This is a study goal, not
                a predicted score.
              </span>
            </p>

            {!targetValid && (
              <p role="alert" data-testid="target-invalid">
                Enter a whole number between 1 and 99.
              </p>
            )}

            <button type="submit" disabled={!canSubmit}>
              {busy ? "Creating…" : "Create profile"}
            </button>
          </form>

          {failure && (
            <p role="alert" data-testid="create-error">
              {failure}
            </p>
          )}

          <section aria-labelledby="existing-heading">
            <h4 id="existing-heading">Profiles on this computer</h4>
            <AsyncBoundary state={list.state} reload={list.reload}>
              {(rows) =>
                rows.length === 0 ? (
                  <p data-testid="no-profiles">
                    No profiles yet. Create one above to begin.
                  </p>
                ) : (
                  <ul data-testid="profile-list">
                    {rows.map((row) => (
                      <li key={row.id}>
                        <span>
                          {row.name} — target {row.target_score}
                        </span>{" "}
                        <button
                          type="button"
                          onClick={() => select(row.id)}
                          disabled={profile?.id === row.id}
                        >
                          {profile?.id === row.id
                            ? "In use"
                            : `Continue as ${row.name}`}
                        </button>
                      </li>
                    ))}
                  </ul>
                )
              }
            </AsyncBoundary>
          </section>
        </>
      )}
    </section>
  );
}
