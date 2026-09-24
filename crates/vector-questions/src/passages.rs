//! Paragraph Comprehension items from public-domain prose.
//!
//! Requirement: REQ-022 (original item + independent verification), REQ-056
//! (per-item provenance). Subtest: PC, "ability to obtain information from
//! written passages".
//!
//! ## Source
//!
//! Project Gutenberg, whose texts are public domain in the USA. The parser strips
//! the Gutenberg header and footer so their boilerplate can never become a
//! passage, and so nothing of Gutenberg's own trademark licence is carried into
//! the corpus. What an item quotes is the underlying public-domain prose.
//!
//! ## The item type, and why it is this one
//!
//! A comprehension question has to be answerable *from the passage*, and the
//! answer has to be checkable by a machine. Both are satisfied by asking which of
//! four statements the passage makes:
//!
//! * the correct option is a clause lifted verbatim from the passage, so it is
//!   true by construction;
//! * each distractor is that same clause with one detail -- a quantity or a name --
//!   replaced by something that appears nowhere in the passage.
//!
//! [`verify`] re-derives both facts from the passage text rather than trusting the
//! builder: the correct option must occur in the passage, and neither a distractor
//! nor its substituted token may occur at all.
//!
//! ## What this tests, stated honestly
//!
//! This is a **detail-location** item: the learner finds the statement in the
//! passage and rejects three near-misses that differ by one number or name. It is
//! not an inference question. Asking what an author implies cannot be verified by a
//! machine from the text alone, and an item that claimed to test inference while
//! being scored by string containment would be dishonest about what it measures.
//!
//! ## Why the distractors are good here when the thesaurus ones were not
//!
//! A Word Knowledge distractor was drawn from a 57,000-word pool, so `mammiform`
//! sat beside `calm` and the answer was guessable without knowing the word. These
//! distractors differ from the truth in exactly one detail, which is what a
//! comprehension question is *for*: the learner locates the statement instead of
//! recognising which option looks out of place.

use std::collections::{BTreeMap, HashSet};

use crate::factory::Rng;

/// A parsed public-domain text.
#[derive(Debug, Clone, Default)]
pub struct Text {
    pub label: String,
    paragraphs: Vec<String>,
    /// Capitalised words appearing anywhere in the text, for name substitutions.
    proper_nouns: Vec<String>,
}

/// A Paragraph Comprehension item.
#[derive(Debug, Clone, PartialEq)]
pub struct PcItem {
    pub objective_id: String,
    /// The passage the learner reads.
    pub passage: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub distractor_rationales: BTreeMap<usize, String>,
    /// The clause the item rests on, verbatim from the passage. This is the item's
    /// evidence, and what the explanation shows.
    pub supporting_clause: String,
    pub source_label: String,
    pub difficulty: f64,
    pub seed: u64,
}

impl PcItem {
    /// Stable identity over the answerable content, excluding the seed.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str("PC");
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.passage));
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.supporting_clause));
        crate::provenance::ContentHash::of_text(&canonical)
            .as_str()
            .to_string()
    }

    /// The explanation shown after answering: the passage's own words.
    pub fn explanation(&self) -> String {
        format!(
            "The passage states: \"{}\" ({})",
            self.supporting_clause, self.source_label
        )
    }
}

/// Why a Paragraph Comprehension item is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcVerificationFailure {
    /// The option marked correct is not in the passage, so the item has no answer.
    CorrectOptionNotInPassage(String),
    /// A distractor is also stated in the passage, so two options are true.
    DistractorAlsoInPassage(String),
    /// A distractor differs only by a token the passage also contains, so the
    /// passage does not rule it out.
    DistractorTokenPresent {
        option: String,
        token: String,
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
    /// The passage is too short to comprehend or too long to read in the time the
    /// subtest allows.
    PassageLength {
        words: usize,
    },
    /// An option is not a clause of comparable length, so it is answerable by shape
    /// rather than by reading.
    ClauseLength {
        option: String,
        words: usize,
    },
    /// A main-idea item names a topic the passage does not actually repeat, so the
    /// item claims the passage is about something the text does not support.
    TopicNotRepeated {
        topic: String,
        occurrences: usize,
    },
}

impl std::fmt::Display for PcVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PcVerificationFailure::CorrectOptionNotInPassage(option) => write!(
                f,
                "the option marked correct is not stated in the passage: {option:?}"
            ),
            PcVerificationFailure::DistractorAlsoInPassage(option) => write!(
                f,
                "distractor {option:?} is also stated in the passage, so two options \
                 would be true"
            ),
            PcVerificationFailure::DistractorTokenPresent { option, token } => write!(
                f,
                "distractor {option:?} differs from the passage only by {token:?}, which \
                 the passage also contains, so it cannot be ruled out"
            ),
            PcVerificationFailure::TooFewOptions(n) => {
                write!(f, "an item needs at least 2 options, found {n}")
            }
            PcVerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            PcVerificationFailure::DuplicateOption(option) => {
                write!(f, "option {option:?} appears more than once")
            }
            PcVerificationFailure::RationaleCoverage { missing, extra } => write!(
                f,
                "rationales missing for {missing:?} and unexpected for {extra:?}"
            ),
            PcVerificationFailure::Blank(what) => write!(f, "{what} is blank"),
            PcVerificationFailure::PassageLength { words } => {
                write!(f, "the passage is {words} words long")
            }
            PcVerificationFailure::ClauseLength { option, words } => write!(
                f,
                "option {option:?} is {words} words, outside the clause band, so it is \
                 answerable by shape"
            ),
            PcVerificationFailure::TopicNotRepeated { topic, occurrences } => write!(
                f,
                "the main-idea topic {topic:?} occurs {occurrences} time(s) in the passage; a \
                 topic the passage does not repeat is not what it is mainly about"
            ),
        }
    }
}

