//! Signed content packs: build, verify, install, roll back.
//!
//! Requirements: REQ-023 (signed, versioned packs), REQ-032 (rollback surface),
//! REQ-048, REQ-056.
//!
//! `CONTENT_PACK_SPEC.md` requires a pack to carry a manifest, a schema version, a
//! compatible app range, a source ledger, licences, the items themselves, and a
//! detached signature; and it requires installation to verify schema, signature,
//! hashes, compatibility, provenance completeness, a prohibited-source scan, answer
//! consistency and reviewer state before atomic activation, with the previous signed
//! pack left available for rollback. This module is that contract.
//!
//! ## Why the checks live here rather than in the tool that calls them
//!
//! A pack arrives from outside the machine. Every check installation performs is a
//! refusal that has to happen whoever is installing, so they are one function rather
//! than a convention each caller is trusted to follow. The tool, the interface and a
//! future updater all call [`install_pack`].
//!
//! ## What a signature does and does not establish
//!
//! It establishes that the holder of a private key attested this content identity.
//! It says nothing about whether a human reviewed the items, which is why every item
//! in a pack must also name its reviewer and why installation refuses an item that
//! does not. Those are two different facts and the pack records both.
//!
//! ## What is deliberately not verified
//!
//! Nothing here re-derives an item's answer from its proof. An item arrives with the
//! `generator_hash` and `verifier_hash` its own pipeline recorded, and this module
//! checks that they are present and differ; re-running verification would mean
//! shipping the ingesters, their sources and their dictionaries inside every install.
//! The provenance chain is checked instead: every item cites a source the pack's own
//! ledger carries, every source carries a licence the corpus permits, and the whole
//! document is signed.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use vector_domain::content::ContentPack;
use vector_persistence::repo::{
    ContentPackRecord, ContentPackRepo, InstallOutcome, NewPackItem, NewPackRecord,
    PackSourceRecord,
};
use vector_persistence::Database;

/// The format tag every pack file carries.
pub const PACK_FORMAT: &str = "vector-content-pack";

/// The pack schema this build understands.
pub const PACK_SCHEMA_VERSION: i64 = 1;

/// Licences a pack may carry.
///
/// This is the prohibited-source scan. A pack whose ledger names a licence outside
/// this list is refused whole rather than item by item: the corpus admits public
/// domain and US government works, and anything else -- a commercially licensed
/// question bank, a scan of a study guide, an unattributed source -- is exactly what
/// the exclusion rule exists to keep out.
///
/// The fifth entry is the computable subtests' own citation, and it needs saying why it is not a
/// loophole. Arithmetic Reasoning, Mathematics Knowledge and Mechanical Comprehension have no
/// source text: their items are drawn by the factory from the published subtest constructs, and
/// their citation is the record of those constructs. That record's licence says what it is --
/// facts only, item text original work -- because the page it cites asserts all rights reserved
/// and claiming otherwise would be the dishonest option. What keeps such a pack honest is not
/// this list but the other two checks that run beside it: an item of those subtests must carry an
/// executable proof the installer recomputes (`COMPUTABLE_SUBTESTS` below), and every citation
/// must name a vault record whose bytes were hashed when they were retrieved. This list checks a
/// declaration; the vault and the proof check the thing itself.
pub const PERMITTED_LICENCES: [&str; 5] = [
    "Public domain in the USA",
    "Public domain (US government work)",
    "Public domain",
    "US-Government-Work",
    "Facts-only; item text is original work",
];

/// Subtests whose answers are computable, and therefore must arrive with an
/// executable proof rather than a source-backed rubric.
const COMPUTABLE_SUBTESTS: [&str; 3] = ["AR", "MK", "MC"];

/// One node of a pack's curriculum graph.
///
/// A pack of items with objective ids is a bag: it says what it contains and not what it
/// teaches. The graph is what makes an objective more than a label -- it names the objective,
/// says which subtest it belongs to, and says what has to come first. A study planner needs
/// that to order a learner's work, and an installer needs it to refuse a pack whose items
/// teach objectives the pack never declared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurriculumNode {
    pub objective_id: String,
    pub subtest: String,
    pub title: String,
    /// Objective ids in this same graph that should be learned first. Empty for a root.
    #[serde(default)]
    pub prerequisites: Vec<String>,
}

/// What a pack claims about how hard its objectives are.
///
/// Per objective rather than per item, and carrying the evidence it rests on: an item's
/// `difficulty` field is what the *generator* believed when it wrote the question, while this
/// is what the pack asserts about the objective after looking at responses. `responses` is
/// what makes that distinction checkable -- an estimate resting on zero responses is a
/// declaration, not a measurement, and the field is what lets a reader tell them apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationEntry {
    pub objective_id: String,
    /// The share of the target population expected to answer correctly, 0.0 to 1.0.
    pub expected_correct: f64,
    /// How many recorded responses the estimate rests on. Zero means "declared".
    pub responses: u64,
    /// Where the estimate came from, in the pack author's words.
    pub basis: String,
}

/// A source in a pack's ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackSource {
    /// The pack's own name for the source. Items cite this id.
    pub id: String,
    pub url: String,
    pub title: String,
    pub content_hash: String,
    pub licence: String,
    pub effective_date: String,
    pub trust: f64,
    pub retrieval_status: String,
}

/// An item in a pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackItem {
    pub id: String,
    pub subtest: String,
    pub objective_id: String,
    pub stem: String,
    #[serde(default)]
    pub passage: Option<String>,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    /// Why each wrong option is wrong, keyed by option index.
    ///
    /// String keys rather than `usize`, because a JSON object's keys are strings and
    /// this is the form the pack travels in: the same shape the command boundary
    /// sends an item in. Verification checks every key really is an option index.
    pub distractor_rationales: BTreeMap<String, String>,
    pub difficulty: f64,
    pub proof_kind: String,
    pub proof_json: String,
    pub content_hash: String,
    pub generator_hash: Option<String>,
    pub verifier_hash: Option<String>,
    pub reviewer: String,
    /// Source ids from this pack's ledger.
    pub sources: Vec<String>,
}

/// Everything a signature covers: the pack's content, without the signature itself.
///
/// This is the struct that gets hashed. It exists so that the bytes hashed are a
/// named type rather than "the document minus three fields", which would silently
/// change meaning the next time a field is added to the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackPayload {
    format: String,
    schema_version: i64,
    name: String,
    version: i64,
    created_at: String,
    app_min: String,
    app_max: Option<String>,
    sources: Vec<PackSource>,
    items: Vec<PackItem>,
    /// Every licence the pack's ledger uses, so a reader can see the terms without
    /// walking the sources.
    licences: Vec<String>,
    /// What the pack teaches, as a graph rather than a set of labels.
    #[serde(default)]
    curriculum: Vec<CurriculumNode>,
    /// What the pack claims about how hard each objective is, and on what evidence.
    #[serde(default)]
    calibration: Vec<CalibrationEntry>,
}

/// A pack file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackDocument {
    #[serde(flatten)]
    payload: PackPayload,
    /// `sha256:...` over the canonical serialization of the payload.
    pub content_hash: String,
    /// Detached Ed25519 signature over the pack identity, hex.
    pub signature: String,
    /// The public key that produced it, hex.
    pub signer: String,
}

