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
use vector_questions::dictionary::{looks_like_misreading, Dictionary};
use vector_questions::factory;
use vector_questions::facts::{self, FactItem, Faq};
use vector_questions::ingestion::AnswerProof;
use vector_questions::neets::{self, EiItem, Glossary};
use vector_questions::passages::{self, PcItem, Text};
use vector_questions::provenance::ContentHash;
use vector_questions::purposes::{self, PurposeItem, Purposes};
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

/// The first item the learner has not seen, or the first item when all are seen.
///
/// Ordered by id in SQL, so the pick is stable across restarts rather than
/// reshuffled by whatever the database returns first. Repeating the first item
/// when the pool is exhausted is deliberate: a finite corpus must not end a
/// session in progress, and the caller's `seen` list says what it has shown.
fn pick<'i>(items: &'i [StoredItem], seen: &[String]) -> Option<&'i StoredItem> {
    items
        .iter()
        .find(|item| !seen.iter().any(|id| id == &item.id))
        .or_else(|| items.first())
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
            // The message lists what the factory can actually serve rather than a
            // hard-coded pair: it still said "AR and MK" after MC was added, which
            // sends a caller looking for a template that is right there.
            let generatable: Vec<String> = ["AR", "MK", "MC"]
                .iter()
                .filter(|subtest| !factory::templates_for(subtest).is_empty())
                .map(|subtest| subtest.to_string())
                .collect();
            anyhow::bail!(
                "the factory produced nothing for subtest {:?}; templates exist for {}",
                request.subtest,
                generatable.join(", ")
            );
        }

        for item in items {
            let content_hash = item.content_hash();
            // Two identities, and both are needed. The content hash catches the same item drawn
            // twice. The question identity -- stem, correct answer, passage -- catches the same
            // question asked again with its wrong answers shuffled, which is the padding the
            // ingestion loops already refuse: one Electronics Information module asked its
            // definitions up to seven times each, and 3,668 items carried 414 distinct questions
            // between them. The factory draws its parameters at random, so it repeats questions
            // the same way, and a learner served both copies is asked one question twice.
            if self.content_hash_exists(&content_hash)?
                || self.question_is_stored(&item.subtest, &item.stem, &item.answer, None)?
            {
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
            passage: None,
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

        let outcome: anyhow::Result<()> = (|| {
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

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
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
            if self.question_is_stored(
                "WK",
                &item.prompt,
                &item.options[item.correct_index],
                None,
            )? {
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
            passage: None,
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
        let outcome: anyhow::Result<()> = (|| {
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

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
    }

    /// Ingest Electronics Information items from one NEETS module glossary.
    ///
    /// One module per call, because an item's evidence is a definition in a
    /// specific module and the citation has to name it. The caller loops modules,
    /// which also means a module that parses badly cannot take the rest down with
    /// it.
    ///
    /// `dictionary` is used only for the OCR check; it is not the source of these
    /// items and is not cited on them.
    pub fn ingest_ei(
        &self,
        glossary: &Glossary,
        dictionary: &Dictionary,
        request: &EiIngestRequest<'_>,
    ) -> anyhow::Result<EiIngestReport> {
        let items = glossary.build_items(
            request.count,
            request.seed,
            request.min_definition_words,
            |term, definition| entry_is_legible(term, definition, dictionary, glossary),
        );

        let mut report = EiIngestReport {
            module: request.module.to_string(),
            entries: glossary.entry_count(),
            built: items.len(),
            ..Default::default()
        };
        if items.is_empty() {
            anyhow::bail!(
                "module {:?} produced no Electronics Information items from {} entries; \
                 either the glossary shape changed or every definition was rejected",
                request.module,
                glossary.entry_count()
            );
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }
            if self.question_is_stored(
                "EI",
                &item.prompt,
                &item.options[item.correct_index],
                None,
            )? {
                report.already_present += 1;
                continue;
            }
            match self.store_ei_verified(glossary, request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.term.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        // Same reasoning as the Word Knowledge path: a run that stored nothing
        // because every item hit the same systemic problem is a failure, not a
        // corpus of zero.
        if report.activated == 0 && report.already_present == 0 {
            let first = report
                .rejected
                .first()
                .map(|rejection| rejection.reason.as_str())
                .unwrap_or("no reason recorded");
            anyhow::bail!(
                "module {:?} stored nothing: all {} item(s) were refused; first reason: {first}",
                request.module,
                report.rejected.len()
            );
        }

        Ok(report)
    }

    /// Verify one Electronics Information item and, only if it proves out, store it.
    ///
    /// Public for the same reason the other two are: the builder's own output
    /// always verifies, so without a directly callable entry point the refusal
    /// path would be unreachable code and deleting the check would break no test.
    pub fn store_ei_verified(
        &self,
        glossary: &Glossary,
        request: &EiIngestRequest<'_>,
        item: &EiItem,
    ) -> anyhow::Result<String> {
        neets::verify(item, glossary)
            .map_err(|failure| anyhow::anyhow!("item failed source verification: {failure}"))?;

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();

        // The rubric records what was checked, so a reviewer can re-check it
        // without re-running the ingester.
        let rubric = format!(
            "NEETS, {}, defines \"{}\" as the definition quoted in the prompt, and every \
             option is a term the same module defines.",
            item.module, item.term
        );
        let proof = AnswerProof::SourceBacked {
            source_id: request.source_id.to_string(),
            rubric,
        };
        let proof_json = serde_json::to_string(&proof)?;

        // What the builder produced, and what the verifier re-derived from the
        // source. Different inputs, so they cannot coincide, which the schema
        // requires before activation.
        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.module,
            item.options.join("\u{2}"),
            item.term
        ));
        let verifier_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}defined={}",
            item.supporting_definition,
            item.term,
            glossary.defines(&item.term)
        ));

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: "EI",
            objective_id: &item.objective_id,
            stem: &item.prompt,
            passage: None,
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

        let outcome: anyhow::Result<()> = (|| {
            repo.cite(&id, request.source_id)?;

            self.db.connection().execute(
                "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
                rusqlite::params![id, verifier_hash.as_str()],
            )?;

            repo.walk_to_content_reviewed(&id, request.generator)?;
            repo.activate(
                &id,
                request.reviewer,
                "ingested from a public-domain Navy training module; definition quoted verbatim",
            )?;

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
    }

    /// Ingest Paragraph Comprehension items from one public-domain text.
    ///
    /// One text per call for the same reason as EI: an item's evidence is a passage
    /// in a named work, and the citation has to name that work, so the caller loops
    /// the corpus and each work is recorded in the vault separately.
    ///
    /// The passage filter is `passages::is_usable_passage`, which is the module's
    /// own definition of prose worth asking about. Passages outside the length band
    /// are refused by `verify` in any case; filtering here keeps the builder from
    /// spending its attempt budget on them.
    pub fn ingest_pc(
        &self,
        text: &Text,
        request: &PcIngestRequest<'_>,
    ) -> anyhow::Result<PcIngestReport> {
        let items = text.build_items(request.count, request.seed, passages::is_usable_passage);

        let mut report = PcIngestReport {
            label: request.label.to_string(),
            paragraphs: text.paragraph_count(),
            built: items.len(),
            ..Default::default()
        };
        // A file with no paragraphs at all is broken -- the markers moved, or the
        // wrong file was passed -- and must not be reported as a corpus of zero.
        if text.is_empty() {
            anyhow::bail!(
                "text {:?} parsed to no paragraphs; the file's markers may have changed",
                request.label
            );
        }
        // A file that parses but yields nothing is a different fact: its prose does
        // not fit the bands, which is a property of the work rather than a fault. It
        // is reported as skipped so a corpus-wide run can continue and say which
        // works contributed nothing, instead of failing on the first unsuitable
        // book and hiding the rest.
        if items.is_empty() {
            report.skipped = Some(format!(
                "no paragraph satisfies the {passage_min}-{passage_max} word passage band \
                 with {clause_min}-{clause_max} word options",
                passage_min = passages::MIN_PASSAGE_WORDS,
                passage_max = passages::MAX_PASSAGE_WORDS,
                clause_min = passages::MIN_CLAUSE_WORDS,
                clause_max = passages::MAX_CLAUSE_WORDS,
            ));
            return Ok(report);
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }
            if self.question_is_stored(
                "PC",
                &item.prompt,
                &item.options[item.correct_index],
                Some(&item.passage),
            )? {
                report.already_present += 1;
                continue;
            }
            match self.store_pc_verified(text, request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.supporting_clause.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        // As with the other two paths: storing nothing because every item hit the
        // same systemic problem is a failure, not a corpus of zero.
        if report.activated == 0 && report.already_present == 0 {
            let first = report
                .rejected
                .first()
                .map(|rejection| rejection.reason.as_str())
                .unwrap_or("no reason recorded");
            anyhow::bail!(
                "text {:?} stored nothing: all {} item(s) were refused; first reason: {first}",
                request.label,
                report.rejected.len()
            );
        }

        Ok(report)
    }

    /// Verify one Paragraph Comprehension item and, only if it proves out, store it.
    ///
    /// Two checks, because they catch different forgeries. `verify` re-derives the
    /// facts from the item's own passage: the correct option is stated there and no
    /// distractor is. That catches a builder that contradicts itself. It cannot
    /// catch a builder that invents a passage wholesale, because the passage travels
    /// inside the item, so the passage is separately required to occur in the source
    /// text. Public for the same reason the other three are: the builder's own output
    /// always verifies, so the refusal path needs a directly callable entry point or
    /// deleting a check would break no test.
    pub fn store_pc_verified(
        &self,
        text: &Text,
        request: &PcIngestRequest<'_>,
        item: &PcItem,
    ) -> anyhow::Result<String> {
        passages::verify(item)
            .map_err(|failure| anyhow::anyhow!("item failed source verification: {failure}"))?;

        if !text.contains_passage(&item.passage) {
            anyhow::bail!(
                "the item's passage does not occur in {:?}, so the item is not source-backed",
                request.label
            );
        }

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();

        // The rubric records what was checked, so a reviewer can re-check it
        // without re-running the ingester.
        let rubric = format!(
            "{} contains the passage, and the passage states the correct option in its own \
             words. Every other option differs from it by one word, and that word does not \
             occur in the passage, so the passage rules the option out.",
            request.label
        );
        let proof = AnswerProof::SourceBacked {
            source_id: request.source_id.to_string(),
            rubric,
        };
        let proof_json = serde_json::to_string(&proof)?;

        // What the builder produced, and what the verifier re-derived. Different
        // inputs, so they cannot coincide, which the schema requires before
        // activation.
        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.supporting_clause,
            item.options.join("\u{2}"),
            item.correct_index
        ));
        let verifier_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}in_source={}\u{1}options={}",
            item.passage,
            item.supporting_clause,
            text.contains_passage(&item.passage),
            item.options.len()
        ));

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: "PC",
            objective_id: &item.objective_id,
            stem: &item.prompt,
            passage: Some(&item.passage),
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

        let outcome: anyhow::Result<()> = (|| {
            repo.cite(&id, request.source_id)?;

            self.db.connection().execute(
                "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
                rusqlite::params![id, verifier_hash.as_str()],
            )?;

            repo.walk_to_content_reviewed(&id, request.generator)?;
            repo.activate(
                &id,
                request.reviewer,
                "ingested from public-domain prose; passage occurs verbatim in the source",
            )?;

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
    }

    /// Ingest factual (General Science and shop-knowledge) items from one
    /// question-and-answer text.
    ///
    /// One work per call for the same reason as PC: an item's evidence is an answer
    /// in a named book, and the citation has to name that book.
    ///
    /// The OCR check is the caller's, passed as `accept`, because the corpus mixes
    /// modern reprints with scanned manuals and only the caller knows which source
    /// it is handing over.
    pub fn ingest_facts(
        &self,
        faq: &Faq,
        request: &FactIngestRequest<'_>,
    ) -> anyhow::Result<FactIngestReport> {
        let subtest = request.subtest;
        let items = faq.build_items(
            subtest,
            request.count,
            request.seed,
            facts::MIN_OPTION_WORDS,
            facts::MAX_OPTION_WORDS,
            // No dictionary check here. The one this corpus has was measured against this
            // very book and refused 226 of 301 items, every refusal an ordinary word; see
            // `option_words_are_known`. The work is a proofread reprint, not a scan, so
            // there is no scanner damage for a check to find.
            |_| true,
        );

        let mut report = FactIngestReport {
            label: request.label.to_string(),
            questions: faq.entry_count(),
            built: items.len(),
            ..Default::default()
        };
        if faq.is_empty() {
            anyhow::bail!(
                "text {:?} parsed to no questions at all; the file's markers or its \
                 question style may have changed",
                request.label
            );
        }
        if items.is_empty() {
            report.skipped = Some(format!(
                "no question has an answer of {}-{} words that the dictionary reads as \
                 clean prose",
                facts::MIN_OPTION_WORDS,
                facts::MAX_OPTION_WORDS
            ));
            return Ok(report);
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }
            if self.question_is_stored(
                request.subtest,
                &item.prompt,
                &item.options[item.correct_index],
                None,
            )? {
                report.already_present += 1;
                continue;
            }
            match self.store_fact_verified(faq, request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.prompt.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        if report.activated == 0 && report.already_present == 0 {
            let first = report
                .rejected
                .first()
                .map(|rejection| rejection.reason.as_str())
                .unwrap_or("no reason recorded");
            anyhow::bail!(
                "text {:?} stored nothing: all {} item(s) were refused; first reason: {first}",
                request.label,
                report.rejected.len()
            );
        }

        Ok(report)
    }

    /// Verify one factual item against its source and, only if it proves out, store it.
    ///
    /// Public for the same reason the other paths' store functions are: the builder's
    /// own output always verifies, so the refusal path needs a directly callable
    /// entry point or deleting the check would break no test.
    pub fn store_fact_verified(
        &self,
        faq: &Faq,
        request: &FactIngestRequest<'_>,
        item: &FactItem,
    ) -> anyhow::Result<String> {
        facts::verify(item, faq)
            .map_err(|failure| anyhow::anyhow!("item failed source verification: {failure}"))?;

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();

        // The rubric records what was checked, so a reviewer can re-check it without
        // re-running the ingester.
        let rubric = format!(
            "{} asks this question and gives this answer; every other option is an \
             answer the same work gives to a different question, so the work itself \
             rules each one out.",
            request.label
        );
        let proof = AnswerProof::SourceBacked {
            source_id: request.source_id.to_string(),
            rubric,
        };
        let proof_json = serde_json::to_string(&proof)?;

        // What the builder produced, and what the verifier re-derived from the
        // source. Different inputs, so they cannot coincide, which the schema
        // requires before activation.
        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.prompt,
            item.options.join("\u{2}"),
            item.correct_index
        ));
        let verifier_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}from_source={}",
            item.supporting_answer,
            item.prompt,
            faq.says(&item.prompt, &item.supporting_answer)
        ));

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: request.subtest,
            objective_id: &item.objective_id,
            stem: &item.prompt,
            passage: None,
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

        let outcome: anyhow::Result<()> = (|| {
            repo.cite(&id, request.source_id)?;

            self.db.connection().execute(
                "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
                rusqlite::params![id, verifier_hash.as_str()],
            )?;

            repo.walk_to_content_reviewed(&id, request.generator)?;
            repo.activate(
                &id,
                request.reviewer,
                "ingested from a public-domain work; question and answer quoted verbatim",
            )?;

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
    }

    /// Ingest Shop Information items from one public-domain tool manual.
    ///
    /// One work per call for the same reason as the others: an item's evidence is a
    /// sentence in a named manual, and the citation has to name that manual.
    ///
    /// The OCR check matters more here than anywhere else in the corpus. These manuals
    /// are scans of 1940s and 1970s print, and a misread word became a *distractor* --
    /// `inuide micrometer` for `inside micrometer` -- which is a wrong answer that looks
    /// like a tool. Every option is checked, not only the correct one.
    pub fn ingest_purposes(
        &self,
        source: &Purposes,
        request: &PurposeIngestRequest<'_>,
    ) -> anyhow::Result<PurposeIngestReport> {
        // The options are checked against the dictionary, the prompt is not: a tool's name is
        // one to three common nouns, so a word the dictionary does not carry is suspect,
        // while the prompt quotes the manual's own prose and would trip a vocabulary rule.
        // An Auto Information option is a clause rather than a name, so the same rule would
        // refuse `to prevent entrance of water around the shield` for words the dictionary
        // does carry anyway -- the rule is applied only where it was measured.
        //
        // Whichever kind of item it is, the prose is checked for the scanner's lost spaces,
        // which is a different question from whether the words are ordinary English: the
        // prompt of `Which tool is used to remove broken screws without damagingthe
        // surrounding material?` is made of words the dictionary carries and one it does not,
        // and the learner cannot tell which.
        let check = |item: &PurposeItem| match request.kind {
            purposes::ItemKind::Tool => {
                has_joined_words(request.dictionary, &item.prompt).is_none()
                    && item
                        .options
                        .iter()
                        .all(|option| option_words_are_known(request.dictionary, option))
            }
            purposes::ItemKind::Function => {
                has_joined_words(request.dictionary, &item.prompt).is_none()
                    && item
                        .options
                        .iter()
                        .all(|option| has_joined_words(request.dictionary, option).is_none())
            }
        };
        let items = match request.kind {
            purposes::ItemKind::Tool => {
                source.build_items(request.subtest, request.count, request.seed, check)
            }
            purposes::ItemKind::Function => {
                source.build_function_items(request.subtest, request.count, request.seed, check)
            }
        };

        let mut report = PurposeIngestReport {
            label: request.label.to_string(),
            statements: source.entry_count(),
            built: items.len(),
            ..Default::default()
        };
        if source.is_empty() {
            anyhow::bail!(
                "text {:?} described no tools at all; the file's shape or its scanner's \
                 habits may have changed",
                request.label
            );
        }
        if items.is_empty() {
            report.skipped = Some(
                "no tool description survived the dictionary check on its options".to_string(),
            );
            return Ok(report);
        }

        for item in items {
            let content_hash = item.content_hash();
            if self.content_hash_exists(&content_hash)? {
                report.already_present += 1;
                continue;
            }
            // Two editions of one manual ask the same question in the same words: TM 9-8000
            // and TM 9-2700 both say `The ammeter is used to indicate the amount of current
            // flowing to and from the battery`. The item is not stored twice, because the
            // content hash cannot see it -- the two items differ in the one thing that is
            // not the question, which distractors were drawn.
            if self.question_is_stored(
                request.subtest,
                &item.prompt,
                &item.options[item.correct_index],
                None,
            )? {
                report.already_present += 1;
                continue;
            }
            match self.store_purpose_verified(source, request, &item) {
                Ok(id) => {
                    report.verified += 1;
                    report.activated += 1;
                    report.item_ids.push(id);
                }
                Err(error) => report.rejected.push(Rejection {
                    template_id: Some(item.prompt.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        if report.activated == 0 && report.already_present == 0 {
            let first = report
                .rejected
                .first()
                .map(|rejection| rejection.reason.as_str())
                .unwrap_or("no reason recorded");
            anyhow::bail!(
                "text {:?} stored nothing: all {} item(s) were refused; first reason: {first}",
                request.label,
                report.rejected.len()
            );
        }

        Ok(report)
    }

    /// Verify one tool item against its source and, only if it proves out, store it.
    pub fn store_purpose_verified(
        &self,
        source: &Purposes,
        request: &PurposeIngestRequest<'_>,
        item: &PurposeItem,
    ) -> anyhow::Result<String> {
        purposes::verify(item, source)
            .map_err(|failure| anyhow::anyhow!("item failed source verification: {failure}"))?;

        let repo = ContentItemRepo::new(self.db);
        let id = format!("q-{}", uuid::Uuid::new_v4());
        let content_hash = item.content_hash();

        let rubric = format!(
            "{} describes this tool and what it is for, in the sentence quoted; every \
             other option is a tool the same manual describes doing something else, so \
             the manual itself rules each one out.",
            request.label
        );
        let proof = AnswerProof::SourceBacked {
            source_id: request.source_id.to_string(),
            rubric,
        };
        let proof_json = serde_json::to_string(&proof)?;

        let generator_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}{}",
            item.prompt,
            item.options.join("\u{2}"),
            item.correct_index
        ));
        let verifier_hash = ContentHash::of_text(&format!(
            "{}\u{1}{}\u{1}from_source={}",
            item.supporting_sentence, item.prompt, true
        ));

        repo.insert_draft(&NewContentItem {
            id: &id,
            subtest: request.subtest,
            objective_id: &item.objective_id,
            stem: &item.prompt,
            passage: None,
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

        let outcome: anyhow::Result<()> = (|| {
            repo.cite(&id, request.source_id)?;

            self.db.connection().execute(
                "UPDATE content_items SET verifier_hash = ?2 WHERE id = ?1",
                rusqlite::params![id, verifier_hash.as_str()],
            )?;

            repo.walk_to_content_reviewed(&id, request.generator)?;
            repo.activate(
                &id,
                request.reviewer,
                "ingested from a public-domain tool manual; description quoted verbatim",
            )?;

            Ok(())
        })();
        self.finish_item(&repo, &id, outcome).map(|_| id)
    }

    /// Finish storing one item, leaving nothing behind if a step fails.
    ///
    /// Storing is a sequence -- insert the draft, cite the source, record the verifier's hash,
    /// walk the item to content-reviewed, activate it -- and the row exists from the first
    /// step. A failure after that used to leave a draft in the store, and because a draft *is*
    /// a row, the question check read it as a question the store already held: an ingestion
    /// naming a source the vault had not recorded reported six items already present and
    /// activated none, while the run's own rejection list showed every item refused.
    ///
    /// A rejected item leaves no row.
    fn finish_item(
        &self,
        repo: &ContentItemRepo<'_>,
        id: &str,
        outcome: anyhow::Result<()>,
    ) -> anyhow::Result<String> {
        match outcome {
            Ok(()) => Ok(id.to_string()),
            Err(error) => {
                if let Err(cleanup) = repo.discard_draft(id) {
                    return Err(error.context(format!(
                        "and the draft it left behind could not be discarded: {cleanup}"
                    )));
                }
                Err(error)
            }
        }
    }

    /// Whether the store already asks this question, whoever asked it first.
    ///
    /// The identity is the one a learner experiences -- stem, correct answer, and the passage
    /// if the item has one -- rather than the text a source happened to use. See
    /// `ContentItemRepo::question_exists` for the two ways a duplicate got past the content
    /// hash and reached the corpus.
    fn question_is_stored(
        &self,
        subtest: &str,
        stem: &str,
        correct: &str,
        passage: Option<&str>,
    ) -> anyhow::Result<bool> {
        ContentItemRepo::new(self.db).question_exists(subtest, stem, correct, passage)
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
        self.next_item_for(subtest, None, seen)
    }

    /// The next item for one named objective, falling back to the subtest.
    ///
    /// The study plan tells the learner which objective a session is for, so a
    /// session that returned items from another objective would contradict the plan
    /// on the very next screen. When the objective holds no servable item the
    /// subtest read is used instead of returning nothing: an objective with no
    /// content yet is a content gap, and refusing to serve anything turns a gap
    /// into a blocked learner. `objective_id = None` is the plain subtest read.
    pub fn next_item_for(
        &self,
        subtest: &str,
        objective_id: Option<&str>,
        seen: &[String],
    ) -> anyhow::Result<Option<ItemDto>> {
        let repo = ContentItemRepo::new(self.db);
        if let Some(objective) = objective_id {
            let items = repo.servable_in(subtest, Some(objective))?;
            if let Some(chosen) = pick(&items, seen) {
                return Ok(Some(ItemDto::from(chosen)));
            }
        }
        let items = repo.servable(subtest)?;
        Ok(pick(&items, seen).map(ItemDto::from))
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
    /// The sources the corpus rests on, with how much of it each one carries.
    ///
    /// The manager shows licences because that is the question a reviewer actually
    /// has: not "which file" but "on what terms may this be here". An item count of
    /// zero is meaningful too -- it marks a source recorded and never used.
    pub fn sources(&self) -> anyhow::Result<Vec<SourceDto>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(
            "SELECT e.id, e.title, e.url, e.license, e.trust,
                    (SELECT COUNT(*) FROM content_item_sources s WHERE s.source_id = e.id)
             FROM evidence_records e
             ORDER BY e.created_at, e.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SourceDto {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                licence: row.get(3)?,
                trust: row.get(4)?,
                item_count: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Items for the manager to list, newest state included.
    ///
    /// `subtest` and `state` are optional filters; `limit` is required because an
    /// unbounded listing of a five-thousand-item corpus is not a view, it is a
    /// stall.
    pub fn items(
        &self,
        subtest: Option<&str>,
        state: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<ContentItemSummaryDto>> {
        let repo = ContentItemRepo::new(self.db);
        let items = repo.all()?;
        let mut out = Vec::with_capacity(limit.min(items.len()));
        for item in items {
            if let Some(wanted) = subtest {
                if item.subtest != wanted {
                    continue;
                }
            }
            if let Some(wanted) = state {
                if item.state != wanted {
                    continue;
                }
            }
            if out.len() >= limit {
                break;
            }
            out.push(ContentItemSummaryDto {
                id: item.id.clone(),
                subtest: item.subtest.clone(),
                state: item.state.clone(),
                objective_id: item.objective_id.clone(),
                preview: truncate(&item.stem, 120),
                correct_answer: item
                    .options
                    .get(item.correct_index)
                    .cloned()
                    .unwrap_or_default(),
                reviewer: item.reviewer.clone(),
                content_hash: item.content_hash.clone(),
                sources: repo.sources(&item.id)?,
            });
        }
        Ok(out)
    }

    /// Withdraw an item from service, recording who did it and why.
    ///
    /// Quarantine is reachable from any state, which is what the schema's
    /// transition guard allows and what a review workflow needs: a bad item must be
    /// withdrawable whether it was active, under review, or already quarantined and
    /// being re-examined.
    pub fn quarantine_item(&self, item_id: &str, actor: &str, reason: &str) -> anyhow::Result<()> {
        let repo = ContentItemRepo::new(self.db);
        let item = repo
            .get(item_id)?
            .ok_or_else(|| anyhow::anyhow!("no such content item: {item_id}"))?;
        if item.state == "quarantined" {
            anyhow::bail!("{item_id} is already quarantined");
        }
        repo.advance(item_id, "quarantined", actor, reason)
    }

    /// Return a quarantined item to service.
    ///
    /// Walks the documented pipeline again rather than jumping to `active`, so the
    /// audit trail shows the item was re-verified rather than merely un-flagged.
    /// The original reviewer is kept: reinstatement returns the item to the state
    /// it was approved in, and it would be dishonest to record a new approval that
    /// nobody gave.
    pub fn reinstate_item(&self, item_id: &str, actor: &str, reason: &str) -> anyhow::Result<()> {
        let repo = ContentItemRepo::new(self.db);
        let item = repo
            .get(item_id)?
            .ok_or_else(|| anyhow::anyhow!("no such content item: {item_id}"))?;
        if item.state != "quarantined" {
            anyhow::bail!(
                "{item_id} is {} and not quarantined, so there is nothing to reinstate",
                item.state
            );
        }
        if item.reviewer.trim().is_empty() {
            anyhow::bail!(
                "{item_id} has no reviewer recorded, so it cannot be returned to service"
            );
        }
        repo.advance(item_id, "draft", actor, reason)?;
        repo.walk_to_content_reviewed(item_id, actor)?;
        repo.activate(item_id, &item.reviewer, reason)
    }

    /// One item's audit trail.
    pub fn item_history(&self, item_id: &str) -> anyhow::Result<Vec<ReviewEntryDto>> {
        let repo = ContentItemRepo::new(self.db);
        Ok(repo
            .history(item_id)?
            .into_iter()
            .map(|entry| ReviewEntryDto {
                from_state: entry.from_state,
                to_state: entry.to_state,
                actor: entry.actor,
                rationale: entry.rationale,
                created_at: entry.created_at,
            })
            .collect())
    }

    /// Everything the content manager shows, in one call.
    pub fn manager_view(&self, limit: usize) -> anyhow::Result<ContentManagerDto> {
        Ok(ContentManagerDto {
            stats: self.stats()?,
            sources: self.sources()?,
            items: self.items(None, None, limit)?,
        })
    }

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

/// A request to ingest Electronics Information items from one NEETS module.
///
/// The source id must already exist in the evidence vault: migration 004 refuses
/// a citation the vault has not seen, so an ingester cannot invent provenance for
/// itself.
#[derive(Debug, Clone)]
pub struct EiIngestRequest<'a> {
    /// The module label recorded on the items, e.g. `NEETS MOD 1`.
    pub module: &'a str,
    /// The vault id of this module's text.
    pub source_id: &'a str,
    /// How many items to attempt.
    pub count: usize,
    pub seed: u64,
    /// Definitions shorter than this cannot identify a concept.
    pub min_definition_words: usize,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// What one module's ingestion run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EiIngestReport {
    pub module: String,
    /// Glossary entries the module yielded.
    pub entries: usize,
    pub built: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: Vec<Rejection>,
    pub item_ids: Vec<String>,
}

impl EiIngestReport {
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }
}

/// What one Paragraph Comprehension ingestion run needs.
///
/// The source id must already exist in the evidence vault: migration 004 refuses a
/// citation the vault has not seen, so an ingester cannot invent provenance for
/// itself.
#[derive(Debug, Clone)]
pub struct PcIngestRequest<'a> {
    /// The work's own title, recorded on every item's rubric.
    pub label: &'a str,
    /// The vault id of this work's text.
    pub source_id: &'a str,
    /// How many items to attempt from this text.
    pub count: usize,
    pub seed: u64,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// What one text's ingestion run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PcIngestReport {
    pub label: String,
    /// Paragraphs the file parsed into, before any were filtered.
    pub paragraphs: usize,
    pub built: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: Vec<Rejection>,
    pub item_ids: Vec<String>,
    /// Why this work contributed nothing, when that is a property of its prose
    /// rather than a failure. `None` on a work that built items.
    pub skipped: Option<String>,
}

impl PcIngestReport {
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }

    /// Whether the work was usable at all.
    pub fn is_productive(&self) -> bool {
        self.built > 0
    }
}

/// What one factual ingestion run needs.
///
/// The dictionary is passed in rather than read here for the same reason the other
/// paths take it: it is 29 MB, and the pipeline is not the layer that reads sources.
#[derive(Debug, Clone)]
pub struct FactIngestRequest<'a> {
    /// The subtest the items serve: `GS` for the science works, `SI`/`AI` for the
    /// shop and automotive manuals.
    pub subtest: &'a str,
    /// The work's own title, recorded on every item's rubric.
    pub label: &'a str,
    /// The vault id of this work's text.
    pub source_id: &'a str,
    pub count: usize,
    pub seed: u64,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// What one work's factual ingestion run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FactIngestReport {
    pub label: String,
    /// Questions the work asks, before any were filtered.
    pub questions: usize,
    pub built: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: Vec<Rejection>,
    pub item_ids: Vec<String>,
    /// Why this work contributed nothing, when that is a property of its prose
    /// rather than a failure. `None` on a work that built items.
    pub skipped: Option<String>,
}

impl FactIngestReport {
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }

    pub fn is_productive(&self) -> bool {
        self.built > 0
    }
}

/// What one tool-manual ingestion run needs.
#[derive(Debug, Clone)]
pub struct PurposeIngestRequest<'a> {
    /// The subtest the items serve: `SI` for the shop manuals, `AI` for the automotive ones.
    pub subtest: &'a str,
    /// Which half of each description the item asks about.
    ///
    /// A tool manual and an automotive manual write the same sentence, `X is used to Y`. Shop
    /// Information asks which tool does Y; Auto Information asks what X is for. The kind is
    /// the subtest's question, so the caller states it rather than the pipeline guessing.
    pub kind: purposes::ItemKind,
    /// The manual's own title, recorded on every item's rubric.
    pub label: &'a str,
    /// The vault id of this manual's text.
    pub source_id: &'a str,
    pub count: usize,
    pub seed: u64,
    /// Used only to refuse an option containing an OCR misreading.
    pub dictionary: &'a Dictionary,
    /// Named reviewer recorded on activation (REQ-056).
    pub reviewer: &'a str,
    /// Actor recorded in the audit trail for the machine steps.
    pub generator: &'a str,
}

/// What one manual's ingestion run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PurposeIngestReport {
    pub label: String,
    /// Tool descriptions the manual yielded, before any were filtered.
    pub statements: usize,
    pub built: usize,
    pub verified: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: Vec<Rejection>,
    pub item_ids: Vec<String>,
    /// Why this manual contributed nothing, when that is a property of its text.
    pub skipped: Option<String>,
}

impl PurposeIngestReport {
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }

    pub fn is_productive(&self) -> bool {
        self.built > 0
    }
}

