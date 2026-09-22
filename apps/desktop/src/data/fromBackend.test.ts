/**
 * Adapting served items into the practice view's shape.
 *
 * The mapping is where a wire-format detail (JSON object keys are strings) meets
 * a render-time assumption (rationales are keyed by numeric option index), so
 * the failures are worth pinning down: each one would otherwise show up as a
 * blank rationale or a wrong "correct" tag rather than as an error.
 */

import { describe, expect, it } from "vitest";

import {
  ItemNotRenderableError,
  practiceQuestionFromItem,
  practiceQuestionsFromItems,
} from "./fromBackend";
import type { ItemDto } from "../ipc/types";

function item(overrides: Partial<ItemDto> = {}): ItemDto {
  return {
    id: "q-1",
    subtest: "AR",
    objective_id: "OBJ-AR-RATE-01",
    stem: "A printer produces 12 pages per minute. How many in 2.5 hours?",
    options: ["180", "1800", "360", "1440"],
    correct_index: 1,
    explanation: "12 * 150 = 1800",
    distractor_rationales: {
      "0": "Used 15 minutes instead of 150.",
      "2": "Treated 2.5 hours as 30 minutes.",
      "3": "Used pages per hour rather than per minute.",
    },
    difficulty: -0.5,
    ...overrides,
  };
}

describe("practiceQuestionFromItem", () => {
  it("maps a served item onto the view's shape", () => {
    const question = practiceQuestionFromItem(item());
    expect(question.id).toBe("q-1");
    expect(question.subtest).toBe("AR");
    expect(question.prompt).toBe(item().stem);
    expect(question.options).toEqual(["180", "1800", "360", "1440"]);
    expect(question.correctIndex).toBe(1);
    expect(question.explanation).toBe("12 * 150 = 1800");
  });

  it("rekeys rationales from strings to the option indices the view looks up", () => {
    const question = practiceQuestionFromItem(item());
    expect(question.distractorRationales[0]).toBe(
      "Used 15 minutes instead of 150.",
    );
    expect(question.distractorRationales[2]).toBe(
      "Treated 2.5 hours as 30 minutes.",
    );
    expect(question.distractorRationales[3]).toBe(
      "Used pages per hour rather than per minute.",
    );
    // The correct option carries no rationale.
    expect(question.distractorRationales[1]).toBeUndefined();
  });

  it("reports provenance as an objective, not as a source", () => {
    const question = practiceQuestionFromItem(item());
    expect(question.objectiveId).toBe("OBJ-AR-RATE-01");
    // A generated item quotes nobody, so it must not present itself as sourced.
    expect(question.sourceId).toBeUndefined();
  });

  it("refuses an item with an empty stem", () => {
    expect(() => practiceQuestionFromItem(item({ stem: "   " }))).toThrow(
      ItemNotRenderableError,
    );
  });

  it("refuses an item whose correct index does not address an option", () => {
    expect(() => practiceQuestionFromItem(item({ correct_index: 4 }))).toThrow(
      /outside 4 options/,
    );
  });

  it("refuses an item with fewer than two options", () => {
    expect(() =>
      practiceQuestionFromItem(
        item({
          options: ["only"],
          correct_index: 0,
          distractor_rationales: {},
        }),
      ),
    ).toThrow(/only 1 option/);
  });

  it("refuses a rationale keyed by something that is not an option index", () => {
    expect(() =>
      practiceQuestionFromItem(
        item({ distractor_rationales: { option_two: "wrong" } }),
      ),
    ).toThrow(/not an option index/);
  });

  it("refuses a rationale keyed by an index past the last option", () => {
    expect(() =>
      practiceQuestionFromItem(
        item({ distractor_rationales: { "9": "nope" } }),
      ),
    ).toThrow(/not an option index/);
  });

  it("converts a batch and refuses the batch if any item is unusable", () => {
    expect(
      practiceQuestionsFromItems([item(), item({ id: "q-2" })]),
    ).toHaveLength(2);
    expect(() =>
      practiceQuestionsFromItems([item(), item({ id: "q-2", stem: "" })]),
    ).toThrow(ItemNotRenderableError);
  });
});
