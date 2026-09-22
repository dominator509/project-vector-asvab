//! Electronics Information items from NEETS, the Navy Electricity and
//! Electronics Training Series.
//!
//! Requirement: REQ-022 (original item + independent verification), REQ-056
//! (per-item provenance). Subtest: EI, "knowledge of electricity and
//! electronics".
//!
//! ## Source
//!
//! NEETS is a US Navy training curriculum published by NETPDTC, a **US government
//! work in the public domain**. Unlike the thesaurus, it is *instructional*: each
//! module ends with a glossary of `TERM —Definition` entries, which is what this
//! module ingests. Module 1 (Matter, Energy, and Direct Current) alone carries
//! 146 entries, and the series has 24 modules.
//!
//! ## Why the distractors here are plausible when the thesaurus's were not
//!
//! Word Knowledge items drew wrong answers from a 57,000-word pool, so an option
//! like `mammiform` sat beside `calm` and the answer was guessable by elimination.
//! A technical glossary does not have that problem: every term in it is a concept
//! from the same domain. Better still, glossary entries are *alphabetically
//! adjacent by relatedness* -- `BLEEDER CURRENT` sits next to `BLEEDER RESISTOR`,
//! `AMMETER` next to `AMPERE` -- so drawing distractors from nearby entries
//! produces options that are genuinely confusable rather than merely present.
//!
//! ## OCR, and what is done about it
//!
//! The archive.org text is OCR of a scanned page, and the scan's font made the
//! reader confuse `c` with `e`: the corpus contains `eleetron`, `eonduct`,
//! `earry`, `eleetrie`, `eurrent`, `reeiproeal` and `resistanee`. An item whose
//! definition reads "the amount of eleetron flow" teaches a learner that
//! `eleetron` is a word.
//!
//! The builder therefore takes a predicate over the definition text and the
//! application layer supplies "every word here is a word": each alphabetic token
//! of four or more characters must be defined by Webster's Unabridged, which the
//! Word Knowledge path already fetched. This is the second use of that
//! dictionary and the reason it is worth its 29 MB.
//!
//! Terms that fail the check cause the entry to be skipped, not repaired. A
//! misspelling is not something this code can correct, and guessing at `eleetron`
//! would be inventing source text.

use std::collections::{BTreeMap, HashSet};

use crate::factory::Rng;

/// One glossary entry.
#[derive(Debug, Clone, PartialEq)]
pub struct GlossaryEntry {
    pub term: String,
    pub definition: String,
    /// The NEETS module the entry came from, recorded so an item can cite it.
    pub module: String,
}

/// A module's glossary, in source order.
#[derive(Debug, Clone, Default)]
pub struct Glossary {
    entries: Vec<GlossaryEntry>,
    /// Terms lowercased, for the "is this a known technical term" check.
    terms: HashSet<String>,
}

/// An Electronics Information item.
#[derive(Debug, Clone, PartialEq)]
pub struct EiItem {
    pub objective_id: String,
    pub term: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub distractor_rationales: BTreeMap<usize, String>,
    /// The glossary definition the item quotes. This is the item's evidence.
    pub supporting_definition: String,
    pub module: String,
    pub difficulty: f64,
    pub seed: u64,
}

impl EiItem {
    /// Stable identity over the answerable content, excluding the seed.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str("EI");
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.term));
        canonical.push('\u{1}');
        canonical.push_str(&normalize(
            self.options.first().map(String::as_str).unwrap_or(""),
        ));
        for option in &self.options {
            canonical.push_str(&normalize(option));
            canonical.push('\u{2}');
        }
        crate::provenance::ContentHash::of_text(&canonical)
            .as_str()
            .to_string()
    }

    /// The explanation shown after answering: the source's own definition.
    pub fn explanation(&self) -> String {
        format!(
            "NEETS defines \"{}\" as: {} ({})",
            self.term, self.supporting_definition, self.module
        )
    }
}

/// Why an Electronics Information item is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EiVerificationFailure {
    /// The option marked correct does not name the entry the prompt quotes.
    CorrectOptionIsNotTheDefinedTerm {
        option: String,
        term: String,
    },
    /// A distractor's term is the term being asked about.
    DistractorIsTheAnswer {
        option: String,
    },
    /// Two options name the same term.
    DuplicateOption(String),
    TooFewOptions(usize),
    CorrectIndexOutOfRange {
        index: usize,
        options: usize,
    },
    RationaleCoverage {
        missing: Vec<usize>,
        extra: Vec<usize>,
    },
    Blank(String),
    /// The term is not in the glossary the item claims to come from.
    UnknownTerm(String),
    /// An option is not a glossary term, so it is not a real distractor.
    OptionIsNotAGlossaryTerm(String),
}

