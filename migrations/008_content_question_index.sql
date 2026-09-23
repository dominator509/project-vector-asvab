-- 008_content_question_index.sql
-- The index the question identity needs.
-- Requirements: REQ-023, REQ-056.
--
-- ## Why this exists
--
-- Ingesting an item now asks the store whether it already asks that question, and the
-- question is identified by its stem, its correct answer and its passage
-- (`ContentItemRepo::question_exists`). That predicate is evaluated once per *built*
-- candidate, and a builder produces far more candidates than items: the Electronics
-- Information path builds about 10,000 candidates per run to find 414 questions, and the
-- Word Knowledge path draws from 200,000 thesaurus lines.
--
-- Without an index the predicate scans `content_items` every time, which turns an
-- ingestion into a quadratic walk. Measured on the 24-module NEETS collection: the run had
-- not finished after ten minutes, where the same run without the question check took about
-- three. The first column is `subtest`, because every caller knows it and it is the
-- cheapest way to cut the candidate set.
--
-- The index is on the *expression*, not on `stem`, because the comparison is case- and
-- space-insensitive by design: `What are the Clamps used for?` and `What are the clamps
-- used for?` are one question, so a plain index on `stem` would not be used by the query
-- and would not help.
--
-- Monotonic: never edits 001 through 007.

CREATE INDEX IF NOT EXISTS idx_content_items_question
    ON content_items (subtest, LOWER(TRIM(stem)));