impl std::error::Error for PcVerificationFailure {}

/// Word-count bounds for a passage.
///
/// PC allows 162 seconds per question, so a passage has to be readable in that
/// time: under 40 words cannot test comprehension of written prose, and over 220
/// would not fit the sitting.
pub const MIN_PASSAGE_WORDS: usize = 40;
pub const MAX_PASSAGE_WORDS: usize = 220;

/// Word-count bounds for an option.
///
/// Two earlier versions failed in opposite directions. Using the whole sentence
/// gave four fifty-word options after a sixty-word passage -- correct, verifiable
/// and unreadable. Cutting a clause out of mid-sentence gave fragments with
/// unbalanced brackets and no verb. A complete *short* sentence is the only version
/// that is both grammatical and scannable, and the ceiling is where prose stops
/// offering them: at 16 words the four texts yielded 67 items between them and
/// Faraday's lectures yielded none.
pub const MIN_CLAUSE_WORDS: usize = 5;
pub const MAX_CLAUSE_WORDS: usize = 26;

/// Whether a passage is prose a learner can be asked about.
///
/// `parse_gutenberg` stops at the end marker, so the licence is already gone, but
/// a work may mention Project Gutenberg in its own preface or notes. Those
/// paragraphs are about the file rather than about anything a reading test could
/// ask, and one of them once reached a built item.
pub fn is_usable_passage(passage: &str) -> bool {
    let words = passage.split_whitespace().count();
    (MIN_PASSAGE_WORDS..=MAX_PASSAGE_WORDS).contains(&words)
        && !passage.to_lowercase().contains("gutenberg")
}

/// Lowercase and collapse whitespace, for comparison.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The words of a text, lowercased and stripped of punctuation.
///
/// Splitting on non-alphanumerics rather than on non-ASCII-alphanumerics keeps a
/// quantity together: `16⅓` is one word, and splitting it would leave a bare `16`
/// and a bare `3` in the token set, so an unrelated `18` elsewhere in the passage
/// would look like the token a distractor altered.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

/// Abbreviations that end with a full stop without ending a sentence.
const ABBREVIATIONS: [&str; 12] = [
    "mr", "mrs", "ms", "dr", "prof", "st", "no", "vol", "fig", "cf", "e.g", "i.e",
];

/// Punctuation that may close a sentence immediately after its terminator.
const CLOSING_MARKS: [char; 7] = ['"', '\u{201d}', '\u{2019}', '\'', ')', ']', '\u{bb}'];

/// Function words that are never the topic of a passage, and never a vocabulary
/// target. Lowercase, because `words` lowercases before these are consulted.
const STOP_WORDS: [&str; 63] = [
    "about", "after", "again", "against", "because", "before", "being", "below", "between",
    "could", "during", "every", "first", "found", "their", "there", "these", "thing", "those",
    "through", "under", "until", "where", "which", "would", "other", "another", "should", "might",
    "still", "since", "shall", "going", "above", "having", "never", "often", "always", "almost",
    "alone", "along", "among", "around", "became", "become", "began", "begin", "called", "cannot",
    "comes", "doing", "enough", "given", "great", "itself", "known", "large", "later", "little",
    "made", "makes", "many", "while",
];

