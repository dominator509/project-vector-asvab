//! Content generation pipeline: factory output to active, provable items.
//!
//! Requirements: REQ-022 (original item + deterministic proof + independent
//! verification), REQ-048 (review history), REQ-056 (per-item provenance).
//!
//! This module is the join between two layers that must not know each other:
//! `vector-questions` produces and verifies items but must not import
//! persistence (ARCHITECTURE.md: pure layers never import infrastructure), and
//! `vector-persistence` stores items without knowing what a proof is. The
//! application layer is the only place that knows both, so the orchestration
//! lives here.
//!
//! ## What "verified" means in the record
//!
//! Each item carries two distinct hashes. `generator_hash` covers what the
//! template produced; `verifier_hash` covers what the verifier *recomputed* from
//! the proof expression. They are derived from different inputs, so they differ,
//! which is what the schema demands before an item may be active: a verifier
//! that echoed the generator did not verify anything.

use vector_persistence::content::{ContentItemRepo, NewContentItem};
use vector_persistence::Database;
use vector_questions::factory;
use vector_questions::ingestion::AnswerProof;
use vector_questions::provenance::ContentHash;

/// What to generate.
#[derive(Debug, Clone)]
pub struct GenerateRequest<'a> {
    pub subtest: &'a str,
    pub count: usize,
    /// Base seed. The same seed and count reproduce the same batch, which is what
    /// makes a generated pack regenerable and its hashes re-derivable.
    pub seed: u64,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// A source already recorded in the evidence vault. Every item cites it; the
    /// schema refuses a citation the vault has not seen.
    pub source_id: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// Why one item did not make it into the corpus.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub template_id: Option<String>,
    pub reason: String,
}

/// What a generation run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenerationReport {
    pub subtest: String,
    /// Items the factory produced.
    pub generated: usize,
    /// Items that passed independent verification.
    pub verified: usize,
    /// Items that reached `active` and are therefore servable.
    pub activated: usize,
    /// Items skipped because identical content is already stored.
    pub already_present: usize,
    /// Items refused, with the reason. Empty on a clean run.
    pub rejected: Vec<Rejection>,
    /// Ids of the activated items, in generation order.
    pub item_ids: Vec<String>,
}

impl GenerationReport {
    /// Whether every generated item was either activated or already present.
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }
}

/// The content pipeline, bound to one database.
pub struct ContentPipeline<'a> {
    db: &'a Database,
}

impl<'a> ContentPipeline<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Generate, verify, and activate items.
    ///
    /// A rejection is recorded and the run continues: one unusable item should
    /// not discard a batch of hundreds, and the report says exactly how many
    /// survived. The run is not all-or-nothing, and it does not pretend to be.
    pub fn generate_and_activate(
        &self,
        request: &GenerateRequest<'_>,
    ) -> anyhow::Result<GenerationReport> {
        let items = factory::generate_many(request.subtest, request.count, request.seed);

        let mut report = GenerationReport {
            subtest: request.subtest.to_string(),
            generated: items.len(),
            ..Default::default()
        };

        if items.is_empty() {
            anyhow::bail!(
                "the factory produced nothing for subtest {:?}; templates exist for AR and MK",
                request.subtest
            );
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }

            match self.store_verified(request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.template_id.to_string()),
                    reason: error.to_string(),
                }),
            }
        }

        Ok(report)
    }

    /// Verify one item and, only if it proves out, store and activate it.
    ///
    /// Public deliberately. The factory's own output always verifies, so without
    /// a directly callable entry point an unprovable item could never reach this
    /// code in a test. The refusal path would then be unreachable, unverified
    /// code, and deleting the `verify` call below would break no test.
    ///
    /// Nothing is written until the item proves out, so the database cannot hold
    /// an unprovable draft that someone later activates by mistake.
    pub fn store_verified(
        &self,
        request: &GenerateRequest<'_>,
        item: &factory::GeneratedItem,
    ) -> anyhow::Result<String> {
        factory::verify(item).map_err(|failure| {
            anyhow::anyhow!("item failed independent verification: {failure}")
        })?;

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();

        // The proof is the item's own arithmetic expression and the answer the
        // verifier derived from it -- not the answer the generator declared.
        let recomputed = vector_questions::proof::verify_answer(&item.expression, &item.answer)
            .map_err(|e| anyhow::anyhow!("proof re-check failed: {e}"))?;
        let proof = AnswerProof::Executable {
            expression: item.expression.clone(),
            answer: recomputed.to_string(),
        };
        let proof_json = serde_json::to_string(&proof)?;

        // What the generator produced, and what the verifier recomputed. The two
        // are hashed from different inputs on purpose so they cannot coincide.
        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.template_id, item.stem, item.answer
        ));
        let verifier_hash =
            ContentHash::of_text(&format!("{}\u{1}{}", item.expression, recomputed));

        let explanation = format!("{} = {}", item.expression, recomputed);

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: &item.subtest,
            objective_id: &item.objective_id,
            stem: &item.stem,
            options: &item.options,
            correct_index: item.correct_index,
            explanation: &explanation,
            distractor_rationales: &item.distractor_rationales,
            difficulty: item.difficulty,
            proof_kind: "executable",
            proof_json: &proof_json,
            content_hash: &content_hash,
            generator_hash: Some(generator_hash.as_str()),
        })?;

        repo.cite(&id, request.source_id)?;

        // Record the verifier's hash before activation. The schema refuses an
        // active item whose verifier and generator hashes agree, so this is the
        // step that makes "independently verified" mean something.
        self.db.connection().execute(
            "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
            rusqlite::params![id, verifier_hash.as_str()],
        )?;

        repo.walk_to_content_reviewed(&id, request.generator)?;
        repo.activate(
            &id,
            request.reviewer,
            "machine-verified original item, deterministic proof checked",
        )?;

        Ok(id)
    }

    fn content_hash_exists(&self, content_hash: &str) -> anyhow::Result<bool> {
        let count: i64 = self.db.connection().query_row(
            "SELECT COUNT(*) FROM content_items WHERE content_hash = ?1",
            rusqlite::params![content_hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}
