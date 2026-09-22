/**
 * The practice container: loading, empty, ready and error surfaces.
 *
 * This is the surface that replaced an unconditional `sampleQuestions` prop, so
 * the behaviour under test is that an empty corpus is *visible* rather than
 * papered over with demonstration content, and that the offered action actually
 * resolves it.
 */

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { BackendProvider } from "../ipc/Backend";
import { createFakeClient } from "../ipc/testing/fakeClient";
import { PracticeContentView } from "./PracticeContentView";

function renderWith(fake: ReturnType<typeof createFakeClient>) {
  return render(
    <BackendProvider available client={fake.client}>
      <PracticeContentView learnerId="learner-1" />
    </BackendProvider>,
  );
}

describe("PracticeContentView", () => {
  it("generates content on a fresh installation and then shows a question", async () => {
    const fake = createFakeClient();
    renderWith(fake);

    // The first paint is a loading state, not an empty question with a blank stem.
    expect(screen.getByTestId("practice-loading")).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId("practice-prompt")).toBeInTheDocument(),
    );
    expect(fake.commands()).toContain("content_generate");
    expect(screen.getByTestId("practice-corpus-summary")).toBeInTheDocument();
  });

  it("says so when a subtest has no content, and offers to fix it", async () => {
    const fake = createFakeClient();
    // Seed only AR, then switch to MK, which is empty.
    await fake.client.contentGenerate("AR", 4, 0);
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "MK" })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "MK" }));

    await waitFor(() =>
      expect(screen.getByTestId("practice-empty")).toBeInTheDocument(),
    );
    // The empty surface states the corpus size rather than claiming there is
    // nothing at all on the device, which would be untrue here.
    expect(screen.getByTestId("practice-empty")).toHaveTextContent(
      /This device holds 4 question/,
    );
    expect(
      screen.getByRole("button", { name: /Prepare 40 MK questions/ }),
    ).toBeInTheDocument();
  });

  it("prepares the chosen subtest when the learner asks", async () => {
    const fake = createFakeClient();
    await fake.client.contentGenerate("AR", 4, 0);
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "MK" })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "MK" }));
    await waitFor(() =>
      expect(screen.getByTestId("practice-empty")).toBeInTheDocument(),
    );

    await userEvent.click(
      screen.getByRole("button", { name: /Prepare 40 MK questions/ }),
    );

    await waitFor(() =>
      expect(screen.getByTestId("practice-prompt")).toBeInTheDocument(),
    );
    // The question actually served is an MK item, not an AR one left over.
    expect(screen.getByText(/Subtest: MK/)).toBeInTheDocument();
  });

  it("shows an error surface instead of an empty question when the backend is down", async () => {
    const fake = createFakeClient({ unavailable: true });
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByTestId("practice-error")).toBeInTheDocument(),
    );
    // A learner must not be shown a question they cannot answer.
    expect(screen.queryByTestId("practice-prompt")).not.toBeInTheDocument();
  });

  it("offers every subtest the device can serve, and no others", async () => {
    const fake = createFakeClient();
    // Word Knowledge items are ingested from a public-domain source rather than
    // generated, so a device that holds none must not offer the subtest at all.
    await fake.client.contentGenerate("AR", 4, 0);
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("practice-prompt")).toBeInTheDocument(),
    );

    expect(
      screen.queryByRole("button", { name: "WK" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "AR" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "MK" })).toBeInTheDocument();
    // Mechanical Comprehension is computable, so the factory serves it.
    expect(screen.getByRole("button", { name: "MC" })).toBeInTheDocument();
  });

  it("offers an ingested subtest once the device holds items for it", async () => {
    const fake = createFakeClient();
    await fake.client.contentGenerate("AR", 4, 0);
    await fake.client.contentGenerate("WK", 3, 0);
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "WK" })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "WK" }));
    await waitFor(() =>
      expect(screen.getByTestId("practice-prompt")).toBeInTheDocument(),
    );
    expect(screen.getByText(/Subtest: WK/)).toBeInTheDocument();
  });

  it("never offers to generate a subtest the factory cannot serve", async () => {
    const fake = createFakeClient();
    // The reachable case: the device holds Word Knowledge items, so the subtest is
    // offered, but every one of them is withdrawn, so the practice set is empty.
    await fake.client.contentGenerate("WK", 3, 0);
    const wkIds = [...fake.state.items.values()]
      .filter((item) => item.subtest === "WK")
      .map((item) => item.id);
    for (const id of wkIds) {
      await fake.client.contentQuarantine(id, "reviewer", "under review");
    }
    // Seeding the corpus uses the command under test, so only calls made after
    // this point say anything about what the view asked for.
    const seeded = fake.calls.length;
    renderWith(fake);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "WK" })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "WK" }));

    await waitFor(() =>
      expect(screen.getByTestId("practice-empty")).toBeInTheDocument(),
    );
    // Word Knowledge items are ingested, not generated, so there is no button that
    // could produce more and the surface says where they come from instead.
    expect(
      screen.queryByRole("button", { name: /Prepare .* WK questions/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("practice-no-generator")).toBeInTheDocument();
    // And it must not have quietly asked the factory for a subtest it cannot serve.
    expect(
      fake.calls
        .slice(seeded)
        .some(
          (call) =>
            call.command === "content_generate" && call.args?.subtest === "WK",
        ),
    ).toBe(false);
  });
});