/// The character span of each sentence in a paragraph.
///
/// Spans rather than strings, because a passage has to be the source's own text.
/// The earlier version joined the sentences it had split with a single space, which
/// silently rewrote the passage wherever the split was not at a space: a paragraph
/// reading `the cap of liberty.” An ancient figure` came back as
/// `the cap of liberty? ” “Where`, and `Penna. R.R. tunnel shields` came back as
/// `Penna. R. R. tunnel shields`. A learner would then be asked to comprehend text
/// the source does not contain, and no amount of checking the item against itself
/// would notice. Cutting the passage out of the paragraph by span makes that
/// impossible by construction.
pub fn sentence_spans(paragraph: &str) -> Vec<(usize, usize)> {
    let characters: Vec<char> = paragraph.chars().collect();
    let mut spans = Vec::new();
    // The first sentence starts at the beginning of the paragraph. Starting from
    // `None` and assigning on the first terminator dropped everything before it.
    let mut start: Option<usize> = Some(0);

    for (index, character) in characters.iter().enumerate() {
        if !matches!(character, '.' | '!' | '?') {
            continue;
        }
        // A terminator only ends a sentence if what follows it is whitespace or the
        // end of the paragraph, possibly after a closing mark. `R.R.` has no space
        // after its first full stop and is one token, not two sentences.
        let mut after = index + 1;
        while after < characters.len() && CLOSING_MARKS.contains(&characters[after]) {
            after += 1;
        }
        if after < characters.len() && !characters[after].is_whitespace() {
            continue;
        }
        let next = characters[after..]
            .iter()
            .find(|c| !c.is_whitespace())
            .copied()
            .unwrap_or(' ');
        if *character == '.' {
            if !next.is_ascii_uppercase() {
                continue;
            }
            let token: String = characters[..index]
                .iter()
                .rev()
                .take_while(|c| c.is_alphanumeric() || **c == '.')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let last_word = token
                .trim_end_matches('.')
                .rsplit(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or("")
                .to_lowercase();
            if ABBREVIATIONS.contains(&last_word.as_str()) {
                continue;
            }
            // A single letter before the stop is an initial (`R. R.`), not the end
            // of a sentence, and splitting there produces a one-letter "sentence".
            if last_word.chars().count() == 1 {
                continue;
            }
        }
        let end = after;
        if let Some(from) = start {
            if characters[from..end].iter().any(|c| !c.is_whitespace()) {
                spans.push((from, end));
            }
        }
        start = Some(end);
    }
    if let Some(from) = start {
        if characters[from..].iter().any(|c| !c.is_whitespace()) {
            spans.push((from, characters.len()));
        }
    }

    // Trim whitespace from both ends of each span, so a sentence stands alone
    // without leading or trailing space while still pointing into the paragraph.
    spans
        .into_iter()
        .filter_map(|(from, to)| {
            let mut from = from;
            let mut to = to;
            while from < to && characters[from].is_whitespace() {
                from += 1;
            }
            while to > from && characters[to - 1].is_whitespace() {
                to -= 1;
            }
            (to > from).then_some((from, to))
        })
        .collect()
}

/// Split prose into sentences.
///
/// Hand-rolled because the crate carries no regular-expression dependency, and
/// because the failure that matters is an abbreviation mistaken for a terminator: a
/// sentence cut at `Mr.` produces a fragment that no longer says anything, and an
/// item built on it quotes half a thought.
///
/// Derived from [`sentence_spans`] so the two can never disagree about where a
/// sentence begins.
pub fn split_sentences(paragraph: &str) -> Vec<String> {
    let characters: Vec<char> = paragraph.chars().collect();
    sentence_spans(paragraph)
        .into_iter()
        .map(|(from, to)| characters[from..to].iter().collect())
        .collect()
}

/// Strip Project Gutenberg's header and footer, returning the work itself.
///
/// The markers are matched in their full `*** ... PROJECT GUTENBERG` form on
/// purpose. The bare phrase "end of" occurs throughout ordinary prose -- Faraday's
/// lectures say "at the end of the tube" on nearly every page -- and matching it
/// truncated a 247 KB book to 46 paragraphs before this was corrected.
fn body(text: &str) -> &str {
    let start = text
        .find("*** START OF THE PROJECT GUTENBERG")
        .or_else(|| text.find("*** START OF THIS PROJECT GUTENBERG"));
    let end = text
        .rfind("*** END OF THE PROJECT GUTENBERG")
        .or_else(|| text.rfind("*** END OF THIS PROJECT GUTENBERG"));
    match (start, end) {
        (Some(start), Some(end)) if end > start => {
            let after = &text[start..];
            let newline = after
                .find('\n')
                .map(|offset| start + offset + 1)
                .unwrap_or(start);
            &text[newline..end]
        }
        _ => text,
    }
}

/// Parse a Gutenberg text into paragraphs.
pub fn parse_gutenberg(text: &str, label: &str) -> Text {
    let body = body(text);
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.trim().is_empty() {
                paragraphs.push(current.split_whitespace().collect::<Vec<_>>().join(" "));
                current.clear();
            }
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(trimmed);
    }
    if !current.trim().is_empty() {
        paragraphs.push(current.split_whitespace().collect::<Vec<_>>().join(" "));
    }

    // Remove the editorial apparatus that survives the markers: contents lists,
    // chapter headings and tables are not prose a learner can comprehend.
    paragraphs.retain(|paragraph| {
        let word_count = paragraph.split_whitespace().count();
        word_count >= 12
            && !paragraph.starts_with('[')
            && !paragraph.starts_with("CHAPTER")
            && !paragraph.starts_with("Chapter")
            && paragraph.chars().filter(|c| c.is_ascii_digit()).count() * 4
                < paragraph.chars().count()
            // A paragraph that refers to illustrations over and over is a list of
            // them. "The book of wonders" carries seventy such lists, and one of
            // them reached a built item as a passage about air locks whose "options"
            // were index entries.
            && paragraph.matches("(illus").count() < 3
    });

    let mut seen: HashSet<String> = HashSet::new();
    let mut proper_nouns: Vec<String> = Vec::new();
    for paragraph in &paragraphs {
        for sentence in split_sentences(paragraph) {
            for word in sentence.split_whitespace() {
                let cleaned: String = word
                    .trim_matches(|c: char| !c.is_ascii_alphabetic())
                    .to_string();
                if cleaned.len() < 3 || !cleaned.chars().next().is_some_and(char::is_uppercase) {
                    continue;
                }
                if seen.insert(cleaned.clone()) {
                    proper_nouns.push(cleaned);
                }
            }
        }
    }

    Text {
        label: label.to_string(),
        paragraphs,
        proper_nouns,
    }
}