/// Whether every word of a learner-facing *option* is a word the dictionary carries.
///
/// ## Why this is not `looks_like_misreading`
///
/// `looks_like_misreading` asks whether a token is one edit from a dictionary word, which
/// is sound for the technical glossary terms it was written for and unsound for prose.
/// Measured against *The book of wonders* it refused 226 of 301 items, and every refusal
/// it printed was an ordinary English word: `produced`, `caused`, `applied`, `began`,
/// `countries`, `today`. Those are one edit from their own base forms -- `produced` is
/// `produce` plus a deletion, because the inflection helper does not know that `-ed`
/// follows a silent `e`. A filter that rejects the language is worse than no filter: it
/// removes real content while looking like diligence.
///
/// So the factual path carries no dictionary check at all, and the shop path checks the
/// one thing whose vocabulary is small and technical enough for the rule to mean
/// something: the options. A tool is one to three words, all of them common nouns, so a
/// word the dictionary does not carry is genuinely suspect -- `inuide micrometer` was a
/// real misreading that reached an option.
///
/// The check stays where it was designed and measured: the NEETS glossary path, whose
/// terms and definitions are short and technical.
fn option_words_are_known(dictionary: &Dictionary, text: &str) -> bool {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|word| word.len() >= 4)
        .all(|word| covers_word(dictionary, word))
}

