/**
 * Response validation for the content commands.
 *
 * The client refuses a malformed response rather than casting it, because a cast
 * is a promise TypeScript cannot keep across the JSON channel. These tests drive
 * the readers with payloads that a plausible backend bug could produce, and
 * assert the refusal names the command.
 */

import { describe, expect, it } from "vitest";

import { createVectorClient } from "./client";
import type { ItemDto } from "./types";

function serve(payload: unknown) {
  return createVectorClient(async () => payload);
}

function validItem(overrides: Record<string, unknown> = {}) {
  return {
    id: "q-1",
    subtest: "AR",
    objective_id: "OBJ-AR-RATE-01",
    stem: "A printer produces 12 pages per minute. How many in 2.5 hours?",
    passage: null,
    options: ["180", "1800", "360", "1440"],
    correct_index: 1,
    explanation: "12 * 150 = 1800",
    distractor_rationales: { "0": "a", "2": "b", "3": "c" },
    difficulty: -0.5,
    ...overrides,
  };
}

describe("content_stats", () => {
  it("accepts a well-formed corpus report", async () => {
    const stats = await serve({
      total: 3,
      servable: 3,
      sources: 1,
      by_state: [{ state: "active", count: 3 }],
      by_subtest: [{ subtest: "AR", count: 3 }],
    }).contentStats();
    expect(stats.total).toBe(3);
    expect(stats.by_state[0]).toEqual({ state: "active", count: 3 });
    expect(stats.by_subtest[0]).toEqual({ subtest: "AR", count: 3 });
  });

  it("refuses a report missing a field rather than rendering undefined", async () => {
    await expect(
      serve({
        total: 3,
        servable: 3,
        by_state: [],
        by_subtest: [],
      }).contentStats(),
    ).rejects.toThrow(/sources/);
  });
});

describe("content_next", () => {
  it("returns a validated item", async () => {
    const item = await serve(validItem()).contentNext("AR", []);
    expect(item).not.toBeNull();
    expect((item as ItemDto).id).toBe("q-1");
  });

  it("treats an explicit null as an empty corpus", async () => {
    // An empty corpus is a fact, not an error, so it must not reject.
    await expect(serve(null).contentNext("AR", [])).resolves.toBeNull();
  });

  it("refuses an absent field, which is a different fact from an empty corpus", async () => {
    await expect(serve(undefined).contentNext("AR", [])).rejects.toThrow(
      /absent entirely/,
    );
  });

  it("refuses an item whose correct index does not address an option", async () => {
    await expect(
      serve(validItem({ correct_index: 7 })).contentNext("AR", []),
    ).rejects.toThrow(/does not address any of the 4 options/);
  });

  it("refuses an item with a single option", async () => {
    await expect(
      serve(validItem({ options: ["only"], correct_index: 0 })).contentNext(
        "AR",
        [],
      ),
    ).rejects.toThrow(/at least 2 options/);
  });

  it("refuses a non-string option", async () => {
    await expect(
      serve(validItem({ options: ["180", 1800] })).contentNext("AR", []),
    ).rejects.toThrow(/option must be a string/);
  });

  it("refuses a rationale that is not a string", async () => {
    await expect(
      serve(validItem({ distractor_rationales: { "0": 3 } })).contentNext(
        "AR",
        [],
      ),
    ).rejects.toThrow(/rationale 0 must be a string/);
  });
});

describe("content_generate", () => {
  it("accepts a clean run and reports what it did", async () => {
    const report = await serve({
      subtest: "AR",
      generated: 40,
      verified: 40,
      activated: 40,
      already_present: 0,
      rejected: [],
    }).contentGenerate("AR", 40, 0);
    expect(report.activated).toBe(40);
    expect(report.rejected).toEqual([]);
  });

  it("carries refusals through so the caller can show them", async () => {
    const report = await serve({
      subtest: "AR",
      generated: 2,
      verified: 1,
      activated: 1,
      already_present: 0,
      rejected: ["ar.work_rate: proof rejected"],
    }).contentGenerate("AR", 2, 0);
    expect(report.rejected).toEqual(["ar.work_rate: proof rejected"]);
  });

  it("refuses a report claiming more activations than verifications", async () => {
    // Verification happens before storage, so this ordering cannot occur. A
    // report that claims it would put a reassuring number over unchecked content.
    await expect(
      serve({
        subtest: "AR",
        generated: 10,
        verified: 4,
        activated: 9,
        already_present: 0,
        rejected: [],
      }).contentGenerate("AR", 10, 0),
    ).rejects.toThrow(/exceeds verified/);
  });
});

describe("the passage on a served item", () => {
  it("accepts a null passage, which is what every subtest but PC has", async () => {
    const item = await serve(validItem()).contentNext("AR", []);
    expect(item?.passage).toBeNull();
  });

  it("carries a Paragraph Comprehension passage through to the view", async () => {
    const passage =
      "The tower stood on the ridge for a hundred years before the surveyors arrived. " +
      "They measured its base and recorded the result in a log.";
    const item = await serve(validItem({ subtest: "PC", passage })).contentNext(
      "PC",
      [],
    );
    expect(item?.passage).toBe(passage);
  });

  it("refuses an item whose passage field is absent entirely", async () => {
    // Absence is not the same fact as "this subtest has no passage". The backend
    // always sends the field, so a missing one means the response is not the shape
    // the reader was written for, and treating it as null would hide that.
    const withoutPassage = validItem();
    delete (withoutPassage as Record<string, unknown>).passage;
    await expect(serve(withoutPassage).contentNext("AR", [])).rejects.toThrow(
      /passage/,
    );
  });

  it("refuses a passage that is neither a string nor null", async () => {
    await expect(
      serve(validItem({ passage: 42 })).contentNext("AR", []),
    ).rejects.toThrow(/passage/);
  });
});
