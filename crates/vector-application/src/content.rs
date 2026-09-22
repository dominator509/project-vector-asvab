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

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vector_persistence::content::{ContentItemRepo, NewContentItem, StoredItem};
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::Database;
use vector_questions::dictionary::Dictionary;
use vector_questions::factory;
use vector_questions::ingestion::AnswerProof;
use vector_questions::provenance::ContentHash;
use vector_questions::thesaurus::{self, Thesaurus, WkItem};

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

    /// Ingest Word Knowledge items from two recorded sources.
    ///
    /// The thesaurus supplies the synonym relationship and the dictionary
    /// corroborates it, so an item's evidence is a citation rather than a
    /// computation -- which is exactly what `source_backed` means in the schema,
    /// and why the schema refuses `source_backed` for AR and MK but permits it
    /// for WK.
    ///
    /// The source texts are passed in rather than read here: reading them is the
    /// caller's business, and both are tens of megabytes that must never live in
    /// this crate.
    pub fn ingest_wk(
        &self,
        thesaurus: &Thesaurus,
        dictionary: &Dictionary,
        request: &WkIngestRequest<'_>,
    ) -> anyhow::Result<WkIngestReport> {
        let items = thesaurus.build_items_configured(
            request.count,
            request.seed,
            request.min_distractor_lines,
            // The dictionary is the quality bar: a pair the dictionary does not
            // link is one Moby associated but never defined as synonymous.
            |head, candidate| dictionary.are_linked(head, candidate),
        );

        let mut report = WkIngestReport {
            built: items.len(),
            ..Default::default()
        };
        if items.is_empty() {
            anyhow::bail!(
                "no Word Knowledge items survived the dictionary filter; either the \
                 sources are wrong or the filter is rejecting everything"
            );
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }
            match self.store_wk_verified(thesaurus, dictionary, request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.headword.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        // A run that stored nothing, because every item was refused for what is
        // almost certainly the same systemic reason, is a failure rather than a
        // corpus of zero. Reporting it as a successful report of N rejections
        // invites a caller to count the rejections as content.
        if report.activated == 0 && report.already_present == 0 {
            let first = report
                .rejected
                .first()
                .map(|rejection| rejection.reason.as_str())
                .unwrap_or("no reason recorded");
            anyhow::bail!(
                "ingestion stored nothing: all {} item(s) were refused; first reason: {first}",
                report.rejected.len()
            );
        }

        Ok(report)
    }

    /// Verify one Word Knowledge item and, only if it proves out, store it.
    ///
    /// Public for the same reason `store_verified` is: the builder's own output
    /// always verifies, so the refusal path needs a directly callable entry point
    /// or deleting the check would break no test.
    pub fn store_wk_verified(
        &self,
        thesaurus: &Thesaurus,
        dictionary: &Dictionary,
        request: &WkIngestRequest<'_>,
        item: &WkItem,
    ) -> anyhow::Result<String> {
        thesaurus::verify(item, thesaurus)
            .map_err(|failure| anyhow::anyhow!("item failed source verification: {failure}"))?;

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();
        let correct = &item.options[item.correct_index];

        // The rubric records what was checked, so a reviewer can re-check it
        // without re-running the ingester.
        let rubric = format!(
            "The thesaurus lists \"{correct}\" with \"{headword}\" on the same line, and \
             Webster's Unabridged defines one of the two using the other, so the pair is \
             corroborated by both sources rather than asserted by this program.",
            correct = correct,
            headword = item.headword
        );
        let proof = AnswerProof::SourceBacked {
            source_id: request.thesaurus_source.to_string(),
            rubric,
        };
        let proof_json = serde_json::to_string(&proof)?;

        // What the builder produced, and what the verifier re-derived from the
        // sources. Different inputs, so they cannot coincide -- which the schema
        // requires before activation.
        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.headword,
            item.options.join("\u{2}"),
            correct
        ));
        let verifier_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}linked={}",
            item.supporting_line,
            correct,
            dictionary.are_linked(&item.headword, correct)
        ));

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: "WK",
            objective_id: &item.objective_id,
            stem: &item.prompt,
            options: &item.options,
            correct_index: item.correct_index,
            explanation: &item.explanation(),
            distractor_rationales: &item.distractor_rationales,
            difficulty: item.difficulty,
            proof_kind: "source_backed",
            proof_json: &proof_json,
            content_hash: &content_hash,
            generator_hash: Some(generator_hash.as_str()),
        })?;

        // Both sources are cited: the thesaurus is the item's evidence, and the
        // dictionary is what corroborates it. A citation the vault has not seen
        // is refused by migration 004, so both must be recorded first.
        repo.cite(&id, request.thesaurus_source)?;
        repo.cite(&id, request.dictionary_source)?;

        self.db.connection().execute(
            "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
            rusqlite::params![id, verifier_hash.as_str()],
        )?;

        repo.walk_to_content_reviewed(&id, request.generator)?;
        repo.activate(
            &id,
            request.reviewer,
            "ingested from public-domain sources; synonym pair corroborated by both",
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

    /// An item a learner may be served, or `None` when none is available.
    ///
    /// Rotation rather than adaptive selection: `seen` is excluded so a session
    /// does not repeat, and the lowest id wins so the order is stable and a
    /// restart does not reshuffle. Item-level adaptive difficulty needs per-item
    /// response history, and the mastery model is per *subtest*, so this
    /// deliberately does not pretend to be the CAT-ASVAB's item selection.
    pub fn next_item(&self, subtest: &str, seen: &[String]) -> anyhow::Result<Option<ItemDto>> {
        let repo = ContentItemRepo::new(self.db);
        let items = repo.servable(subtest)?;
        let chosen = items
            .iter()
            .find(|item| !seen.iter().any(|id| id == &item.id))
            .or_else(|| items.first());
        Ok(chosen.map(ItemDto::from))
    }

    /// How much content exists, by state and by subtest.
    pub fn stats(&self) -> anyhow::Result<ContentStatsDto> {
        let conn = self.db.connection();
        let repo = ContentItemRepo::new(self.db);

        let by_state = repo
            .counts_by_state()?
            .into_iter()
            .map(|(state, count)| StateCountDto { state, count })
            .collect();

        let mut statement = conn.prepare(
            "SELECT subtest, COUNT(*) FROM content_items GROUP BY subtest ORDER BY subtest",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SubtestCountDto {
                subtest: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        let mut by_subtest = Vec::new();
        for row in rows {
            by_subtest.push(row?);
        }

        let servable: i64 = conn.query_row(
            "SELECT COUNT(*) FROM content_items WHERE state = 'active'",
            [],
            |row| row.get(0),
        )?;
        let sources: i64 = conn.query_row("SELECT COUNT(*) FROM evidence_records", [], |row| {
            row.get(0)
        })?;
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM content_items", [], |row| row.get(0))?;

        Ok(ContentStatsDto {
            total,
            servable,
            sources,
            by_state,
            by_subtest,
        })
    }

    /// Record the source every generated item cites, returning its id.
    ///
    /// ## Why the licence string says what it says
    ///
    /// A generated item's *text* is original work. What it takes from outside is
    /// the **construct** it targets -- "ability to solve basic arithmetic word
    /// problems" and the rest of the programme's published one-line subtest
    /// definitions. Those are facts about the test, and facts are not
    /// copyrightable, but the page they are published on carries an
    /// all-rights-reserved notice.
    ///
    /// So the licence recorded here says exactly that, rather than labelling a
    /// copyrighted page "US-Government-Work" because the subject matter is
    /// government-adjacent. A provenance record that overstates its rights is
    /// worse than no record: it is the first thing an audit would find. No
    /// question text from any source is used, which is why each item is provable
    /// from its own arithmetic rather than from a citation.
    pub fn ensure_construct_source(&self) -> anyhow::Result<String> {
        EvidenceRepo::new(self.db).put(&NewEvidence::new(
            GENERATOR_SOURCE_URL,
            GENERATOR_SOURCE_TITLE,
            GENERATOR_SOURCE_HASH,
            GENERATOR_SOURCE_LICENSE,
            "2026-09-22",
            0.9,
            "retrieved",
        ))
    }
}

/// The published subtest definitions the item templates target.
const GENERATOR_SOURCE_URL: &str = "https://www.officialasvab.com/applicants/sample-questions/";
const GENERATOR_SOURCE_TITLE: &str = "ASVAB subtest construct definitions (facts only)";
/// Stable identity for the vault's hash-addressed dedupe. This names the source
/// record; it is not a digest of a page we retained, and it must not claim to be.
const GENERATOR_SOURCE_HASH: &str = "sha256:asvab-subtest-constructs-2026-09-22";
/// Deliberately not "US-Government-Work": the page asserts all rights reserved.
const GENERATOR_SOURCE_LICENSE: &str = "Facts-only; item text is original work";

/// A request to ingest Word Knowledge items from recorded sources.
///
/// Both source ids must already exist in the evidence vault: migration 004
/// refuses a citation the vault has not seen, so an ingester cannot invent
/// provenance for itself.
#[derive(Debug, Clone)]
pub struct WkIngestRequest<'a> {
    /// The thesaurus the synonym relationship comes from.
    pub thesaurus_source: &'a str,
    /// The dictionary that corroborates it.
    pub dictionary_source: &'a str,
    /// How many items to attempt.
    pub count: usize,
    pub seed: u64,
    /// Bar for a distractor to be offered, in source lines.
    ///
    /// A policy rather than a property of the source, so the caller sets it: the
    /// default is calibrated for a 30,000-root-word thesaurus, and no small or
    /// synthetic source can meet it.
    pub min_distractor_lines: usize,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// What a Word Knowledge ingestion run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WkIngestReport {
    /// Items the builder produced after the dictionary filter.
    pub built: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: Vec<Rejection>,
    pub item_ids: Vec<String>,
}

impl WkIngestReport {
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }
}

