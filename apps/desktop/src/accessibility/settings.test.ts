/**
 * EP-005 acceptance: accessibility settings (REQ-035).
 */

import { describe, expect, it } from "vitest";
import {
  a11yStyleVars,
  clampFontScale,
  DEFAULT_A11Y_SETTINGS,
  MAX_FONT_SCALE,
  MIN_FONT_SCALE,
  normalizeA11ySettings,
  prefersReducedMotion,
} from "./settings";

describe("accessibility settings", () => {
  it("defaults to no modifications", () => {
    expect(DEFAULT_A11Y_SETTINGS.highContrast).toBe(false);
    expect(DEFAULT_A11Y_SETTINGS.fontScale).toBe(1);
    expect(DEFAULT_A11Y_SETTINGS.reducedMotion).toBe(false);
  });

  it("clamps the font scale to the supported range", () => {
    expect(clampFontScale(1.5)).toBe(1.5);
    expect(clampFontScale(0.1)).toBe(MIN_FONT_SCALE);
    expect(clampFontScale(99)).toBe(MAX_FONT_SCALE);
    expect(clampFontScale(-3)).toBe(MIN_FONT_SCALE);
  });

  it("falls back to the default for non-finite font scales", () => {
    // NaN in a CSS value silently breaks all typography.
    expect(clampFontScale(Number.NaN)).toBe(DEFAULT_A11Y_SETTINGS.fontScale);
    expect(clampFontScale(Number.POSITIVE_INFINITY)).toBe(MAX_FONT_SCALE);
    expect(clampFontScale(Number.NEGATIVE_INFINITY)).toBe(MIN_FONT_SCALE);
  });

  it("never allows text below the browser default size", () => {
    // A scale under 1 defeats the purpose of the setting.
    expect(clampFontScale(0.5)).toBeGreaterThanOrEqual(MIN_FONT_SCALE);
  });

  it("normalizes partial input without dropping known fields", () => {
    const settings = normalizeA11ySettings({
      highContrast: true,
      fontScale: 5,
    });
    expect(settings.highContrast).toBe(true);
    expect(settings.fontScale).toBe(MAX_FONT_SCALE);
    expect(settings.reducedMotion).toBe(false);
    expect(settings.relaxedSpacing).toBe(false);
  });

  it("emits the font scale as a CSS variable", () => {
    const vars = a11yStyleVars({ ...DEFAULT_A11Y_SETTINGS, fontScale: 1.5 });
    expect(vars["--vector-font-scale"]).toBe("1.5");
  });

  it("disables transitions when reduced motion is requested", () => {
    const on = a11yStyleVars({ ...DEFAULT_A11Y_SETTINGS, reducedMotion: true });
    const off = a11yStyleVars({
      ...DEFAULT_A11Y_SETTINGS,
      reducedMotion: false,
    });
    expect(on["--vector-transition-duration"]).toBe("0s");
    expect(off["--vector-transition-duration"]).not.toBe("0s");
  });

  it("widens letter and word spacing in relaxed mode", () => {
    const relaxed = a11yStyleVars({
      ...DEFAULT_A11Y_SETTINGS,
      relaxedSpacing: true,
    });
    expect(relaxed["--vector-letter-spacing"]).not.toBe("normal");
    expect(relaxed["--vector-word-spacing"]).not.toBe("normal");
    expect(relaxed["--vector-line-height"]).toBe("2");
  });

  it("reports reduced motion intent", () => {
    expect(
      prefersReducedMotion({ ...DEFAULT_A11Y_SETTINGS, reducedMotion: true }),
    ).toBe(true);
    expect(prefersReducedMotion(DEFAULT_A11Y_SETTINGS)).toBe(false);
  });

  it("clamps an out-of-range scale even when passed straight to the style vars", () => {
    // The style helper must not trust its caller.
    const vars = a11yStyleVars({ ...DEFAULT_A11Y_SETTINGS, fontScale: 100 });
    expect(vars["--vector-font-scale"]).toBe(String(MAX_FONT_SCALE));
  });
});
