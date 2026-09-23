/**
 * Behaviour of the data-bearing views against the command boundary.
 *
 * Every view here is driven through a fake transport (`ipc/testing/fakeClient`)
 * so the assertions are about real view behaviour — validation, persistence
 * readback, refusal surfacing, and the unavailable state — rather than about a
 * mock's return value. The fake keeps durable state, so a test can read back the
 * effect of a click from the state model as well as from the DOM.
 *
 * What this file does not prove: that the Rust services behind the boundary
 * behave correctly. That is proven by `cargo test --workspace` and by
 * `vector-desktop --self-check` running the real command layer in the binary.
 */

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ReactNode } from "react";

import { BackendProvider } from "../ipc/Backend";
import { createFakeClient, type FakeClient } from "../ipc/testing/fakeClient";
import { ProfileProvider } from "../state/ProfileContext";
import type { StorageLike } from "../state/activeProfile";
import { OnboardingView } from "./OnboardingView";
import { TodayView } from "./TodayView";
import { SourcesView } from "./SourcesView";
import { ReadinessPanel, ReadinessView } from "./ReadinessView";
import { PrivacyView, DELETE_CONFIRMATION_PHRASE } from "./PrivacyView";
import { PracticeView } from "./PracticeView";
import { Shell } from "../App";
import { sampleQuestions } from "../data/sample";

function memoryStorage(initial: Record<string, string> = {}): StorageLike {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
}

interface Harness {
  fake: FakeClient;
  storage: StorageLike;
}

function harness(
  options: Parameters<typeof createFakeClient>[0] = {},
): Harness {
  return { fake: createFakeClient(options), storage: memoryStorage() };
}

/** Mount a view inside the providers it depends on in the real application. */
function mount(ui: ReactNode, h: Harness) {
  return render(
    <BackendProvider available client={h.fake.client}>
      <ProfileProvider storage={h.storage}>{ui}</ProfileProvider>
    </BackendProvider>,
  );
}

/** Mount a view with no reachable backend. */
function mountDetached(ui: ReactNode) {
  return render(
    <BackendProvider available={false}>
      <ProfileProvider storage={memoryStorage()}>{ui}</ProfileProvider>
    </BackendProvider>,
  );
}

// ---------------------------------------------------------------------------
// REQ-001 onboarding
// ---------------------------------------------------------------------------

