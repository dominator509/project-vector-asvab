//! Content item repository over the 003/004 schema.
//!
//! Requirements: REQ-022 (items carry a deterministic proof), REQ-048 (review
//! state, history, rollback, audit), REQ-056 (per-item provenance).
//!
//! The schema is the authority for what an item may be. This layer's job is to
//! drive the documented lifecycle (`QUESTION_FACTORY.md`) and to record the
//! audit trail that the schema deliberately cannot generate on its own: a
//! `CHECK` can refuse an illegal transition, but it cannot write down who made a
//! legal one.
//!
//! It stores `proof_kind` and `proof_json` as opaque text rather than depending
//! on the question factory's types. Persistence implements storage; the pure
//! `vector-questions` layer owns what a proof *is*, and the application layer is
//! the only place that knows both.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};

use crate::db::Database;
use crate::repo::{new_id, now};

/// The lifecycle states, matching the `CHECK` in `003_content_items.sql` and
/// `ItemState` in `vector-questions`.
pub const STATES: [&str; 6] = [
    "draft",
    "machine_validated",
    "independent_verified",
    "content_reviewed",
    "active",
    "quarantined",
];

/// The documented linear pipeline, in order (`QUESTION_FACTORY.md`).
pub const PIPELINE: [&str; 4] = [
    "draft",
    "machine_validated",
    "independent_verified",
    "content_reviewed",
];

/// An item to insert. Items always enter at `draft`; the schema refuses anything
/// else, so this type has no state field to get wrong.
#[derive(Debug, Clone)]
pub struct NewContentItem<'a> {
    pub id: &'a str,
    pub subtest: &'a str,
    pub objective_id: &'a str,
    pub stem: &'a str,
    /// The passage the item is about, for subtests that have one.
    ///
    /// `None` for every subtest but Paragraph Comprehension, and the schema refuses
    /// a PC item without it, so "this item has no passage" and "this item's passage
    /// was forgotten" are not the same state.
    pub passage: Option<&'a str>,
    pub options: &'a [String],
    pub correct_index: usize,
    pub explanation: &'a str,
    pub distractor_rationales: &'a BTreeMap<usize, String>,
    pub difficulty: f64,
    /// `executable` or `source_backed`, matching the schema's `CHECK`.
    pub proof_kind: &'a str,
    /// Serialized proof. Opaque here by design.
    pub proof_json: &'a str,
    pub content_hash: &'a str,
    /// Hash of the generator's work, when the item was machine-drafted.
    pub generator_hash: Option<&'a str>,
}

/// An item as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredItem {
    pub id: String,
    pub subtest: String,
    pub objective_id: String,
    pub state: String,
    pub stem: String,
    pub passage: Option<String>,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    pub distractor_rationales: BTreeMap<usize, String>,
    pub difficulty: f64,
    pub proof_kind: String,
    pub proof_json: String,
    pub reviewer: String,
    pub content_hash: String,
    pub generator_hash: Option<String>,
    pub verifier_hash: Option<String>,
}

/// One entry in an item's audit trail (REQ-048).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewEntry {
    pub from_state: String,
    pub to_state: String,
    pub actor: String,
    pub rationale: String,
    pub created_at: String,
}

pub struct ContentItemRepo<'a> {
    db: &'a Database,
}