/// Why a pack was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum PackRefusal {
    /// The file was not a pack at all.
    Malformed(String),
    UnsupportedFormat {
        found: String,
    },
    UnsupportedSchema {
        found: i64,
        supported: i64,
    },
    IncompatibleApp {
        pack_requires: String,
        running: String,
    },
    ContentHashMismatch {
        recorded: String,
        computed: String,
    },
    NotSigned,
    UntrustedSigner {
        found: String,
    },
    InvalidSignature(String),
    NoItems,
    /// An item cites no source, or cites one the ledger does not carry.
    MissingProvenance {
        item: String,
        detail: String,
    },
    /// A source carries a licence the corpus does not permit.
    SourceNotPermitted {
        source: String,
        licence: String,
    },
    /// The answer does not address the options, or the options are not usable.
    AnswerInconsistent {
        item: String,
        detail: String,
    },
    /// An item arrives without a named reviewer.
    UnreviewedItem {
        item: String,
    },
    /// Two items share a content hash, so one of them is unreachable.
    DuplicateContent {
        hash: String,
    },
    /// A computable subtest arrived with a rubric instead of a proof.
    WrongProofKind {
        item: String,
        subtest: String,
    },
    /// An item teaches an objective the curriculum graph does not carry.
    ObjectiveNotInCurriculum {
        item: String,
        objective_id: String,
    },
    /// An item's subtest disagrees with the subtest its objective belongs to.
    ObjectiveSubtestMismatch {
        objective_id: String,
        curriculum: String,
        item: String,
    },
    /// A node requires an objective the graph does not carry.
    UnknownPrerequisite {
        objective_id: String,
        prerequisite: String,
    },
    /// The prerequisite relation has a cycle, so no order satisfies it.
    CurriculumCycle {
        objective_id: String,
    },
    /// An objective that items teach has no calibration entry.
    CalibrationMissing {
        objective_id: String,
    },
    /// A calibration entry names an objective the graph does not carry.
    CalibrationUnknownObjective {
        objective_id: String,
    },
    /// An expected-correct share outside 0.0 to 1.0, which is not a share at all.
    CalibrationOutOfRange {
        objective_id: String,
        expected_correct: f64,
    },
}

impl std::fmt::Display for PackRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackRefusal::Malformed(detail) => write!(f, "the pack file is malformed: {detail}"),
            PackRefusal::UnsupportedFormat { found } => write!(
                f,
                "the file is format {found:?}, not {PACK_FORMAT:?}, so it is not a content pack"
            ),
            PackRefusal::UnsupportedSchema { found, supported } => write!(
                f,
                "the pack is schema version {found}, and this build installs version {supported}"
            ),
            PackRefusal::IncompatibleApp {
                pack_requires,
                running,
            } => write!(
                f,
                "the pack requires application {pack_requires} and this is {running}"
            ),
            PackRefusal::ContentHashMismatch { recorded, computed } => write!(
                f,
                "the pack records content hash {recorded} but its contents hash to {computed}"
            ),
            PackRefusal::NotSigned => write!(f, "the pack carries no signature"),
            PackRefusal::UntrustedSigner { found } => write!(
                f,
                "the pack is signed by {found}, which is not the key this installation trusts"
            ),
            PackRefusal::InvalidSignature(detail) => {
                write!(f, "the pack signature does not verify: {detail}")
            }
            PackRefusal::NoItems => write!(f, "the pack contains no items"),
            PackRefusal::MissingProvenance { item, detail } => {
                write!(f, "item {item} has no usable provenance: {detail}")
            }
            PackRefusal::SourceNotPermitted { source, licence } => write!(
                f,
                "source {source} carries licence {licence:?}, which the corpus does not permit"
            ),
            PackRefusal::AnswerInconsistent { item, detail } => {
                write!(f, "item {item} is not answerable: {detail}")
            }
            PackRefusal::UnreviewedItem { item } => {
                write!(f, "item {item} arrives without a named reviewer")
            }
            PackRefusal::DuplicateContent { hash } => {
                write!(f, "two items share the content hash {hash}")
            }
            PackRefusal::WrongProofKind { item, subtest } => write!(
                f,
                "item {item} serves {subtest}, which is computable, but arrives with a \
                 source-backed rubric instead of an executable proof"
            ),
            PackRefusal::ObjectiveNotInCurriculum { item, objective_id } => write!(
                f,
                "item {item} teaches {objective_id}, which the curriculum graph does not carry"
            ),
            PackRefusal::ObjectiveSubtestMismatch {
                objective_id,
                curriculum,
                item,
            } => write!(
                f,
                "item {item} belongs to objective {objective_id}, which the curriculum assigns \
                 to {curriculum}, not to {item}"
            ),
            PackRefusal::UnknownPrerequisite {
                objective_id,
                prerequisite,
            } => write!(
                f,
                "{objective_id} requires {prerequisite}, which the curriculum graph does not carry"
            ),
            PackRefusal::CurriculumCycle { objective_id } => write!(
                f,
                "the curriculum's prerequisites form a cycle through {objective_id}, so no \
                 learning order satisfies them"
            ),
            PackRefusal::CalibrationMissing { objective_id } => write!(
                f,
                "objective {objective_id} has items and no calibration entry"
            ),
            PackRefusal::CalibrationUnknownObjective { objective_id } => write!(
                f,
                "the calibration names {objective_id}, which the curriculum graph does not carry"
            ),
            PackRefusal::CalibrationOutOfRange {
                objective_id,
                expected_correct,
            } => write!(
                f,
                "objective {objective_id} claims an expected-correct share of \
                 {expected_correct}, which is not a share between 0 and 1"
            ),
        }
    }
}

impl std::error::Error for PackRefusal {}

// ---------------------------------------------------------------------------
// Hex, because the workspace carries no base64 dependency and a signature has to be
// written as text to travel in a JSON file.
// ---------------------------------------------------------------------------

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push((high * 16 + low) as u8);
    }
    Some(out)
}

impl PackDocument {
    /// The bytes the content hash covers.
    fn payload_bytes(&self) -> Vec<u8> {
        // Serialization is deterministic for this type: fields keep declaration
        // order and every collection is built in a stable order (items by id,
        // sources by id, licences sorted), so the same content hashes the same way on
        // every machine.
        serde_json::to_vec(&self.payload).expect("the payload is serializable")
    }

    /// Recompute the content hash from the pack's own contents.
    pub fn computed_content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.payload_bytes());
        format!("sha256:{:x}", hasher.finalize())
    }

    pub fn format(&self) -> &str {
        &self.payload.format
    }

    pub fn schema_version(&self) -> i64 {
        self.payload.schema_version
    }

    pub fn name(&self) -> &str {
        &self.payload.name
    }

    pub fn version(&self) -> i64 {
        self.payload.version
    }

    pub fn items(&self) -> &[PackItem] {
        &self.payload.items
    }

    pub fn sources(&self) -> &[PackSource] {
        &self.payload.sources
    }

    pub fn licences(&self) -> &[String] {
        &self.payload.licences
    }

    /// What the pack teaches, as the graph it declares.
    pub fn curriculum(&self) -> &[CurriculumNode] {
        &self.payload.curriculum
    }

    /// What the pack claims about how hard each objective is, and on what evidence.
    pub fn calibration(&self) -> &[CalibrationEntry] {
        &self.payload.calibration
    }

    pub fn app_min(&self) -> &str {
        &self.payload.app_min
    }

    pub fn app_max(&self) -> Option<&str> {
        self.payload.app_max.as_deref()
    }
}

