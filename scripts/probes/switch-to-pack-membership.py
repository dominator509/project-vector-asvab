"""Switch the pack installer from re-parenting items to recording membership.

Written as a script because the two edits are inside a long closure and a mis-cut
would either drop the membership write or leave the re-parenting in place, and the
difference only shows up when two real packs are installed one over the other.
"""

PATH = "crates/vector-persistence/src/repo.rs"

REPARENT = """                if let Some(existing_id) = existing {
                    already_present += 1;
                    if is_new_version {
                        conn.execute(
                            "UPDATE content_items SET pack_id = ?2, updated_at = ?3
                             WHERE id = ?1",
                            params![existing_id, pack_id, now()],
                        )?;
                    }
                    continue;
                }"""

REPLACEMENT = """                if existing.is_some() {
                    already_present += 1;
                    continue;
                }"""

SOURCE_LOOP = "                for source_id in &item.source_ids {"

MEMBERSHIP = """                // Membership, not ownership, is what serving follows: this pack
                // contains this item, whether the row was written now or was already
                // here from an earlier version.
                conn.execute(
                    "INSERT OR IGNORE INTO content_pack_items (pack_id, item_id)
                     VALUES (?1, ?2)",
                    params![pack_id, item.id],
                )?;

                for source_id in &item.source_ids {"""


def main() -> int:
    with open(PATH, encoding="utf-8") as handle:
        text = handle.read()

    if REPARENT not in text:
        print("the re-parenting block is not in the file; nothing to change")
        return 1
    text = text.replace(REPARENT, REPLACEMENT)

    if SOURCE_LOOP not in text:
        print("cannot find the citation loop")
        return 1
    text = text.replace(SOURCE_LOOP, MEMBERSHIP, 1)

    # `is_new_version` existed only for the re-parenting decision.
    text = text.replace(
        "            let is_new_version = already_registered.is_none();\n", ""
    )

    with open(PATH, "w", encoding="utf-8") as handle:
        handle.write(text)
    print("the installer records membership instead of re-parenting")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
