-- 003_content_items.sql
-- Content domain: the item store that EP-007 never shipped.
-- Requirements: REQ-022, REQ-023, REQ-048, REQ-053, REQ-056.
--
-- Before this migration the database had nowhere to put a question. `attempts`
-- carried a `question_id` referencing a table that did not exist, `content_packs`
-- registered pack metadata with no items behind it, and the only questions in the
-- product were three literals in `apps/desktop/src/data/sample.ts`. This is the
-- missing store.
--
-- The constraints here are deliberately stricter than the application checks in
-- `vector-questions::ingestion`. `ItemProvenance::is_complete` enforces REQ-056 in
-- Rust, but a repository method can be bypassed by a future caller, a migration
-- script or a bad merge. Where a rule can be expressed as a constraint it is, so
-- that an item with incomplete provenance is not a representable state -- the
-- same reasoning that makes unapproved auto-merge unrepresentable in
-- `pull_requests`. Monotonic: never edits 001 or 002.

-- ---------------------------------------------------------------------------
-- Items. Lifecycle vocabulary matches `ItemState` in vector-questions exactly;
-- `state` is TEXT rather than an enum because SQLite has no enum type and a
-- CHECK is the enforceable equivalent.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS content_items (
    id                TEXT PRIMARY KEY,
    -- Nullable: an item drafted before pack assembly has no pack yet.
    pack_id           TEXT REFERENCES content_packs(id) ON DELETE SET NULL,
    subtest           TEXT NOT NULL CHECK (length(trim(subtest)) > 0),
    -- REQ-056: the versioned learning objective this item serves.
    objective_id      TEXT NOT NULL DEFAULT '',
    state             TEXT NOT NULL CHECK (state IN (
                          'draft',
                          'machine_validated',
                          'independent_verified',
                          'content_reviewed',
                          'active',
                          'quarantined'
                      )),
    stem              TEXT NOT NULL CHECK (length(trim(stem)) > 0),
    -- Ordered JSON array of options. Order is presentation order and is
    -- meaningful, so it is stored rather than derived.
    options_json      TEXT NOT NULL CHECK (json_valid(options_json)),
    correct_index     INTEGER NOT NULL CHECK (correct_index >= 0),
    explanation       TEXT NOT NULL DEFAULT '',
    -- JSON object: option index -> why that distractor is wrong. Named
    -- misconceptions, per QUESTION_FACTORY.md step 5.
    distractors_json  TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(distractors_json)),
    -- Signed internal difficulty estimate. Calibration against real learners is
    -- a separate, later concern; this is the provisional target.
    difficulty        REAL NOT NULL DEFAULT 0.0
                          CHECK (difficulty >= -4.0 AND difficulty <= 4.0),
    proof_kind        TEXT NOT NULL CHECK (proof_kind IN ('executable', 'source_backed')),
    proof_json        TEXT NOT NULL CHECK (json_valid(proof_json)),
    reviewer          TEXT NOT NULL DEFAULT '',
    -- ContentHash of the item as authored. UNIQUE so identical content
    -- deduplicates to one identity rather than silently double-serving.
    content_hash      TEXT NOT NULL UNIQUE CHECK (length(trim(content_hash)) > 0),
    generator_hash    TEXT,
    verifier_hash     TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,

    -- The correct answer must index an option that exists. Without this a
    -- correct_index of 3 against two options is storable and unanswerable.
    CHECK (correct_index < json_array_length(options_json)),

    -- An item needs at least two options to be a question at all.
    CHECK (json_array_length(options_json) >= 2),

    -- REQ-056, mirrored from ItemProvenance::is_complete: activation requires an
    -- objective and a named reviewer.
    CHECK (state <> 'active' OR length(trim(objective_id)) > 0),
    CHECK (state <> 'active' OR length(trim(reviewer)) > 0),

    -- An independent verification that produced identical output to the
    -- generator is not verification (QUESTION_FACTORY.md step 7). Scoped to
    -- activation: comparing them earlier is not an error, only activating on it.
    CHECK (
        state <> 'active'
        OR generator_hash IS NULL
        OR verifier_hash IS NULL
        OR generator_hash <> verifier_hash
    ),

    -- Closes a gap found in the existing Rust: `AnswerProof::is_deterministic`
    -- has no callers and `ProvenanceError::MissingProof` is never constructed, so
    -- REQ-022's "deterministic proof" was unenforced. Arithmetic Reasoning and
    -- Mathematics Knowledge are computable, so a deterministic proof is required
    -- for them rather than merely available. Verbal and factual subtests may
    -- carry a source-backed rubric, which is not deterministic by construction.
    CHECK (
        state <> 'active'
        OR subtest NOT IN ('AR', 'MK')
        OR proof_kind = 'executable'
    )
);

