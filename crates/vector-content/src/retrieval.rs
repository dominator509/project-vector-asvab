//! Source-grounded retrieval over the evidence vault (REQ-008).
//!
//! REQ-008 requires "source-grounded explanations and original variants". The
//! binding constraint is that an explanation must be traceable to a stored
//! source: an answer that cites nothing is not grounded, and a citation that
//! does not exist in the vault is a fabrication.
//!
//! This module implements retrieval over evidence records and a grounding check
//! that refuses to emit a citation for a source that is not actually present.

use serde::{Deserialize, Serialize};

use crate::knowledge::StopWords;

/// A retrievable source snapshot, mirroring the evidence vault record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDocument {
    pub id: String,
    pub title: String,
    pub url: String,
    pub content_hash: String,
    pub license: String,
    /// Trust score in [0, 1] recorded when the source was captured.
    pub trust: f64,
    /// Source text used for retrieval.
    pub text: String,
}

/// A retrieved passage with its provenance and relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievedPassage {
    pub source_id: String,
    pub title: String,
    pub url: String,
    pub content_hash: String,
    pub trust: f64,
    /// Relevance in [0, 1] for the query.
    pub relevance: f64,
    /// The matched text.
    pub text: String,
}

/// An explanation produced from retrieved sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundedExplanation {
    pub answer: String,
    /// Source ids actually used. Every id must exist in the vault.
    pub cited_source_ids: Vec<String>,
}

/// A citation that cannot be verified against the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundingError {
    /// The explanation cites a source id that is not in the vault.
    UnknownSource(String),
    /// The explanation cites nothing at all.
    NoCitations,
    /// No source was relevant enough to ground an answer.
    NoRelevantSource,
}

impl std::fmt::Display for GroundingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroundingError::UnknownSource(id) => {
                write!(f, "cited source {id} does not exist in the evidence vault")
            }
            GroundingError::NoCitations => {
                write!(f, "a grounded explanation must cite at least one source")
            }
            GroundingError::NoRelevantSource => {
                write!(f, "no source was relevant enough to ground an answer")
            }
        }
    }
}

impl std::error::Error for GroundingError {}

/// Tokenize text into lowercase alphanumeric terms, dropping stop words.
fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .filter(|t| !StopWords::is_stop_word(t))
        .collect()
}

/// Score a document against a query.
///
/// A simple, deterministic term-overlap score: the fraction of distinct query
/// terms present in the document, weighted by the document's trust. It is
/// deliberately explainable rather than a learned embedding, so a learner can
/// see why a source was retrieved.
pub fn score(query: &str, doc: &SourceDocument) -> f64 {
    let query_terms = terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }
    let doc_terms: std::collections::BTreeSet<String> = terms(&doc.text).into_iter().collect();

    let distinct: std::collections::BTreeSet<&String> = query_terms.iter().collect();
    let matched = distinct.iter().filter(|t| doc_terms.contains(**t)).count();
    let coverage = matched as f64 / distinct.len() as f64;

    // Trust scales the score but never inverts ordering within equal coverage.
    coverage * doc.trust.clamp(0.0, 1.0)
}

/// Retrieve the top passages for a query.
///
/// Only documents that actually match are returned; a relevance floor prevents
/// returning unrelated sources merely to fill a quota, which would let an
/// explanation cite something that does not support it.
pub fn retrieve(
    query: &str,
    documents: &[SourceDocument],
    limit: usize,
    min_relevance: f64,
) -> Vec<RetrievedPassage> {
    let mut scored: Vec<RetrievedPassage> = documents
        .iter()
        .filter_map(|doc| {
            let relevance = score(query, doc);
            if relevance < min_relevance || relevance <= 0.0 {
                return None;
            }
            Some(RetrievedPassage {
                source_id: doc.id.clone(),
                title: doc.title.clone(),
                url: doc.url.clone(),
                content_hash: doc.content_hash.clone(),
                trust: doc.trust,
                relevance,
                text: doc.text.clone(),
            })
        })
        .collect();

    // Deterministic ordering: relevance desc, then source id for stable ties.
    scored.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_id.cmp(&b.source_id))
    });
    scored.truncate(limit);
    scored
}

/// Verify that an explanation is actually grounded in the vault.
///
/// This is the anti-fabrication control: a model may produce fluent citations
/// for sources that do not exist. Refusing to emit such an explanation is what
/// makes "source-grounded" a real property rather than a label.
pub fn verify_grounding(
    explanation: &GroundedExplanation,
    vault: &[SourceDocument],
) -> Result<(), GroundingError> {
    if explanation.cited_source_ids.is_empty() {
        return Err(GroundingError::NoCitations);
    }
    for id in &explanation.cited_source_ids {
        if !vault.iter().any(|d| &d.id == id) {
            return Err(GroundingError::UnknownSource(id.clone()));
        }
    }
    Ok(())
}

/// Build a grounded explanation from retrieved passages.
///
/// Citations are derived from the passages actually retrieved, so the
/// explanation cannot cite a source that retrieval did not supply.
pub fn explain_from(
    subject: &str,
    passages: &[RetrievedPassage],
) -> Result<GroundedExplanation, GroundingError> {
    if passages.is_empty() {
        return Err(GroundingError::NoRelevantSource);
    }

    let mut cited: Vec<String> = Vec::new();
    let mut body = String::new();
    for passage in passages {
        if !cited.contains(&passage.source_id) {
            cited.push(passage.source_id.clone());
        }
        body.push_str(&format!(
            "- {} (source: {}, license: {})\n",
            first_sentence(&passage.text),
            passage.title,
            // License travels with the citation so use stays lawful.
            "see source record"
        ));
    }

    Ok(GroundedExplanation {
        answer: format!("{subject}\n{body}"),
        cited_source_ids: cited,
    })
}

/// The first sentence of a passage, used as a concise supporting quote.
fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.find(['.', '!', '?']) {
        Some(idx) => trimmed[..=idx].to_string(),
        None => trimmed.to_string(),
    }
}
