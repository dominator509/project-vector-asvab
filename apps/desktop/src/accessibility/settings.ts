/**
 * Accessibility settings and the derived presentation rules (REQ-035).
 *
 * SPEC-004 requires keyboard-only operation and an accessibility settings view.
 * These settings are modelled as data with pure derivation functions so the
 * behaviour can be tested directly rather than only through the DOM.
 */

/** Accessibility preferences the learner controls. */
export interface A11ySettings {
  /** Apply the higher-contrast palette. */
  highContrast: boolean;
  /** Scale all typography. */
  fontScale: number;
  /** Reduce or remove non-essential motion. */
  reducedMotion: boolean;
  /** Always show a visible focus ring, even for mouse users. */
  alwaysShowFocus: boolean;
  /** Announce state changes to screen readers more verbosely. */
  verboseAnnouncements: boolean;
  /** Use a dyslexia-friendly letter/word spacing. */
  relaxedSpacing: boolean;
}

export const DEFAULT_A11Y_SETTINGS: A11ySettings = {
  highContrast: false,
  fontScale: 1,
  reducedMotion: false,
  alwaysShowFocus: false,
  verboseAnnouncements: false,
  relaxedSpacing: false,
};

/** Supported font scale bounds. */
export const MIN_FONT_SCALE = 1;
export const MAX_FONT_SCALE = 2;

/**
 * Clamp a font scale into the supported range.
 *
 * A scale below 1 would render text smaller than the browser default, which
 * harms the users this setting exists for; a scale above 2 makes the exam UI
 * unusable. Infinite values clamp to the nearest bound, since a caller asking
 * for "infinitely large" text clearly wants the maximum. `NaN` is the only
 * input with no meaningful direction, so it falls back to the default rather
 * than poisoning a CSS value.
 */
export function clampFontScale(scale: number): number {
  if (Number.isNaN(scale)) return DEFAULT_A11Y_SETTINGS.fontScale;
  if (scale === Number.POSITIVE_INFINITY) return MAX_FONT_SCALE;
  if (scale === Number.NEGATIVE_INFINITY) return MIN_FONT_SCALE;
  if (scale < MIN_FONT_SCALE) return MIN_FONT_SCALE;
  if (scale > MAX_FONT_SCALE) return MAX_FONT_SCALE;
  return scale;
}

/** Normalise arbitrary input into a valid settings object. */
export function normalizeA11ySettings(
  input: Partial<A11ySettings>,
): A11ySettings {
  return {
    highContrast: input.highContrast ?? DEFAULT_A11Y_SETTINGS.highContrast,
    fontScale: clampFontScale(
      input.fontScale ?? DEFAULT_A11Y_SETTINGS.fontScale,
    ),
    reducedMotion: input.reducedMotion ?? DEFAULT_A11Y_SETTINGS.reducedMotion,
    alwaysShowFocus:
      input.alwaysShowFocus ?? DEFAULT_A11Y_SETTINGS.alwaysShowFocus,
    verboseAnnouncements:
      input.verboseAnnouncements ?? DEFAULT_A11Y_SETTINGS.verboseAnnouncements,
    relaxedSpacing:
      input.relaxedSpacing ?? DEFAULT_A11Y_SETTINGS.relaxedSpacing,
  };
}

/** The CSS custom properties derived from the settings. */
export type A11yStyleVars = Record<string, string>;

/**
 * Derive inline CSS variables from the settings.
 *
 * These are emitted as custom properties so the stylesheet owns the actual
 * appearance while the settings stay testable data.
 */
export function a11yStyleVars(settings: A11ySettings): A11yStyleVars {
  const normalized = normalizeA11ySettings(settings);
  return {
    "--vector-font-scale": String(normalized.fontScale),
    "--vector-letter-spacing": normalized.relaxedSpacing ? "0.06em" : "normal",
    "--vector-word-spacing": normalized.relaxedSpacing ? "0.16em" : "normal",
    "--vector-line-height": normalized.relaxedSpacing ? "2" : "1.6",
    "--vector-transition-duration": normalized.reducedMotion ? "0s" : "160ms",
  };
}

/** Whether motion should be suppressed. */
export function prefersReducedMotion(settings: A11ySettings): boolean {
  return normalizeA11ySettings(settings).reducedMotion;
}