describe("onboarding", () => {
  it("refuses a blank name and an off-scale target", async () => {
    const user = userEvent.setup();
    const h = harness();
    mount(<OnboardingView />, h);

    const submit = screen.getByRole("button", { name: /create profile/i });
    expect(submit).toBeDisabled();

    await user.type(screen.getByLabelText(/your name or initials/i), "   ");
    expect(submit).toBeDisabled();

    await user.type(screen.getByLabelText(/your name or initials/i), "Ada");
    await user.clear(screen.getByLabelText(/target afqt score/i));
    await user.type(screen.getByLabelText(/target afqt score/i), "120");
    expect(screen.getByTestId("target-invalid")).toBeInTheDocument();
    expect(submit).toBeDisabled();

    // Nothing was sent, so nothing can have been stored.
    expect(h.fake.commands()).not.toContain("create_profile");
    expect(h.fake.state.profiles.size).toBe(0);
  });

  it("creates a profile, reads it back, and remembers the selection", async () => {
    const user = userEvent.setup();
    const h = harness();
    mount(<OnboardingView />, h);

    await user.type(screen.getByLabelText(/your name or initials/i), "Ada");
    await user.clear(screen.getByLabelText(/target afqt score/i));
    await user.type(screen.getByLabelText(/target afqt score/i), "65");
    await user.click(screen.getByRole("button", { name: /create profile/i }));

    await waitFor(() =>
      expect(screen.getByTestId("profile-list")).toBeInTheDocument(),
    );

    // The durable effect, read back from the state model.
    const stored = [...h.fake.state.profiles.values()];
    expect(stored).toHaveLength(1);
    expect(stored[0]).toMatchObject({ name: "Ada", target_score: 65 });

    // The readback happened, and the choice was remembered.
    expect(h.fake.commands()).toContain("get_profile");
    expect(h.storage.getItem("vector.activeProfileId")).toBe(stored[0].id);
    expect(screen.getByRole("button", { name: /in use/i })).toBeInTheDocument();
  });

  it("says the profile is local and needs no account", async () => {
    const h = harness();
    mount(<OnboardingView />, h);
    expect(screen.getByTestId("onboarding-privacy")).toHaveTextContent(
      /no account, no email address and no sign-in/i,
    );
    // Let the profile list settle so the assertion below is made against a
    // quiet tree rather than one mid-update.
    expect(await screen.findByTestId("no-profiles")).toBeInTheDocument();
  });

  it("reports a backend refusal instead of pretending it succeeded", async () => {
    const user = userEvent.setup();
    const h = harness();
    // A learner already exists with this name in the backend's own rules; force
    // the refusal by making the create call fail.
    h.fake.client.createProfile = async () => {
      throw new Error("storage error: disk is full");
    };
    mount(<OnboardingView />, h);

    await user.type(screen.getByLabelText(/your name or initials/i), "Ada");
    await user.click(screen.getByRole("button", { name: /create profile/i }));

    expect(await screen.findByTestId("create-error")).toHaveTextContent(
      /disk is full/,
    );
    expect(h.fake.state.profiles.size).toBe(0);
  });

  it("explains itself when no backend is reachable", () => {
    mountDetached(<OnboardingView />);
    expect(screen.getByTestId("backend-unavailable")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /create profile/i }),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-003, REQ-010, REQ-040
// ---------------------------------------------------------------------------

describe("today's plan", () => {
  it("shows the objective the plan chose, and nothing when no pack declares one", async () => {
    const h = harness();
    h.storage.setItem("vector.activeProfileId", "");
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    // The stub's plan names no objective, which is what a device with no pack gets: the view
    // must render no objective label rather than an empty one.
    mount(<TodayView />, h);
    await waitFor(() =>
      expect(screen.getByTestId("today-drills")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("drill-objective-AR")).not.toBeInTheDocument();
  });

  it("asks for a plan once a learner exists and renders the stored analytics", async () => {
    const h = harness();
    h.storage.setItem("vector.activeProfileId", "");
    const profile = await h.fake.client.createProfile("Ada", 60);
    await h.fake.client.recordAttempt({
      attemptId: "a-1",
      learnerId: profile.id,
      subtest: "AR",
      questionId: "q-1",
      correct: true,
      latencyMs: 1000,
    });
    await h.fake.client.recordAttempt({
      attemptId: "a-2",
      learnerId: profile.id,
      subtest: "AR",
      questionId: "q-2",
      correct: false,
      latencyMs: 3000,
    });
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<TodayView />, h);

    await waitFor(() =>
      expect(screen.getByTestId("today-drills")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("today-total")).toHaveTextContent("30");

    const table = screen.getByTestId("analytics-table");
    const arRow = within(table).getByRole("rowheader", { name: "AR" });
    expect(arRow.closest("tr")).toHaveTextContent("50%");
    expect(arRow.closest("tr")).toHaveTextContent("2");

    expect(screen.getByTestId("health-status")).toHaveTextContent(
      /verified by operation succeeded/i,
    );
    expect(screen.getByTestId("latency-status")).toHaveTextContent(/mean/i);
  });

  it("re-plans when the available time changes", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<TodayView />, h);
    await waitFor(() =>
      expect(screen.getByTestId("today-total")).toHaveTextContent("30"),
    );

    await user.selectOptions(
      screen.getByLabelText(/time available today/i),
      "60",
    );
    await waitFor(() =>
      expect(screen.getByTestId("today-total")).toHaveTextContent("60"),
    );

    const budgets = h.fake.calls
      .filter((c) => c.command === "study_plan")
      .map((c) => (c.args as { available_minutes: number }).available_minutes);
    expect(budgets).toEqual([30, 60]);
  });

  it("says nothing needs work rather than inventing filler", async () => {
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    h.fake.client.studyPlan = async () => ({ drills: [], total_minutes: 0 });

    mount(<TodayView />, h);
    expect(await screen.findByTestId("plan-empty")).toBeInTheDocument();
  });

  it("tells the learner to create a profile when none is selected", async () => {
    const h = harness();
    mount(<TodayView />, h);
    expect(await screen.findByTestId("no-profile")).toBeInTheDocument();
    expect(h.fake.commands()).not.toContain("study_plan");
  });

  it("does not render a plan without a backend", () => {
    mountDetached(<TodayView />);
    expect(screen.getByTestId("backend-unavailable")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-011
// ---------------------------------------------------------------------------

describe("readiness panel", () => {
  it("renders the stored estimate and never an official score claim", async () => {
    const h = harness({
      readiness: {
        low: 0.44,
        high: 0.66,
        confidence: 0.6,
        official_score_claim: false,
      },
    });
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<ReadinessPanel />, h);

    expect(await screen.findByTestId("readiness-band")).toHaveTextContent(
      "44%",
    );
    expect(screen.getByTestId("readiness-disclaimer")).toHaveTextContent(
      /not an official ASVAB or AFQT score prediction/i,
    );
    expect(
      screen.queryByTestId("readiness-illegal-claim"),
    ).not.toBeInTheDocument();
  });

  it("counts the attempts the estimate rests on", async () => {
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    await h.fake.client.recordAttempt({
      attemptId: "a-1",
      learnerId: profile.id,
      subtest: "AR",
      questionId: "q-1",
      correct: true,
      latencyMs: 1000,
    });
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<ReadinessPanel />, h);
    expect(await screen.findByTestId("readiness-evidence")).toHaveTextContent(
      /1 recorded attempt/i,
    );
  });

  it("shows a wide band as wide rather than implying precision", async () => {
    const h = harness({
      readiness: {
        low: 0,
        high: 1,
        confidence: 0,
        official_score_claim: false,
      },
    });
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<ReadinessPanel />, h);
    expect(await screen.findByTestId("readiness-wide")).toBeInTheDocument();
  });

  it("renders an error, not a number, when the backend refuses", async () => {
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    h.fake.client.readiness = async () => {
      throw new Error("storage error: database is locked");
    };

    mount(<ReadinessPanel />, h);
    expect(await screen.findByTestId("backend-error")).toHaveTextContent(
      /database is locked/i,
    );
    expect(screen.queryByTestId("readiness-band")).not.toBeInTheDocument();
  });

  it("keeps the presentational view honest about a claiming band", () => {
    render(
      <ReadinessView
        band={{
          low: 40,
          high: 60,
          confidence: 0.9,
          official_score_claim: true,
        }}
      />,
    );
    expect(screen.getByTestId("readiness-illegal-claim")).toBeInTheDocument();
  });

  it("recomputes mastery from stored attempts and re-reads the estimate", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    for (const [index, correct] of [true, false].entries()) {
      await h.fake.client.recordAttempt({
        attemptId: `a-${index}`,
        learnerId: profile.id,
        subtest: "AR",
        questionId: `q-${index}`,
        correct,
        latencyMs: 1000,
      });
    }
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<ReadinessPanel />, h);
    await screen.findByTestId("readiness-band");
    expect(h.fake.state.mastery.size).toBe(0);

    await user.click(
      screen.getByRole("button", { name: /recompute mastery/i }),
    );

    expect(await screen.findByTestId("recompute-status")).toHaveTextContent(
      /Recomputed 1 subtest estimate/i,
    );

    // The durable effect: one derived row, with the Laplace estimate from 1 of 2.
    const derived = h.fake.state.mastery.get(`${profile.id}:AR`);
    expect(derived).toBeDefined();
    expect(derived?.score).toBeCloseTo(0.5, 9);
    expect(derived?.uncertainty).toBeCloseTo(1 / Math.sqrt(3), 9);
  });

  it("reports a refused recompute instead of presenting a stale estimate as fresh", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    h.fake.client.recomputeMastery = async () => {
      throw new Error("storage error: database is locked");
    };

    mount(<ReadinessPanel />, h);
    await screen.findByTestId("readiness-band");
    await user.click(
      screen.getByRole("button", { name: /recompute mastery/i }),
    );

    expect(await screen.findByTestId("recompute-error")).toHaveTextContent(
      /database is locked/i,
    );
  });
});

// ---------------------------------------------------------------------------
// REQ-020 evidence vault
// ---------------------------------------------------------------------------

describe("sources view", () => {
  it("says the vault is empty rather than showing nothing at all", async () => {
    const h = harness();
    mount(<SourcesView />, h);
    expect(await screen.findByTestId("vault-empty")).toBeInTheDocument();
  });

  it("lists stored snapshots with their provenance", async () => {
    const h = harness();
    await h.fake.client.evidencePut({
      url: "https://www.officialasvab.com/",
      title: "Official ASVAB programme",
      content_hash: "sha256:abc123def456ghi789",
      license: "public-domain",
      effective_date: "2025-01-01",
      trust: 0.9,
      retrieval_status: "retrieved",
    });

    mount(<SourcesView />, h);

    const table = await screen.findByTestId("vault-table");
    expect(table).toHaveTextContent("Official ASVAB programme");
    expect(table).toHaveTextContent("public-domain");
    expect(table).toHaveTextContent("2025-01-01");
    expect(within(table).getByRole("link")).toHaveAttribute(
      "href",
      "https://www.officialasvab.com/",
    );
  });

  it("flags a snapshot that was not retrieved as unverified", async () => {
    const h = harness();
    await h.fake.client.evidencePut({
      url: "https://example.test/missing",
      title: "Unreachable source",
      content_hash: "sha256:deadbeef00112233",
      license: "unknown",
      effective_date: "2025-01-01",
      trust: 0.2,
      retrieval_status: "failed",
    });

    mount(<SourcesView />, h);
    expect(await screen.findByText(/not verified/i)).toBeInTheDocument();
  });

  it("explains that snapshots cannot be edited", async () => {
    const h = harness();
    mount(<SourcesView />, h);
    expect(screen.getByTestId("vault-explainer")).toHaveTextContent(
      /append-only and cannot be edited/i,
    );
    expect(await screen.findByTestId("vault-empty")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /edit/i }),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// REQ-030, REQ-033, REQ-034, REQ-035
// ---------------------------------------------------------------------------

describe("local data export and erasure", () => {
  it("exports an archive and proves it is on disk before claiming success", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<PrivacyView reports={[]} />, h);

    await user.click(
      screen.getByRole("button", { name: /export local data/i }),
    );

    const status = await screen.findByTestId("export-status");
    expect(status).toHaveTextContent(/integrity check: ok/i);
    expect(h.fake.state.backups).toHaveLength(1);
    // The archive list was re-read, which is what makes the claim checkable.
    expect(h.fake.commands()).toContain("backup_list");
  });

  it("refuses a restore whose archive no longer matches its recorded digest", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<PrivacyView reports={[]} />, h);
    await user.click(
      screen.getByRole("button", { name: /export local data/i }),
    );
    await screen.findByTestId("export-status");

    // Tamper with the archive behind the UI's back.
    h.fake.state.backups[0].checksum = "0".repeat(64);

    await user.click(
      screen.getByRole("button", { name: /restore this archive/i }),
    );

    const error = await screen.findByTestId("restore-error");
    expect(error).toHaveTextContent(/does not match the recorded digest/i);
    expect(error).toHaveTextContent(/your current data is unchanged/i);
    expect(screen.queryByTestId("restore-status")).not.toBeInTheDocument();
  });

  it("restores an intact archive and reports the rows read back", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    await h.fake.client.recordAttempt({
      attemptId: "a-1",
      learnerId: profile.id,
      subtest: "AR",
      questionId: "q-1",
      correct: true,
      latencyMs: 900,
    });
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<PrivacyView reports={[]} />, h);
    await user.click(
      screen.getByRole("button", { name: /export local data/i }),
    );
    await screen.findByTestId("export-status");

    // Destroy the live data, then restore it.
    await h.fake.client.resetLocalData("DELETE");
    expect(h.fake.state.attempts.size).toBe(0);

    await user.click(
      screen.getByRole("button", { name: /restore this archive/i }),
    );

    expect(await screen.findByTestId("restore-status")).toHaveTextContent(
      /1 recorded attempt/i,
    );
    expect(h.fake.state.attempts.size).toBe(1);
  });

  it("erases local data only when the exact phrase is supplied", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);

    mount(<PrivacyView reports={[]} />, h);

    const button = screen.getByRole("button", { name: /delete everything/i });
    expect(button).toBeDisabled();

    await user.type(screen.getByLabelText(/type DELETE to confirm/i), "DELETE");
    await user.click(button);

    const status = await screen.findByTestId("delete-status");
    expect(status).toHaveTextContent(/Removed 1 profile/i);
    expect(status).toHaveTextContent(/Remaining: 0 profile/i);

    // The durable effect, read back from the state model.
    expect(h.fake.state.profiles.size).toBe(0);
    expect(h.fake.state.evidence.size).toBe(0);
  });

  it("reports a refused erasure instead of claiming the data is gone", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    h.fake.client.resetLocalData = async () => {
      throw new Error("storage error: database is locked");
    };

    mount(<PrivacyView reports={[]} />, h);
    await user.type(
      screen.getByLabelText(/type DELETE to confirm/i),
      DELETE_CONFIRMATION_PHRASE,
    );
    await user.click(
      screen.getByRole("button", { name: /delete everything/i }),
    );

    const status = await screen.findByTestId("delete-status");
    expect(status).toHaveTextContent(/was not deleted/i);
    expect(status).toHaveTextContent(/database is locked/i);
    expect(h.fake.state.profiles.size).toBe(1);
  });

  it("says the data was not deleted when there is no backend at all", async () => {
    const user = userEvent.setup();
    mountDetached(<PrivacyView reports={[]} />);

    await user.type(
      screen.getByLabelText(/type DELETE to confirm/i),
      DELETE_CONFIRMATION_PHRASE,
    );
    await user.click(
      screen.getByRole("button", { name: /delete everything/i }),
    );

    expect(await screen.findByTestId("delete-status")).toHaveTextContent(
      /was not deleted/i,
    );
  });
});

