/**
 * Local full-text search across lessons, error notebook entries and evidence
 * records (REQ-044).
 *
 * Search is deliberately local: no query leaves the machine. The index is built
 * in memory from already-loaded records, which keeps the offline guarantee
 * (REQ-042) intact.
 */

/** The categories a result can belong to. */
export type SearchCategory = "lesson" | "error" | "evidence";

/** A searchable record. */
export interface SearchDocument {
  id: string;
  category: SearchCategory;
  title: string;
  body: string;
  /** Subtest code, when the record belongs to one. */
  subtest?: string;
  /** Provenance for evidence records. */
  sourceId?: string;
}

/** A ranked search result. */
export interface SearchResult {
  document: SearchDocument;
  score: number;
  /** The snippet shown to the learner, with the matched terms intact. */
  snippet: string;
  matchedTerms: string[];
}

/** Words with no discriminative value in a study corpus. */
const STOP_WORDS = new Set([
  "a",
  "an",
  "and",
  "are",
  "as",
  "at",
  "be",
  "but",
  "by",
  "for",
  "from",
  "has",
  "have",
  "in",
  "into",
  "is",
  "it",
  "its",
  "of",
  "on",
  "or",
  "that",
  "the",
  "their",
  "then",
  "there",
  "these",
  "they",
  "this",
  "to",
  "was",
  "were",
  "what",
  "when",
  "which",
  "who",
  "will",
  "with",
]);

/** Length of the snippet window, in characters. */
export const SNIPPET_LENGTH = 160;

/** Tokenize text into lowercase alphanumeric terms, dropping stop words. */
export function tokenize(text: string): string[] {
  return text
    .split(/[^A-Za-z0-9]+/)
    .map((t) => t.toLowerCase())
    .filter((t) => t.length > 0 && !STOP_WORDS.has(t));
}

/** Whether a term is a prefix of, or equal to, a token. */
function termMatches(token: string, term: string): boolean {
  // Prefix matching so a learner typing "arith" finds "arithmetic".
  return token.startsWith(term);
}

/**
 * Score a document against query terms.
 *
 * Title matches weigh more than body matches, because a term in the title is a
 * stronger signal of what the record is about.
 */
export function scoreDocument(
  doc: SearchDocument,
  terms: string[],
): { score: number; matchedTerms: string[] } {
  if (terms.length === 0) return { score: 0, matchedTerms: [] };

  const titleTokens = tokenize(doc.title);
  const bodyTokens = tokenize(doc.body);
  // Occurrence counts, so a document that repeats a term ranks above one that
  // mentions it once.
  const bodyCounts = new Map<string, number>();
  for (const token of bodyTokens) {
    bodyCounts.set(token, (bodyCounts.get(token) ?? 0) + 1);
  }

  let score = 0;
  const matchedTerms: string[] = [];

  for (const term of terms) {
    const inTitle = titleTokens.some((t) => termMatches(t, term));
    let bodyHits = 0;
    for (const [token, count] of bodyCounts) {
      if (termMatches(token, term)) bodyHits += count;
    }

    if (inTitle) score += 3;
    if (bodyHits > 0) {
      score += 1 + Math.log2(bodyHits);
      matchedTerms.push(term);
    } else if (inTitle) {
      matchedTerms.push(term);
    }
  }

  // Every query term must appear somewhere, or the document is not a match.
  if (matchedTerms.length < terms.length) {
    return { score: 0, matchedTerms: [] };
  }

  return { score, matchedTerms };
}

/** Build a snippet centred on the first matched term. */
export function buildSnippet(body: string, terms: string[]): string {
  const flat = body.replace(/\s+/g, " ").trim();
  if (flat.length <= SNIPPET_LENGTH) return flat;

  const lower = flat.toLowerCase();
  let hit = -1;
  for (const term of terms) {
    const idx = lower.indexOf(term);
    if (idx >= 0 && (hit < 0 || idx < hit)) hit = idx;
  }

  if (hit < 0) return `${flat.slice(0, SNIPPET_LENGTH)}…`;

  const half = Math.floor(SNIPPET_LENGTH / 2);
  const start = Math.max(0, hit - half);
  const end = Math.min(flat.length, start + SNIPPET_LENGTH);
  const prefix = start > 0 ? "…" : "";
  const suffix = end < flat.length ? "…" : "";
  return `${prefix}${flat.slice(start, end)}${suffix}`;
}

/**
 * Search the corpus.
 *
 * An empty query returns nothing rather than the whole corpus: a search box
 * that dumps everything on focus is not a search.
 */
export function search(
  documents: SearchDocument[],
  query: string,
  options: { limit?: number; categories?: SearchCategory[] } = {},
): SearchResult[] {
  const terms = tokenize(query);
  if (terms.length === 0) return [];

  const allowed = options.categories;
  const results: SearchResult[] = [];

  for (const doc of documents) {
    if (allowed && !allowed.includes(doc.category)) continue;
    const { score, matchedTerms } = scoreDocument(doc, terms);
    if (score <= 0) continue;
    results.push({
      document: doc,
      score,
      snippet: buildSnippet(doc.body, matchedTerms),
      matchedTerms,
    });
  }

  // Deterministic ordering: score desc, then id asc so equal scores are stable
  // between renders and between runs.
  results.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.document.id.localeCompare(b.document.id);
  });

  return options.limit !== undefined
    ? results.slice(0, options.limit)
    : results;
}
