-- 004_content_source_integrity.sql
-- Requirements: REQ-020 (evidence vault), REQ-048, REQ-056.
--
-- 003 made `content_item_sources.source_id` a non-empty string and nothing more.
-- An item could therefore cite "SRC-DOES-NOT-EXIST" and satisfy REQ-056, because
-- "cites at least one source" was checked without asking whether the source was
-- ever recorded. Provenance that can name a source the vault has never seen is
-- not provenance; it is a comment.
--
-- Migration 003 also established the pattern for rules that read another table:
-- a CHECK cannot reference a second table, so these are triggers. The two below
-- close the loop in both directions -- a citation must name a recorded source,
-- and a recorded source cannot be deleted out from under the items citing it.
--
-- Monotonic: never edits 001, 002 or 003.

-- A citation must resolve. Applies to inserts, and to updates of the key, so the
-- rule cannot be side-stepped by inserting a valid pair and then rewriting it.
CREATE TRIGGER IF NOT EXISTS content_item_sources_must_resolve
BEFORE INSERT ON content_item_sources
WHEN NOT EXISTS (
    SELECT 1 FROM evidence_records WHERE id = NEW.source_id
)
BEGIN
    SELECT RAISE(
        ABORT,
        'cited source is not in the evidence vault; record it before citing it'
    );
END;

CREATE TRIGGER IF NOT EXISTS content_item_sources_must_resolve_on_update
BEFORE UPDATE OF source_id ON content_item_sources
WHEN NOT EXISTS (
    SELECT 1 FROM evidence_records WHERE id = NEW.source_id
)
BEGIN
    SELECT RAISE(
        ABORT,
        'cited source is not in the evidence vault; record it before citing it'
    );
END;

-- The reverse direction. Deleting a recorded source would leave every item that
-- cites it with a dangling citation, and the citation triggers above only run on
-- the citation table. Provenance is a two-sided obligation.
CREATE TRIGGER IF NOT EXISTS evidence_records_no_delete_while_cited
BEFORE DELETE ON evidence_records
WHEN EXISTS (
    SELECT 1 FROM content_item_sources WHERE source_id = OLD.id
)
BEGIN
    SELECT RAISE(
        ABORT,
        'source is cited by one or more content items and cannot be deleted'
    );
END;
