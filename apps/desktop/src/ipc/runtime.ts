/**
 * Runtime detection for the Tauri backend.
 *
 * The same `dist/` bundle is served three ways: inside the packaged desktop
 * application, by `vite preview` for the Playwright suite, and by `vite` during
 * development. Only the first has a Rust process behind it. Detecting that
 * explicitly is what lets every view say "the local database is not reachable"
 * instead of silently rendering sample data that looks like real progress.
 *
 * `__TAURI_INTERNALS__` is injected by the Tauri runtime before the bundle
 * executes; it is the documented marker (Tauri v2 `core.js` uses it for exactly
 * this purpose), not a heuristic on the user agent.
 */

import { BackendUnavailableError } from "./errors";
import type { Invoke } from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/** Whether a Tauri backend can be reached from this document. */
export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.__TAURI_INTERNALS__ === "object" &&
    window.__TAURI_INTERNALS__ !== null
  );
}

/**
 * Invoke a command through the real Tauri bridge.
 *
 * `@tauri-apps/api/core` is imported dynamically so that the module graph does
 * not require the bridge to exist: in a plain browser the import still resolves,
 * and only the call fails.
 */
export const tauriInvoke: Invoke = async (command, args) => {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(command, args);
};

/**
 * An `invoke` that always fails, used when no backend is present.
 *
 * Throwing `BackendUnavailableError` rather than returning empty data is
 * deliberate: an empty array would be indistinguishable from "the learner has
 * no history yet", which is a different and much more dangerous claim.
 */
export const unavailableInvoke: Invoke = async (command) => {
  throw new BackendUnavailableError(
    `the local backend is not available, so "${command}" cannot run. ` +
      `This happens when the interface is opened outside the desktop application.`,
  );
};
