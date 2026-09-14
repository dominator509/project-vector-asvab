/**
 * Backend context, data-fetching hook and the honest status surfaces.
 *
 * Every data-bearing view in this application depends on a local SQLite
 * database reached through the Tauri boundary. When that boundary is absent —
 * the bundle is open in a browser, or the database failed to open — the UI must
 * say so. It must not fall back to demonstration data, because a learner cannot
 * tell fabricated progress from real progress and the whole product claim is
 * that the numbers are evidence-backed.
 *
 * `useAsync` therefore exposes exactly four states, and the components below
 * render three of them explicitly.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { createVectorClient, type VectorClient } from "./client";
import { BackendUnavailableError } from "./errors";
import { isTauriRuntime, tauriInvoke, unavailableInvoke } from "./runtime";

export interface Backend {
  client: VectorClient;
  /** False when the bundle is running outside the desktop application. */
  available: boolean;
  /** Human-readable explanation, present only when `available` is false. */
  reason: string | null;
  /**
   * Set when the webview reached the command layer and the marker was stored.
   * Null while the call is in flight, and when the call failed — including the
   * case where the bundle rendered but no command could be answered, which is
   * a real failure mode rather than a cosmetic one.
   */
  boundaryMarker: string | null;
  /** Why the marker call failed, when it did. */
  boundaryError: string | null;
}

const BackendContext = createContext<Backend | null>(null);

/**
 * Identifies this frontend build.
 *
 * Baked in at build time by Vite (`define` in `vite.config.ts`), so the stored
 * marker identifies the bundle that made the call rather than whatever happens
 * to be checked out.
 */
declare const __VECTOR_BUILD_STAMP__: unknown;

export const FRONTEND_BUILD_STAMP: string =
  typeof __VECTOR_BUILD_STAMP__ === "string" ? __VECTOR_BUILD_STAMP__ : "unset";

export interface BackendProviderProps {
  children: ReactNode;
  /** Override detection; used by tests to install a recording fake. */
  available?: boolean;
  /** Override the transport; used by tests. */
  client?: VectorClient;
}

export function BackendProvider({
  children,
  available,
  client,
}: BackendProviderProps) {
  const reachable = available ?? isTauriRuntime();

  const resolved = useMemo<VectorClient>(
    () =>
      client ?? createVectorClient(reachable ? tauriInvoke : unavailableInvoke),
    [client, reachable],
  );

  const [boundaryMarker, setBoundaryMarker] = useState<string | null>(null);
  const [boundaryError, setBoundaryError] = useState<string | null>(null);

  /**
   * Prove the webview-to-Rust hop on mount.
   *
   * Everything else in the interface depends on this hop, and it can fail while
   * the page still renders — a Content-Security-Policy that blocks the IPC
   * bridge, or a command that was never registered, both look like a working
   * window. Recording and surfacing the outcome turns that assumption into an
   * observed fact.
   */
  useEffect(() => {
    if (!reachable) return;
    let live = true;
    resolved
      .uiReady(FRONTEND_BUILD_STAMP)
      .then((id) => {
        if (live) setBoundaryMarker(id);
      })
      .catch((error: unknown) => {
        if (live) {
          setBoundaryError(
            error instanceof Error ? error.message : String(error),
          );
        }
      });
    return () => {
      live = false;
    };
  }, [reachable, resolved]);

  const value = useMemo<Backend>(
    () => ({
      client: resolved,
      available: reachable,
      reason: reachable
        ? null
        : "This interface is running outside the desktop application, so the " +
          "local database is not reachable.",
      boundaryMarker,
      boundaryError,
    }),
    [resolved, reachable, boundaryMarker, boundaryError],
  );

  return (
    <BackendContext.Provider value={value}>{children}</BackendContext.Provider>
  );
}

/**
 * The backend in effect for this subtree.
 *
 * When no provider is present the result is an honest "no backend configured"
 * value rather than an exception. That keeps every view mountable on its own —
 * which is what lets a component test render a single view without building the
 * whole application — and it cannot mask a wiring mistake in the real app,
 * because `App` always installs a provider.
 */
const DETACHED_BACKEND: Backend = {
  client: createVectorClient(unavailableInvoke),
  available: false,
  reason:
    "This interface is running outside the desktop application, so the local " +
    "database is not reachable.",
  boundaryMarker: null,
  boundaryError: null,
};

