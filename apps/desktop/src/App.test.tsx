/**
 * EP-005 acceptance: application shell, keyboard navigation and accessibility
 * (REQ-035, SPEC-004 "keyboard-only operation is required").
 *
 * These tests drive the real component tree through the DOM. They assert
 * behaviour a keyboard or screen-reader user depends on, not implementation
 * details. They replace a previous placeholder test that asserted
 * `expect(true).toBe(true)` and proved nothing.
 */

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "./App";
import { VIEWS } from "./app/views";

describe("application shell", () => {
  it("renders the product name as a level-one heading", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { level: 1, name: /project vector/i }),
    ).toBeInTheDocument();
  });

  it("exposes a primary navigation landmark", () => {
    render(<App />);
    expect(
      screen.getByRole("navigation", { name: /primary/i }),
    ).toBeInTheDocument();
  });

  it("offers a skip link to the main content", () => {
    // Without a skip link a keyboard user must tab through the entire nav.
    render(<App />);
    const skip = screen.getByRole("link", { name: /skip to main content/i });
    expect(skip).toHaveAttribute("href", "#main-content");
  });

  it("renders a main landmark that is focusable", () => {
    render(<App />);
    const main = screen.getByRole("main");
    expect(main).toHaveAttribute("id", "main-content");
    expect(main).toHaveAttribute("tabindex", "-1");
  });

  it("provides an accessible name for every navigation entry", () => {
    render(<App />);
    const nav = screen.getByRole("navigation", { name: /primary/i });
    const buttons = within(nav).getAllByRole("button");
    expect(buttons.length).toBe(VIEWS.length);
    for (const button of buttons) {
      expect(button.textContent?.trim().length ?? 0).toBeGreaterThan(0);
    }
  });

  it("marks the active view with aria-current", () => {
    render(<App />);
    const current = screen
      .getAllByRole("button")
      .filter((b) => b.getAttribute("aria-current") === "page");
    expect(current).toHaveLength(1);
  });
});

describe("keyboard-only navigation", () => {
  it("navigates to a view with the keyboard alone", async () => {
    const user = userEvent.setup();
    render(<App />);

    const nav = screen.getByRole("navigation", { name: /primary/i });
    const practice = within(nav).getByRole("button", { name: /^practice$/i });

    // Focus and activate with the keyboard, never a mouse click.
    practice.focus();
    expect(practice).toHaveFocus();
    await user.keyboard("{Enter}");

    expect(
      screen.getByRole("heading", { level: 2, name: /^practice$/i }),
    ).toBeInTheDocument();
  });

  it("moves focus with arrow keys within the navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    const nav = screen.getByRole("navigation", { name: /primary/i });
    const buttons = within(nav).getAllByRole("button");
    const first = buttons[0];
    const second = buttons[1];

    first.focus();
    await user.keyboard("{ArrowDown}");
    expect(second).toHaveFocus();

    await user.keyboard("{ArrowUp}");
    expect(first).toHaveFocus();
  });

  it("wraps arrow-key focus at the ends of the navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    const nav = screen.getByRole("navigation", { name: /primary/i });
    const buttons = within(nav).getAllByRole("button");

    buttons[0].focus();
    await user.keyboard("{ArrowUp}");
    expect(buttons[buttons.length - 1]).toHaveFocus();

    await user.keyboard("{ArrowDown}");
    expect(buttons[0]).toHaveFocus();
  });

  it("supports Home and End within the navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    const nav = screen.getByRole("navigation", { name: /primary/i });
    const buttons = within(nav).getAllByRole("button");

    buttons[2].focus();
    await user.keyboard("{End}");
    expect(buttons[buttons.length - 1]).toHaveFocus();

    await user.keyboard("{Home}");
    expect(buttons[0]).toHaveFocus();
  });

  it("announces view changes through a polite live region", async () => {
    const user = userEvent.setup();
    render(<App />);

    const live = screen.getByTestId("live-region");
    expect(live).toHaveAttribute("aria-live", "polite");

    const nav = screen.getByRole("navigation", { name: /primary/i });
    await user.click(within(nav).getByRole("button", { name: /^search$/i }));

    expect(live).toHaveTextContent(/showing search/i);
  });

  it("updates the document title on navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    const nav = screen.getByRole("navigation", { name: /primary/i });
    await user.click(within(nav).getByRole("button", { name: /^readiness$/i }));

    expect(document.title).toMatch(/readiness/i);
  });
});

describe("every declared view is reachable", () => {
  it("can navigate to each view in the registry", async () => {
    const user = userEvent.setup();
    render(<App />);
    const nav = screen.getByRole("navigation", { name: /primary/i });

    for (const view of VIEWS) {
      const button = within(nav).getByRole("button", { name: view.label });
      await user.click(button);
      expect(
        screen.getByRole("heading", { level: 2, name: view.label }),
      ).toBeInTheDocument();
    }
  });

  it("never renders two active views at once", async () => {
    const user = userEvent.setup();
    render(<App />);
    const nav = screen.getByRole("navigation", { name: /primary/i });

    for (const view of VIEWS) {
      await user.click(within(nav).getByRole("button", { name: view.label }));
      const current = screen
        .getAllByRole("button")
        .filter((b) => b.getAttribute("aria-current") === "page");
      expect(current).toHaveLength(1);
    }
  });
});
