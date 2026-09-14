/**
 * Which learner profile this installation is currently using.
 *
 * The choice is remembered locally, because a learner who closes the
 * application and reopens it should not have to find their own profile again.
 * Access is through an injected `StorageLike` rather than `localStorage`
 * directly so the behaviour — including what happens with corrupt or absent
 * values — is testable without a DOM.
 */

export const ACTIVE_PROFILE_KEY = "vector.activeProfileId";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * The remembered profile id, or null.
 *
 * Only the shape this module writes is accepted: a whitespace-only or empty
 * value is treated as absent rather than passed on, because an empty id would
 * be sent to the backend and produce a confusing "not found" for a lexically
 * valid request.
 */
export function readActiveProfileId(storage: StorageLike): string | null {
  let raw: string | null;
  try {
    raw = storage.getItem(ACTIVE_PROFILE_KEY);
  } catch {
    // Storage can be unavailable (disabled, or a privacy mode that throws).
    // Losing the remembered choice is recoverable; crashing the app is not.
    return null;
  }
  if (raw === null) return null;
  const trimmed = raw.trim();
  return trimmed.length === 0 ? null : trimmed;
}

export function writeActiveProfileId(storage: StorageLike, id: string): void {
  try {
    storage.setItem(ACTIVE_PROFILE_KEY, id);
  } catch {
    // Remembering the choice is a convenience; failing to persist it must not
    // prevent the learner from using the profile in this session.
  }
}

export function clearActiveProfileId(storage: StorageLike): void {
  try {
    storage.removeItem(ACTIVE_PROFILE_KEY);
  } catch {
    // As above.
  }
}

/**
 * Pick the profile to use, given what is stored and what exists.
 *
 * A remembered id whose profile has been deleted must not win: that would leave
 * the application pointing at a learner that no longer exists. Falling back to
 * the only profile, or to none, keeps the state consistent with storage.
 */
export function resolveActiveProfileId(
  stored: string | null,
  availableIds: readonly string[],
): string | null {
  if (stored !== null && availableIds.includes(stored)) return stored;
  if (availableIds.length === 1) return availableIds[0];
  return null;
}
