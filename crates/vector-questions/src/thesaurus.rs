//! Word Knowledge items from a public-domain thesaurus.
//!
//! Requirement: REQ-022 (original item + independent verification), REQ-056
//! (per-item provenance). Subtest: WK, "ability to identify the best synonym for
//! a given word".
//!
//! ## Why this subtest first
//!
//! WK allows **36 seconds per question** and is one of the four subtests that
//! determine the AFQT score, so it is the highest-leverage target in the product
//! and the cheapest to source lawfully: a word and its synonyms are facts about
//! the language, not authored expression.
//!
//! ## Source
//!
//! Moby Thesaurus II by Grady Ward, **public domain in the USA** (Project
//! Gutenberg eBook #3202): 30,259 root words, comma-separated ASCII, one root
//! word followed by its associated terms per line. Public domain matters here
//! beyond convenience: Moby carries no copyleft and no attribution obligation, so
//! it can be used in a commercial product without encumbering it. The GCIDE
//! dictionary, by contrast, is derived from the same 1913 Webster's but is GPL,
//! which is why it is not the source.
//!
//! ## What "verified" means for a verbal item
//!
//! AR and MK items are proved by arithmetic: the item carries an expression a
//! machine re-evaluates. A synonym item has no arithmetic, but it does have
//! ground truth — the source itself. [`verify`] re-derives two facts from the
//! thesaurus rather than trusting the builder:
//!
//! * the option marked correct really is listed with the headword, and
//! * no distractor is listed with the headword anywhere in the source.
//!
//! The second is the important one. Moby is *associative* rather than strictly
//! synonymous, and its lines overlap heavily, so a word drawn from an unrelated
//! line may still be a legitimate synonym. An item whose distractor is also a
//! synonym has no single right answer, and this check is what refuses it.
//!
//! ## Known limitation, stated rather than hidden
//!
//! A distractor rationale can say only what the source supports: that a word is
//! *not* among the terms listed with the headword. Explaining *why* two words
//! differ needs definitions, which an associative thesaurus does not carry. Item
//! `explanation` therefore shows the source's own related-term list, which is
//! genuinely instructive, and the per-distractor rationale makes the narrower
//! claim. Definitional rationales need a dictionary source (Webster's 1913 is
//! public domain) and are the follow-up, not something this module pretends to
//! have.

use std::collections::{BTreeMap, HashMap};

use crate::factory::Rng;

/// One source line: a root word and the terms Moby associates with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub headword: String,
    pub related: Vec<String>,
}

/// A parsed thesaurus, with an index for the co-occurrence check.
#[derive(Debug, Clone, Default)]
pub struct Thesaurus {
    entries: Vec<Entry>,
    /// Lowercased word -> indices of the entries whose line contains it.
    lines_by_word: HashMap<String, Vec<u32>>,
    /// Lowercased headword -> its entry index.
    ///
    /// Without this, resolving a headword is a linear scan of every entry with a
    /// `to_lowercase` allocation per comparison. The mutuality filter performs
    /// two such lookups per candidate and the builder probes hundreds of
    /// candidates per item, so the un-indexed version turned a 150 ms ingest into
    /// one that did not finish in ten minutes.
    entry_index: HashMap<String, u32>,
    /// Every word in the source, deduplicated, for distractor selection.
    vocabulary: Vec<String>,
}

/// Case- and whitespace-insensitive equality without allocating.
fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// How many source lines a distractor must appear on to be offered.
///
/// ## What this achieves, and what it does not
///
/// Measured against the real corpus. At 1, distractors included `mammiform`,
/// `naif`, `nympha` and `untremulous`; at 6 those are gone, so this removes the
/// extreme outliers.
///
/// It does **not** make the distractors plausible, and the reason is a property
/// of the source rather than of the number: a word's line count measures how many
/// *senses* Moby associates it with, which is polysemy, not commonness. It does
/// not separate `merely` from `unendowed`.
///
/// A second hypothesis was tested and disproven, recorded so nobody repeats it:
/// requiring the dictionary to define a distractor does not help either. 21% of
/// distractors are undefined, but so are some *correct* answers (`pep`), and the
/// defined ones include `whelk` and `becharm`. Webster's attests a word; it does
/// not say the word is ordinary.
///
/// The real fix is a word-frequency source, which neither of these supplies. See
/// the module documentation.
/// The default for [`Thesaurus::build_items_filtered`].
///
/// Public because it is a policy the caller may need to override: it is
/// calibrated for a 30,000-root-word source, and a small or synthetic source
/// cannot supply distractors at this bar at all.
pub const MIN_DISTRACTOR_LINES: usize = 6;