/// The work's own title, read from the file's header rather than assumed.
///
/// A citation has to name the work an item came from, and the header is where the
/// file states it. Two shapes exist in the corpus: a leading `Title:` field, and
/// the sentence `The Project Gutenberg eBook of <title>`. Reading it beats passing
/// a file name through, which would put `doe-nuclear-1` in front of a learner.
pub fn gutenberg_title(text: &str) -> Option<String> {
    let header = match text.find("*** START OF THE PROJECT GUTENBERG") {
        Some(marker) => &text[..marker],
        None => text,
    };
    for line in header.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Title:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
        if let Some(index) = trimmed.find("The Project Gutenberg eBook of ") {
            let rest = trimmed[index + "The Project Gutenberg eBook of ".len()..].trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

impl Text {
    pub fn paragraph_count(&self) -> usize {
        self.paragraphs.len()
    }

    /// The parsed paragraphs, in file order.
    ///
    /// Exposed for the same reason `Glossary::definitions_of` is: it is what lets a
    /// caller check an item against the text it claims to quote, rather than
    /// against the item's own copy of it.
    pub fn paragraphs(&self) -> &[String] {
        &self.paragraphs
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.is_empty()
    }

    /// The comprehension types this builder can produce, and the only objective ids
    /// it will ever stamp.
    ///
    /// CAT-ASVAB PC asks for four kinds of reading question. Two of them are
    /// machine-checkable from the passage alone, and this module builds both:
    ///
    /// * **DETAIL** (`OBJ-PC-DETAIL-01`) - a clause the passage states, against
    ///   near-misses that each alter one quantity or name.
    /// * **MAIN_IDEA** (`OBJ-PC-MAINIDEA-01`) - the single option whose subject is
    ///   the passage's dominant repeated topic. The correct option names a content
    ///   word the passage repeats; every distractor names a content word taken from
    ///   *another* passage in the same source, so it cannot be the topic of this one.
    /// * **VOCAB_IN_CONTEXT** (`OBJ-PC-VOCAB-01`) - a word the passage uses, against
    ///   three words the passage does not contain.
    ///
    /// **INFERENCE is deliberately not built.** An item that asks what the author
    /// implies cannot be scored by any check this module can perform: the passage
    /// does not *state* the answer, so string containment cannot confirm it, and a
    /// wrong option is not wrong for a mechanical reason. Shipping an "inference"
    /// item scored by containment would mislabel a detail item, and the module has
    /// committed to saying what it measures rather than to filling a type count.
    /// The gap is recorded here rather than papered over.
    pub const KINDS: [&'static str; 3] =
        ["OBJ-PC-DETAIL-01", "OBJ-PC-MAINIDEA-01", "OBJ-PC-VOCAB-01"];

    /// Whether this passage occurs in the source text.
    ///
    /// `verify` re-derives the facts from the item's own passage, which the builder
    /// wrote, so it catches a builder that contradicts itself but not one that
    /// invents a passage. This closes that: an item cannot be called source-backed
    /// unless its passage is a passage the source actually contains.
    pub fn contains_passage(&self, passage: &str) -> bool {
        let needle = normalize(passage);
        !needle.is_empty()
            && self
                .paragraphs
                .iter()
                .any(|paragraph| normalize(paragraph).contains(&needle))
    }

    /// Build up to `count` items.
    ///
    /// `accept` vets a candidate passage; the application layer uses it to reject
    /// text that carries the source's own apparatus.
    pub fn build_items<F>(&self, count: usize, seed: u64, mut accept: F) -> Vec<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        if self.paragraphs.is_empty() {
            return Vec::new();
        }
        let mut rng = Rng::new(seed);
        let mut items: Vec<PcItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let attempts = count.saturating_mul(12).max(64);
        for _ in 0..attempts {
            if items.len() >= count {
                break;
            }
            let Some(item) = self.build_one(&mut rng, seed, &mut accept) else {
                continue;
            };
            if seen.insert(item.content_hash()) {
                items.push(item);
            }
        }
        items
    }

    /// Build one item of any supported kind. The kind is drawn from `KINDS`, so a
    /// corpus-wide run produces a mix of detail, main-idea and vocabulary items
    /// rather than the single stem the builder used to stamp on every PC item.
    fn build_one<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        match Self::KINDS[(rng.range(0, Self::KINDS.len() as i64 - 1)) as usize] {
            "OBJ-PC-MAINIDEA-01" => self.build_main_idea(rng, seed, accept),
            "OBJ-PC-VOCAB-01" => self.build_vocab_in_context(rng, seed, accept),
            _ => self.build_detail(rng, seed, accept),
        }
    }

    /// A passage in the comprehension band that the caller accepts.
    ///
    /// `verify` refuses a passage outside `MIN_PASSAGE_WORDS..=MAX_PASSAGE_WORDS`, so
    /// every builder must draw from that band rather than from the paragraph list.
    /// The detail builder gets this from the clause band it also has to satisfy; the
    /// main-idea and vocabulary builders have no clause band, so they check here.
    fn pick_passage<F>(&self, rng: &mut Rng, accept: &mut F) -> Option<String>
    where
        F: FnMut(&str) -> bool,
    {
        for _ in 0..32 {
            let paragraph = self
                .paragraphs
                .get(rng.range(0, self.paragraphs.len() as i64 - 1) as usize)?
                .clone();
            let count = paragraph.split_whitespace().count();
            if (MIN_PASSAGE_WORDS..=MAX_PASSAGE_WORDS).contains(&count) && accept(&paragraph) {
                return Some(paragraph);
            }
        }
        None
    }

    /// The topic of a passage, as the content word it repeats most.
    ///
    /// Returned with the number of its occurrences, so a caller can refuse a passage
    /// with no clearly dominant subject instead of inventing one. Function words and
    /// anything under five letters are skipped: the topic of a passage is a noun, not
    /// `the`.
    fn dominant_topic(&self, passage: &str) -> Option<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for word in words(passage) {
            // `words` lowercases and strips punctuation already. A short word or a
            // stop word is never the subject of anything.
            if word.len() < 5 || STOP_WORDS.contains(&word.as_str()) {
                continue;
            }
            *counts.entry(word).or_insert(0) += 1;
        }
        let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
        // Highest count first; ties broken by the word itself so the pick is stable
        // across runs rather than depending on hash iteration order.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let (word, count) = ranked.into_iter().next()?;
        // One mention is not a topic. Requiring a repeat is what makes "the passage
        // is mostly about X" a fact about the text rather than about one sentence.
        if count < 2 {
            return None;
        }
        Some((word, count))
    }

    /// A topic word that appears in some *other* passage of this source, so it can be
    /// used as a main-idea distractor: true of another passage, false of this one.
    fn topic_from_elsewhere(&self, this_passage: &str, rng: &mut Rng) -> Option<String> {
        let here: HashSet<String> = words(this_passage).into_iter().collect();
        let mut pool: Vec<String> = Vec::new();
        for paragraph in &self.paragraphs {
            if paragraph == this_passage {
                continue;
            }
            if let Some((topic, count)) = self.dominant_topic(paragraph) {
                // It must be dominant elsewhere and absent here, or it is not a
                // distractor for *this* passage's topic.
                if count >= 2 && !here.contains(&topic) {
                    pool.push(topic);
                }
            }
        }
        pool.sort();
        pool.dedup();
        if pool.is_empty() {
            return None;
        }
        Some(pool[(rng.range(0, pool.len() as i64 - 1)) as usize].clone())
    }

    /// MAIN_IDEA: "the passage is mostly about ___", the blank filled by the
    /// passage's dominant repeated topic against three topics drawn from other
    /// passages. Every distractor is *false here* by construction, not by taste.
    fn build_main_idea<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        let paragraph = self.pick_passage(rng, accept)?;
        let (topic, _count) = self.dominant_topic(&paragraph)?;
        let mut distractors: Vec<String> = Vec::new();
        // Three distinct topics from elsewhere; `topic_from_elsewhere` excludes this
        // passage's own words, so none of them can be the topic here.
        for _ in 0..24 {
            if distractors.len() == 3 {
                break;
            }
            let Some(other) = self.topic_from_elsewhere(&paragraph, rng) else {
                break;
            };
            if other != topic && !distractors.contains(&other) {
                distractors.push(other);
            }
        }
        if distractors.len() < 3 {
            return None;
        }

        let mut options: Vec<(String, bool)> = vec![(topic.clone(), true)];
        for d in &distractors {
            options.push((d.clone(), false));
        }
        rng.shuffle(&mut options);
        let correct_index = options
            .iter()
            .position(|(_, is_correct)| *is_correct)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (option, is_correct)) in options.iter().enumerate() {
            if *is_correct {
                continue;
            }
            distractor_rationales.insert(
                index,
                format!(
                    "{option:?} is the subject of another passage in this source, not of this one; \
                     this passage repeats {topic:?}."
                ),
            );
        }

        Some(PcItem {
            objective_id: "OBJ-PC-MAINIDEA-01".to_string(),
            passage: paragraph,
            prompt: "Which of the following best states what the passage is mainly about?"
                .to_string(),
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            // The evidence for a main-idea item is the passage's own repeated topic
            // word; `verify` re-checks that it is present and repeated.
            supporting_clause: topic,
            source_label: self.label.clone(),
            difficulty: 0.5,
            seed,
        })
    }

    /// VOCAB_IN_CONTEXT: a word the passage actually uses, against three words the
    /// passage does not contain. The correct answer is checkable by containment --
    /// it is in the text -- and each distractor is checkable by *absence*, which is
    /// the same discipline the detail items use.
    ///
    /// This is a weaker item than a true synonym test: it asks which word appears in
    /// the passage, which is recognition rather than meaning. It is offered here
    /// because that is the strongest vocabulary claim a passage alone can support,
    /// and the prompt is written to say exactly that rather than to claim more.
    fn build_vocab_in_context<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        let paragraph = self.pick_passage(rng, accept)?;
        let present: Vec<String> = {
            let mut set: Vec<String> = words(&paragraph)
                .into_iter()
                .filter(|w| w.len() >= 6 && !STOP_WORDS.contains(&w.as_str()))
                .collect();
            set.sort();
            set.dedup();
            set
        };
        if present.is_empty() {
            return None;
        }
        let target = present[(rng.range(0, present.len() as i64 - 1)) as usize].clone();
        let here: HashSet<String> = words(&paragraph).into_iter().collect();

        // Three plausible words that do not occur in the passage, drawn from the rest
        // of the source so they are the same register as the passage's own vocabulary.
        let mut absent: Vec<String> = Vec::new();
        for other in &self.paragraphs {
            if other == &paragraph {
                continue;
            }
            for word in words(other) {
                if word.len() >= 6 && !STOP_WORDS.contains(&word.as_str()) && !here.contains(&word)
                {
                    absent.push(word);
                }
            }
        }
        absent.sort();
        absent.dedup();
        if absent.len() < 3 {
            return None;
        }
        let mut distractors: Vec<String> = Vec::new();
        for _ in 0..64 {
            if distractors.len() == 3 {
                break;
            }
            let candidate = absent[(rng.range(0, absent.len() as i64 - 1)) as usize].clone();
            if candidate != target && !distractors.contains(&candidate) {
                distractors.push(candidate);
            }
        }
        if distractors.len() < 3 {
            return None;
        }

        let mut options: Vec<(String, bool)> = vec![(target.clone(), true)];
        for d in &distractors {
            options.push((d.clone(), false));
        }
        rng.shuffle(&mut options);
        let correct_index = options
            .iter()
            .position(|(_, is_correct)| *is_correct)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (option, is_correct)) in options.iter().enumerate() {
            if *is_correct {
                continue;
            }
            distractor_rationales.insert(
                index,
                format!("{option:?} does not occur anywhere in this passage."),
            );
        }

        Some(PcItem {
            objective_id: "OBJ-PC-VOCAB-01".to_string(),
            passage: paragraph,
            prompt: "Which of these words occurs in the passage below?".to_string(),
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            supporting_clause: target,
            source_label: self.label.clone(),
            difficulty: 0.2,
            seed,
        })
    }

    fn build_detail<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        let paragraph = self
            .paragraphs
            .get(rng.range(0, self.paragraphs.len() as i64 - 1) as usize)?;
        let characters: Vec<char> = paragraph.chars().collect();
        let spans = sentence_spans(paragraph);
        let sentences: Vec<String> = spans
            .iter()
            .map(|(from, to)| characters[*from..*to].iter().collect())
            .collect();
        // A passage needs at least two sentences. One is never enough: a passage
        // must reach `MIN_PASSAGE_WORDS` to be worth comprehending, while an option
        // must stay under `MAX_CLAUSE_WORDS` to be comparable, and a single sentence
        // that satisfies the first can never satisfy the second.
        if sentences.len() < 2 {
            return None;
        }
        // Two or three sentences: enough to carry a detail and its context, short
        // enough to read in the time PC allows.
        let take = (rng.range(2, 3) as usize).min(sentences.len());
        let start = rng.range(0, (sentences.len() - take) as i64) as usize;
        // Cut from the paragraph rather than joining the sentences back together:
        // the passage has to be the source's own text, character for character.
        let passage: String = characters[spans[start].0..spans[start + take - 1].1]
            .iter()
            .collect();

        let word_count = passage.split_whitespace().count();
        if !(MIN_PASSAGE_WORDS..=MAX_PASSAGE_WORDS).contains(&word_count) {
            return None;
        }
        if !accept(&passage) {
            return None;
        }

        let mut candidates: Vec<&String> = sentences[start..start + take]
            .iter()
            // An option must be a complete short statement. Cutting a clause out of
            // mid-sentence produced fragments like
            // `candle or from powdered charcoal) and 23 parts of oxygen by weight`
            // -- an unbalanced bracket and no verb -- which is not something a
            // learner can weigh as a claim about a passage.
            .filter(|sentence| {
                let count = sentence.split_whitespace().count();
                (MIN_CLAUSE_WORDS..=MAX_CLAUSE_WORDS).contains(&count)
            })
            .collect();
        rng.shuffle(&mut candidates);
        let supporting = candidates.first()?;
        let support_words: Vec<&str> = supporting.split_whitespace().collect();

        // The statement must carry something a distractor can alter: a quantity or a
        // name. Without one there is nothing to build a plausible distractor from
        // and the item would have to invent its options.
        let (element_position, element) = find_element(&support_words)?;

        let clause: Vec<String> = support_words.iter().map(|word| word.to_string()).collect();
        let correct = render_clause(&clause);
        if correct.split_whitespace().count() < MIN_CLAUSE_WORDS {
            return None;
        }

        let variants = clause_variants(
            &clause,
            element_position,
            &element,
            &passage,
            &self.proper_nouns,
            rng,
        );
        if variants.len() < 3 {
            return None;
        }

        let mut options: Vec<(String, bool)> = vec![(correct.clone(), true)];
        for variant in variants.into_iter().take(3) {
            options.push((variant, false));
        }
        rng.shuffle(&mut options);

        let correct_index = options
            .iter()
            .position(|(_, is_correct)| *is_correct)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (option, is_correct)) in options.iter().enumerate() {
            if *is_correct {
                continue;
            }
            let (stated, altered) = token_difference(option, &correct).unwrap_or_default();
            distractor_rationales.insert(
                index,
                format!("The passage states {stated:?} here; it does not state {altered:?}."),
            );
        }

        Some(PcItem {
            objective_id: "OBJ-PC-DETAIL-01".to_string(),
            passage,
            prompt: "According to the passage, which of the following is stated?".to_string(),
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            supporting_clause: correct,
            source_label: self.label.clone(),
            difficulty: 0.4,
            seed,
        })
    }
}

