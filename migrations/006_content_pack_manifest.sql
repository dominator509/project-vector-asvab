-- 006_content_pack_manifest.sql
-- Packs carry the identity and manifest a reviewer needs to judge them.
-- Requirements: REQ-023 (signed, versioned packs), REQ-032 (rollback surface),
-- REQ-048, REQ-056.
--
-- `content_packs` was created in 002 with `id, name, version, signature, status,
-- created_at`, and nothing ever wrote to it: the rollback acceptance test published
-- rows directly and the production path had no pack at all. A registry that records
-- a bare signature string cannot answer the questions installation has to ask --
-- which key signed it, what content the signature covers, what schema the pack was
-- written for, and what is in it -- so those fields are added here.
--
-- Existing rows keep working: every new column has a default that means "this row
-- predates the manifest". A pack without a manifest and a content hash is refused by
-- the installer, which is the honest reading of a registry row that carries neither.
--
-- Monotonic: never edits 001 through 005.

ALTER TABLE content_packs ADD COLUMN signer TEXT NOT NULL DEFAULT '';

-- The digest the signature covers. `signature` alone says nothing about what was
-- signed.
ALTER TABLE content_packs ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';

-- The pack schema version, so an installer can refuse a pack written for a format
-- it does not implement rather than guessing at the fields.
ALTER TABLE content_packs ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;

-- The manifest as received, stored verbatim: a reviewer has to be able to read what
-- a pack claimed at the moment it was installed, not what the current code would
-- have written.
ALTER TABLE content_packs ADD COLUMN manifest_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(manifest_json));

ALTER TABLE content_packs ADD COLUMN item_count INTEGER NOT NULL DEFAULT 0
    CHECK (item_count >= 0);

-- An active pack must carry the identity installation verified. A registry row that
-- is serving learners while recording no signer, no content hash and no manifest is
-- exactly the state this column set exists to make unrepresentable.
--
-- Scoped to rows inserted after this migration: the guard fires on the transition
-- into `active`, and a row whose defaults have not been replaced cannot get there.
CREATE TRIGGER IF NOT EXISTS content_packs_active_requires_identity
BEFORE UPDATE OF status ON content_packs
WHEN NEW.status = 'active'
    AND (
        length(trim(NEW.signer)) = 0
        OR length(trim(NEW.content_hash)) = 0
        OR NEW.schema_version < 1
        OR NEW.item_count < 1
    )
BEGIN
    SELECT RAISE(
        ABORT,
        'an active pack must record its signer, content hash, schema version and item count'
    );
END;

-- The same rule on the way in, for a pack inserted directly as active.
CREATE TRIGGER IF NOT EXISTS content_packs_active_requires_identity_insert
BEFORE INSERT ON content_packs
WHEN NEW.status = 'active'
    AND (
        length(trim(NEW.signer)) = 0
        OR length(trim(NEW.content_hash)) = 0
        OR NEW.schema_version < 1
        OR NEW.item_count < 1
    )
BEGIN
    SELECT RAISE(
        ABORT,
        'an active pack must record its signer, content hash, schema version and item count'
    );
END;
