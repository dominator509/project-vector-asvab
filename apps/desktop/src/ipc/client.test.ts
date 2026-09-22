/**
 * Contract tests for the command boundary (SPEC-003).
 *
 * The webview and the Rust core are separate programs, so nothing here can be
 * checked by the TypeScript compiler: a misspelled command or argument name
 * compiles cleanly and fails at runtime as `undefined`. These tests pin the wire
 * contract against a recorded transport, and `apps/desktop/src-tauri/src/commands.rs`
 * plus `tests/command_boundary.rs` are the Rust half of the same contract.
 */

import { describe, expect, it, vi } from "vitest";

import {
  BackendError,
  COMMAND_NAMES,
  MalformedResponseError,
  createVectorClient,
} from "./client";
import type { Invoke } from "./types";

/** A transport that records calls and answers from a script. */
function recorder(answers: Record<string, unknown>) {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const invoke: Invoke = async (command, args) => {
    calls.push(args === undefined ? { command } : { command, args });
    if (!(command in answers)) {
      throw new Error(`unscripted command ${command}`);
    }
    const answer = answers[command];
    if (answer instanceof Error) throw answer;
    return answer;
  };
  return { calls, invoke };
}

const health = {
  component: "database",
  healthy: true,
  basis: "operation_succeeded",
  profiles: 2,
};

const profile = { id: "learner-1", name: "Ada", target_score: 60 };

describe("command names and arguments", () => {
  it("calls health with no arguments", async () => {
    const { calls, invoke } = recorder({ health });
    await createVectorClient(invoke).health();
    expect(calls).toEqual([{ command: "health" }]);
  });

  it("sends snake_case argument names that the Rust commands declare", async () => {
    const { calls, invoke } = recorder({
      create_profile: profile,
      get_profile: profile,
      record_attempt: true,
      analytics: { total: 0, correct: 0, accuracy: 0, mean_latency_ms: 0 },
      set_mastery: null,
      study_plan: { drills: [], total_minutes: 0 },
      backup_create: {
        path: "/tmp/b.db",
        checksum: "a".repeat(64),
        bytes: 1,
        integrity: "ok",
        encrypted: false,
        attempt_rows: 0,
      },
      backup_restore: { rows: 0 },
      latency_probe: {
        operation: "op",
        iterations: 1,
        mean_micros: 1,
        max_micros: 1,
      },
      evidence_put: "ev-1",
    });
    const client = createVectorClient(invoke);

    await client.createProfile("Ada", 60);
    await client.getProfile("learner-1");
    await client.recordAttempt({
      attemptId: "a-1",
      learnerId: "learner-1",
      subtest: "AR",
      questionId: "q-1",
      correct: true,
      latencyMs: 900,
    });
    await client.setMastery("learner-1", "AR", 0.5, 0.2);
    await client.studyPlan("learner-1", 60, 30);
    await client.backupCreate("C:/tmp");
    await client.backupRestore("C:/tmp/b.db", "a".repeat(64));
    await client.latencyProbe(50);
    await client.evidencePut({
      url: "https://example.test/",
      title: "t",
      content_hash: "sha256:x",
      license: "cc0",
      effective_date: "2025-01-01",
      trust: 0.5,
      retrieval_status: "retrieved",
    });

    expect(calls.map((c) => c.command)).toEqual([
      "create_profile",
      "get_profile",
      "record_attempt",
      "set_mastery",
      "study_plan",
      "backup_create",
      "backup_restore",
      "latency_probe",
      "evidence_put",
    ]);
    expect(calls[0].args).toEqual({ name: "Ada", target_score: 60 });
    expect(calls[2].args).toEqual({
      attempt_id: "a-1",
      learner_id: "learner-1",
      subtest: "AR",
      question_id: "q-1",
      correct: true,
      latency_ms: 900,
    });
    expect(calls[4].args).toEqual({
      learner_id: "learner-1",
      target_score: 60,
      available_minutes: 30,
    });
    expect(calls[7].args).toEqual({ iterations: 50 });
  });

  it("never calls a command outside the declared list", async () => {
    // The union type and the Rust handler list must agree; a command that is not
    // registered in `generate_handler!` would fail at runtime.
    const client = createVectorClient(async () => {
      throw new Error("unused");
    });
    const used: string[] = [];
    const spied = createVectorClient(async (command) => {
      used.push(command);
      throw new Error("stop");
    });

    // Every method is reachable and none escapes the union.
    const attempts: Array<() => Promise<unknown>> = [
      () => spied.health(),
      () => spied.createProfile("n", 1),
      () => spied.getProfile("i"),
      () => spied.listProfiles(),
      () =>
        spied.recordAttempt({
          attemptId: "a",
          learnerId: "l",
          subtest: "AR",
          questionId: "q",
          correct: true,
          latencyMs: 1,
        }),
      () => spied.analytics("l", "AR"),
      () => spied.analyticsAll("l"),
      () => spied.setMastery("l", "AR", 0.1, 0.1),
      () => spied.mastery("l"),
      () => spied.studyPlan("l", 1, 1),
      () => spied.readiness("l"),
      () => spied.evidenceList(),
      () => spied.evidenceGet("e"),
      () =>
        spied.evidencePut({
          url: "u",
          title: "t",
          content_hash: "h",
          license: "l",
          effective_date: "d",
          trust: 0.5,
          retrieval_status: "retrieved",
        }),
      () => spied.backupCreate("d"),
      () => spied.backupRestore("s", "c"),
      () => spied.latencyProbe(1),
      () => spied.appPaths(),
      () => spied.backupList("d"),
      () => spied.resetLocalData("DELETE"),
      () => spied.uiReady("stamp"),
      () => spied.recomputeMastery("l"),
      () => spied.contentGenerate("AR", 5, 0),
      () => spied.contentNext("AR", []),
      () => spied.contentStats(),
    ];
    for (const attempt of attempts) {
      await attempt().catch(() => undefined);
    }

    expect(used.sort()).toEqual([...COMMAND_NAMES].sort());
    // Silence the unused-client lint without weakening the assertions above.
    expect(client).toBeDefined();
  });

  it("returns the boolean that record_attempt reports", async () => {
    const { invoke } = recorder({ record_attempt: false });
    const result = await createVectorClient(invoke).recordAttempt({
      attemptId: "a",
      learnerId: "l",
      subtest: "AR",
      questionId: "q",
      correct: true,
      latencyMs: 1,
    });
    expect(result).toBe(false);
  });
});