/// Something in a clause that a distractor can alter.
#[derive(Debug, Clone, PartialEq)]
enum Element {
    /// A quantity, which is the safer substitution: the passage either states it or
    /// does not.
    Number(String),
    /// A name, drawn from elsewhere in the text so the passage cannot contain it.
    Name(String),
}

/// The first alterable element in a clause, preferring a quantity.
fn find_element(words: &[&str]) -> Option<(usize, Element)> {
    // A number that labels a figure or a volume is apparatus, not a fact. Altering
    // one produced options like `with the water in this apparatus (fig. 27)`, which
    // asks the learner to compare captions rather than to read the passage.
    const APPARATUS_BEFORE_NUMBER: [&str; 8] =
        ["fig", "fig.", "(fig", "(fig.", "no", "no.", "vol", "vol."];
    for (position, word) in words.iter().enumerate() {
        let digits: String = word.chars().filter(char::is_ascii_digit).collect();
        if digits.len() < 2 {
            continue;
        }
        let previous = position
            .checked_sub(1)
            .and_then(|index| words.get(index))
            .map(|word| word.to_lowercase())
            .unwrap_or_default();
        if APPARATUS_BEFORE_NUMBER.contains(&previous.as_str()) {
            continue;
        }
        // A token that begins with a letter is a label, not a quantity: altering
        // `F14` asks the learner to compare diagram captions.
        if word.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        return Some((position, Element::Number(digits)));
    }
    for (position, word) in words.iter().enumerate() {
        if position == 0 {
            // The first word is capitalised by grammar, not by being a name.
            continue;
        }
        let cleaned = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
        // A word in full capitals is an abbreviation or a heading, and altering one
        // produces a distractor that differs in nothing checkable.
        if cleaned.len() >= 3
            && cleaned.chars().next().is_some_and(char::is_uppercase)
            && !cleaned.chars().all(|c| c.is_ascii_uppercase())
        {
            return Some((position, Element::Name(cleaned.to_string())));
        }
    }
    None
}

