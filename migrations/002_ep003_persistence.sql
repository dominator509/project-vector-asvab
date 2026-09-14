-- 002_ep003_persistence.sql
-- EP-003: Data and persistence.
-- Requirements: REQ-010, REQ-020, REQ-032, REQ-033, REQ-054.
--
-- Adds the durable operational tables that the learning, evidence, repair and
-- content-release surfaces persist through. Monotonic: never edits 001.

-- ---------------------------------------------------------------------------
-- Attempts: append-only practice/exam attempts (REQ-010, REQ-054).
-- `id` is client-supplied so a retried submission is idempotent.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS attempts (
    id          TEXT PRIMARY KEY,
    learner_id  TEXT NOT NULL REFERENCES learner_profile(id) ON DELETE CASCADE,
    subtest     TEXT NOT NULL,
    question_id TEXT NOT NULL,
    correct     INTEGER NOT NULL CHECK (correct IN (0, 1)),
    latency_ms  INTEGER NOT NULL CHECK (latency_ms >= 0),
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attempts_learner_subtest
    ON attempts (learner_id, subtest);

-- ---------------------------------------------------------------------------
-- Mastery: per-learner, per-subtest estimate with explicit uncertainty
-- (REQ-010). One row per (learner, subtest).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS mastery (
    learner_id  TEXT NOT NULL REFERENCES learner_profile(id) ON DELETE CASCADE,
    subtest     TEXT NOT NULL,
    score       REAL NOT NULL CHECK (score >= 0.0 AND score <= 1.0),
    uncertainty REAL NOT NULL CHECK (uncertainty >= 0.0),
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (learner_id, subtest)
);

-- ---------------------------------------------------------------------------
-- Evidence vault: hash-addressed, immutable source snapshots (REQ-020).
-- content_hash is UNIQUE so identical content deduplicates to one identity.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS evidence_records (
    id               TEXT PRIMARY KEY,
    content_hash     TEXT NOT NULL UNIQUE,
    url              TEXT NOT NULL,
    title            TEXT NOT NULL,
    license          TEXT NOT NULL,
    effective_date   TEXT NOT NULL,
    trust            REAL NOT NULL CHECK (trust >= 0.0 AND trust <= 1.0),
    retrieval_status TEXT NOT NULL,
    created_at       TEXT NOT NULL
);

-- Immutability guard: vault rows are append-only. Any UPDATE is refused so a
-- malfunctioning caller cannot rewrite provenance after the fact (REQ-020).
CREATE TRIGGER IF NOT EXISTS evidence_records_no_update
BEFORE UPDATE ON evidence_records
BEGIN
    SELECT RAISE(ABORT, 'evidence vault records are immutable');
END;

-- ---------------------------------------------------------------------------
-- Pull requests produced by the repair lane (REQ-032). Merge requires an
-- explicit approver; the CHECK makes auto-merge unrepresentable.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS pull_requests (
    id          TEXT PRIMARY KEY,
    repo        TEXT NOT NULL,
    title       TEXT NOT NULL,
    diff        TEXT NOT NULL,
    approved    INTEGER NOT NULL DEFAULT 0 CHECK (approved IN (0, 1)),
    approved_by TEXT,
    merged      INTEGER NOT NULL DEFAULT 0 CHECK (merged IN (0, 1)),
    created_at  TEXT NOT NULL,
    -- Claiming approval without naming an approver is not a representable state,
    -- and a merged row must carry explicit approval (REQ-032).
    CHECK (approved = 0 OR approved_by IS NOT NULL),
    CHECK (merged = 0 OR (approved = 1 AND approved_by IS NOT NULL))
);

-- ---------------------------------------------------------------------------
-- Content packs: signed, versioned study content with rollback (REQ-032 pack
-- rollback surface used by SPEC-002). Only one active version per pack name.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS content_packs (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    version    INTEGER NOT NULL CHECK (version >= 1),
    signature  TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('active', 'superseded', 'quarantined')),
    created_at TEXT NOT NULL,
    UNIQUE (name, version)
);

CREATE INDEX IF NOT EXISTS idx_content_packs_active
    ON content_packs (name, status);
