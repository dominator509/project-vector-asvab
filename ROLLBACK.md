# Rollback

Binary and data rollback are separate. Before risky migration, create and verify pre-upgrade backup. If old binary cannot safely read new schema, restore the backup rather than pretending downgrade is safe.

Content/source/provider-policy packs are immutable versions activated by an atomic pointer, making rollback auditable and reversible.
