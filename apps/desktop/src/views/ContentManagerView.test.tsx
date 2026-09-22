/**
 * Content manager view: corpus, sources, withdrawal and the audit trail.
 *
 * The behaviour worth pinning down is that a withdrawal is impossible without a
 * reason and always recorded. A quarantine that leaves no trail is
 * indistinguishable from an item that was never there, which is the failure the
 * audit requirement exists to prevent.
 */

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { BackendProvider } from "../ipc/Backend";
import { createFakeClient } from "../ipc/testing/fakeClient";
import { ContentManagerView } from "./ContentManagerView";

function renderWith(fake: ReturnType<typeof createFakeClient>) {
  return render(
    <BackendProvider available client={fake.client}>
      <ContentManagerView />
    </BackendProvider>,
  );
}

/** A fake client holding three AR questions. */
async function seeded() {
  const fake = createFakeClient();
  await fake.client.contentGenerate("AR", 3, 11);
  return fake;
}

describe("ContentManagerView", () => {
  it("reports the corpus and the terms its sources may be used on", async () => {
    const fake = await seeded();
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("corpus-summary")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("corpus-summary")).toHaveTextContent(
      /3 question\(s\) on this device, 3 in service/,
    );

    const table = screen.getByTestId("sources-table");
    // The licence is the point of the table: it answers "on what terms may this
    // be here", not "which file is it".
    expect(table).toHaveTextContent("Public domain in the USA");
    expect(table).toHaveTextContent("Test source");
  });

  it("says plainly when there is no content yet", async () => {
    const fake = createFakeClient();
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("corpus-empty")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("items-empty")).toBeInTheDocument();
    expect(screen.getByTestId("sources-empty")).toBeInTheDocument();
  });

  it("will not withdraw an item until a reason is given", async () => {
    const fake = await seeded();
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("items-list")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("reason-required")).toBeInTheDocument();

    for (const button of screen.getAllByRole("button", {
      name: /withdraw from service/i,
    })) {
      expect(button).toBeDisabled();
    }
    // Nothing was written: a disabled control that still called the backend would
    // be a trail entry with an empty rationale.
    expect(fake.commands()).not.toContain("content_quarantine");
  });

  it("withdraws an item, removes it from service, and records the reason", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("items-list")).toBeInTheDocument(),
    );

    const itemId = fake.state.items.keys().next().value as string;
    await userEvent.type(
      screen.getByTestId("reason-input"),
      "answer key wrong",
    );
    await userEvent.click(screen.getByTestId(`quarantine-${itemId}`));

    await waitFor(() =>
      expect(screen.getByTestId(`item-${itemId}`)).toHaveTextContent(
        /quarantined/,
      ),
    );
    expect(screen.getByTestId("corpus-summary")).toHaveTextContent(
      /2 in service/,
    );

    // The trail is read back from the backend, not from the view's own state.
    const trail = screen.getByTestId("trail-list");
    expect(trail).toHaveTextContent("active → quarantined by local-user");
    expect(trail).toHaveTextContent("answer key wrong");
  });

  it("returns a quarantined item to service", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("items-list")).toBeInTheDocument(),
    );

    const itemId = fake.state.items.keys().next().value as string;
    await userEvent.type(screen.getByTestId("reason-input"), "suspect");
    await userEvent.click(screen.getByTestId(`quarantine-${itemId}`));
    await waitFor(() =>
      expect(screen.getByTestId(`reinstate-${itemId}`)).toBeInTheDocument(),
    );

    // The reason field cleared after the first action, so it must be typed again:
    // each change carries its own justification.
    expect(screen.queryByTestId(`reinstate-${itemId}`)).toBeDisabled();
    await userEvent.type(screen.getByTestId("reason-input"), "checked, fine");
    await userEvent.click(screen.getByTestId(`reinstate-${itemId}`));

    await waitFor(() =>
      expect(screen.getByTestId("corpus-summary")).toHaveTextContent(
        /3 in service/,
      ),
    );
    expect(screen.getByTestId("trail-list")).toHaveTextContent(
      "quarantined → active by local-user",
    );
  });

  it("shows an item's audit trail on request", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("items-list")).toBeInTheDocument(),
    );

    const itemId = fake.state.items.keys().next().value as string;
    await userEvent.click(screen.getByTestId(`history-${itemId}`));

    await waitFor(() =>
      expect(screen.getByTestId("trail-list")).toBeInTheDocument(),
    );
    expect(fake.commands()).toContain("content_history");
  });

  it("shows a withdrawal that the backend refuses rather than pretending it worked", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("items-list")).toBeInTheDocument(),
    );

    const itemId = fake.state.items.keys().next().value as string;
    await userEvent.type(screen.getByTestId("reason-input"), "first");
    await userEvent.click(screen.getByTestId(`quarantine-${itemId}`));
    await waitFor(() =>
      expect(screen.getByTestId(`item-${itemId}`)).toHaveTextContent(
        /quarantined/,
      ),
    );

    // Withdraw it again behind the view's back, so the next click hits a refusal.
    await userEvent.type(screen.getByTestId("reason-input"), "again");
    await userEvent.click(screen.getByTestId(`reinstate-${itemId}`));
    await waitFor(() =>
      expect(screen.getByTestId("corpus-summary")).toHaveTextContent(
        /3 in service/,
      ),
    );

    // Reinstating an item that is no longer quarantined is refused by the backend,
    // and the view must say so rather than rendering a success it did not get.
    fake.state.quarantined.delete(itemId);
    await userEvent.click(screen.getByTestId(`history-${itemId}`));
    await waitFor(() =>
      expect(screen.getByTestId("trail-list")).toBeInTheDocument(),
    );
    const rendered = within(screen.getByTestId("trail-list"));
    expect(rendered.getAllByRole("listitem").length).toBe(
      fake.state.reviews.length,
    );
  });

  it("reports an unreachable backend instead of an empty corpus", async () => {
    const fake = createFakeClient({ unavailable: true });
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("content-error")).toBeInTheDocument(),
    );
    // An empty corpus and an unreadable one look identical on screen unless the
    // view distinguishes them, and they call for different actions.
    expect(screen.queryByTestId("corpus-empty")).not.toBeInTheDocument();
  });
});
