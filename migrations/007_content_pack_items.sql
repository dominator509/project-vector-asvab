-- 007_content_pack_items.sql
-- Which items each pack version contains.
-- Requirements: REQ-023 (versioned packs), REQ-032 (rollback), REQ-056.
--
-- ## Why membership is not `content_items.pack_id`
--
-- `content_items.pack_id` records which pack *first delivered* a row, and that is a
-- provenance fact worth keeping: it says where the content came from. It is the wrong
-- field to derive service from, because a pack version is not the only pack that can
-- contain an item. A second version that repackages the same 7,000 questions would
-- otherwise either have to steal them from the first version -- so that rolling back
-- withdrew everything -- or leave them behind, so that publishing it withdrew
-- everything. Both happened while building this: installing a real second pack took
-- the corpus from 7,016 servable items to none.
--
-- Membership is therefore its own relation. An item serves while *any* pack that
-- contains it is active, and a pack contains exactly the items it was built with. That
-- makes the two cases behave correctly and differently:
--
--   * v2 repackages v1's questions. Both packs list them, so rollback to v1 keeps
--     them serving and publishing v2 keeps them serving.
--   * v2 adds questions v1 never had. Those belong to v2 alone, so rolling back to v1
--     withdraws them -- which is what a rollback is for.
--
-- Monotonic: never edits 001 through 006.

CREATE TABLE IF NOT EXISTS content_pack_items (
    pack_id TEXT NOT NULL REFERENCES content_packs(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL REFERENCES content_items(id) ON DELETE CASCADE,
    PRIMARY KEY (pack_id, item_id)
);

-- Serving asks "which packs contain this item", so the index runs that way round.
CREATE INDEX IF NOT EXISTS idx_content_pack_items_item
    ON content_pack_items (item_id);