impl std::fmt::Display for EiVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm { option, term } => write!(
                f,
                "the option marked correct is {option:?}, but the prompt defines {term:?}"
            ),
            EiVerificationFailure::DistractorIsTheAnswer { option } => {
                write!(f, "distractor {option:?} is the term being asked about")
            }
            EiVerificationFailure::DuplicateOption(option) => {
                write!(f, "option {option:?} appears more than once")
            }
            EiVerificationFailure::TooFewOptions(n) => {
                write!(f, "an item needs at least 2 options, found {n}")
            }
            EiVerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            EiVerificationFailure::RationaleCoverage { missing, extra } => write!(
                f,
                "rationales missing for {missing:?} and unexpected for {extra:?}"
            ),
            EiVerificationFailure::Blank(what) => write!(f, "{what} is blank"),
            EiVerificationFailure::UnknownTerm(term) => {
                write!(f, "{term:?} is not in the glossary")
            }
            EiVerificationFailure::OptionIsNotAGlossaryTerm(option) => write!(
                f,
                "option {option:?} is not a glossary term, so it is not a real distractor"
            ),
        }
    }
}

impl std::error::Error for EiVerificationFailure {}

/// Case- and whitespace-insensitive normalisation for comparison.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Split a glossary line into term and definition.
///
/// Two separators appear across the series, and which one a module uses is a
/// property of its scan rather than of its subject: modules 1, 2, 5, 8, 9 and 10
/// use an em dash, while modules 3, 4 and 7 use a spaced hyphen. Supporting only
/// the em dash silently discarded three modules -- they parsed to one or two
/// entries instead of hundreds, which is exactly the kind of quiet loss that a
/// count would have hidden if the per-module numbers were not printed.
///
/// The hyphen must be spaced to count, because hyphens occur inside terms
/// (`P-N JUNCTION`) and inside definitions.
fn split_entry(line: &str) -> Option<(&str, &str)> {
    let em = line
        .find('\u{2014}')
        .map(|index| (index, '\u{2014}'.len_utf8()));
    let hyphen = line.find(" - ").map(|index| (index, 3));
    let (index, width) = match (em, hyphen) {
        (Some(a), Some(b)) => {
            if a.0 <= b.0 {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };

    let term = line[..index].trim();
    let definition = line[index + width..]
        .trim_start_matches(['^', '~', ' ', '\t', '-', '\u{2014}'])
        .trim();
    if term.is_empty() || definition.is_empty() {
        return None;
    }
    // A term is a short run of capitals and separators. Anything longer is a
    // continuation line that happens to contain a dash.
    if term.len() > 44 {
        return None;
    }
    let first = term.chars().next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if !term
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || " ,()/-.'&".contains(c))
    {
        return None;
    }
    Some((term, definition))
}

/// Whether a line is a section heading rather than definition text.
///
/// All-caps and short, with no em dash (an em dash would have made it an entry).
/// The glossary is followed by appendices whose headings have exactly this shape,
/// and without this check the last glossary entry absorbs the next section's
/// title -- which is what `WRAPPED` did in the fixture before a diagnostic caught
/// it silently carrying "APPENDIX B SOMETHING ELSE" as part of its definition.
fn looks_like_section_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 30 {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == ' ' || c == '-')
        && trimmed.chars().any(|c| c.is_ascii_uppercase())
}

/// Parse a NEETS glossary: `TERM —Definition`, with definitions wrapping onto
/// following lines.
///
/// Entries are **not** blank-separated in this corpus -- `AMMETER —...` directly
/// precedes `AMPERE —...` -- so a line is a new entry when it matches the shape,
/// not when it follows a blank line.
pub fn parse_neets_glossary(text: &str, module: &str) -> Glossary {
    let mut entries: Vec<GlossaryEntry> = Vec::new();
    let mut current: Option<(String, String)> = None;

    let flush = |current: &mut Option<(String, String)>, entries: &mut Vec<GlossaryEntry>| {
        if let Some((term, definition)) = current.take() {
            let definition = definition.split_whitespace().collect::<Vec<_>>().join(" ");
            if !definition.is_empty() {
                entries.push(GlossaryEntry {
                    term,
                    definition,
                    module: module.to_string(),
                });
            }
        }
    };

    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if let Some((term, definition)) = split_entry(line) {
            flush(&mut current, &mut entries);
            current = Some((term.to_string(), definition.to_string()));
        } else if looks_like_section_heading(line) {
            // The glossary has ended; what follows is another section, not a
            // continuation of the last definition.
            flush(&mut current, &mut entries);
        } else if let Some((_, definition)) = current.as_mut() {
            // A continuation line. Blank lines are skipped rather than ending the
            // entry, because OCR inserts them mid-definition.
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                definition.push(' ');
                definition.push_str(trimmed);
            }
        }
    }
    flush(&mut current, &mut entries);

    let terms = entries.iter().map(|e| normalize(&e.term)).collect();
    Glossary { entries, terms }
}