/// The token in a learner-facing run of prose that the scanner ran two words together in.
///
/// A lost space is the failure this corpus actually has, and a vocabulary check on the
/// options does not see it. `damaging the` reached an item's prompt as `damagingthe`,
/// `in a vise` as `ina vise`, and `riveted, or` as `riveted,or`. None of those is one edit
/// from a dictionary word -- `looks_like_misreading` compares edit distance and a joined
/// pair is not a substitution -- but each is a token the dictionary does not carry that
/// comes apart into two tokens it does. The rule is applied to the prose a learner reads:
/// the purpose a shop prompt asks about, and the clauses an Auto Information item offers as
/// options. It is never applied to a manual's whole text, where an unknown token is usually
/// technical vocabulary rather than damage.
///
/// Returns the offending token so a refusal can name what it refused.
pub fn has_joined_words(dictionary: &Dictionary, text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        // Punctuation that has a word on both sides of it, with no space, is the same
        // damage: `soldering,or brazing` offers a learner a word that does not exist.
        let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
        let joined_by_punctuation = trimmed.char_indices().any(|(index, character)| {
            matches!(character, ',' | ';')
                && index > 0
                && index + character.len_utf8() < trimmed.len()
        });
        if joined_by_punctuation {
            return Some(token.to_string());
        }
        // Only a word of letters can be two words run together; `T-bevel` and `1-57` are
        // hyphenated names and figures, not lost spaces. A hyphenated compound can lose the
        // same space between its parts -- TM 9-8000 reads `allowing for engine-todrive train
        // clearance`, where `todrive` is `to drive` -- so each part is examined on its own.
        if trimmed.len() < 5 || !trimmed.chars().all(char::is_alphabetic) {
            for part in trimmed.split('-') {
                if part.len() >= 5 && comes_apart_into_two_words(dictionary, part) {
                    return Some(token.to_string());
                }
            }
            continue;
        }
        let lower = trimmed.to_lowercase();
        if comes_apart_into_two_words(dictionary, &lower) {
            return Some(token.to_string());
        }
    }
    None
}

