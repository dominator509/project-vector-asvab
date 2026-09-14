/**
 * Errors raised at the command boundary.
 *
 * Kept in their own module so that the transport layer (`runtime.ts`) and the
 * typed client can both refer to them without importing each other. The client
 * re-exports them, because that is where callers expect to find them.
 */

/** The command failed on the Rust side. */
export class BackendError extends Error {
  readonly command: string;

  constructor(command: string, message: string) {
    super(message);
    this.name = "BackendError";
    this.command = command;
  }
}

/** No local backend is reachable; the interface is running outside the app. */
export class BackendUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BackendUnavailableError";
  }
}

/** The backend answered, but not in the shape the contract promises. */
export class MalformedResponseError extends Error {
  readonly command: string;

  constructor(command: string, detail: string) {
    super(`${command} returned an unexpected payload: ${detail}`);
    this.name = "MalformedResponseError";
    this.command = command;
  }
}