impl Glossary {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn module(&self) -> &str {
        self.entries
            .first()
            .map(|e| e.module.as_str())
            .unwrap_or("")
    }

    /// Whether `term` is a term this glossary defines.
    pub fn defines(&self, term: &str) -> bool {
        self.terms.contains(&normalize(term))
    }

    pub fn definition_of(&self, term: &str) -> Option<&str> {
        let wanted = normalize(term);
        self.entries
            .iter()
            .find(|entry| normalize(&entry.term) == wanted)
            .map(|entry| entry.definition.as_str())
    }

    /// Where a term sits in the glossary.
    ///
    /// Exposed because adjacency is the property the distractor rule relies on,
    /// and a test has to be able to assert it without reading private state.
    pub fn position_of(&self, term: &str) -> Option<usize> {
        let wanted = normalize(term);
        self.entries
            .iter()
            .position(|entry| normalize(&entry.term) == wanted)
    }

    /// Whether the glossary gives `term` this definition somewhere.
    ///
    /// Some modules define a term more than once with different wording, so
    /// comparing against the *first* definition alone rejects items quoting a
    /// perfectly real entry. Forty-seven items failed that way on the real corpus.
    pub fn has_definition(&self, term: &str, definition: &str) -> bool {
        let wanted = normalize(term);
        let wanted_definition = normalize(definition);
        self.entries.iter().any(|entry| {
            normalize(&entry.term) == wanted && normalize(&entry.definition) == wanted_definition
        })
    }

    /// How many entries define this term.
    ///
    /// More than one means the source repeats it, which matters because an item's
    /// evidence names a single definition.
    pub fn definitions_of(&self, term: &str) -> usize {
        let wanted = normalize(term);
        self.entries
            .iter()
            .filter(|entry| normalize(&entry.term) == wanted)
            .count()
    }

    /// Build up to `count` items.
    ///
    /// `definition_ok` vets the definition text; the application layer supplies
    /// the OCR check described in the module documentation.
    pub fn build_items<F>(
        &self,
        count: usize,
        seed: u64,
        min_definition_words: usize,
        mut definition_ok: F,
    ) -> Vec<EiItem>
    where
        F: FnMut(&str) -> bool,
    {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let mut rng = Rng::new(seed);
        let mut items: Vec<EiItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let attempts = count.saturating_mul(8).max(64);
        for _ in 0..attempts {
            if items.len() >= count {
                break;
            }
            let Some(item) =
                self.build_one(&mut rng, seed, min_definition_words, &mut definition_ok)
            else {
                continue;
            };
            if seen.insert(item.content_hash()) {
                items.push(item);
            }
        }
        items
    }

