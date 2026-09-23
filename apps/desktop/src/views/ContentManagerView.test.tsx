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

describe("the packs panel", () => {
  it("says there are no packs rather than showing an empty table", async () => {
    const fake = await seeded();
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("packs-empty")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("packs-table")).not.toBeInTheDocument();
  });

  it("installs a pack and reports what it delivered", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("pack-path-input")).toBeInTheDocument(),
    );

    // The button is disabled until there is a path: an install with nothing to
    // install is a failure the learner would have to guess at.
    expect(screen.getByTestId("pack-install")).toBeDisabled();

    await userEvent.type(
      screen.getByTestId("pack-path-input"),
      "C:/packs/core-asvab.vpack",
    );
    await userEvent.click(screen.getByTestId("pack-install"));

    await waitFor(() =>
      expect(screen.getByTestId("packs-table")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("packs-table")).toHaveTextContent("core-asvab");
    expect(screen.getByTestId("packs-table")).toHaveTextContent("verified");
    expect(screen.getByTestId("pack-notice")).toHaveTextContent(
      /Installed core-asvab v1/,
    );
    // The path is cleared so a second click cannot reinstall the same file by
    // accident.
    expect(screen.getByTestId("pack-path-input")).toHaveValue("");
  });

  it("shows a refused install as an error rather than as success", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("pack-path-input")).toBeInTheDocument(),
    );

    await userEvent.type(
      screen.getByTestId("pack-path-input"),
      "C:/packs/untrusted.vpack",
    );
    await userEvent.click(screen.getByTestId("pack-install"));

    await waitFor(() =>
      expect(screen.getByTestId("action-error")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("action-error")).toHaveTextContent(
      /not signed by the key this installation trusts/,
    );
    expect(screen.queryByTestId("pack-notice")).not.toBeInTheDocument();
  });

  it("shows what an installed pack teaches, and how its difficulty was arrived at", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("pack-path-input")).toBeInTheDocument(),
    );

    await userEvent.type(screen.getByTestId("pack-path-input"), "core.vpack");
    await userEvent.click(screen.getByTestId("pack-install"));
    await waitFor(() =>
      expect(screen.getByTestId("packs-table")).toBeInTheDocument(),
    );

    const panel = screen.getByTestId("pack-objectives-pack-core-asvab-1");
    // The objective, its subtest and its prerequisite edge.
    expect(panel).toHaveTextContent("Rate problems");
    expect(panel).toHaveTextContent("(MK)");
    expect(
      screen.getByTestId("objective-after-OBJ-MK-ALGEBRA-01"),
    ).toHaveTextContent("after OBJ-AR-RATE-01");

    // A figure that rests on no responses says so; one that rests on 120 says that.
    expect(
      screen.getByTestId("objective-calibration-OBJ-AR-RATE-01"),
    ).toHaveTextContent(
      "62% expected correct, declared, no responses recorded",
    );
    expect(
      screen.getByTestId("objective-calibration-OBJ-MK-ALGEBRA-01"),
    ).toHaveTextContent("44% expected correct, 120 response(s)");
  });

  it("rolls a pack back and says what is in service again", async () => {
    const fake = await seeded();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("pack-path-input")).toBeInTheDocument(),
    );

    // Two installs, so there is an earlier version to go back to.
    await userEvent.type(screen.getByTestId("pack-path-input"), "first.vpack");
    await userEvent.click(screen.getByTestId("pack-install"));
    await waitFor(() =>
      expect(screen.getByTestId("packs-table")).toBeInTheDocument(),
    );
    await userEvent.type(screen.getByTestId("pack-path-input"), "second.vpack");
    await userEvent.click(screen.getByTestId("pack-install"));
    await waitFor(() =>
      expect(screen.getByTestId("packs-table")).toHaveTextContent("2"),
    );

    const rows = within(screen.getByTestId("packs-table")).getAllByRole("row");
    // Header plus two versions; the newest is active and the older superseded.
    expect(rows).toHaveLength(3);
    expect(rows[1]).toHaveTextContent("2");
    expect(rows[1]).toHaveTextContent("active");
    expect(rows[2]).toHaveTextContent("superseded");

    // Only the active version offers a rollback; the older one has nothing below it
    // to go back to.
    await userEvent.click(
      screen.getByTestId("pack-rollback-pack-core-asvab-2"),
    );

    await waitFor(() =>
      expect(screen.getByTestId("pack-notice")).toHaveTextContent(
        /Rolled core-asvab back to v1/,
      ),
    );
    const after = within(screen.getByTestId("packs-table")).getAllByRole("row");
    expect(after[1]).toHaveTextContent("2");
    expect(after[1]).toHaveTextContent("quarantined");
    expect(after[2]).toHaveTextContent("1");
    expect(after[2]).toHaveTextContent("active");
  });

  it("reports a registry that cannot be read as an empty list without breaking the corpus view", async () => {
    const fake = await seeded();
    const client = {
      ...fake.client,
      contentPacks: async () => {
        throw new Error("the registry is unreadable");
      },
    };
    render(
      <BackendProvider available client={client}>
        <ContentManagerView />
      </BackendProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("corpus-summary")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("packs-empty")).toBeInTheDocument();
  });
});
