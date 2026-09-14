/**
 * Accessibility settings view (REQ-035).
 *
 * Every control is a native form element inside a fieldset with a legend, so
 * keyboard and screen-reader operation work without custom key handling.
 */

import {
  MAX_FONT_SCALE,
  MIN_FONT_SCALE,
  type A11ySettings,
} from "../accessibility/settings";

export interface AccessibleSettingsProps {
  settings: A11ySettings;
  onChange: (settings: A11ySettings) => void;
}

export function AccessibleSettings({
  settings,
  onChange,
}: AccessibleSettingsProps) {
  const update = <K extends keyof A11ySettings>(
    key: K,
    value: A11ySettings[K],
  ) => {
    onChange({ ...settings, [key]: value });
  };

  const toggles: Array<{
    key: keyof A11ySettings;
    label: string;
    help: string;
  }> = [
    {
      key: "highContrast",
      label: "High contrast",
      help: "Increase contrast between text and background.",
    },
    {
      key: "reducedMotion",
      label: "Reduce motion",
      help: "Remove non-essential animation and transitions.",
    },
    {
      key: "alwaysShowFocus",
      label: "Always show focus outline",
      help: "Keep a visible focus ring even when using a mouse.",
    },
    {
      key: "verboseAnnouncements",
      label: "Verbose announcements",
      help: "Announce more state changes to screen readers.",
    },
    {
      key: "relaxedSpacing",
      label: "Relaxed text spacing",
      help: "Increase letter, word and line spacing for easier reading.",
    },
  ];

  return (
    <section aria-labelledby="a11y-heading">
      <h3 id="a11y-heading">Accessibility settings</h3>

      <fieldset>
        <legend>Display and input</legend>
        {toggles.map(({ key, label, help }) => (
          <div key={key} className="setting-row">
            <input
              type="checkbox"
              id={`a11y-${key}`}
              checked={Boolean(settings[key])}
              aria-describedby={`a11y-${key}-help`}
              onChange={(e) => update(key, e.target.checked as never)}
            />
            <label htmlFor={`a11y-${key}`}>{label}</label>
            <p id={`a11y-${key}-help`} className="help-text">
              {help}
            </p>
          </div>
        ))}
      </fieldset>

      <fieldset>
        <legend>Text size</legend>
        <label htmlFor="a11y-font-scale">
          Font scale:{" "}
          <span data-testid="font-scale-value">{settings.fontScale}</span>x
        </label>
        <input
          type="range"
          id="a11y-font-scale"
          min={MIN_FONT_SCALE}
          max={MAX_FONT_SCALE}
          step={0.1}
          value={settings.fontScale}
          aria-describedby="a11y-font-scale-help"
          onChange={(e) => update("fontScale", Number(e.target.value))}
        />
        <p id="a11y-font-scale-help" className="help-text">
          Scales all text between {MIN_FONT_SCALE}x and {MAX_FONT_SCALE}x.
        </p>
      </fieldset>

      <button type="button" onClick={() => onChange({ ...settings })}>
        Apply settings
      </button>
    </section>
  );
}