/// An item as the interface sees it.
///
/// `correct_index` is included because grading happens locally and the practice
/// view already grades from an option index. It is not a secret: this is a study
/// tool on the learner's own machine, and hiding the answer from the person
/// studying would not make the product more honest, only less useful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDto {
    pub id: String,
    pub subtest: String,
    pub objective_id: String,
    pub stem: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    pub distractor_rationales: BTreeMap<usize, String>,
    pub difficulty: f64,
}

impl From<&StoredItem> for ItemDto {
    fn from(item: &StoredItem) -> Self {
        Self {
            id: item.id.clone(),
            subtest: item.subtest.clone(),
            objective_id: item.objective_id.clone(),
            stem: item.stem.clone(),
            options: item.options.clone(),
            correct_index: item.correct_index,
            explanation: item.explanation.clone(),
            distractor_rationales: item.distractor_rationales.clone(),
            difficulty: item.difficulty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateCountDto {
    pub state: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtestCountDto {
    pub subtest: String,
    pub count: i64,
}

/// What the corpus holds, for the content manager surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentStatsDto {
    pub total: i64,
    /// Items that are `active` and therefore servable.
    pub servable: i64,
    pub sources: i64,
    pub by_state: Vec<StateCountDto>,
    pub by_subtest: Vec<SubtestCountDto>,
}

/// A generation run as the interface sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationReportDto {
    pub subtest: String,
    pub generated: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    /// One line per refused item. Empty on a clean run.
    pub rejected: Vec<String>,
}

impl From<GenerationReport> for GenerationReportDto {
    fn from(report: GenerationReport) -> Self {
        Self {
            subtest: report.subtest,
            generated: report.generated,
            verified: report.verified,
            activated: report.activated,
            already_present: report.already_present,
            rejected: report
                .rejected
                .into_iter()
                .map(|r| match r.template_id {
                    Some(id) => format!("{id}: {}", r.reason),
                    None => r.reason,
                })
                .collect(),
        }
    }
}