/// Whether a word the dictionary does not carry comes apart into two words it does.
///
/// Every split is tried and both halves have to be two letters or more: `damagingthe` is
/// `damaging` and `the`, and `todrive` is `to` and `drive`.
fn comes_apart_into_two_words(dictionary: &Dictionary, word: &str) -> bool {
    if covers_word(dictionary, word) {
        return false;
    }
    let lower = word.to_lowercase();
    for index in 2..lower.len().saturating_sub(1) {
        if !lower.is_char_boundary(index) {
            continue;
        }
        let (left, right) = lower.split_at(index);
        if covers_word(dictionary, left) && covers_word(dictionary, right) {
            return true;
        }
    }
    false
}

/// Whether the dictionary carries a word, allowing for the inflections Webster's omits.
///
/// The suffix list is longer than the one in the dictionary module because this runs over
/// a learner-facing option rather than over a glossary term: `-ies` to `-y` (`batteries`),
/// `-ied` to `-y` (`applied`), and a silent `e` before `-ed` or `-ing` (`produced`,
/// `causing`) all have to resolve, or the check refuses ordinary tools.
fn covers_word(dictionary: &Dictionary, word: &str) -> bool {
    if dictionary.covers(word) {
        return true;
    }
    let lower = word.to_lowercase();
    for suffix in ["s", "es", "ed", "ing", "ly", "est", "er"] {
        if let Some(stem) = lower.strip_suffix(suffix) {
            if stem.len() >= 3
                && (dictionary.covers(stem) || dictionary.covers(&format!("{stem}e")))
            {
                return true;
            }
        }
    }
    // `-ies` and `-ied` are `-y` in the base form.
    for suffix in ["ies", "ied"] {
        if let Some(stem) = lower.strip_suffix(suffix) {
            if stem.len() >= 2 && dictionary.covers(&format!("{stem}y")) {
                return true;
            }
        }
    }
    false
}

