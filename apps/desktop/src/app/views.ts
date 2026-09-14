/**
 * Application view model (SPEC-004).
 *
 * SPEC-004 lists the primary views. They are modelled as a closed union so an
 * unknown view cannot be navigated to, and so the test suite can assert that
 * every declared view is reachable by keyboard.
 */

export type ViewId =
  | "onboarding"
  | "diagnostic"
  | "today"
  | "lesson"
  | "practice"
  | "cat"
  | "paper"
  | "review"
  | "readiness"
  | "explore"
  | "evidence"
  | "tutor"
  | "content"
  | "providers"
  | "privacy"
  | "accessibility"
  | "search";

/** A navigable view with its learner-facing label. */
export interface ViewDefinition {
  id: ViewId;
  label: string;
  /** Sort order within the navigation, ascending. */
  order: number;
}

/**
 * The primary views required by SPEC-004.
 *
 * `order` is explicit so navigation is stable and testable rather than relying
 * on object key iteration order.
 */
export const VIEWS: ViewDefinition[] = [
  { id: "onboarding", label: "Getting started", order: 0 },
  { id: "diagnostic", label: "Diagnostic", order: 1 },
  { id: "today", label: "Today's plan", order: 2 },
  { id: "lesson", label: "Lessons", order: 3 },
  { id: "practice", label: "Practice", order: 4 },
  { id: "cat", label: "CAT simulator", order: 5 },
  { id: "paper", label: "Paper simulator", order: 6 },
  { id: "review", label: "Review queue", order: 7 },
  { id: "readiness", label: "Readiness", order: 8 },
  { id: "explore", label: "Jobs explorer", order: 9 },
  { id: "evidence", label: "Sources", order: 10 },
  { id: "tutor", label: "AI tutor", order: 11 },
  { id: "content", label: "Content manager", order: 12 },
  { id: "providers", label: "Provider settings", order: 13 },
  { id: "privacy", label: "Privacy and crashes", order: 14 },
  { id: "accessibility", label: "Accessibility", order: 15 },
  { id: "search", label: "Search", order: 16 },
];

/** The view shown when the app starts. */
export const DEFAULT_VIEW: ViewId = "today";

/** Whether a string is a declared view id. */
export function isViewId(value: string): value is ViewId {
  return VIEWS.some((v) => v.id === value);
}

/** Look up a view definition. */
export function viewDefinition(id: ViewId): ViewDefinition {
  const found = VIEWS.find((v) => v.id === id);
  if (!found) {
    // Unreachable while ViewId is a closed union, but a total function is
    // safer than a non-null assertion if the union is later widened.
    throw new Error(`unknown view: ${id}`);
  }
  return found;
}

/**
 * Test mode suppresses distracting UI and reproduces allowed navigation
 * constraints (SPEC-004).
 */
export interface TestModeState {
  active: boolean;
  /** The exam form under simulation, when a simulator is running. */
  form: "cat" | "paper" | null;
}

/**
 * Which chrome should be hidden while an exam simulation is running.
 *
 * Keeping this declarative means the rule is testable without rendering, and
 * the UI cannot accidentally drift from it.
 */
export function hiddenDuringTestMode(state: TestModeState): string[] {
  if (!state.active) return [];
  return ["nav", "search", "tutor", "readiness"];
}