/// The clause with its element replaced, once per candidate substitution.
///
/// Only the digits of a quantity are replaced, never the punctuation around them.
/// Replacing a comma too turned `made of this sort--20, 30, 40` into
/// `sort--21 30, 40`, which is not a sentence anybody wrote.
fn clause_variants(
    clause: &[String],
    position: usize,
    element: &Element,
    passage: &str,
    proper_nouns: &[String],
    rng: &mut Rng,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let original = match element {
        Element::Number(digits) => digits.clone(),
        Element::Name(name) => name.clone(),
    };
    let baseline = render_clause(clause);

    let render = |replacement: &str| -> String {
        let mut words = clause.to_vec();
        if let Some(word) = words.get_mut(position) {
            *word = word.replace(&original, replacement);
        }
        render_clause(&words)
    };

    match element {
        Element::Number(digits) => {
            let Ok(value) = digits.parse::<i64>() else {
                return out;
            };
            for candidate in [
                value + 1,
                value + 2,
                value + 7,
                value + 11,
                value.saturating_sub(1),
                value.saturating_sub(2),
            ] {
                if candidate <= 0 || candidate == value {
                    continue;
                }
                let replacement = candidate.to_string();
                // A quantity the passage states elsewhere would make the distractor
                // true, which is the one thing it must not be.
                if passage_has_number(passage, &replacement) {
                    continue;
                }
                let variant = render(&replacement);
                if variant != baseline && !out.contains(&variant) {
                    out.push(variant);
                }
                if out.len() >= 6 {
                    return out;
                }
            }
        }
        Element::Name(name) => {
            for _ in 0..12 {
                let candidate = rng.pick(proper_nouns).clone();
                if candidate.eq_ignore_ascii_case(name)
                    || passage.to_lowercase().contains(&candidate.to_lowercase())
                {
                    continue;
                }
                let variant = render(&candidate);
                if variant != baseline && !out.contains(&variant) {
                    out.push(variant);
                }
                break;
            }
        }
    }

    out
}

