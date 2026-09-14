/**
 * Active learner profile, shared across views.
 *
 * Every data-bearing surface — plan, analytics, readiness, practice recording —
 * is scoped to one learner, so the selection has to live above the views. The
 * provider also owns the reasoning rule in `resolveActiveProfileId`: it is not
 * enough to remember an id, the id has to still exist.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import type { ProfileDto } from "../ipc/types";
import { useAsync, useBackend } from "../ipc/Backend";
import {
  clearActiveProfileId,
  readActiveProfileId,
  resolveActiveProfileId,
  writeActiveProfileId,
  type StorageLike,
} from "./activeProfile";

export interface ActiveProfile {
  /** The selected profile, or null when none exists or is selected. */
  profile: ProfileDto | null;
  /** Every profile on this installation. */
  profiles: ProfileDto[];
  /** Select a profile and remember the choice. */
  select: (id: string | null) => void;
  /** Re-read the profile list from storage. */
  reload: () => void;
  /** True while the profile list is being read. */
  loading: boolean;
  /** Set when the profile list could not be read. */
  error: string | null;
  /** True when no local backend is reachable. */
  unavailable: boolean;
}

const ProfileContext = createContext<ActiveProfile | null>(null);

/** Browser storage, or a no-op stand-in when it is unavailable. */
function defaultStorage(): StorageLike {
  if (typeof window !== "undefined" && window.localStorage) {
    return window.localStorage;
  }
  return {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  };
}

export interface ProfileProviderProps {
  children: ReactNode;
  /** Override storage; used by tests. */
  storage?: StorageLike;
}

export function ProfileProvider({ children, storage }: ProfileProviderProps) {
  const backend = useBackend();
  const store = useMemo(() => storage ?? defaultStorage(), [storage]);
  const [selected, setSelected] = useState<string | null>(() =>
    readActiveProfileId(store),
  );

  const { state, reload } = useAsync(
    (client) => client.listProfiles(),
    [backend.available],
    true,
  );

  const profiles = state.status === "ready" ? state.data : [];

  // Reconcile the remembered id against what actually exists. Without this a
  // deleted profile would stay selected and every request would fail.
  useEffect(() => {
    if (state.status !== "ready") return;
    const ids = state.data.map((p) => p.id);
    const resolved = resolveActiveProfileId(selected, ids);
    if (resolved !== selected) {
      setSelected(resolved);
      if (resolved === null) clearActiveProfileId(store);
      else writeActiveProfileId(store, resolved);
    }
  }, [state, selected, store]);

  const select = useCallback(
    (id: string | null) => {
      setSelected(id);
      if (id === null) clearActiveProfileId(store);
      else writeActiveProfileId(store, id);
    },
    [store],
  );

  const profile = useMemo(
    () => profiles.find((p) => p.id === selected) ?? null,
    [profiles, selected],
  );

  const value = useMemo<ActiveProfile>(
    () => ({
      profile,
      profiles,
      select,
      reload,
      loading: state.status === "loading",
      error: state.status === "error" ? state.message : null,
      unavailable: state.status === "unavailable" || !backend.available,
    }),
    [profile, profiles, select, reload, state, backend.available],
  );

  return (
    <ProfileContext.Provider value={value}>{children}</ProfileContext.Provider>
  );
}

/**
 * The active profile, or an empty selection when no provider is present.
 *
 * Returning an empty selection rather than throwing keeps every view renderable
 * on its own, which is what lets a component test mount a single view without
 * constructing the whole application. The empty selection is honest about the
 * situation: it says no profile is selected, not that one exists.
 */
const NO_PROFILE: ActiveProfile = {
  profile: null,
  profiles: [],
  select: () => undefined,
  reload: () => undefined,
  loading: false,
  error: null,
  unavailable: true,
};

export function useActiveProfile(): ActiveProfile {
  return useContext(ProfileContext) ?? NO_PROFILE;
}