// ---------------------------------------------------------------------------
// The webview-to-Rust hop
// ---------------------------------------------------------------------------

describe("boundary handshake", () => {
  it("records a marker identifying this bundle", async () => {
    const h = harness();
    mount(<SourcesView />, h);

    await waitFor(() => expect(h.fake.state.uiReadyMarkers).toHaveLength(1));
    const marker = h.fake.state.uiReadyMarkers[0];
    expect(marker.bundle.length).toBeGreaterThan(0);
    expect(marker.bundle).not.toBe("unset");
  });

  it("surfaces a broken hop instead of looking healthy", async () => {
    // A window that renders while every command fails is the failure mode this
    // exists to catch: without it the interface would look normal and empty.
    const h = harness();
    h.fake.client.uiReady = async () => {
      throw new Error("command ui_ready not found");
    };

    render(
      <BackendProvider available client={h.fake.client}>
        <Shell />
      </BackendProvider>,
    );

    expect(await screen.findByTestId("ipc-broken")).toHaveTextContent(
      /could not reach the local core/i,
    );
  });

  it("says nothing about a broken hop when the hop works", async () => {
    const h = harness();
    render(
      <BackendProvider available client={h.fake.client}>
        <Shell />
      </BackendProvider>,
    );

    await waitFor(() => expect(h.fake.state.uiReadyMarkers).toHaveLength(1));
    expect(screen.queryByTestId("ipc-broken")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// The plan's objective reaches the practice session
// ---------------------------------------------------------------------------

describe("the plan's objective reaches practice", () => {
  /** A harness with a learner, a plan that names an objective, and a corpus. */
  async function plannedSession() {
    const h = harness({ planObjectives: { AR: "OBJ-AR-TEST-02" } });
    h.storage.setItem("vector.activeProfileId", "");
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.storage.setItem("vector.activeProfileId", profile.id);
    await h.fake.client.contentGenerate("AR", 12, 5);
    return h;
  }

  it("starts the objective the plan named, and the corpus answers for it", async () => {
    const user = userEvent.setup();
    const h = await plannedSession();

    render(
      <BackendProvider available client={h.fake.client}>
        <ProfileProvider storage={h.storage}>
          <Shell />
        </ProfileProvider>
      </BackendProvider>,
    );

    // The plan is on the default view; its drill is the way into a session.
    const start = await screen.findByTestId("drill-start-AR");
    expect(start).toHaveTextContent("OBJ-AR-TEST-02");
    await user.click(start);

    // The session says which objective it is serving...
    expect(await screen.findByTestId("practice-objective")).toHaveTextContent(
      "OBJ-AR-TEST-02",
    );

    // ...and every item it asked for was narrowed to that objective. This is the
    // assertion that fails if the objective stops being sent: the fake's corpus
    // holds OBJ-AR-TEST-01 items too, and they are served when the read widens.
    const serves = h.fake.calls.filter(
      (call) => call.command === "content_next",
    );
    expect(serves.length).toBeGreaterThan(0);
    for (const call of serves) {
      expect(call.args?.objectiveId).toBe("OBJ-AR-TEST-02");
    }

    const questions = await screen.findAllByRole("radio");
    expect(questions.length).toBeGreaterThan(0);
    expect(screen.getByTestId("practice-corpus-summary")).toHaveTextContent(
      /from a corpus of 12/,
    );
  });

  it("does not offer a control that would go nowhere", async () => {
    // Rendered without a navigation callback, the drill stays a label. A button
    // that did nothing would be worse than no button.
    const h = await plannedSession();
    mount(<TodayView />, h);

    await waitFor(() =>
      expect(screen.getByTestId("today-drills")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("drill-objective-AR")).toHaveTextContent(
      "OBJ-AR-TEST-02",
    );
    expect(screen.queryByTestId("drill-start-AR")).not.toBeInTheDocument();
  });

  it("hands the drill's own subtest and objective to the caller", async () => {
    const user = userEvent.setup();
    const h = await plannedSession();
    const started: Array<{ subtest: string; objectiveId: string | null }> = [];
    mount(<TodayView onPractise={(request) => started.push(request)} />, h);

    await user.click(await screen.findByTestId("drill-start-WK"));
    await user.click(screen.getByTestId("drill-start-AR"));

    expect(started).toEqual([
      { subtest: "WK", objectiveId: null },
      { subtest: "AR", objectiveId: "OBJ-AR-TEST-02" },
    ]);
  });
});

describe("practice persistence", () => {
  it("records the attempt and reports the stored total read back", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);

    render(
      <BackendProvider available client={h.fake.client}>
        <PracticeView questions={sampleQuestions} learnerId={profile.id} />
      </BackendProvider>,
    );

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    const status = await screen.findByTestId("attempt-persistence");
    expect(status).toHaveTextContent(
      /Saved\. Stored attempts for this subtest: 1/,
    );
    expect(h.fake.state.attempts.size).toBe(1);
  });

  it("does not double-count a repeated submission", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);

    render(
      <BackendProvider available client={h.fake.client}>
        <PracticeView questions={sampleQuestions} learnerId={profile.id} />
      </BackendProvider>,
    );

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));
    await screen.findByTestId("attempt-persistence");
    expect(h.fake.state.attempts.size).toBe(1);

    // Move on, answer correctly, and confirm the first attempt was not re-sent.
    await user.click(screen.getByRole("button", { name: /next question/i }));
    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    await waitFor(() => expect(h.fake.state.attempts.size).toBe(2));
    const ids = h.fake.calls
      .filter((c) => c.command === "record_attempt")
      .map((c) => (c.args as { attempt_id: string }).attempt_id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("says the attempt was not saved when the write is refused", async () => {
    const user = userEvent.setup();
    const h = harness();
    const profile = await h.fake.client.createProfile("Ada", 60);
    h.fake.client.recordAttempt = async () => {
      throw new Error("storage error: read-only file system");
    };

    render(
      <BackendProvider available client={h.fake.client}>
        <PracticeView questions={sampleQuestions} learnerId={profile.id} />
      </BackendProvider>,
    );

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    expect(await screen.findByTestId("attempt-persistence")).toHaveTextContent(
      /was not saved: storage error: read-only file system/i,
    );
  });

  it("claims nothing about storage when no learner is selected", async () => {
    const user = userEvent.setup();
    render(<PracticeView questions={sampleQuestions} />);

    await user.click(screen.getAllByRole("radio")[1]);
    await user.click(screen.getByRole("button", { name: /check answer/i }));

    // The session counter still works; no persistence claim is made.
    expect(screen.getByTestId("attempt-summary")).toHaveTextContent(
      /1 attempted, 0 incorrect/i,
    );
    expect(screen.queryByTestId("attempt-persistence")).not.toBeInTheDocument();
  });
});