/// Whether a glossary definition survived the scan intact.
///
/// The module text is OCR, and this scan read `c` as `e`: the corpus contains
/// `eleetron`, `eurrent`, `eonduct` and `resistanee`. An item whose definition
/// reads "the amount of eleetron flow" teaches a learner that `eleetron` is a
/// word, which is worse than having no item at all.
///
/// The test is deliberately narrow. Refusing every token the dictionary lacks
/// discards legitimate technical vocabulary -- Webster's 1913 predates
/// electronics, so `amplidyne` would be refused -- and applied to NEETS module 5
/// that rule produced **zero** items. `looks_like_misreading` targets the error
/// this scanner actually makes. A term the module defines itself is accepted
/// outright, because the glossary is the authority on its own vocabulary.
pub fn entry_is_legible(
    term: &str,
    definition: &str,
    dictionary: &Dictionary,
    glossary: &Glossary,
) -> bool {
    let tokens = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_ascii_alphabetic())
            .filter(|token| token.len() >= 4)
            .map(str::to_string)
            .collect()
    };

    // The term's own words are checked against the dictionary alone. The glossary
    // exemption below must not apply to them, and the reason is subtle: `LILTER`
    // *is* defined by the glossary, because it heads the entry `BANDPASS LILTER`.
    // Exempting it let a misspelled term certify itself, and fourteen items reached
    // the corpus with a misspelling as their correct answer.
    let term_is_clean = tokens(term)
        .iter()
        .all(|token| !looks_like_misreading(dictionary, token));

    // Definition tokens may be technical vocabulary the dictionary never carried,
    // so a term the module defines itself is accepted outright. That is what keeps
    // `amplidyne` from being read as a typo.
    let definition_is_clean = tokens(definition)
        .iter()
        .all(|token| glossary.defines(token) || !looks_like_misreading(dictionary, token));

    term_is_clean && definition_is_clean
}

