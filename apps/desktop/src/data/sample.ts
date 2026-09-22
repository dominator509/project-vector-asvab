/**
 * Sample data for the UI layer.
 *
 * This is demonstration content only. It is clearly named and confined to the
 * desktop app so it can never be mistaken for the real, sourced content packs
 * that EP-007 delivers. Every source-bearing record carries an id that traces
 * to a claim, per CONTENT_GOVERNANCE.md.
 */

import type { SearchDocument } from "../search/search";

export interface PracticeQuestion {
  id: string;
  subtest: string;
  prompt: string;
  options: string[];
  /** Index of the correct option. */
  correctIndex: number;
  explanation: string;
  /** Why each wrong option is wrong — the distractor rationale. */
  distractorRationales: Record<number, string>;
  /**
   * The learning objective an item serves. Generated items carry this, because
   * what a generated item takes from outside is the construct it targets rather
   * than a source it quotes.
   */
  objectiveId?: string;
  /**
   * The source a *sourced* item draws on. Demonstration items carry this; a
   * generated item has no third-party text to cite and deliberately does not
   * borrow this field to look sourced.
   */
  sourceId?: string;
}

export interface ReviewCard {
  id: string;
  subtest: string;
  prompt: string;
  /** Days until due; <= 0 means due now. */
  dueInDays: number;
  /** Current FSRS stability, in days. */
  stability: number;
  lapses: number;
}

export const sampleQuestions: PracticeQuestion[] = [
  {
    id: "q-ar-1",
    subtest: "AR",
    prompt:
      "A printer produces 12 pages per minute. How many pages does it produce in 2.5 hours?",
    options: ["180", "1,800", "360", "1,440"],
    correctIndex: 1,
    explanation:
      "Convert hours to minutes: 2.5 hours = 150 minutes. Then 12 pages/minute x 150 minutes = 1,800 pages.",
    distractorRationales: {
      0: "This is 12 x 15, using 15 minutes instead of 150 — the hour conversion was dropped.",
      2: "This is 12 x 30, treating 2.5 hours as 30 minutes.",
      3: "This is 24 x 60, using pages per hour rather than per minute.",
    },
    sourceId: "SRC-ASVAB-004",
  },
  {
    id: "q-wk-1",
    subtest: "WK",
    prompt: "Choose the word that most nearly means the same as CANDID.",
    options: ["Evasive", "Frank", "Cautious", "Ornate"],
    correctIndex: 1,
    explanation: "Candid means open and truthful, which is closest to frank.",
    distractorRationales: {
      0: "Evasive is the opposite of candid: it means avoiding a direct answer.",
      2: "Cautious describes care taken to avoid risk, not openness.",
      3: "Ornate describes elaborate decoration, unrelated to honesty.",
    },
    sourceId: "SRC-ASVAB-002",
  },
  {
    id: "q-pc-1",
    subtest: "PC",
    prompt:
      "Read the passage: 'Many recruits underestimate the value of sleep. A rested mind retains new material far better than a tired one.' What is the main idea?",
    options: [
      "Recruits should study longer hours.",
      "Sleep materially improves retention of new material.",
      "Fatigue has no measurable effect on learning.",
      "New material is difficult for everyone.",
    ],
    correctIndex: 1,
    explanation:
      "The passage's central claim is that a rested mind retains new material better.",
    distractorRationales: {
      0: "The passage argues for rest, not longer study hours.",
      2: "The passage states the opposite: fatigue harms retention.",
      3: "Difficulty for everyone is not mentioned.",
    },
    sourceId: "SRC-ASVAB-001",
  },
];

export const reviewCards: ReviewCard[] = [
  {
    id: "rc-1",
    subtest: "AR",
    prompt: "Unit conversion in rate problems",
    dueInDays: -2,
    stability: 4.2,
    lapses: 2,
  },
  {
    id: "rc-2",
    subtest: "WK",
    prompt: "Vocabulary: candid, frank, evasive",
    dueInDays: 0,
    stability: 7.8,
    lapses: 0,
  },
  {
    id: "rc-3",
    subtest: "PC",
    prompt: "Identifying a passage's main idea",
    dueInDays: 3,
    stability: 21.5,
    lapses: 0,
  },
];

/**
 * The daily plan and the readiness band are no longer declared here.
 *
 * Both were previously fixed constants in this file, and the views rendered
 * them, so the dashboard displayed numbers that came from source code rather
 * than from the learner's own history — a fabricated plan and a fabricated
 * readiness estimate. They now come from the real service layer over the Tauri
 * boundary (`TodayView`, `ReadinessPanel`), and nothing in the product reads a
 * demonstration plan.
 */

export const searchCorpus: SearchDocument[] = [
  {
    id: "lesson-ar-rates",
    category: "lesson",
    title: "Arithmetic Reasoning: rate problems",
    body: "Rate problems use the relationship rate multiplied by time equals distance. Always convert units before multiplying.",
    subtest: "AR",
  },
  {
    id: "lesson-wk-synonyms",
    category: "lesson",
    title: "Word Knowledge: synonyms",
    body: "Synonyms are words with similar meanings. Build vocabulary breadth rather than memorising single pairs.",
    subtest: "WK",
  },
  {
    id: "error-unit-conversion",
    category: "error",
    title: "Mistake: forgot to convert minutes to hours",
    body: "In a rate problem I multiplied pages per minute by a number of hours. The unit conversion was skipped.",
    subtest: "AR",
  },
  {
    id: "evidence-asvab-policy",
    category: "evidence",
    title: "Official ASVAB subtest descriptions",
    body: "The arithmetic reasoning subtest measures the ability to solve mathematical word problems.",
    sourceId: "SRC-ASVAB-004",
  },
];