/// What to build a pack from.
#[derive(Debug, Clone)]
pub struct BuildPackRequest<'a> {
    pub name: &'a str,
    pub version: i64,
    /// The oldest application version that can install this pack.
    pub app_min: &'a str,
    /// The newest, when the pack is known not to work past a version.
    pub app_max: Option<&'a str>,
    /// What the pack teaches. Empty means "derive one node per objective the items use",
    /// which produces a graph with no prerequisites -- honest, and less useful than a real
    /// curriculum, which is why the caller can supply one.
    pub curriculum: Vec<CurriculumNode>,
    /// What the pack claims about difficulty. Empty means "declare one entry per taught
    /// objective from the items' own difficulty scale, with no responses behind it".
    pub calibration: Vec<CalibrationEntry>,
}

/// Round a derived expected-correct share to a stable, round-trip-safe precision.
///
/// The pack's content hash is computed over the serialized payload, and
/// `verify_pack` recomputes it from what was parsed back off disk. A raw `f64`
/// produced by a sigmoid is not guaranteed to survive that text round trip with
/// identical bytes -- `0.24973989440488234` serializes to more digits than it
/// parses back as -- so the recomputed hash can differ from the recorded one for a
/// pack nobody tampered with. Six decimal places is finer than a declared,
/// unmeasured estimate carries information, and it round-trips exactly.
fn round_share(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

/// Assemble a pack from everything the store currently serves.
///
/// Only `active` items are packed: a draft, a quarantined item or one that was
/// withdrawn is not content anybody attested to, and a pack is an attestation.
pub fn build_pack(
    db: &Database,
    request: &BuildPackRequest<'_>,
    key: &SigningKey,
) -> anyhow::Result<PackDocument> {
    let repo = vector_persistence::content::ContentItemRepo::new(db);
    let items = repo.all()?;
    let servable: Vec<_> = items
        .iter()
        .filter(|item| item.state == "active")
        .cloned()
        .collect();
    if servable.is_empty() {
        anyhow::bail!("the store holds no active items, so there is nothing to attest in a pack");
    }

    // The source ledger: every source cited by a packed item, read from the vault so
    // the licence and URL travel with the pack rather than being re-typed.
    let mut source_ids: HashSet<String> = HashSet::new();
    for item in &servable {
        for source in repo.sources(&item.id)? {
            source_ids.insert(source);
        }
    }
    let vault = vector_persistence::repo::EvidenceRepo::new(db);
    let mut sources: Vec<PackSource> = Vec::new();
    for id in &source_ids {
        let record = vault
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("item provenance cites {id}, which the vault lacks"))?;
        sources.push(PackSource {
            id: record.id,
            url: record.url,
            title: record.title,
            content_hash: record.content_hash,
            licence: record.license,
            effective_date: record.effective_date,
            trust: record.trust,
            retrieval_status: record.retrieval_status,
        });
    }
    sources.sort_by(|a, b| a.id.cmp(&b.id));

    let mut packed: Vec<PackItem> = Vec::new();
    for item in servable {
        let mut cited = repo.sources(&item.id)?;
        cited.sort();
        packed.push(PackItem {
            id: item.id.clone(),
            subtest: item.subtest.clone(),
            objective_id: item.objective_id.clone(),
            stem: item.stem.clone(),
            passage: item.passage.clone(),
            options: item.options.clone(),
            correct_index: item.correct_index,
            explanation: item.explanation.clone(),
            distractor_rationales: item
                .distractor_rationales
                .iter()
                .map(|(index, why)| (index.to_string(), why.clone()))
                .collect(),
            difficulty: item.difficulty,
            proof_kind: item.proof_kind.clone(),
            proof_json: item.proof_json.clone(),
            content_hash: item.content_hash.clone(),
            generator_hash: item.generator_hash.clone(),
            verifier_hash: item.verifier_hash.clone(),
            reviewer: item.reviewer.clone(),
            sources: cited,
        });
    }
    packed.sort_by(|a, b| a.id.cmp(&b.id));

    let mut licences: Vec<String> = sources
        .iter()
        .map(|source| source.licence.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    licences.sort();

    // What the pack teaches. A caller that supplies a graph gets the one it supplied; a caller
    // that does not gets one node per objective the items use, with no prerequisites. The
    // second is not a curriculum -- it is a declaration of what the pack touches, and it is
    // what makes the "every item's objective is declared" check meaningful for a pack built
    // before anyone wrote a curriculum.
    let curriculum = if request.curriculum.is_empty() {
        let mut nodes: Vec<CurriculumNode> = packed
            .iter()
            .map(|item| (item.objective_id.clone(), item.subtest.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|(objective_id, subtest)| CurriculumNode {
                title: objective_id.clone(),
                objective_id,
                subtest,
                prerequisites: Vec::new(),
            })
            .collect();
        nodes.sort_by(|a, b| a.objective_id.cmp(&b.objective_id));
        nodes
    } else {
        let mut nodes = request.curriculum.clone();
        nodes.sort_by(|a, b| a.objective_id.cmp(&b.objective_id));
        nodes
    };

    // What the pack claims about difficulty. The default reads the items' own difficulty
    // scale, which is a logit-like value centred on zero, and converts it to an
    // expected-correct share. `responses: 0` and the basis string say plainly that this is a
    // declaration rather than a measurement.
    let calibration = if request.calibration.is_empty() {
        let mut by_objective: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for item in &packed {
            by_objective
                .entry(item.objective_id.clone())
                .or_default()
                .push(item.difficulty);
        }
        by_objective
            .into_iter()
            .map(|(objective_id, difficulties)| {
                let mean = difficulties.iter().sum::<f64>() / difficulties.len() as f64;
                CalibrationEntry {
                    objective_id,
                    // The content hash covers the serialized payload, and the pack is
                    // verified by re-serializing what was parsed out of the file. A
                    // raw sigmoid can print with more significant digits than it
                    // re-parses to -- `0.24973989440488234` reads back as
                    // `0.2497398944048823` -- which changes the bytes and makes the
                    // hash disagree with itself. Rounding to a share with a fixed
                    // decimal precision keeps build and verify byte-identical while
                    // staying far finer than a declared, unmeasured estimate needs.
                    expected_correct: round_share(1.0 / (1.0 + (-mean).exp())),
                    responses: 0,
                    basis: "declared from the items' difficulty scale; no responses recorded"
                        .to_string(),
                }
            })
            .collect()
    } else {
        let mut entries = request.calibration.clone();
        entries.sort_by(|a, b| a.objective_id.cmp(&b.objective_id));
        entries
    };

    let payload = PackPayload {
        format: PACK_FORMAT.to_string(),
        schema_version: PACK_SCHEMA_VERSION,
        name: request.name.to_string(),
        version: request.version,
        created_at: chrono::Utc::now().to_rfc3339(),
        app_min: request.app_min.to_string(),
        app_max: request.app_max.map(str::to_string),
        sources,
        items: packed,
        licences,
        curriculum,
        calibration,
    };

    let mut document = PackDocument {
        payload,
        content_hash: String::new(),
        signature: String::new(),
        signer: String::new(),
    };
    document.content_hash = document.computed_content_hash();

    // The domain type owns the signing payload, so the pack identity is signed in
    // exactly the form the rest of VECTOR signs and verifies.
    let identity = ContentPack::new(request.name, &document.content_hash)
        .map_err(|error| anyhow::anyhow!("cannot sign the pack identity: {error}"))?;
    let mut identity = identity;
    identity.set_version(u32::try_from(request.version).unwrap_or(1));
    let signed = identity
        .sign(key)
        .map_err(|error| anyhow::anyhow!("cannot sign the pack identity: {error}"))?;
    document.signature = to_hex(signed.signature().unwrap_or_default());
    document.signer = to_hex(signed.signer().unwrap_or_default());

    Ok(document)
}

/// Serialize a pack for writing to a file.
pub fn pack_bytes(document: &PackDocument) -> anyhow::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Parse a pack file.
pub fn parse_pack(bytes: &[u8]) -> Result<PackDocument, PackRefusal> {
    serde_json::from_slice(bytes).map_err(|error| PackRefusal::Malformed(error.to_string()))
}

/// Verify a pack against everything installation requires.
///
/// `trusted_signer` is the public key this installation accepts. A pack signed by any
/// other key is refused: a valid signature only proves *someone* attested the pack,
/// and an installation that accepted any signer would accept a pack signed by its own
/// author.
pub fn verify_pack(
    document: &PackDocument,
    trusted_signer: &[u8],
    running_version: &str,
) -> Result<(), PackRefusal> {
    if document.format() != PACK_FORMAT {
        return Err(PackRefusal::UnsupportedFormat {
            found: document.format().to_string(),
        });
    }
    if document.schema_version() != PACK_SCHEMA_VERSION {
        return Err(PackRefusal::UnsupportedSchema {
            found: document.schema_version(),
            supported: PACK_SCHEMA_VERSION,
        });
    }

    // Compatibility is checked before cryptography because it is the check a caller
    // can act on, and a version mismatch is not an attack.
    if !(document.app_min()..=document.app_max().unwrap_or("\u{10ffff}")).contains(&running_version)
    {
        return Err(PackRefusal::IncompatibleApp {
            pack_requires: match document.app_max() {
                Some(max) => format!("{} to {max}", document.app_min()),
                None => format!("{} or newer", document.app_min()),
            },
            running: running_version.to_string(),
        });
    }

    let computed = document.computed_content_hash();
    if computed != document.content_hash {
        return Err(PackRefusal::ContentHashMismatch {
            recorded: document.content_hash.clone(),
            computed,
        });
    }

    if document.signature.is_empty() || document.signer.is_empty() {
        return Err(PackRefusal::NotSigned);
    }
    let signature = from_hex(&document.signature).ok_or(PackRefusal::NotSigned)?;
    let signer = from_hex(&document.signer).ok_or(PackRefusal::NotSigned)?;
    if signer != trusted_signer {
        return Err(PackRefusal::UntrustedSigner {
            found: document.signer.clone(),
        });
    }
    let identity = ContentPack::from_signed(
        document.name(),
        u32::try_from(document.version()).unwrap_or(1),
        &document.content_hash,
        signature,
        signer,
    )
    .map_err(|error| PackRefusal::InvalidSignature(error.to_string()))?;
    identity
        .verify()
        .map_err(|error| PackRefusal::InvalidSignature(error.to_string()))?;

    if document.items().is_empty() {
        return Err(PackRefusal::NoItems);
    }

    // The prohibited-source scan, over the ledger rather than the licences array, so
    // a pack cannot pass by listing a permitted licence it does not use.
    let ledger: HashSet<&str> = document.sources().iter().map(|s| s.id.as_str()).collect();
    for source in document.sources() {
        if !PERMITTED_LICENCES.contains(&source.licence.as_str()) {
            return Err(PackRefusal::SourceNotPermitted {
                source: source.id.clone(),
                licence: source.licence.clone(),
            });
        }
        if source.content_hash.trim().is_empty() || source.url.trim().is_empty() {
            return Err(PackRefusal::MissingProvenance {
                item: source.id.clone(),
                detail: "the source records no URL or content hash".to_string(),
            });
        }
    }

    let mut seen_hashes: HashSet<&str> = HashSet::new();
    for item in document.items() {
        if item.sources.is_empty() {
            return Err(PackRefusal::MissingProvenance {
                item: item.id.clone(),
                detail: "the item cites no source".to_string(),
            });
        }
        for cited in &item.sources {
            if !ledger.contains(cited.as_str()) {
                return Err(PackRefusal::MissingProvenance {
                    item: item.id.clone(),
                    detail: format!("it cites {cited}, which the ledger does not carry"),
                });
            }
        }
        if item.reviewer.trim().is_empty() {
            return Err(PackRefusal::UnreviewedItem {
                item: item.id.clone(),
            });
        }
        if item.options.len() < 2 {
            return Err(PackRefusal::AnswerInconsistent {
                item: item.id.clone(),
                detail: format!("{} option(s)", item.options.len()),
            });
        }
        if item.correct_index >= item.options.len() {
            return Err(PackRefusal::AnswerInconsistent {
                item: item.id.clone(),
                detail: format!(
                    "correct_index {} is outside {} options",
                    item.correct_index,
                    item.options.len()
                ),
            });
        }
        if item.stem.trim().is_empty() {
            return Err(PackRefusal::AnswerInconsistent {
                item: item.id.clone(),
                detail: "the stem is blank".to_string(),
            });
        }
        for key in item.distractor_rationales.keys() {
            match key.parse::<usize>() {
                Ok(index) if index < item.options.len() && index != item.correct_index => {}
                _ => {
                    return Err(PackRefusal::AnswerInconsistent {
                        item: item.id.clone(),
                        detail: format!(
                            "a distractor rationale is keyed by {key:?}, which is not a wrong \
                             option's index"
                        ),
                    })
                }
            }
        }
        if item.subtest == "PC" && item.passage.as_deref().unwrap_or("").trim().is_empty() {
            return Err(PackRefusal::AnswerInconsistent {
                item: item.id.clone(),
                detail: "a comprehension item carries no passage".to_string(),
            });
        }
        if COMPUTABLE_SUBTESTS.contains(&item.subtest.as_str()) && item.proof_kind != "executable" {
            return Err(PackRefusal::WrongProofKind {
                item: item.id.clone(),
                subtest: item.subtest.clone(),
            });
        }
        // An item whose generator and verifier agree was not independently verified.
        if let (Some(generator), Some(verifier)) = (&item.generator_hash, &item.verifier_hash) {
            if generator == verifier {
                return Err(PackRefusal::AnswerInconsistent {
                    item: item.id.clone(),
                    detail: "the generator and verifier recorded the same hash".to_string(),
                });
            }
        } else {
            return Err(PackRefusal::AnswerInconsistent {
                item: item.id.clone(),
                detail: "the item records no generator and verifier hashes".to_string(),
            });
        }
        if !seen_hashes.insert(item.content_hash.as_str()) {
            return Err(PackRefusal::DuplicateContent {
                hash: item.content_hash.clone(),
            });
        }
    }

    verify_curriculum(document)?;
    Ok(())
}

/// Whether the pack's curriculum graph and calibration agree with its items.
///
/// Three relations, each of which a pack full of real questions can still get wrong:
///
/// * **every item's objective is a node**, with the node's subtest matching the item's. An
///   item that teaches an objective the graph never declared is content a study planner cannot
///   place, which is the reason for having a graph at all;
/// * **the prerequisites form a graph, not a wish list**: each names a node that exists, and
///   the relation is acyclic. A cycle means no order satisfies the curriculum, and a planner
///   that trusted it would either loop or silently pick one;
/// * **every objective with items carries exactly one calibration entry**, in range. An
///   objective with items and no estimate is a claim about difficulty that was never made.
fn verify_curriculum(document: &PackDocument) -> Result<(), PackRefusal> {
    let by_objective: BTreeMap<&str, &CurriculumNode> = document
        .payload
        .curriculum
        .iter()
        .map(|node| (node.objective_id.as_str(), node))
        .collect();

    let mut taught: BTreeSet<&str> = BTreeSet::new();
    for item in &document.payload.items {
        let Some(node) = by_objective.get(item.objective_id.as_str()) else {
            return Err(PackRefusal::ObjectiveNotInCurriculum {
                item: item.id.clone(),
                objective_id: item.objective_id.clone(),
            });
        };
        if node.subtest != item.subtest {
            return Err(PackRefusal::ObjectiveSubtestMismatch {
                objective_id: item.objective_id.clone(),
                curriculum: node.subtest.clone(),
                item: item.subtest.clone(),
            });
        }
        taught.insert(item.objective_id.as_str());
    }

    for node in &document.payload.curriculum {
        for prerequisite in &node.prerequisites {
            if !by_objective.contains_key(prerequisite.as_str()) {
                return Err(PackRefusal::UnknownPrerequisite {
                    objective_id: node.objective_id.clone(),
                    prerequisite: prerequisite.clone(),
                });
            }
        }
    }

    // Depth-first from every node: a prerequisite chain that returns to where it started is a
    // cycle, and naming the node it closed on is what makes the refusal actionable.
    let mut settled: BTreeSet<&str> = BTreeSet::new();
    for start in by_objective.keys().copied() {
        let mut path: Vec<&str> = Vec::new();
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        let mut visiting: BTreeSet<&str> = BTreeSet::new();
        while let Some((node_id, next)) = stack.pop() {
            if next == 0 {
                if settled.contains(node_id) {
                    continue;
                }
                if !visiting.insert(node_id) {
                    return Err(PackRefusal::CurriculumCycle {
                        objective_id: node_id.to_string(),
                    });
                }
                path.push(node_id);
            }
            let prerequisites = by_objective
                .get(node_id)
                .map(|node| node.prerequisites.as_slice())
                .unwrap_or(&[]);
            if next < prerequisites.len() {
                stack.push((node_id, next + 1));
                let prerequisite = prerequisites[next].as_str();
                if visiting.contains(prerequisite) {
                    return Err(PackRefusal::CurriculumCycle {
                        objective_id: prerequisite.to_string(),
                    });
                }
                stack.push((prerequisite, 0));
                continue;
            }
            visiting.remove(node_id);
            settled.insert(node_id);
            path.pop();
        }
    }

    let mut calibrated: BTreeSet<&str> = BTreeSet::new();
    for entry in &document.payload.calibration {
        if !by_objective.contains_key(entry.objective_id.as_str()) {
            return Err(PackRefusal::CalibrationUnknownObjective {
                objective_id: entry.objective_id.clone(),
            });
        }
        if !(0.0..=1.0).contains(&entry.expected_correct) {
            return Err(PackRefusal::CalibrationOutOfRange {
                objective_id: entry.objective_id.clone(),
                expected_correct: entry.expected_correct,
            });
        }
        if !calibrated.insert(entry.objective_id.as_str()) {
            return Err(PackRefusal::CalibrationUnknownObjective {
                objective_id: entry.objective_id.clone(),
            });
        }
    }
    for objective_id in &taught {
        if !calibrated.contains(objective_id) {
            return Err(PackRefusal::CalibrationMissing {
                objective_id: (*objective_id).to_string(),
            });
        }
    }

    Ok(())
}

/// What an installation did.
///
/// Serializable because it is what the command boundary returns: the interface reports
/// what an install delivered, and a caller reading a report that could not cross the
/// boundary would have to re-read the store to learn the same thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallReport {
    pub name: String,
    pub version: i64,
    pub content_hash: String,
    pub items: usize,
    pub installed: usize,
    pub already_present: usize,
    pub sources_added: usize,
    pub sources_reused: usize,
}

/// Verify a pack and install it. Nothing is written unless every check passes.
pub fn install_pack(
    db: &Database,
    bytes: &[u8],
    trusted_signer: &[u8],
    running_version: &str,
) -> anyhow::Result<InstallReport> {
    let document = parse_pack(bytes).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    verify_pack(&document, trusted_signer, running_version)
        .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;

    let manifest_json = serde_json::to_string(&document.payload)?;
    let record = NewPackRecord {
        name: document.name(),
        version: document.version(),
        signature: &document.signature,
        signer: &document.signer,
        content_hash: &document.content_hash,
        schema_version: document.schema_version(),
        manifest_json: &manifest_json,
        item_count: document.items().len() as i64,
    };
    let sources: Vec<PackSourceRecord<'_>> = document
        .sources()
        .iter()
        .map(|source| PackSourceRecord {
            id: &source.id,
            url: &source.url,
            title: &source.title,
            content_hash: &source.content_hash,
            licence: &source.licence,
            effective_date: &source.effective_date,
            trust: source.trust,
            retrieval_status: &source.retrieval_status,
        })
        .collect();
    // Keys were verified as option indices before this point, so the conversion
    // cannot drop a rationale silently.
    let mut rationales: Vec<BTreeMap<usize, String>> = Vec::with_capacity(document.items().len());
    for item in document.items() {
        let mut map = BTreeMap::new();
        for (key, why) in &item.distractor_rationales {
            let index: usize = key
                .parse()
                .map_err(|_| anyhow::anyhow!("rationale key {key:?} is not an option index"))?;
            map.insert(index, why.clone());
        }
        rationales.push(map);
    }

    let items: Vec<NewPackItem<'_>> = document
        .items()
        .iter()
        .zip(rationales.iter())
        .map(|(item, rationales)| NewPackItem {
            id: &item.id,
            subtest: &item.subtest,
            objective_id: &item.objective_id,
            stem: &item.stem,
            passage: item.passage.as_deref(),
            options: &item.options,
            correct_index: item.correct_index,
            explanation: &item.explanation,
            distractor_rationales: rationales,
            difficulty: item.difficulty,
            proof_kind: &item.proof_kind,
            proof_json: &item.proof_json,
            content_hash: &item.content_hash,
            generator_hash: item.generator_hash.as_deref(),
            verifier_hash: item.verifier_hash.as_deref(),
            reviewer: &item.reviewer,
            source_ids: item.sources.clone(),
        })
        .collect();

    let outcome: InstallOutcome = ContentPackRepo::new(db).install(&record, &sources, &items)?;

    Ok(InstallReport {
        name: document.name().to_string(),
        version: document.version(),
        content_hash: document.content_hash.clone(),
        items: document.items().len(),
        installed: outcome.installed,
        already_present: outcome.already_present,
        sources_added: outcome.added_sources,
        sources_reused: outcome.reused_sources,
    })
}

/// One objective a pack teaches, as the content manager lists it.
///
/// The curriculum travels inside the pack's manifest, so this is what the interface can show
/// without unpacking it: what the objective is, which subtest it belongs to, what has to come
/// first, and what the pack claims about its difficulty -- carrying `responses` beside the
/// estimate so a reader can tell a measured figure from a declared one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackObjectiveDto {
    pub objective_id: String,
    pub subtest: String,
    pub title: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// The declared share expected to answer correctly, when the pack calibrates this
    /// objective.
    pub expected_correct: Option<f64>,
    /// How many responses the estimate rests on. Zero means it is a declaration.
    pub responses: Option<u64>,
    pub basis: Option<String>,
}