/// Shorten text for a listing without cutting mid-word where avoidable.
fn truncate(text: &str, limit: usize) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= limit {
        return cleaned;
    }
    let cut: String = cleaned.chars().take(limit).collect();
    match cut.rsplit_once(' ') {
        Some((head, _)) => format!("{head}…"),
        None => format!("{cut}…"),
    }
}
/// A source the corpus rests on, as the content manager lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDto {
    pub id: String,
    pub title: String,
    pub url: String,
    pub licence: String,
    pub trust: f64,
    /// How many items cite this source, active or not.
    pub item_count: i64,
}

/// An item as the content manager lists it.
///
/// A preview rather than the whole item: the manager shows enough to recognise a
/// question and decide about it, and the full stem with all four options is what
/// the practice view is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentItemSummaryDto {
    pub id: String,
    pub subtest: String,
    pub state: String,
    pub objective_id: String,
    pub preview: String,
    /// The option marked correct, so a reviewer can judge the item without
    /// opening it.
    pub correct_answer: String,
    pub reviewer: String,
    pub content_hash: String,
    pub sources: Vec<String>,
}

/// One entry in an item's audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewEntryDto {
    pub from_state: String,
    pub to_state: String,
    pub actor: String,
    pub rationale: String,
    pub created_at: String,
}

/// What the content manager shows about the corpus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentManagerDto {
    pub stats: ContentStatsDto,
    pub sources: Vec<SourceDto>,
    pub items: Vec<ContentItemSummaryDto>,
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
    /// The passage to read, for Paragraph Comprehension. `None` elsewhere.
    ///
    /// Without this an item of that subtest is unanswerable: the question refers to
    /// a text, and a learner served the question alone has nothing to refer to.
    pub passage: Option<String>,
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
            passage: item.passage.clone(),
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