CREATE INDEX IF NOT EXISTS idx_content_items_serving
    ON content_items (subtest, state);

CREATE INDEX IF NOT EXISTS idx_content_items_pack
    ON content_items (pack_id);

-- REQ-048: an item's review history must be answerable, so every item carries an
-- objective and the audit trail is a separate append-only table below.
CREATE INDEX IF NOT EXISTS idx_content_items_objective
    ON content_items (objective_id);

-- ---------------------------------------------------------------------------
-- Item sources. REQ-056 requires every active item to cite its sources; an
-- empty citation list is treated as absent, matching `is_complete`, which
-- rejects `source_ids.is_empty()`.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS content_item_sources (
    item_id   TEXT NOT NULL REFERENCES content_items(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL CHECK (length(trim(source_id)) > 0),
    PRIMARY KEY (item_id, source_id)
);

CREATE INDEX IF NOT EXISTS idx_content_item_sources_source
    ON content_item_sources (source_id);

-- ---------------------------------------------------------------------------
-- Review audit trail (REQ-048: review state, history, rollback, audit).
-- Append-only, like the evidence vault: a review record that can be rewritten
-- after the fact is not an audit trail.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS content_item_reviews (
    id          TEXT PRIMARY KEY,
    item_id     TEXT NOT NULL REFERENCES content_items(id) ON DELETE CASCADE,
    from_state  TEXT NOT NULL,
    to_state    TEXT NOT NULL,
    actor       TEXT NOT NULL CHECK (length(trim(actor)) > 0),
    rationale   TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_content_item_reviews_item
    ON content_item_reviews (item_id, created_at);

CREATE TRIGGER IF NOT EXISTS content_item_reviews_no_update
BEFORE UPDATE ON content_item_reviews
BEGIN
    SELECT RAISE(ABORT, 'review history is append-only');
END;

-- ---------------------------------------------------------------------------
-- Invariants that need more than one row to evaluate, so a CHECK cannot carry
-- them. Triggers are the mechanism, as with the evidence vault.
-- ---------------------------------------------------------------------------

-- Items enter the pipeline at the beginning. QUESTION_FACTORY.md state machine:
-- DRAFT -> MACHINE_VALIDATED -> INDEPENDENT_VERIFIED -> CONTENT_REVIEWED ->
-- ACTIVE. A pack import that ships pre-reviewed items still activates them
-- through the documented transitions, so this costs nothing legitimate.
CREATE TRIGGER IF NOT EXISTS content_items_must_start_draft
BEFORE INSERT ON content_items
WHEN NEW.state <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'items enter the pipeline as draft (QUESTION_FACTORY.md)');
END;

-- Activation requires at least one cited source. Cannot be a CHECK because it
-- reads another table.
CREATE TRIGGER IF NOT EXISTS content_items_active_requires_source
BEFORE UPDATE OF state ON content_items
WHEN NEW.state = 'active'
     AND NOT EXISTS (
         SELECT 1 FROM content_item_sources WHERE item_id = NEW.id
     )
BEGIN
    SELECT RAISE(ABORT, 'an active item must cite at least one source (REQ-056)');
END;

-- Transition legality, mirroring `is_allowed_transition` exactly. A state
-- machine enforced only in Rust is a state machine that a future caller can
-- skip; making an illegal transition abort in the database cannot be bypassed.
CREATE TRIGGER IF NOT EXISTS content_items_transition_guard
BEFORE UPDATE OF state ON content_items
WHEN NEW.state <> OLD.state
     AND NOT (
         (OLD.state = 'draft'                AND NEW.state = 'machine_validated')
      OR (OLD.state = 'machine_validated'    AND NEW.state = 'independent_verified')
      OR (OLD.state = 'independent_verified' AND NEW.state = 'content_reviewed')
      OR (OLD.state = 'content_reviewed'     AND NEW.state = 'active')
      OR (NEW.state = 'quarantined')
      OR (OLD.state = 'quarantined'          AND NEW.state = 'draft')
     )
BEGIN
    SELECT RAISE(ABORT, 'illegal item state transition');
END;