    fn build_one<F>(
        &self,
        rng: &mut Rng,
        seed: u64,
        min_definition_words: usize,
        definition_ok: &mut F,
    ) -> Option<EiItem>
    where
        F: FnMut(&str) -> bool,
    {
        let index = rng.range(0, self.entries.len() as i64 - 1) as usize;
        let entry = &self.entries[index];

        // A definition too short to identify a concept, or too long to read in
        // the 40 seconds EI allows per question, makes a poor item either way.
        let word_count = entry.definition.split_whitespace().count();
        if word_count < min_definition_words || word_count > 30 {
            return None;
        }
        // The caller's veto, which is where OCR noise is caught.
        if !definition_ok(&entry.definition) {
            return None;
        }

        // Distractors come from *neighbouring* entries first. In a technical
        // glossary adjacency is relatedness -- `BLEEDER CURRENT` beside
        // `BLEEDER RESISTOR` -- so these are the terms a learner actually
        // confuses, not filler drawn from anywhere in the language.
        let mut options: Vec<(String, bool)> = vec![(entry.term.clone(), true)];
        let neighbours = 6usize;
        for distance in 1..=neighbours {
            if options.len() >= 4 {
                break;
            }
            for candidate_index in [index.checked_sub(distance), Some(index + distance)]
                .into_iter()
                .flatten()
            {
                if options.len() >= 4 {
                    break;
                }
                let Some(candidate) = self.entries.get(candidate_index) else {
                    continue;
                };
                if normalize(&candidate.term) == normalize(&entry.term) {
                    continue;
                }
                if options
                    .iter()
                    .any(|(existing, _)| normalize(existing) == normalize(&candidate.term))
                {
                    continue;
                }
                // A term conspicuously longer than the others is the option a
                // learner eliminates first.
                if candidate.term.len().abs_diff(entry.term.len()) > 10 {
                    continue;
                }
                options.push((candidate.term.clone(), false));
            }
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
        for (position, (term, is_correct)) in options.iter().enumerate() {
            if *is_correct {
                continue;
            }
            // The rationale can only claim what the source supports: this term
            // has its own definition, and it is not the one quoted.
            let rationale = match self.definition_of(term) {
                Some(definition) => format!("NEETS defines \"{term}\" differently: {definition}"),
                None => format!("\"{term}\" is a different term in the same module."),
            };
            distractor_rationales.insert(position, rationale);
        }

        Some(EiItem {
            objective_id: "OBJ-EI-TERMINOLOGY-01".to_string(),
            term: entry.term.clone(),
            prompt: format!("Which term means: \"{}\"?", entry.definition),
            options: options.into_iter().map(|(term, _)| term).collect(),
            correct_index,
            distractor_rationales,
            supporting_definition: entry.definition.clone(),
            module: entry.module.clone(),
            difficulty: 0.3,
            seed,
        })
    }
}

/// Independently verify an Electronics Information item against the glossary.
///
/// Re-derives the facts from the source rather than trusting the builder: the
/// term the prompt quotes must be the option marked correct, every option must be
/// a term the glossary defines, and no distractor may be the answer.
pub fn verify(item: &EiItem, glossary: &Glossary) -> Result<(), EiVerificationFailure> {
    if item.term.trim().is_empty() {
        return Err(EiVerificationFailure::Blank("term".to_string()));
    }
    if item.prompt.trim().is_empty() {
        return Err(EiVerificationFailure::Blank("prompt".to_string()));
    }
    if item.supporting_definition.trim().is_empty() {
        return Err(EiVerificationFailure::Blank(
            "supporting definition".to_string(),
        ));
    }
    if item.options.len() < 2 {
        return Err(EiVerificationFailure::TooFewOptions(item.options.len()));
    }
    if item.correct_index >= item.options.len() {
        return Err(EiVerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }
    if !glossary.defines(&item.term) {
        return Err(EiVerificationFailure::UnknownTerm(item.term.clone()));
    }

    // The quoted definition must be one the source actually gives for the answer.
    // "One", not "the first": a module may define a term twice with different
    // wording, and an item built from the second entry is not wrong.
    if !glossary.has_definition(&item.term, &item.supporting_definition) {
        return Err(EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm {
            option: item.supporting_definition.clone(),
            term: item.term.clone(),
        });
    }

    let correct = &item.options[item.correct_index];
    if normalize(correct) != normalize(&item.term) {
        return Err(EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm {
            option: correct.clone(),
            term: item.term.clone(),
        });
    }

    for (position, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(EiVerificationFailure::Blank(format!("option {position}")));
        }
        if item.options[..position]
            .iter()
            .any(|earlier| normalize(earlier) == normalize(option))
        {
            return Err(EiVerificationFailure::DuplicateOption(option.clone()));
        }
        // Every option must be a term the source defines, or it is not a real
        // alternative a learner could have met in the module.
        if !glossary.defines(option) {
            return Err(EiVerificationFailure::OptionIsNotAGlossaryTerm(
                option.clone(),
            ));
        }
        if position != item.correct_index && normalize(option) == normalize(&item.term) {
            return Err(EiVerificationFailure::DistractorIsTheAnswer {
                option: option.clone(),
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
        return Err(EiVerificationFailure::RationaleCoverage { missing, extra });
    }
    for (position, rationale) in &item.distractor_rationales {
        if rationale.trim().is_empty() {
            return Err(EiVerificationFailure::Blank(format!(
                "rationale for option {position}"
            )));
        }
    }

    Ok(())
}