/// Render a clause, dropping the boundary mark that ended it.
fn render_clause(words: &[String]) -> String {
    words
        .join(" ")
        .trim_matches(|c: char| {
            c == ','
                || c == ';'
                || c == ':'
                || c == ' '
                || c == '\u{2014}'
                || c == '\u{2013}'
                || c == '-'
        })
        .trim()
        .to_string()
}

/// The word by which a distractor differs from the correct clause, as written.
///
/// Both words are returned because the learner-facing explanation is only useful if
/// it can name what the passage does say alongside what the option says. Only the
/// surrounding punctuation is trimmed: trimming everything that is not an ASCII
/// alphanumeric turned `18⅓` into `18`, so the explanation quoted a number that
/// appears nowhere.
fn token_difference(distractor: &str, correct: &str) -> Option<(String, String)> {
    let original: Vec<&str> = correct.split_whitespace().collect();
    let altered: Vec<&str> = distractor.split_whitespace().collect();
    if original.len() != altered.len() {
        return None;
    }
    let bare = |word: &str| {
        word.trim_matches(|c: char| !c.is_alphanumeric())
            .to_string()
    };
    original
        .iter()
        .zip(altered.iter())
        .find(|(a, b)| bare(a) != bare(b))
        .map(|(a, b)| (bare(a), bare(b)))
}