/// A pack as the interface lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackDto {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub signer: String,
    pub content_hash: String,
    pub schema_version: i64,
    pub item_count: i64,
    pub created_at: String,
    /// Whether the signature still verifies against the key that signed it. A pack
    /// whose stored identity no longer verifies is one a reader should not trust.
    pub signature_valid: bool,
    /// What the pack teaches, read out of the manifest it was installed with.
    ///
    /// Empty for a pack built before the manifest carried a curriculum -- the field is
    /// `#[serde(default)]` so those packs still parse, and the interface says "declares no
    /// curriculum" rather than showing an empty list as if it were one.
    #[serde(default)]
    pub objectives: Vec<PackObjectiveDto>,
}

impl From<ContentPackRecord> for InstalledPackDto {
    fn from(record: ContentPackRecord) -> Self {
        let signature_valid = from_hex(&record.signature)
            .zip(from_hex(&record.signer))
            .and_then(|(signature, signer)| {
                ContentPack::from_signed(
                    &record.name,
                    u32::try_from(record.version).unwrap_or(1),
                    &record.content_hash,
                    signature,
                    signer,
                )
                .ok()
            })
            .map(|pack| pack.verify().is_ok())
            .unwrap_or(false);
        let objectives = objectives_from_manifest(&record.manifest_json);
        Self {
            id: record.id,
            name: record.name,
            version: record.version,
            status: record.status,
            signer: record.signer,
            content_hash: record.content_hash,
            schema_version: record.schema_version,
            item_count: record.item_count,
            created_at: record.created_at,
            signature_valid,
            objectives,
        }
    }
}