describe("error handling", () => {
  it("maps a rejected string into a typed backend error", async () => {
    const invoke: Invoke = async () => {
      throw "invalid request: a learner name is required";
    };
    await expect(
      createVectorClient(invoke).createProfile("", 50),
    ).rejects.toThrow(BackendError);

    try {
      await createVectorClient(invoke).createProfile("", 50);
    } catch (error) {
      expect(error).toBeInstanceOf(BackendError);
      expect((error as BackendError).command).toBe("create_profile");
      expect((error as BackendError).message).toBe(
        "invalid request: a learner name is required",
      );
    }
  });

  it("maps a rejected Error into a typed backend error", async () => {
    const invoke: Invoke = async () => {
      throw new Error("storage error: disk is full");
    };
    await expect(createVectorClient(invoke).health()).rejects.toMatchObject({
      name: "BackendError",
      command: "health",
      message: "storage error: disk is full",
    });
  });
});

describe("response validation", () => {
  it("rejects a payload that is missing a declared field", async () => {
    // A silent `undefined` here would render an empty name rather than an error.
    const { invoke } = recorder({
      get_profile: { id: "learner-1", name: "Ada" },
    });
    await expect(
      createVectorClient(invoke).getProfile("learner-1"),
    ).rejects.toThrow(MalformedResponseError);
  });

  it("rejects a payload with the wrong field type", async () => {
    const { invoke } = recorder({
      health: { ...health, profiles: "two" },
    });
    await expect(createVectorClient(invoke).health()).rejects.toThrow(
      /field "profiles" must be a finite number/,
    );
  });

  it("rejects a list entry that is not an object", async () => {
    const { invoke } = recorder({ list_profiles: [profile, 7] });
    await expect(createVectorClient(invoke).listProfiles()).rejects.toThrow(
      MalformedResponseError,
    );
  });

  it("rejects an analytics_all entry that is not a pair", async () => {
    const { invoke } = recorder({ analytics_all: [["AR"]] });
    await expect(createVectorClient(invoke).analyticsAll("l")).rejects.toThrow(
      /expected a \[subtest, analytics\] pair/,
    );
  });

  it("accepts a well-formed analytics_all response", async () => {
    const { invoke } = recorder({
      analytics_all: [
        [
          "AR",
          { total: 3, correct: 2, accuracy: 0.667, mean_latency_ms: 1000 },
        ],
      ],
    });
    const pairs = await createVectorClient(invoke).analyticsAll("l");
    expect(pairs).toEqual([
      ["AR", { total: 3, correct: 2, accuracy: 0.667, mean_latency_ms: 1000 }],
    ]);
  });
});

describe("ADR-010 is enforced at the boundary", () => {
  it("refuses a readiness payload that claims an official score", async () => {
    // The view must never be handed a claim it would have to decide about.
    const { invoke } = recorder({
      readiness: {
        low: 40,
        high: 60,
        confidence: 0.5,
        official_score_claim: true,
      },
    });
    await expect(createVectorClient(invoke).readiness("l")).rejects.toThrow(
      /official_score_claim was true/,
    );
  });

  it("refuses an inverted readiness band", async () => {
    const { invoke } = recorder({
      readiness: {
        low: 0.8,
        high: 0.2,
        confidence: 0.5,
        official_score_claim: false,
      },
    });
    await expect(createVectorClient(invoke).readiness("l")).rejects.toThrow(
      /inverted/,
    );
  });

  it("passes through a valid band unchanged", async () => {
    const { invoke } = recorder({
      readiness: {
        low: 0.48,
        high: 0.71,
        confidence: 0.62,
        official_score_claim: false,
      },
    });
    await expect(createVectorClient(invoke).readiness("l")).resolves.toEqual({
      low: 0.48,
      high: 0.71,
      confidence: 0.62,
      official_score_claim: false,
    });
  });
});

describe("the declared command list", () => {
  it("contains every command the Rust handler registers", () => {
    // Kept as a literal so this test fails loudly if the surface changes
    // without a deliberate update on both sides.
    expect([...COMMAND_NAMES]).toEqual([
      "app_paths",
      "health",
      "create_profile",
      "get_profile",
      "list_profiles",
      "record_attempt",
      "analytics",
      "analytics_all",
      "set_mastery",
      "mastery",
      "study_plan",
      "readiness",
      "evidence_list",
      "evidence_get",
      "evidence_put",
      "backup_create",
      "backup_restore",
      "backup_list",
      "latency_probe",
      "reset_local_data",
      "ui_ready",
      "recompute_mastery",
      "content_generate",
      "content_next",
      "content_stats",
    ]);
  });

  it("does not silently swallow a transport rejection", async () => {
    const spy = vi.fn(async () => {
      throw "boom";
    });
    await expect(createVectorClient(spy).listProfiles()).rejects.toThrow(
      "boom",
    );
    expect(spy).toHaveBeenCalledWith("list_profiles", undefined);
  });
});