impl<'a> ContentItemRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert an item as `draft`.
    ///
    /// The schema refuses any other starting state, so there is no parameter for
    /// one here.
    pub fn insert_draft(&self, item: &NewContentItem<'_>) -> anyhow::Result<()> {
        if item.correct_index >= item.options.len() {
            anyhow::bail!(
                "correct_index {} is outside {} options",
                item.correct_index,
                item.options.len()
            );
        }
        let options_json = serde_json::to_string(item.options)?;
        let distractors_json = serde_json::to_string(item.distractor_rationales)?;
        let timestamp = now();
        self.db.connection().execute(
            "INSERT INTO content_items (
                 id, subtest, objective_id, state, stem, passage, options_json,
                 correct_index, explanation, distractors_json, difficulty, proof_kind,
                 proof_json, reviewer, content_hash, generator_hash, created_at,
                 updated_at
             ) VALUES (
                 ?1, ?2, ?3, 'draft', ?4, ?15, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                 '', ?12, ?13, ?14, ?14
             )",
            params![
                item.id,
                item.subtest,
                item.objective_id,
                item.stem,
                options_json,
                item.correct_index as i64,
                item.explanation,
                distractors_json,
                item.difficulty,
                item.proof_kind,
                item.proof_json,
                item.content_hash,
                item.generator_hash,
                timestamp,
                item.passage,
            ],
        )?;
        Ok(())
    }

    /// Cite a source. The schema refuses a citation the vault has not recorded.
    pub fn cite(&self, item_id: &str, source_id: &str) -> anyhow::Result<()> {
        self.db.connection().execute(
            "INSERT INTO content_item_sources (item_id, source_id) VALUES (?1, ?2)",
            params![item_id, source_id],
        )?;
        Ok(())
    }

    /// Remove a draft row that never became an item.
    ///
    /// Storing an item is a sequence of statements -- insert a draft, cite its source, record
    /// the verifier's hash, walk it to content-reviewed, activate it -- and the row exists
    /// from the first of them. A failure in the middle therefore used to leave a draft behind,
    /// and an ingestion naming a source the vault had not recorded produced six of them: rows
    /// no learner could be served and no reviewer could verify, which the store then counted
    /// as questions it already held.
    ///
    /// The guard is `state = 'draft'` on purpose. An active, quarantined or superseded item is
    /// a record a learner, a reviewer or a pack may depend on, and this method exists only for
    /// the window in which nothing outside the row itself refers to it.
    pub fn discard_draft(&self, item_id: &str) -> anyhow::Result<()> {
        self.db.connection().execute(
            "DELETE FROM content_items WHERE id = ?1 AND state = 'draft'",
            params![item_id],
        )?;
        Ok(())
    }

    /// Move an item to `to`, recording who did it and why.
    ///
    /// Legality is the schema's decision: the `content_items_transition_guard`
    /// trigger aborts an undocumented transition. This method adds the audit row
    /// that a `CHECK` cannot write.
    pub fn advance(
        &self,
        item_id: &str,
        to: &str,
        actor: &str,
        rationale: &str,
    ) -> anyhow::Result<()> {
        if !STATES.contains(&to) {
            anyhow::bail!("{to:?} is not a lifecycle state");
        }
        let conn = self.db.connection();
        let transaction = conn.unchecked_transaction()?;

        let from: String = transaction
            .query_row(
                "SELECT state FROM content_items WHERE id = ?1",
                params![item_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("no such content item: {item_id}"))?;

        // The trigger refuses an illegal transition; a zero-row update means the
        // item does not exist, which is checked above, so this is the real move.
        transaction.execute(
            "UPDATE content_items SET state = ?2, updated_at = ?3 WHERE id = ?1",
            params![item_id, to, now()],
        )?;

        transaction.execute(
            "INSERT INTO content_item_reviews
                 (id, item_id, from_state, to_state, actor, rationale, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id("rev"), item_id, from, to, actor, rationale, now()],
        )?;

        transaction.commit()?;
        Ok(())
    }

    /// Walk an item along the documented pipeline to `content_reviewed`.
    ///
    /// Each step is a separate recorded transition rather than one jump, because
    /// the pipeline's intermediates are the evidence that each check ran.
    pub fn walk_to_content_reviewed(&self, item_id: &str, actor: &str) -> anyhow::Result<()> {
        for state in &PIPELINE[1..] {
            self.advance(item_id, state, actor, "pipeline step")?;
        }
        Ok(())
    }

    /// Name the reviewer, then activate.
    ///
    /// Both in one method because an activation records a decision, and the
    /// schema requires the reviewer to be named for it to be representable at
    /// all.
    pub fn activate(&self, item_id: &str, reviewer: &str, rationale: &str) -> anyhow::Result<()> {
        if reviewer.trim().is_empty() {
            anyhow::bail!("activation requires a named reviewer (REQ-056)");
        }
        self.db.connection().execute(
            "UPDATE content_items SET reviewer = ?2, updated_at = ?3 WHERE id = ?1",
            params![item_id, reviewer, now()],
        )?;
        self.advance(item_id, "active", reviewer, rationale)
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Option<StoredItem>> {
        let conn = self.db.connection();
        let row = conn
            .query_row(
                "SELECT id, subtest, objective_id, state, stem, options_json,
                        correct_index, explanation, distractors_json, difficulty,
                        proof_kind, proof_json, reviewer, content_hash,
                        generator_hash, verifier_hash, passage
                 FROM content_items WHERE id = ?1",
                params![id],
                read_item,
            )
            .optional()?;
        row.transpose()
    }

    /// Items a learner may be served: `active`, with complete provenance, and not
    /// withdrawn by a pack rollback.
    ///
    /// An item delivered by a pack is served only while that pack is the active one
    /// for its name. Rolling a pack back therefore withdraws its items in one
    /// statement, without touching the items themselves -- which is what keeps a
    /// rollback atomic and reversible. Items ingested outside any pack have no pack
    /// and are always eligible.
    ///
    /// The predicate mirrors `ContentItem::is_servable`. The schema already
    /// guarantees active implies complete, so this is a belt-and-braces read
    /// rather than the enforcement point.
    pub fn servable(&self, subtest: &str) -> anyhow::Result<Vec<StoredItem>> {
        self.servable_in(subtest, None)
    }

    /// The same read, narrowed to one objective when one is named.
    ///
    /// A study plan names the objective a session is for, so practice that ignored
    /// the objective would deliver items from a different skill than the plan just
    /// told the learner to work on. `None` keeps the earlier subtest-wide read, so a
    /// caller with no objective in hand behaves exactly as before.
    ///
    /// `?2 IS NULL OR objective_id = ?2` rather than two statements: the provenance
    /// predicate below is the part that must never diverge between the narrow and
    /// the wide read, and one SQL string cannot drift from itself.
    pub fn servable_in(
        &self,
        subtest: &str,
        objective_id: Option<&str>,
    ) -> anyhow::Result<Vec<StoredItem>> {
        self.query(
            "SELECT id, subtest, objective_id, state, stem, options_json,
                    correct_index, explanation, distractors_json, difficulty,
                    proof_kind, proof_json, reviewer, content_hash,
                    generator_hash, verifier_hash, passage
             FROM content_items
             WHERE state = 'active' AND subtest = ?1
               AND (?2 IS NULL OR objective_id = ?2)
               AND (
                     NOT EXISTS (
                         SELECT 1 FROM content_pack_items m
                         WHERE m.item_id = content_items.id
                     )
                     OR EXISTS (
                         SELECT 1 FROM content_pack_items m
                         JOIN content_packs p ON p.id = m.pack_id
                         WHERE m.item_id = content_items.id AND p.status = 'active'
                     )
                   )
             ORDER BY id",
            params![subtest, objective_id],
        )
    }

    pub fn all(&self) -> anyhow::Result<Vec<StoredItem>> {
        self.query(
            "SELECT id, subtest, objective_id, state, stem, options_json,
                    correct_index, explanation, distractors_json, difficulty,
                    proof_kind, proof_json, reviewer, content_hash,
                    generator_hash, verifier_hash, passage
             FROM content_items ORDER BY subtest, id",
            [],
        )
    }

    /// Whether this subtest already asks this question, whoever asked it first.
    ///
    /// The content hash covers the prompt *and* the sentence it was built from, so it catches
    /// a re-ingestion of the same source text and nothing else. Two things get past it, and
    /// both reached the corpus:
    ///
    /// * two editions of one manual state the same thing in different words -- TM 9-8000 and
    ///   TM 9-2700 are two editions of *Principles of Automotive Vehicles*, and one says `The
    ///   ammeter is used to indicate ...` where the other says `An ammeter is used to ...`;
    /// * a builder draws different wrong answers for the same question, which is how the
    ///   Electronics Information bank came to ask one glossary definition up to seven times
    ///   with the same four options in a different order.
    ///
    /// So the question is identified by what a learner answers: the stem, the correct answer,
    /// and the passage if there is one. The correct answer is part of the key on purpose --
    /// `Choose the word that most nearly means the same as irenic` has more than one true
    /// answer, and each of them is a question worth asking. The passage is part of it for the
    /// same reason: a Paragraph Comprehension stem can be generic ("According to the
    /// passage, which of the following is stated?") and only the passage makes it a question.
    ///
    /// The comparison ignores case and surrounding space, because `What are the Clamps used
    /// for?` and `What are the clamps used for?` are one question.
    pub fn question_exists(
        &self,
        subtest: &str,
        stem: &str,
        correct: &str,
        passage: Option<&str>,
    ) -> anyhow::Result<bool> {
        let conn = self.db.connection();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM content_items
             WHERE subtest = ?1 AND state IN ('active', 'draft')
               AND LOWER(TRIM(stem)) = LOWER(TRIM(?2))
               AND LOWER(TRIM(COALESCE(
                     json_extract(options_json, '$[' || correct_index || ']'), '')))
                   = LOWER(TRIM(?3))
               AND LOWER(TRIM(COALESCE(passage, ''))) = LOWER(TRIM(COALESCE(?4, '')))",
            params![subtest, stem, correct, passage],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Item counts per lifecycle state, for the content manager surface.
    pub fn counts_by_state(&self) -> anyhow::Result<Vec<(String, i64)>> {
        let conn = self.db.connection();
        let mut statement = conn
            .prepare("SELECT state, COUNT(*) FROM content_items GROUP BY state ORDER BY state")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// An item's audit trail in order (REQ-048).
    pub fn history(&self, item_id: &str) -> anyhow::Result<Vec<ReviewEntry>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(
            "SELECT from_state, to_state, actor, rationale, created_at
             FROM content_item_reviews WHERE item_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = statement.query_map(params![item_id], |row| {
            Ok(ReviewEntry {
                from_state: row.get(0)?,
                to_state: row.get(1)?,
                actor: row.get(2)?,
                rationale: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Sources an item cites.
    pub fn sources(&self, item_id: &str) -> anyhow::Result<Vec<String>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(
            "SELECT source_id FROM content_item_sources WHERE item_id = ?1 ORDER BY source_id",
        )?;
        let rows = statement.query_map(params![item_id], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> anyhow::Result<Vec<StoredItem>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(sql)?;
        let rows = statement.query_map(params, read_item)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row??);
        }
        Ok(out)
    }
}

/// Read one item row. Returns a nested result because the JSON columns can fail
/// to parse independently of the row itself.
fn read_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<StoredItem, anyhow::Error>> {
    let options_json: String = row.get(5)?;
    let distractors_json: String = row.get(8)?;
    let correct_index: i64 = row.get(6)?;

    let parsed = (|| -> anyhow::Result<StoredItem> {
        Ok(StoredItem {
            id: row.get(0)?,
            subtest: row.get(1)?,
            objective_id: row.get(2)?,
            state: row.get(3)?,
            stem: row.get(4)?,
            options: serde_json::from_str(&options_json)?,
            correct_index: usize::try_from(correct_index)?,
            explanation: row.get(7)?,
            distractor_rationales: serde_json::from_str(&distractors_json)?,
            difficulty: row.get(9)?,
            proof_kind: row.get(10)?,
            proof_json: row.get(11)?,
            reviewer: row.get(12)?,
            content_hash: row.get(13)?,
            generator_hash: row.get(14)?,
            verifier_hash: row.get(15)?,
            passage: row.get(16)?,
        })
    })();

    Ok(parsed)
}