/// How much longer or shorter than the answer a distractor may be.
///
/// An option conspicuously longer than the others is the one a learner
/// eliminates first, so this closes the cheapest way to answer without knowing
/// any of the words. Like the line count it removes outliers rather than
/// establishing plausibility.
const MAX_DISTRACTOR_LENGTH_GAP: usize = 4;

/// A Word Knowledge item.
#[derive(Debug, Clone, PartialEq)]
pub struct WkItem {
    pub objective_id: String,
    pub headword: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub distractor_rationales: BTreeMap<usize, String>,
    /// The source line the item rests on: the headword and its related terms.
    /// This is the item's evidence, and what the explanation shows.
    pub supporting_line: String,
    /// How many terms the source lists for the headword, before filtering.
    pub related_count: usize,
    pub difficulty: f64,
    pub seed: u64,
}

impl WkItem {
    /// Stable identity over the answerable content, excluding the seed.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str("WK");
        canonical.push('\u{1}');
        canonical.push_str(&self.headword.to_lowercase());
        canonical.push('\u{1}');
        for option in &self.options {
            canonical.push_str(&option.to_lowercase());
            canonical.push('\u{2}');
        }
        canonical.push('\u{1}');
        canonical.push_str(&self.correct_index.to_string());
        crate::provenance::ContentHash::of_text(&canonical)
            .as_str()
            .to_string()
    }

    /// The explanation shown after answering: the source's own list.
    pub fn explanation(&self) -> String {
        format!(
            "The source lists these terms with \"{}\": {}",
            self.headword, self.supporting_line
        )
    }
}

/// Why a Word Knowledge item is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WkVerificationFailure {
    /// The option marked correct is not listed with the headword.
    CorrectOptionNotInSource {
        option: String,
        headword: String,
    },
    /// A distractor is listed with the headword, so it may also be a synonym.
    AmbiguousDistractor {
        option: String,
        headword: String,
    },
    TooFewOptions(usize),
    CorrectIndexOutOfRange {
        index: usize,
        options: usize,
    },
    DuplicateOption(String),
    RationaleCoverage {
        missing: Vec<usize>,
        extra: Vec<usize>,
    },
    Blank(String),
    /// The headword is not in the source at all.
    UnknownHeadword(String),
}

impl std::fmt::Display for WkVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WkVerificationFailure::CorrectOptionNotInSource { option, headword } => {
                write!(f, "the source does not list {option:?} with {headword:?}")
            }
            WkVerificationFailure::AmbiguousDistractor { option, headword } => write!(
                f,
                "{option:?} is listed with {headword:?}, so it may also be a synonym \
                 and the item would have no single right answer"
            ),
            WkVerificationFailure::TooFewOptions(n) => {
                write!(f, "an item needs at least 2 options, found {n}")
            }
            WkVerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            WkVerificationFailure::DuplicateOption(option) => {
                write!(f, "option {option:?} appears more than once")
            }
            WkVerificationFailure::RationaleCoverage { missing, extra } => write!(
                f,
                "rationales missing for {missing:?} and unexpected for {extra:?}"
            ),
            WkVerificationFailure::Blank(what) => write!(f, "{what} is blank"),
            WkVerificationFailure::UnknownHeadword(word) => {
                write!(f, "the headword {word:?} is not in the source")
            }
        }
    }
}

impl std::error::Error for WkVerificationFailure {}

/// Normalise a word for lookup: Moby mixes case and spacing.
fn key(word: &str) -> String {
    word.trim().to_lowercase()
}