/// Read the objectives a stored manifest declares, with their calibration.
///
/// The registry stores the pack's *payload* -- the bytes the signature covers, without the
/// signature itself -- so that is what this parses. A manifest that does not parse, or one
/// written before the curriculum existed, yields an empty list: the pack registry is not the
/// place to fail a read because a pack is older, and the interface distinguishes "declares
/// nothing" from "cannot be read" by the pack's own schema version.
fn objectives_from_manifest(manifest_json: &str) -> Vec<PackObjectiveDto> {
    let Ok(payload) = serde_json::from_str::<PackPayload>(manifest_json) else {
        return Vec::new();
    };
    let calibration: BTreeMap<&str, &CalibrationEntry> = payload
        .calibration
        .iter()
        .map(|entry| (entry.objective_id.as_str(), entry))
        .collect();
    let mut objectives: Vec<PackObjectiveDto> = payload
        .curriculum
        .iter()
        .map(|node| {
            let entry = calibration.get(node.objective_id.as_str());
            PackObjectiveDto {
                objective_id: node.objective_id.clone(),
                subtest: node.subtest.clone(),
                title: node.title.clone(),
                prerequisites: node.prerequisites.clone(),
                expected_correct: entry.map(|entry| entry.expected_correct),
                responses: entry.map(|entry| entry.responses),
                basis: entry.map(|entry| entry.basis.clone()),
            }
        })
        .collect();
    objectives.sort_by(|a, b| a.objective_id.cmp(&b.objective_id));
    objectives
}

