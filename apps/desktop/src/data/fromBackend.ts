/**
 * Adapt served items into the shape the practice view renders.
 *
 * The practice view was written against `sample.ts`, which is demonstration
 * content. Real items arrive from the command boundary as `ItemDto` and differ
 * in two ways that matter:
 *
 * - `distractor_rationales` is keyed by *string* on the wire, because JSON object
 *   keys are strings. The view looks rationales up by numeric option index, so
 *   the keys are converted here rather than at every lookup. Doing it in the view
 *   would mean a `String(index)` conversion in the render path, which is the kind
 *   of thing that works until someone passes an index that is a string already.
 * - provenance is an `objective_id`, not a `source_id`. A generated item is
 *   original text that targets a published construct; it quotes nobody. Mapping
 *   the objective into the old `sourceId` field would make a generated item
 *   display as though it were sourced, which is the opposite of the provenance
 *   discipline the content pipeline exists to enforce.
 */

import type { ItemDto } from "../ipc/types";
import type { PracticeQuestion } from "./sample";

/** Why an item could not be shown to a learner. */
export class ItemNotRenderableError extends Error {
  constructor(
    readonly itemId: string,
    reason: string,
  ) {
    super(`item ${itemId} cannot be rendered: ${reason}`);
    this.name = "ItemNotRenderableError";
  }
}

/**
 * Convert one served item.
 *
 * Throws rather than returning a partly-filled question. A question rendered
 * with an empty stem or a missing correct option looks to the learner like a
 * content gap and is really a boundary gap, and the two need different fixes.
 */
export function practiceQuestionFromItem(item: ItemDto): PracticeQuestion {
  if (item.stem.trim().length === 0) {
    throw new ItemNotRenderableError(item.id, "the stem is empty");
  }
  if (item.options.length < 2) {
    throw new ItemNotRenderableError(
      item.id,
      `only ${item.options.length} option(s)`,
    );
  }
  if (
    !Number.isInteger(item.correct_index) ||
    item.correct_index < 0 ||
    item.correct_index >= item.options.length
  ) {
    throw new ItemNotRenderableError(
      item.id,
      `correct_index ${item.correct_index} is outside ${item.options.length} options`,
    );
  }

  const rationales: Record<number, string> = {};
  for (const [key, rationale] of Object.entries(item.distractor_rationales)) {
    const index = Number(key);
    if (!Number.isInteger(index) || index < 0 || index >= item.options.length) {
      throw new ItemNotRenderableError(
        item.id,
        `distractor rationale is keyed by ${key}, which is not an option index`,
      );
    }
    rationales[index] = rationale;
  }

  // A Paragraph Comprehension item is a passage plus a question about it, so an
  // item of that subtest without a passage cannot be answered. The store refuses
  // one, which makes this a boundary check rather than a content check: reaching it
  // means the response is not the shape the command contract describes.
  if (item.subtest === "PC" && (item.passage ?? "").trim().length === 0) {
    throw new ItemNotRenderableError(
      item.id,
      "a Paragraph Comprehension item must carry the passage it is about",
    );
  }

  return {
    id: item.id,
    subtest: item.subtest,
    prompt: item.stem,
    passage: item.passage ?? undefined,
    options: item.options,
    correctIndex: item.correct_index,
    explanation: item.explanation,
    distractorRationales: rationales,
    objectiveId: item.objective_id,
  };
}

/** Convert a batch, refusing the whole batch if any item is unrenderable. */
export function practiceQuestionsFromItems(
  items: ItemDto[],
): PracticeQuestion[] {
  return items.map(practiceQuestionFromItem);
}