/// Whether a token is usable as an option: a single plain English word.
///
/// Phrases, proper nouns and hyphenated forms are excluded. A WK item asks a
/// learner to choose between single words, and "a cappella" or "A-bomb" as an
/// option would test something other than vocabulary.
fn is_plain_word(word: &str) -> bool {
    let trimmed = word.trim();
    if trimmed.len() < 3 || trimmed.len() > 14 {
        return false;
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    // A leading capital in Moby usually marks a proper noun or an acronym.
    !trimmed.chars().next().is_some_and(char::is_uppercase)
}

/// Parse Moby's format: one root word, then its associated terms, comma
/// separated. Malformed lines are skipped rather than guessed at.
pub fn parse_moby(text: &str) -> Thesaurus {
    let mut entries: Vec<Entry> = Vec::new();
    let mut lines_by_word: HashMap<String, Vec<u32>> = HashMap::new();
    let mut entry_index: HashMap<String, u32> = HashMap::new();
    let mut vocabulary: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in text.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split(',');
        let Some(headword) = fields.next() else {
            continue;
        };
        let headword = headword.trim();
        if headword.is_empty() {
            continue;
        }
        let related: Vec<String> = fields
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect();
        if related.is_empty() {
            continue;
        }

        let index = entries.len() as u32;
        // Index every word on the line, including the headword, so a lookup asks
        // "does any line contain both words?" rather than the narrower question
        // of whether one is a headword.
        for word in std::iter::once(headword).chain(related.iter().map(String::as_str)) {
            lines_by_word.entry(key(word)).or_default().push(index);
            if is_plain_word(word) {
                let normalized = key(word);
                if seen.insert(normalized.clone()) {
                    vocabulary.push(word.trim().to_string());
                }
            }
        }
        entries.push(Entry {
            headword: headword.to_string(),
            related,
        });
        // First definition of a headword wins; the source does not repeat them,
        // and silently letting a later line overwrite an earlier one would make
        // the parse depend on line order in a way nobody would expect.
        entry_index.entry(key(headword)).or_insert(index);
    }

    vocabulary.sort();
    Thesaurus {
        entries,
        lines_by_word,
        entry_index,
        vocabulary,
    }
}