/// Every pack the registry holds.
pub fn installed_packs(db: &Database) -> anyhow::Result<Vec<InstalledPackDto>> {
    Ok(ContentPackRepo::new(db)
        .list()?
        .into_iter()
        .map(InstalledPackDto::from)
        .collect())
}

/// Roll back to the previous version of a pack.
pub fn rollback_pack(db: &Database, name: &str) -> anyhow::Result<InstalledPackDto> {
    Ok(ContentPackRepo::new(db).rollback(name)?.into())
}

/// Put a registered pack version back into service.
///
/// The companion to `rollback_pack`. `install_pack` will not do this -- by design, since a
/// reinstall must not silently undo a deliberate rollback -- so a transition back to a version
/// the installation already holds is its own command, and the caller has to mean it.
pub fn activate_pack(db: &Database, name: &str, version: i64) -> anyhow::Result<InstalledPackDto> {
    Ok(ContentPackRepo::new(db).activate(name, version)?.into())
}

/// Test-only field access.
///
/// Half the refusals this module performs can only be proven by producing a pack
/// whose stored fields disagree with its signature: tampering at rest, a schema
/// version this build does not implement, an item that cites a source the ledger
/// lacks. Reaching those states through the public API would mean adding setters
/// whose only purpose is to create invalid packs, which is an invitation to misuse
/// them. Following `vector_domain::content::ContentPack`, the hooks are compiled only
/// under `cfg(test)`, so production code cannot reach them and the tests that need
/// them live in this file.
#[cfg(test)]
impl PackDocument {
    /// Recompute the content hash and re-sign, as a pack author would.
    ///
    /// Two different failures are worth distinguishing, and the tests have to reach
    /// both. A file edited at rest breaks the hash, and that is what the hash is for.
    /// A pack that a signer *did* attest while it violates a rule is the other case:
    /// a signature proves who attested the content and never that the content obeys
    /// the corpus's rules. Reaching the second case needs a pack that is internally
    /// consistent -- correct hash, valid signature -- and still wrong, which is what
    /// this produces.
    fn reseal_for_test(&mut self, key: &SigningKey) {
        self.content_hash = self.computed_content_hash();
        let mut identity = ContentPack::new(&self.payload.name, &self.content_hash)
            .expect("the pack has a content hash");
        identity.set_version(u32::try_from(self.payload.version).unwrap_or(1));
        let signed = identity.sign(key).expect("signing succeeds");
        self.signature = to_hex(signed.signature().unwrap_or_default());
        self.signer = to_hex(signed.signer().unwrap_or_default());
    }

    fn set_schema_version_for_test(&mut self, version: i64) {
        self.payload.schema_version = version;
    }

