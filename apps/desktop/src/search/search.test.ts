/**
 * EP-005 acceptance: local full-text search (REQ-044).
 */

import { describe, expect, it } from "vitest";
import {
  buildSnippet,
  scoreDocument,
  search,
  tokenize,
  type SearchDocument,
} from "./search";

function doc(
  id: string,
  category: SearchDocument["category"],
  title: string,
  body: string,
): SearchDocument {
  return { id, category, title, body };
}

const CORPUS: SearchDocument[] = [
  doc(
    "lesson-1",
    "lesson",
    "Arithmetic Reasoning: rates",
    "Rate problems use the relationship rate multiplied by time equals distance.",
  ),
  doc(
    "lesson-2",
    "lesson",
    "Word Knowledge: synonyms",
    "Synonyms are words with similar meanings. Vocabulary breadth matters.",
  ),
  doc(
    "error-1",
    "error",
    "Mistake: rate problem units",
    "I forgot to convert minutes to hours in the rate problem.",
  ),
  doc(
    "evidence-1",
    "evidence",
    "Official ASVAB policy",
    "The arithmetic reasoning subtest measures mathematical problem solving.",
  ),
];

describe("tokenize", () => {
  it("lowercases and drops stop words", () => {
    expect(tokenize("The Rate OF the problem")).toEqual(["rate", "problem"]);
  });

  it("returns nothing for pure stop words", () => {
    expect(tokenize("the of and a to")).toEqual([]);
  });

  it("splits on punctuation", () => {
    expect(tokenize("rate-time,distance")).toEqual([
      "rate",
      "time",
      "distance",
    ]);
  });
});

describe("scoreDocument", () => {
  it("returns zero when a query term is absent", () => {
    const { score } = scoreDocument(CORPUS[0], ["photosynthesis"]);
    expect(score).toBe(0);
  });

  it("requires every query term to match", () => {
    // Partial matches would return documents that do not answer the query.
    const { score } = scoreDocument(CORPUS[0], ["rate", "photosynthesis"]);
    expect(score).toBe(0);
  });

  it("weights title matches above body matches", () => {
    const titled = scoreDocument(
      doc("t", "lesson", "rate problems", "unrelated body text"),
      ["rate"],
    );
    const bodied = scoreDocument(
      doc("b", "lesson", "unrelated title", "rate appears in the body"),
      ["rate"],
    );
    expect(titled.score).toBeGreaterThan(bodied.score);
  });
});

describe("search", () => {
  it("returns nothing for an empty query", () => {
    // A search box that dumps the whole corpus on focus is not a search.
    expect(search(CORPUS, "")).toEqual([]);
    expect(search(CORPUS, "   ")).toEqual([]);
    expect(search(CORPUS, "the of and")).toEqual([]);
  });

  it("finds documents matching the query", () => {
    const results = search(CORPUS, "rate");
    const ids = results.map((r) => r.document.id);
    expect(ids).toContain("lesson-1");
    expect(ids).toContain("error-1");
    expect(ids).not.toContain("lesson-2");
  });

  it("matches prefixes so partial typing works", () => {
    const results = search(CORPUS, "arith");
    const ids = results.map((r) => r.document.id);
    expect(ids).toContain("lesson-1");
    expect(ids).toContain("evidence-1");
  });

  it("filters by category", () => {
    const onlyErrors = search(CORPUS, "rate", { categories: ["error"] });
    expect(onlyErrors.length).toBeGreaterThan(0);
    expect(onlyErrors.every((r) => r.document.category === "error")).toBe(true);
  });

  it("honours the result limit", () => {
    const results = search(CORPUS, "rate", { limit: 1 });
    expect(results).toHaveLength(1);
  });

  it("orders deterministically for equal scores", () => {
    const a = search(CORPUS, "problem");
    const b = search(CORPUS, "problem");
    expect(a.map((r) => r.document.id)).toEqual(b.map((r) => r.document.id));
  });

  it("is case insensitive", () => {
    const lower = search(CORPUS, "rate").map((r) => r.document.id);
    const upper = search(CORPUS, "RATE").map((r) => r.document.id);
    expect(lower).toEqual(upper);
  });

  it("returns no results for a query that matches nothing", () => {
    expect(search(CORPUS, "quantum chromodynamics")).toEqual([]);
  });

  it("attaches a snippet and the matched terms", () => {
    const [first] = search(CORPUS, "rate");
    expect(first.snippet.length).toBeGreaterThan(0);
    expect(first.matchedTerms).toContain("rate");
  });
});

describe("buildSnippet", () => {
  it("returns the body unchanged when it is already short", () => {
    expect(buildSnippet("short body", ["short"])).toBe("short body");
  });

  it("centres a long body on the matched term", () => {
    const body = `${"x".repeat(400)} needle ${"y".repeat(400)}`;
    const snippet = buildSnippet(body, ["needle"]);
    expect(snippet).toContain("needle");
    expect(snippet.length).toBeLessThan(body.length);
  });

  it("marks truncation with ellipses", () => {
    const body = `${"x".repeat(400)} needle ${"y".repeat(400)}`;
    const snippet = buildSnippet(body, ["needle"]);
    expect(snippet.startsWith("…")).toBe(true);
    expect(snippet.endsWith("…")).toBe(true);
  });

  it("collapses whitespace so snippets render on one line", () => {
    const snippet = buildSnippet("a\n\n  b\t\tc", ["a"]);
    expect(snippet).not.toContain("\n");
    expect(snippet).not.toContain("\t");
  });
});
