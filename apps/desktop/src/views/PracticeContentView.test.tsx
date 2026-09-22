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
      /none for MK/,
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

  it("offers only subtests the factory can actually generate for", async () => {
    const fake = createFakeClient();
    renderWith(fake);
    await waitFor(() =>
      expect(screen.getByTestId("practice-prompt")).toBeInTheDocument(),
    );

    // Word Knowledge has no template yet; offering it would be a button that
    // always fails.
    expect(
      screen.queryByRole("button", { name: "WK" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "AR" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "MK" })).toBeInTheDocument();
  });
});