    fn set_curriculum_for_test(&mut self, nodes: Vec<CurriculumNode>) {
        self.payload.curriculum = nodes;
    }

    fn set_calibration_for_test(&mut self, entries: Vec<CalibrationEntry>) {
        self.payload.calibration = entries;
    }

    fn set_item_sources_for_test(&mut self, index: usize, sources: Vec<String>) {
        self.payload.items[index].sources = sources;
    }

    fn set_item_reviewer_for_test(&mut self, index: usize, reviewer: &str) {
        self.payload.items[index].reviewer = reviewer.to_string();
    }

    fn set_item_correct_index_for_test(&mut self, index: usize, correct_index: usize) {
        self.payload.items[index].correct_index = correct_index;
    }

    fn set_item_subtest_for_test(&mut self, index: usize, subtest: &str) {
        self.payload.items[index].subtest = subtest.to_string();
    }

    fn set_item_proof_kind_for_test(&mut self, index: usize, proof_kind: &str) {
        self.payload.items[index].proof_kind = proof_kind.to_string();
    }

    fn set_item_verifier_hash_for_test(&mut self, index: usize, verifier_hash: Option<String>) {
        self.payload.items[index].verifier_hash = verifier_hash;
    }

    fn set_item_content_hash_for_test(&mut self, index: usize, content_hash: String) {
        self.payload.items[index].content_hash = content_hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ContentPipeline, GenerateRequest};
    use vector_persistence::repo::{EvidenceRepo, NewEvidence};
    use vector_persistence::{Migration, MigrationManager};

    const RUNNING: &str = "0.1.0";

    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new(tag: &str) -> Self {
                let mut path = std::env::temp_dir();
                let unique = format!(
                    "vector-packs-unit-{}-{}-{}-{}",
                    tag,
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_nanos(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                );
                path.push(unique);
                std::fs::create_dir_all(&path).expect("create temp dir");
                Self { path }
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn migrations() -> Vec<Migration> {
        MigrationManager::load_from_dir(std::path::Path::new("../../migrations"))
            .expect("migrations load")
    }

    fn store(tag: &str) -> (tempdir::TempDir, Database) {
        let dir = tempdir::TempDir::new(tag);
        let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
        MigrationManager::apply(&mut db, &migrations()).expect("migrate");
        let source = EvidenceRepo::new(&db)
            .put(&NewEvidence::new(
                "https://www.officialasvab.com/applicants/sample-questions/",
                "ASVAB subtest constructs (facts only)",
                "sha256:pack-unit-source",
                "Public domain in the USA",
                "2026-09-22",
                0.9,
                "retrieved",
            ))
            .expect("record source");
        ContentPipeline::new(&db)
            .generate_and_activate(&GenerateRequest {
                subtest: "AR",
                count: 4,
                seed: 20_260_922,
                reviewer: "content-reviewer",
                source_id: &source,
                generator: "factory",
            })
            .expect("generate");
        (dir, db)
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7_u8; 32])
    }

    fn trusted() -> Vec<u8> {
        key().verifying_key().to_bytes().to_vec()
    }

    fn good(tag: &str) -> (tempdir::TempDir, PackDocument) {
        let (dir, db) = store(tag);
        let document = build_pack(
            &db,
            &BuildPackRequest {
                name: "core-asvab",
                version: 1,
                app_min: "0.1.0",
                app_max: None,
                curriculum: Vec::new(),
                calibration: Vec::new(),
            },
            &key(),
        )
        .expect("build");
        (dir, document)
    }

    #[test]
    fn a_freshly_built_pack_verifies() {
        let (_dir, document) = good("unit-fresh");
        verify_pack(&document, &trusted(), RUNNING).expect("a fresh pack must verify");
    }

    /// The graph and the calibration a pack was built with are the ones it carries.
    ///
    /// The graph has to cover every objective the items teach -- a curriculum that declares
    /// only some of them is refused, which is the point of declaring one -- so the nodes are
    /// derived from what the store built and then given titles, prerequisites and calibration
    /// of their own.
    #[test]
    fn a_pack_carries_the_curriculum_and_calibration_it_was_built_with() {
        let (dir, db) = store("unit-curriculum");
        let base = build_pack(
            &db,
            &BuildPackRequest {
                name: "core-asvab",
                version: 1,
                app_min: "0.1.0",
                app_max: None,
                curriculum: Vec::new(),
                calibration: Vec::new(),
            },
            &key(),
        )
        .expect("build");
        let mut objectives: Vec<(String, String)> = base
            .curriculum()
            .iter()
            .map(|node| (node.objective_id.clone(), node.subtest.clone()))
            .collect();
        objectives.sort();
        assert!(
            objectives.len() >= 2,
            "the store should build several objectives"
        );

        // A chain: each objective requires the one before it, which is the shape a study
        // planner reads.
        let nodes: Vec<CurriculumNode> = objectives
            .iter()
            .enumerate()
            .map(|(index, (objective_id, subtest))| CurriculumNode {
                objective_id: objective_id.clone(),
                subtest: subtest.clone(),
                title: format!("Objective {objective_id}"),
                prerequisites: if index == 0 {
                    Vec::new()
                } else {
                    vec![objectives[index - 1].0.clone()]
                },
            })
            .collect();
        let entries: Vec<CalibrationEntry> = objectives
            .iter()
            .enumerate()
            .map(|(index, (objective_id, _))| CalibrationEntry {
                objective_id: objective_id.clone(),
                expected_correct: if index == 0 { 0.62 } else { 0.5 },
                responses: if index == 0 { 1840 } else { 0 },
                basis: if index == 0 {
                    "trial of 1,840 responses from the 2026 field study".to_string()
                } else {
                    "declared; no responses recorded".to_string()
                },
            })
            .collect();

        let document = build_pack(
            &db,
            &BuildPackRequest {
                name: "core-asvab",
                version: 1,
                app_min: "0.1.0",
                app_max: None,
                curriculum: nodes.clone(),
                calibration: entries.clone(),
            },
            &key(),
        )
        .expect("build");
        assert_eq!(document.curriculum(), nodes.as_slice());
        assert_eq!(document.calibration(), entries.as_slice());
        verify_pack(&document, &trusted(), RUNNING).expect("a pack with a curriculum verifies");

        // It survives the round trip through the bytes, which is how it reaches a reviewer.
        let bytes = pack_bytes(&document).expect("serialize");
        let parsed = parse_pack(&bytes).expect("parse");
        assert_eq!(parsed.curriculum(), nodes.as_slice());
        assert_eq!(parsed.calibration(), entries.as_slice());
        let _ = dir;
    }

    /// A pack built without a curriculum still declares one: a node per objective it teaches.
    #[test]
    fn a_pack_built_without_a_curriculum_declares_what_it_teaches() {
        let (_dir, document) = good("unit-derived-curriculum");
        assert!(!document.curriculum().is_empty());
        for item in document.items() {
            assert!(
                document
                    .curriculum()
                    .iter()
                    .any(|node| node.objective_id == item.objective_id
                        && node.subtest == item.subtest),
                "the derived graph must carry every objective the items use"
            );
        }
        // And every taught objective carries a calibration entry, which says plainly that it
        // rests on no responses.
        for node in document.curriculum() {
            let entry = document
                .calibration()
                .iter()
                .find(|entry| entry.objective_id == node.objective_id)
                .expect("every node is calibrated");
            assert_eq!(entry.responses, 0);
            assert!((0.0..=1.0).contains(&entry.expected_correct));
        }
    }