/// Whether the passage states this quantity as a number of its own.
fn passage_has_number(passage: &str, digits: &str) -> bool {
    passage
        .split(|c: char| !c.is_ascii_digit())
        .any(|run| run == digits)
}

/// Independently verify a Paragraph Comprehension item against its own passage.
///
/// Re-derives the facts from the passage text rather than trusting the builder: the
/// correct option must be stated in the passage, no distractor may be, and no
/// distractor may differ only by a token the passage also contains.
pub fn verify(item: &PcItem) -> Result<(), PcVerificationFailure> {
    if item.passage.trim().is_empty() {
        return Err(PcVerificationFailure::Blank("passage".to_string()));
    }
    if item.prompt.trim().is_empty() {
        return Err(PcVerificationFailure::Blank("prompt".to_string()));
    }
    if item.supporting_clause.trim().is_empty() {
        return Err(PcVerificationFailure::Blank(
            "supporting clause".to_string(),
        ));
    }
    let passage_words = item.passage.split_whitespace().count();
    if !(MIN_PASSAGE_WORDS..=MAX_PASSAGE_WORDS).contains(&passage_words) {
        return Err(PcVerificationFailure::PassageLength {
            words: passage_words,
        });
    }
    if item.options.len() < 2 {
        return Err(PcVerificationFailure::TooFewOptions(item.options.len()));
    }
    if item.correct_index >= item.options.len() {
        return Err(PcVerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }

    let haystack = normalize(&item.passage);
    let passage_tokens: HashSet<String> = words(&item.passage).into_iter().collect();

    // A single-word item (main idea, vocabulary) is checked by word, not by clause:
    // the answer is a word the passage uses, so requiring it to be a passage-length
    // clause would refuse every one of them. Branching on the objective keeps the
    // detail checks exactly as strict as they were.
    let single_word = matches!(
        item.objective_id.as_str(),
        "OBJ-PC-MAINIDEA-01" | "OBJ-PC-VOCAB-01"
    );
    let correct = &item.options[item.correct_index];
    if single_word {
        // `words()` lowercases and strips punctuation, and `passage_tokens` is built
        // from it, so membership is a word-level test rather than a substring one.
        if !passage_tokens.contains(&correct.to_lowercase()) {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                correct.clone(),
            ));
        }
        if correct.to_lowercase() != item.supporting_clause.to_lowercase() {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                item.supporting_clause.clone(),
            ));
        }
        // MAIN_IDEA's answer has to be a *topic*: the passage must repeat it. A word
        // that appears once is not what the passage is mainly about, so the item
        // would be asserting something the text does not support.
        if item.objective_id == "OBJ-PC-MAINIDEA-01" {
            let occurrences = words(&item.passage)
                .into_iter()
                .filter(|word| word == &correct.to_lowercase())
                .count();
            if occurrences < 2 {
                return Err(PcVerificationFailure::TopicNotRepeated {
                    topic: correct.clone(),
                    occurrences,
                });
            }
        }
    } else {
        if !haystack.contains(&normalize(correct)) {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                correct.clone(),
            ));
        }
        if normalize(correct) != normalize(&item.supporting_clause) {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                item.supporting_clause.clone(),
            ));
        }
    }

    for (position, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(PcVerificationFailure::Blank(format!("option {position}")));
        }
        if item.options[..position]
            .iter()
            .any(|earlier| normalize(earlier) == normalize(option))
        {
            return Err(PcVerificationFailure::DuplicateOption(option.clone()));
        }
        if single_word {
            // Every option is one word, so they are comparable by construction. The
            // clause-length band below is a detail-item rule and does not apply.
            if position == item.correct_index {
                continue;
            }
            if passage_tokens.contains(&option.to_lowercase()) {
                return Err(PcVerificationFailure::DistractorAlsoInPassage(
                    option.clone(),
                ));
            }
            continue;
        }
        // Options must be comparable in length, or the odd one out is answerable by
        // shape rather than by reading the passage.
        let option_words = option.split_whitespace().count();
        if !(MIN_CLAUSE_WORDS..=MAX_CLAUSE_WORDS).contains(&option_words) {
            return Err(PcVerificationFailure::ClauseLength {
                option: option.clone(),
                words: option_words,
            });
        }
        if position == item.correct_index {
            continue;
        }
        if haystack.contains(&normalize(option)) {
            return Err(PcVerificationFailure::DistractorAlsoInPassage(
                option.clone(),
            ));
        }
        // The option has to be false *because of* its altered word. If that word
        // also appears in the passage, the passage does not rule the option out.
        if let Some((stated, altered)) = token_difference(option, &item.supporting_clause) {
            let bare = altered.to_lowercase();
            if !bare.is_empty() && passage_tokens.contains(&bare) {
                return Err(PcVerificationFailure::DistractorTokenPresent {
                    option: option.clone(),
                    token: format!("{altered} (the passage states {stated})"),
                });
            }
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
        return Err(PcVerificationFailure::RationaleCoverage { missing, extra });
    }
    for (position, rationale) in &item.distractor_rationales {
        if rationale.trim().is_empty() {
            return Err(PcVerificationFailure::Blank(format!(
                "rationale for option {position}"
            )));
        }
    }

    Ok(())
}
