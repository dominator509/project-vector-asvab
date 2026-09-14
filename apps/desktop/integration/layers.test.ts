/**
 * Integration tests: cross-layer behaviour through real boundaries.
 *
 * TESTING.md: "integration uses real SQLite and real parsers". These tests
 * exercise the Rust layers together rather than in isolation, and the SQLite
 * paths use real files rather than in-memory databases.
 */

import { describe, expect, it } from "vitest";

// The Rust integration surface is exercised by `cargo test --workspace`, which
// scripts/test-integration.sh runs first. What belongs here is the frontend's
// integration with the data shapes those layers produce, so the two halves
// cannot drift apart silently.

import { search, type SearchDocument } from "../src/search/search";
import { sampleQuestions, reviewCards } from "../src/data/sample";
import {
  DEFAULT_A11Y_SETTINGS,
  normalizeA11ySettings,
  MAX_FONT_SCALE,
} from "../src/accessibility/settings";
import { VIEWS, isViewId, viewDefinition } from "../src/app/views";
import { createVectorClient } from "../src/ipc/client";

describe("data shape integration", () => {
  it("every sample question has a valid answer index and full distractor coverage", () => {
    for (const question of sampleQuestions) {
      expect(question.correctIndex).toBeGreaterThanOrEqual(0);
      expect(question.correctIndex).toBeLessThan(question.options.length);

      // REQ-005 requires distractor rationales; every wrong option needs one.
      for (let i = 0; i < question.options.length; i += 1) {
        if (i === question.correctIndex) continue;
        expect(
          question.distractorRationales[i],
          `${question.id} option ${i} needs a rationale`,
        ).toBeTruthy();
      }

      // Every question must cite a source (REQ-056 provenance).
      expect(question.sourceId).toMatch(/^SRC-/);
    }
  });

  it("review cards carry real FSRS state", () => {
    for (const card of reviewCards) {
      expect(card.stability).toBeGreaterThan(0);
      expect(card.lapses).toBeGreaterThanOrEqual(0);
      expect(Number.isFinite(card.dueInDays)).toBe(true);
    }
  });

  it("the readiness layer refuses an official-score claim before it reaches a view", async () => {
    // ADR-010: no precise predicted score before a calibration cohort exists.
    // This used to assert against a hard-coded demonstration constant, which
    // proved nothing about the pipeline. The real guarantee lives at the IPC
    // boundary, so that is what is exercised: whatever the backend ever
    // returns, a claim cannot reach the renderer.
    const claiming = createVectorClient(async () => ({
      low: 0.4,
      high: 0.6,
      confidence: 0.5,
      official_score_claim: true,
    }));
    await expect(claiming.readiness("learner-1")).rejects.toThrow(
      /official_score_claim was true/,
    );

    const honest = createVectorClient(async () => ({
      low: 0.43,
      high: 0.68,
      confidence: 0.57,
      official_score_claim: false,
    }));
    const band = await honest.readiness("learner-1");
    expect(band.official_score_claim).toBe(false);
    expect(band.low).toBeLessThanOrEqual(band.high);
    expect(band.confidence).toBeLessThan(1);
  });
});

describe("view registry integration", () => {
  it("every registered view resolves to a definition", () => {
    for (const view of VIEWS) {
      const definition = viewDefinition(view.id);
      expect(definition.id).toBe(view.id);
      expect(definition.label.length).toBeGreaterThan(0);
    }
  });

  it("view ids are unique and ordering is total", () => {
    const ids = VIEWS.map((v) => v.id);
    expect(new Set(ids).size).toBe(ids.length);

    const orders = VIEWS.map((v) => v.order);
    expect(new Set(orders).size).toBe(orders.length);
  });

  it("unknown view ids are rejected", () => {
    expect(isViewId("today")).toBe(true);
    expect(isViewId("not-a-view")).toBe(false);
  });
});

describe("search integration with the content corpus", () => {
  it("finds a lesson when searching its topic", () => {
    const corpus: SearchDocument[] = [
      {
        id: "l1",
        category: "lesson",
        title: "Rate problems",
        body: "Rate multiplied by time gives distance.",
      },
    ];
    const results = search(corpus, "rate distance");
    expect(results).toHaveLength(1);
    expect(results[0].document.id).toBe("l1");
  });

  it("returns nothing for an empty query rather than everything", () => {
    const corpus: SearchDocument[] = [
      { id: "l1", category: "lesson", title: "A", body: "B" },
    ];
    expect(search(corpus, "")).toEqual([]);
  });
});

describe("accessibility integration", () => {
  it("clamps hostile settings before they reach CSS", () => {
    const clamped = normalizeA11ySettings({ fontScale: 999 });
    expect(clamped.fontScale).toBe(MAX_FONT_SCALE);

    const nan = normalizeA11ySettings({ fontScale: Number.NaN });
    expect(nan.fontScale).toBe(DEFAULT_A11Y_SETTINGS.fontScale);
  });
});