export function useBackend(): Backend {
  return useContext(BackendContext) ?? DETACHED_BACKEND;
}

// ---------------------------------------------------------------------------
// Async data
// ---------------------------------------------------------------------------

export type AsyncState<T> =
  | { status: "loading" }
  | { status: "unavailable"; message: string }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

export interface AsyncResult<T> {
  state: AsyncState<T>;
  /** Re-run the request. Safe to call before the first one settles. */
  reload: () => void;
}

/**
 * Run a client call and track its outcome.
 *
 * `enabled` lets a caller hold the request until its inputs exist — asking for a
 * study plan before a learner profile is selected is a programming error, not a
 * backend failure, and should not be reported as one.
 */
export function useAsync<T>(
  run: (client: VectorClient) => Promise<T>,
  deps: readonly unknown[],
  enabled = true,
): AsyncResult<T> {
  const backend = useBackend();
  const [state, setState] = useState<AsyncState<T>>({ status: "loading" });
  const [nonce, setNonce] = useState(0);
  const runRef = useRef(run);
  runRef.current = run;

  useEffect(() => {
    // No backend at all is not "still loading": saying so immediately is what
    // stops a view from sitting on a spinner that can never resolve.
    if (!backend.available) {
      setState({
        status: "unavailable",
        message:
          backend.reason ??
          "The local database is not reachable, so this information is not available.",
      });
      return;
    }
    if (!enabled) {
      // Held back by a missing input (no learner selected yet). The caller owns
      // that case and renders it itself, so nothing is shown from here.
      setState({ status: "loading" });
      return;
    }
    // A stale response must not overwrite a newer one: the learner can change
    // profile while a slow query is in flight.
    let live = true;
    setState({ status: "loading" });

    runRef
      .current(backend.client)
      .then((data) => {
        if (live) setState({ status: "ready", data });
      })
      .catch((error: unknown) => {
        if (!live) return;
        if (error instanceof BackendUnavailableError) {
          setState({
            status: "unavailable",
            message: backend.reason ?? error.message,
          });
          return;
        }
        setState({
          status: "error",
          message: error instanceof Error ? error.message : String(error),
        });
      });

    return () => {
      live = false;
    };
    // `deps` is the caller-declared dependency list; the hook cannot infer it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, nonce, backend, ...deps]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { state, reload };
}

// ---------------------------------------------------------------------------
// Status surfaces
// ---------------------------------------------------------------------------

/** Rendered instead of data when no backend is reachable. */
export function UnavailableNotice({ message }: { message?: string }) {
  return (
    <p
      className="status-notice"
      role="status"
      data-testid="backend-unavailable"
    >
      {message ??
        "The local database is not reachable, so this information is not available."}
      <span className="status-hint">
        {" "}
        Open Project VECTOR as the desktop application to use it.
      </span>
    </p>
  );
}

/** Rendered when the backend answered with a failure. */
export function ErrorNotice({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  return (
    <p
      className="status-notice status-error"
      role="alert"
      data-testid="backend-error"
    >
      {message}
      {onRetry && (
        <>
          {" "}
          <button type="button" onClick={onRetry}>
            Try again
          </button>
        </>
      )}
    </p>
  );
}

export interface AsyncBoundaryProps<T> {
  state: AsyncState<T>;
  reload: () => void;
  /** Rendered while loading. */
  loading?: ReactNode;
  children: (data: T) => ReactNode;
}

/**
 * Render `children` once data exists, or the appropriate status otherwise.
 *
 * Centralising this means no view can accidentally render a partial result,
 * which is how a "0%" gets displayed while the real value is still loading.
 */
export function AsyncBoundary<T>({
  state,
  reload,
  loading,
  children,
}: AsyncBoundaryProps<T>) {
  switch (state.status) {
    case "loading":
      return (
        <p role="status" data-testid="backend-loading">
          {loading ?? "Loading…"}
        </p>
      );
    case "unavailable":
      return <UnavailableNotice message={state.message} />;
    case "error":
      return <ErrorNotice message={state.message} onRetry={reload} />;
    case "ready":
      return <>{children(state.data)}</>;
  }
}