impl Thesaurus {
    /// Number of root words parsed.
    pub fn root_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of distinct single words usable as options.
    pub fn vocabulary_size(&self) -> usize {
        self.vocabulary.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether any single source line contains both words.
    ///
    /// This is the ground-truth test for "these two are associated", and what
    /// makes a synonym item checkable.
    ///
    /// The posting lists are built in ascending entry order, so they are already
    /// sorted and the intersection is a linear merge. The obvious
    /// `left.iter().any(|i| right.contains(i))` is quadratic, and the most common
    /// English words appear in thousands of lines, which turns a check that runs
    /// hundreds of times per item into the slowest thing in the pipeline.
    pub fn co_occurs(&self, a: &str, b: &str) -> bool {
        let (Some(left), Some(right)) = (
            self.lines_by_word.get(&key(a)),
            self.lines_by_word.get(&key(b)),
        ) else {
            return false;
        };
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            match left[i].cmp(&right[j]) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
            }
        }
        false
    }

    /// The terms the source lists with `headword`, in source order.
    pub fn related_to(&self, headword: &str) -> Option<Vec<String>> {
        let index = *self.entry_index.get(&key(headword))?;
        Some(self.entries[index as usize].related.clone())
    }

    /// Whether `headword`'s own line lists `word`.
    ///
    /// Narrower than [`Self::co_occurs`], which is satisfied by any shared line
    /// in either direction.
    pub fn lists(&self, headword: &str, word: &str) -> bool {
        self.entry_index.get(&key(headword)).is_some_and(|index| {
            self.entries[*index as usize]
                .related
                .iter()
                .any(|term| same(term, word))
        })
    }

    /// Whether the source treats `word` as a root word with a line of its own.
    pub fn is_root_word(&self, word: &str) -> bool {
        self.entry_index.contains_key(&key(word))
    }

    /// How many source lines mention `word`.
    ///
    /// A crude centrality measure, and the only one these sources offer. Common
    /// English words appear in many of Moby's association lines; obscure ones
    /// appear in one or two. It is what makes a distractor plausible rather than
    /// obviously wrong.
    pub fn frequency(&self, word: &str) -> usize {
        self.lines_by_word
            .get(&key(word))
            .map(|lines| lines.len())
            .unwrap_or(0)
    }

    /// Whether the source lists each word on the other's line.
    ///
    /// ## Why this filter exists, and what it is not
    ///
    /// Moby is associative rather than strictly synonymous, and its lines run to
    /// over a hundred terms. A uniform draw from such a line produces pairs that
    /// the source does support but that make poor vocabulary questions: it lists
    /// `productivity` with `moxie`, and `maudlin` with `sodden`, and neither
    /// pairing is what a learner should be taught as a synonym.
    ///
    /// Mutual listing is a signal available from the source itself. A pair that
    /// appears on both lines is far more likely to be a genuine synonym pair than
    /// a one-directional association, so the builder requires it.
    ///
    /// It is a *quality* filter, not a validity check: [`verify`] does not
    /// require mutuality, because a one-directional pair is still supported by
    /// the source and the verifier's job is to refuse items the source does not
    /// support. Tightening quality is the builder's business and is deliberately
    /// not smuggled into the verification rules.
    pub fn is_mutual(&self, a: &str, b: &str) -> bool {
        self.lists(a, b) && self.lists(b, a)
    }

    /// Build up to `count` Word Knowledge items.
    ///
    /// Deterministic from `seed`: the same seed yields the same items, so a pack
    /// can be regenerated and its hashes re-derived rather than trusted.
    pub fn build_items(&self, count: usize, seed: u64) -> Vec<WkItem> {
        self.build_items_filtered(count, seed, |_, _| true)
    }

    /// Build items, keeping only headword/synonym pairs the caller accepts.
    ///
    /// The filter is how the dictionary layer tightens quality without this
    /// module needing to know that a dictionary exists. It receives the headword
    /// and the candidate synonym; returning `false` rejects the pair.
    ///
    /// `FnMut` rather than `Fn` so a caller can accumulate statistics while
    /// filtering, which is what makes a filter's decisions assertable in a test
    /// instead of merely observable through which items appear.
    pub fn build_items_filtered<F>(&self, count: usize, seed: u64, accept: F) -> Vec<WkItem>
    where
        F: FnMut(&str, &str) -> bool,
    {
        self.build_items_configured(count, seed, MIN_DISTRACTOR_LINES, accept)
    }

    /// As [`Self::build_items_filtered`], with the distractor bar set explicitly.
    ///
    /// The threshold is a policy rather than a property of the source, so it is a
    /// parameter: a fixture with a dozen words cannot supply distractors that
    /// appear on six lines, and hard-coding the real corpus's calibration made
    /// every small-source test produce nothing.
    pub fn build_items_configured<F>(
        &self,
        count: usize,
        seed: u64,
        min_distractor_lines: usize,
        mut accept: F,
    ) -> Vec<WkItem>
    where
        F: FnMut(&str, &str) -> bool,
    {
        // An empty source is not an error here, but it must not reach the random
        // draw: `Rng::range(0, len - 1)` on zero entries is `range(0, -1)`, which
        // panics. Returning nothing lets the caller decide whether an empty
        // corpus is a refusal or a legitimate no-op.
        if self.entries.is_empty() {
            return Vec::new();
        }
        let mut rng = Rng::new(seed);
        let mut items: Vec<WkItem> = Vec::new();
        let mut seen_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Bounded: each headword is tried once, so a corpus with few usable root
        // words returns fewer items rather than spinning.
        let attempts = count.saturating_mul(8).max(64);
        for _ in 0..attempts {
            if items.len() >= count {
                break;
            }
            let Some(item) = self.build_one(&mut rng, seed, min_distractor_lines, &mut accept)
            else {
                continue;
            };
            if seen_hashes.insert(item.content_hash()) {
                items.push(item);
            }
        }
        items
    }

    fn build_one<F>(
        &self,
        rng: &mut Rng,
        seed: u64,
        min_distractor_lines: usize,
        accept: &mut F,
    ) -> Option<WkItem>
    where
        F: FnMut(&str, &str) -> bool,
    {
        let entry = &self.entries[(rng.range(0, self.entries.len() as i64 - 1)) as usize];

        // A headword that is a phrase or a proper noun does not make a vocabulary
        // question, and one with too few listed terms cannot fill four options.
        if !is_plain_word(&entry.headword) {
            return None;
        }
        let candidates: Vec<&String> = entry
            .related
            .iter()
            .filter(|word| is_plain_word(word) && key(word) != key(&entry.headword))
            // Mutual listing, so the item teaches a real synonym pair rather than
            // one of Moby's loose associations. See `is_mutual`.
            .filter(|word| self.is_mutual(&entry.headword, word))
            // The caller's own quality bar, applied last so it only ever sees
            // pairs that already cleared the source-derived checks.
            .filter(|word| accept(&entry.headword, word))
            .collect();
        if candidates.is_empty() {
            return None;
        }

        let correct = (*rng.pick(&candidates)).clone();
        if key(&correct) == key(&entry.headword) {
            return None;
        }

        // Distractors must not be associated with the headword anywhere in the
        // source, or the item may have more than one defensible answer.
        //
        // They must also be *plausible*. Drawing uniformly from the vocabulary
        // produced options like `unaustereness` and `mammiform` beside `calm`, and
        // an item whose wrong answers are obviously wrong can be answered without
        // knowing the word at all -- which defeats the point of a vocabulary
        // question. A root word with a line of its own, mentioned on several
        // lines, is a word the source treats as ordinary English.
        let mut options: Vec<(String, bool)> = vec![(correct.clone(), true)];
        let probes = 400;
        for _ in 0..probes {
            if options.len() >= 4 {
                break;
            }
            let candidate = rng.pick(&self.vocabulary).clone();
            if key(&candidate) == key(&entry.headword) {
                continue;
            }
            if options
                .iter()
                .any(|(existing, _)| key(existing) == key(&candidate))
            {
                continue;
            }
            if self.co_occurs(&entry.headword, &candidate) {
                continue;
            }
            if !self.is_root_word(&candidate) {
                continue;
            }
            if self.frequency(&candidate) < min_distractor_lines {
                continue;
            }
            // A length outlier is the option a learner eliminates first.
            let gap = candidate.len().abs_diff(correct.len());
            if gap > MAX_DISTRACTOR_LENGTH_GAP {
                continue;
            }
            options.push((candidate, false));
        }
        if options.len() != 4 {
            return None;
        }

        rng.shuffle(&mut options);
        let correct_index = options
            .iter()
            .position(|(_, is_correct)| *is_correct)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (_, is_correct)) in options.iter().enumerate() {
            if !is_correct {
                distractor_rationales.insert(
                    index,
                    format!(
                        "The source does not list this with \"{}\", so the two are not \
                         treated as synonyms there.",
                        entry.headword
                    ),
                );
            }
        }

        let preview: Vec<String> = entry.related.iter().take(12).cloned().collect();
        let supporting_line = if preview.len() < entry.related.len() {
            format!("{}… ({} terms)", preview.join(", "), entry.related.len())
        } else {
            preview.join(", ")
        };

        Some(WkItem {
            objective_id: "OBJ-WK-SYNONYM-01".to_string(),
            headword: entry.headword.clone(),
            prompt: format!(
                "Choose the word that most nearly means the same as {}.",
                entry.headword.to_uppercase()
            ),
            options: options.into_iter().map(|(word, _)| word).collect(),
            correct_index,
            distractor_rationales,
            supporting_line,
            related_count: entry.related.len(),
            difficulty: 0.2,
            seed,
        })
    }
}