    #[test]
    fn a_pack_whose_item_teaches_an_undeclared_objective_is_refused() {
        let (_dir, mut document) = good("unit-undeclared-objective");
        let graph = document
            .curriculum()
            .iter()
            .filter(|node| node.objective_id != document.items()[0].objective_id)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            !graph.is_empty(),
            "the store builds more than one objective"
        );
        document.set_curriculum_for_test(graph);
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::ObjectiveNotInCurriculum { .. }) => {}
            other => panic!("expected an undeclared-objective refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_prerequisites_are_unknown_is_refused() {
        let (_dir, mut document) = good("unit-unknown-prerequisite");
        let mut graph = document.curriculum().to_vec();
        graph[0].prerequisites = vec!["OBJ-NOBODY-KNOWS-01".to_string()];
        document.set_curriculum_for_test(graph);
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::UnknownPrerequisite { .. }) => {}
            other => panic!("expected an unknown-prerequisite refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_prerequisites_form_a_cycle_is_refused() {
        let (_dir, mut document) = good("unit-curriculum-cycle");
        let mut graph = document.curriculum().to_vec();
        assert!(
            graph.len() >= 2,
            "a cycle needs two nodes, and the store builds several objectives"
        );
        // Two nodes that each require the other: no learning order satisfies this.
        graph[0].prerequisites = vec![graph[1].objective_id.clone()];
        graph[1].prerequisites = vec![graph[0].objective_id.clone()];
        document.set_curriculum_for_test(graph);
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::CurriculumCycle { .. }) => {}
            other => panic!("expected a cycle refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_objective_has_no_calibration_is_refused() {
        let (_dir, mut document) = good("unit-calibration-missing");
        let kept = document.calibration()[0].objective_id.clone();
        document.set_calibration_for_test(vec![CalibrationEntry {
            objective_id: kept,
            expected_correct: 0.5,
            responses: 10,
            basis: "one entry, so the others are missing".to_string(),
        }]);
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::CalibrationMissing { .. }) => {}
            other => panic!("expected a missing-calibration refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_calibration_is_out_of_range_is_refused() {
        let (_dir, mut document) = good("unit-calibration-range");
        let mut entries = document.calibration().to_vec();
        entries[0].expected_correct = 1.4;
        document.set_calibration_for_test(entries);
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::CalibrationOutOfRange { .. }) => {}
            other => panic!("expected an out-of-range refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_contents_changed_is_refused() {
        let (_dir, mut document) = good("unit-tamper");
        document.payload.items[0].stem = "A question nobody attested.".to_string();
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::ContentHashMismatch { .. }) => {}
            other => panic!("expected a content-hash refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_signature_was_swapped_is_refused() {
        let (_dir, mut document) = good("unit-swap");
        let (_other_dir, other) = good("unit-swap-other");
        document.signature = other.signature.clone();
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::InvalidSignature(_)) => {}
            other => panic!("expected an invalid-signature refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_with_no_signature_is_refused() {
        let (_dir, mut document) = good("unit-unsigned");
        document.signature.clear();
        document.signer.clear();
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::NotSigned) => {}
            other => panic!("expected an unsigned refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_for_another_schema_version_is_refused() {
        let (_dir, mut document) = good("unit-schema");
        document.set_schema_version_for_test(99);
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::UnsupportedSchema { found: 99, .. }) => {}
            other => panic!("expected a schema refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_item_cites_a_source_the_ledger_lacks_is_refused() {
        let (_dir, mut document) = good("unit-provenance");
        document.set_item_sources_for_test(0, vec!["ev-invented".to_string()]);
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::MissingProvenance { detail, .. }) => {
                assert!(detail.contains("ev-invented"), "{detail}")
            }
            other => panic!("expected a provenance refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_item_cites_nothing_is_refused() {
        let (_dir, mut document) = good("unit-no-source");
        document.set_item_sources_for_test(0, Vec::new());
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::MissingProvenance { detail, .. }) => {
                assert!(detail.contains("cites no source"), "{detail}")
            }
            other => panic!("expected a provenance refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_item_names_no_reviewer_is_refused() {
        let (_dir, mut document) = good("unit-unreviewed");
        document.set_item_reviewer_for_test(0, "");
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::UnreviewedItem { .. }) => {}
            other => panic!("expected an unreviewed-item refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_item_has_no_unique_answer_is_refused() {
        let (_dir, mut document) = good("unit-answers");
        document.set_item_correct_index_for_test(0, 99);
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::AnswerInconsistent { detail, .. }) => {
                assert!(detail.contains("outside"), "{detail}")
            }
            other => panic!("expected an answer refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_comprehension_item_has_no_passage_is_refused() {
        let (_dir, mut document) = good("unit-pc");
        document.set_item_subtest_for_test(0, "PC");
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::AnswerInconsistent { detail, .. }) => {
                assert!(detail.contains("passage"), "{detail}")
            }
            other => panic!("expected a passage refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_computable_item_carries_a_rubric_is_refused() {
        let (_dir, mut document) = good("unit-proofkind");
        document.set_item_proof_kind_for_test(0, "source_backed");
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::WrongProofKind { subtest, .. }) => assert_eq!(subtest, "AR"),
            other => panic!("expected a proof-kind refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_whose_generator_and_verifier_agree_is_refused() {
        let (_dir, mut document) = good("unit-hashes");
        let recorded = document.items()[0].generator_hash.clone();
        document.set_item_verifier_hash_for_test(0, recorded);
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::AnswerInconsistent { detail, .. }) => {
                assert!(detail.contains("same hash"), "{detail}")
            }
            other => panic!("expected a verification refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_with_two_items_of_the_same_content_is_refused() {
        let (_dir, mut document) = good("unit-duplicate");
        let first = document.items()[0].content_hash.clone();
        document.set_item_content_hash_for_test(1, first);
        // Signed by the pack's own key: a valid signature is not a review, and the
        // semantic rules have to refuse this on their own.
        document.reseal_for_test(&key());
        match verify_pack(&document, &trusted(), RUNNING) {
            Err(PackRefusal::DuplicateContent { .. }) => {}
            other => panic!("expected a duplicate-content refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_file_that_is_not_a_pack_is_refused_rather_than_guessed_at() {
        match parse_pack(b"{\"not\": \"a pack\"}") {
            Err(PackRefusal::Malformed(_)) => {}
            other => panic!("expected a malformed refusal, got {other:?}"),
        }
    }

    #[test]
    fn the_hex_helpers_round_trip_and_refuse_odd_input() {
        let bytes = vec![0_u8, 1, 15, 16, 255];
        assert_eq!(to_hex(&bytes), "00010f10ff");
        assert_eq!(from_hex("00010f10ff"), Some(bytes));
        assert_eq!(from_hex("abc"), None, "odd length is not a byte string");
        assert_eq!(from_hex("zz"), None, "non-hex is not a byte string");
    }
}
