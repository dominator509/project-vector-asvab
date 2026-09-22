-- 005_content_passage.sql
-- Paragraph Comprehension items carry the passage they are about.
-- Requirements: REQ-022, REQ-023, REQ-053.
--
-- Why this is a migration rather than a field on the item's stem: a Paragraph
-- Comprehension item is a passage plus a question about it, and the passage is not
-- the question. Folding it into `stem` would make every reader -- the serving API,
-- the content manager, the attempt record, any future export -- parse one column to
-- recover two facts, and would leave the passage absent from the schema's own
-- account of what an item is.
--
-- Why a constraint rather than a convention: `item.passage` is optional in Rust
-- because only PC uses it, and an optional field that PC items happen to fill in is
-- exactly the kind of rule a future ingester or a bad merge drops silently. The
-- result would be a servable question with nothing to read, which is unanswerable
-- rather than merely thin. Making it a CHECK means that state cannot be stored.
--
-- Monotonic: never edits 001 through 004.

-- SQLite's ALTER TABLE ADD COLUMN cannot add a CHECK constraint, so the column is
-- added plainly and the rule is enforced by triggers below. This is the same
-- technique 003 uses for the cross-table rules it could not express inline.
ALTER TABLE content_items ADD COLUMN passage TEXT;

-- The table is rebuilt nowhere and the column is never dropped, so a repeat run of
-- this migration is a no-op. `MigrationManager` records applied migrations by name;
-- this guard exists so that a manual application of the file cannot fail halfway
-- and leave the schema looking unmigrated.
CREATE TRIGGER IF NOT EXISTS content_items_pc_requires_passage_insert
BEFORE INSERT ON content_items
WHEN NEW.subtest = 'PC'
    AND (NEW.passage IS NULL OR length(trim(NEW.passage)) = 0)
BEGIN
    SELECT RAISE(
        ABORT,
        'a Paragraph Comprehension item must carry the passage it is about'
    );
END;

-- An UPDATE that moves an item into or within PC must not be able to strip the
-- passage either: `UPDATE content_items SET passage = NULL` on an active PC item
-- would otherwise leave a servable question with nothing to read.
CREATE TRIGGER IF NOT EXISTS content_items_pc_requires_passage_update
BEFORE UPDATE ON content_items
WHEN NEW.subtest = 'PC'
    AND (NEW.passage IS NULL OR length(trim(NEW.passage)) = 0)
BEGIN
    SELECT RAISE(
        ABORT,
        'a Paragraph Comprehension item must carry the passage it is about'
    );
END;

-- A passage has to be long enough to comprehend. `MIN_PASSAGE_WORDS` in
-- `vector-questions::passages` is 40 and the builder refuses anything shorter, but
-- the store is the last line: an item whose passage is one sentence asks the learner
-- to find a detail in a text that has none.
--
-- The floor is a character count rather than a word count. Counting words in SQLite
-- means counting spaces, which is wrong for any text carrying a newline or a double
-- space, and a rule that silently miscalculates is worse than a weaker rule that is
-- exact. 100 characters is below what 40 words of English can occupy -- 40 words
-- would have to average under two and a half characters each to fit -- so this
-- cannot refuse a passage the builder accepts, while still refusing `"See above."`.
-- The exact word-count rule stays in `verify`, where words can be counted.
CREATE TRIGGER IF NOT EXISTS content_items_pc_passage_is_substantial
BEFORE INSERT ON content_items
WHEN NEW.subtest = 'PC'
    AND NEW.passage IS NOT NULL
    AND length(trim(NEW.passage)) < 100
BEGIN
    SELECT RAISE(
        ABORT,
        'a Paragraph Comprehension passage must be long enough to comprehend'
    );
END;

-- The same rule on the way in must hold on the way through: an item restated as PC
-- is subject to the floor as well.
CREATE TRIGGER IF NOT EXISTS content_items_pc_passage_is_substantial_update
BEFORE UPDATE ON content_items
WHEN NEW.subtest = 'PC'
    AND NEW.passage IS NOT NULL
    AND length(trim(NEW.passage)) < 100
BEGIN
    SELECT RAISE(
        ABORT,
        'a Paragraph Comprehension passage must be long enough to comprehend'
    );
END;