/// Independently verify a Word Knowledge item against the source.
///
/// Re-derives both facts from the thesaurus rather than trusting the builder, so
/// a builder bug cannot validate itself.
pub fn verify(item: &WkItem, thesaurus: &Thesaurus) -> Result<(), WkVerificationFailure> {
    if item.headword.trim().is_empty() {
        return Err(WkVerificationFailure::Blank("headword".to_string()));
    }
    if item.prompt.trim().is_empty() {
        return Err(WkVerificationFailure::Blank("prompt".to_string()));
    }
    if thesaurus.related_to(&item.headword).is_none() {
        return Err(WkVerificationFailure::UnknownHeadword(
            item.headword.clone(),
        ));
    }
    if item.options.len() < 2 {
        return Err(WkVerificationFailure::TooFewOptions(item.options.len()));
    }
    if item.correct_index >= item.options.len() {
        return Err(WkVerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }

    let correct = &item.options[item.correct_index];
    if !thesaurus.co_occurs(&item.headword, correct) {
        return Err(WkVerificationFailure::CorrectOptionNotInSource {
            option: correct.clone(),
            headword: item.headword.clone(),
        });
    }

    for (index, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(WkVerificationFailure::Blank(format!("option {index}")));
        }
        if item.options[..index]
            .iter()
            .any(|earlier| key(earlier) == key(option))
        {
            return Err(WkVerificationFailure::DuplicateOption(option.clone()));
        }
        if index != item.correct_index && thesaurus.co_occurs(&item.headword, option) {
            return Err(WkVerificationFailure::AmbiguousDistractor {
                option: option.clone(),
                headword: item.headword.clone(),
            });
        }
    }

    let expected: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    let present: Vec<usize> = item.distractor_rationales.keys().copied().collect();
    let missing: Vec<usize> = expected
        .iter()
        .copied()
        .filter(|i| !present.contains(i))
        .collect();
    let extra: Vec<usize> = present
        .iter()
        .copied()
        .filter(|i| !expected.contains(i))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(WkVerificationFailure::RationaleCoverage { missing, extra });
    }
    for (index, rationale) in &item.distractor_rationales {
        if rationale.trim().is_empty() {
            return Err(WkVerificationFailure::Blank(format!(
                "rationale for option {index}"
            )));
        }
    }

    Ok(())
}
