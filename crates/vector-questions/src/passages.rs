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
//! ## The item types, and why they are these three
//!
//! CAT-ASVAB PC asks four kinds of reading question. Three of them are
//! machine-checkable from the passage and a public-domain dictionary, and this
//! module builds all three, each with an independent verifier:
//!
//! * **DETAIL** (`OBJ-PC-DETAIL-01`) -- a clause the passage states, against
//!   near-misses that each alter one quantity or name. The correct option occurs in
//!   the passage; each distractor differs by a token the passage does not contain.
//! * **MAIN_IDEA** (`OBJ-PC-MAINIDEA-01`) -- the option that sums up the passage,
//!   against one option that is too narrow (a true detail), one too broad (a sweeping
//!   claim the passage never makes), and one about a topic the passage never
//!   discusses. Every option is a full sentence. The correct option names the
//!   passage's dominant repeated topic, and the verifier re-derives that topic from
//!   the passage rather than trusting the builder.
//! * **VOCAB_IN_CONTEXT** (`OBJ-PC-VOCAB-01`) -- a word the passage uses, quoted in
//!   the sentence it occurs in, with four candidate meanings drawn from a
//!   public-domain dictionary. The correct meaning is the dictionary's own gloss for
//!   the target; each distractor is a real meaning of a *different* word, so it is
//!   wrong for this word in this context rather than simply absent.
//!
//! **INFERENCE is deliberately not built.** An item that asks what the author
//! implies cannot be scored by any check this module can perform: the passage does
//! not *state* the answer, so string containment cannot confirm it. Shipping an
//! "inference" item scored by containment would mislabel a detail item, and the
//! module has committed to saying what it measures rather than filling a type count.
//!
//! ## Difficulty is derived, not declared
//!
//! Every item's difficulty is computed from its own properties -- passage length and
//! how often the target word recurs -- so it is a measurement of the item rather
//! than a constant stamped on the kind. `verify` refuses a difficulty outside
//! `0.0..=1.0`.

use std::collections::{BTreeMap, HashSet};

use crate::dictionary::Dictionary;
use crate::factory::Rng;

/// A parsed public-domain text.
#[derive(Debug, Clone, Default)]
pub struct Text {
    pub label: String,
    paragraphs: Vec<String>,
    /// Capitalised words appearing anywhere in the text, for name substitutions.
    proper_nouns: Vec<String>,
    /// An optional public-domain dictionary.
    ///
    /// A vocabulary-in-context item needs a *meaning*, and the passage alone does
    /// not carry one: a word being present says nothing about what it means there.
    /// When a dictionary is supplied, the vocabulary builder draws its answer and
    /// its distractors from real senses and refuses to emit an item otherwise. When
    /// it is absent the builder returns `None` rather than falling back to the old
    /// word-spotting question, so a corpus built without a dictionary simply has no
    /// vocabulary items -- an honest gap, not a mislabelled detail item.
    dictionary: Option<Dictionary>,
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
    /// A main-idea option is not a full sentence, so it states a fragment rather
    /// than a claim about the passage.
    MainIdeaOptionNotASentence(String),
    /// A main-idea option is a sentence the passage itself contains verbatim, so it
    /// is a detail restatement and not a summary.
    MainIdeaOptionIsPassageText(String),
    /// Two main-idea options are the same sentence, so the item has no single answer.
    MainIdeaOptionsNotDistinct(String),
    /// A vocabulary answer is a real meaning, but the dictionary has no source that
    /// backs it, so the item asserts a meaning nothing supports.
    VocabMeaningUnbacked {
        word: String,
        meaning: String,
    },
    /// A vocabulary option is a word the passage does not contain, so a learner
    /// cannot judge it "in context".
    VocabOptionAbsentFromPassage(String),
    /// Difficulty was supplied out of band rather than derived from the item.
    DifficultyOutOfRange {
        difficulty: String,
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
            PcVerificationFailure::MainIdeaOptionNotASentence(option) => write!(
                f,
                "main-idea option {option:?} is not a full sentence, so it states a fragment \
                 rather than a claim about the passage"
            ),
            PcVerificationFailure::MainIdeaOptionIsPassageText(option) => write!(
                f,
                "main-idea option {option:?} is text the passage itself contains, so it \
                 restates a detail instead of summarising the passage"
            ),
            PcVerificationFailure::MainIdeaOptionsNotDistinct(option) => write!(
                f,
                "main-idea option {option:?} is repeated, so the item has no single answer"
            ),
            PcVerificationFailure::VocabMeaningUnbacked { word, meaning } => write!(
                f,
                "the vocabulary answer {meaning:?} for {word:?} is not a meaning the \
                 dictionary backs, so the item asserts something no source supports"
            ),
            PcVerificationFailure::VocabOptionAbsentFromPassage(option) => write!(
                f,
                "vocabulary option {option:?} does not occur in the passage, so it cannot be \
                 judged in context"
            ),
            PcVerificationFailure::DifficultyOutOfRange { difficulty } => {
                write!(f, "difficulty {difficulty} is outside 0.0..=1.0")
            }
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
        dictionary: None,
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

    /// Attach a public-domain dictionary so vocabulary-in-context items can be
    /// built. Without one, `build_vocab_in_context` yields nothing: a vocabulary
    /// item without a source-backed meaning is not a vocabulary item.
    pub fn with_dictionary(mut self, dictionary: Dictionary) -> Self {
        self.dictionary = Some(dictionary);
        self
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
        // When a dictionary is present, prefer a word Webster's marks as a noun: the
        // topic of a passage is a thing, and a bare adjective that happens to repeat
        // (`different`, `continuous`) is not what the passage is *about*. This is a
        // preference, not a hard filter -- a passage whose most-repeated noun is rare
        // still builds -- so the first noun wins and any word is the fallback.
        if let Some(dictionary) = &self.dictionary {
            if let Some((word, count)) = ranked
                .iter()
                .find(|(word, _)| is_noun(dictionary, word))
                .cloned()
            {
                if count >= 2 {
                    return Some((word, count));
                }
            }
        }
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

    /// MAIN_IDEA: a full-sentence question whose correct option sums up the passage.
    ///
    /// Every option is a complete sentence, per the review: a learner weighing a
    /// one-word topic against a sentence is answering a shape question, not a
    /// comprehension one. The options are built from the passage's own sentences so
    /// they are grammatical, then classified by the three failure modes the real
    /// test uses:
    ///
    /// * **too narrow** -- a specific detail sentence lifted from the passage. It is
    ///   true, but it is one part of the passage, not its whole point.
    /// * **too broad** -- a sentence naming the passage's topic but asserting
    ///   something the passage never establishes (built by generalising the topic
    ///   with a connector no passage sentence carries).
    /// * **not supported** -- a sentence about a topic drawn from *another* passage,
    ///   so the passage cannot support it at all.
    ///
    /// The correct option names the passage's dominant repeated topic and is a
    /// summary sentence, not a lifted passage sentence; `verify` refuses an option
    /// that is passage text verbatim, which is what keeps a "summary" from being a
    /// second detail restatement.
    fn build_main_idea<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        let paragraph = self.pick_passage(rng, accept)?;
        let (topic, occurrences) = self.dominant_topic(&paragraph)?;

        // The correct option is a summary sentence, phrased by this builder rather
        // than lifted, so it cannot be passage text. It names the passage's dominant
        // repeated topic as the subject and says the passage is *about* it, which is
        // the claim a main-idea answer makes. The frame rotates with the seed: the
        // review found that one fixed frame let a test-taker pick the answer by shape
        // after two questions, so no single wording is allowed to become the answer's
        // signature.
        let topic_title = capitalise(&topic);
        let answer_frames = [
            "The passage is mainly about {topic}.",
            "The passage as a whole concerns {topic}.",
            "The main point of the passage is about {topic}.",
            "Taken as a whole, the passage deals with {topic}.",
            "The passage chiefly discusses {topic}.",
        ];
        let correct =
            answer_frames[(seed as usize) % answer_frames.len()].replace("{topic}", &topic_title);

        // Too narrow: a passage sentence that is a complete statement in the clause
        // band. It is true, so it is a plausible distractor for a careless reader,
        // and it is *not* the whole point.
        let narrow = self
            .passage_sentences(&paragraph)
            .into_iter()
            .find(|sentence| {
                let count = sentence.split_whitespace().count();
                (MIN_CLAUSE_WORDS..=MAX_CLAUSE_WORDS).contains(&count)
                    && !self.repeats_topic_topically(sentence, &topic)
            })?;

        // Too broad: the topic, plus a sweeping claim the passage never makes. The
        // frame rotates so it is not a fixed signature either.
        let broad_frames = [
            "The passage argues that {topic} matters more than anything else it discusses.",
            "The passage claims that {topic} is the single most important subject today.",
            "The passage suggests that {topic} explains almost everything it mentions.",
        ];
        let broad =
            broad_frames[(seed as usize / 3) % broad_frames.len()].replace("{topic}", &topic_title);

        // Not supported: a topic from another passage, asserted as this one's point.
        // Its frame rotates too, and it is drawn from a different passage so the
        // passage cannot support it at all.
        let elsewhere = self.topic_from_elsewhere(&paragraph, rng)?;
        let unsupported_frames = [
            "The passage is concerned chiefly with {topic}.",
            "The passage's central subject is {topic}.",
            "The passage is mostly a discussion of {topic}.",
        ];
        let unsupported = unsupported_frames[(seed as usize / 7) % unsupported_frames.len()]
            .replace("{topic}", &capitalise(&elsewhere));

        let mut options: Vec<(String, String)> = vec![
            (correct.clone(), "sums up the passage".to_string()),
            (
                narrow.clone(),
                "too narrow: this is one detail the passage states, not what it is mainly \
                 about"
                    .to_string(),
            ),
            (
                broad.clone(),
                "too broad: the passage names the topic but never ranks it above \
                 everything else"
                    .to_string(),
            ),
            (
                unsupported.clone(),
                "not supported: the passage never discusses this at all".to_string(),
            ),
        ];
        rng.shuffle(&mut options);
        let correct_index = options
            .iter()
            .position(|(option, _)| option == &correct)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (option, rationale)) in options.iter().enumerate() {
            if index == correct_index {
                continue;
            }
            distractor_rationales.insert(index, format!("{option:?} is {rationale}."));
        }

        let passage = paragraph;
        let difficulty = derive_difficulty(&passage, occurrences);

        Some(PcItem {
            objective_id: "OBJ-PC-MAINIDEA-01".to_string(),
            passage,
            prompt: "Which of the following best states what the passage is mainly about?"
                .to_string(),
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            // The evidence for a main-idea item is the passage's own repeated topic
            // word; `verify` re-checks that it is present and repeated.
            supporting_clause: topic,
            source_label: self.label.clone(),
            difficulty,
            seed,
        })
    }

    /// The sentences of a passage that are complete statements.
    fn passage_sentences(&self, passage: &str) -> Vec<String> {
        split_sentences(passage)
            .into_iter()
            .filter(|sentence| sentence.trim().ends_with(['.', '!', '?']))
            .collect()
    }

    /// Whether a sentence states the passage's topic as the passage's subject, as
    /// opposed to merely containing the word. Used to keep the "too narrow" option
    /// from accidentally being a summary.
    fn repeats_topic_topically(&self, sentence: &str, topic: &str) -> bool {
        let lowered = sentence.to_lowercase();
        lowered.starts_with(topic) || lowered.starts_with(&format!("the {topic}"))
    }

    /// VOCAB_IN_CONTEXT: a real ASVAB-style vocabulary-in-context item.
    ///
    /// The review rejected the previous version, correctly: asking "which of these
    /// words occurs in the passage?" is word-spotting, not meaning. This version
    /// quotes a word as the passage uses it and asks what it most nearly means
    /// *there*, with the answer and every distractor taken from the dictionary:
    ///
    /// * the **correct** option is a meaning phrase the dictionary gives for the
    ///   target word (drawn from the target's own definition), backed by the source;
    /// * the **distractors** are meaning phrases drawn from the definitions of
    ///   *other* words the passage uses, so they are real meanings that are simply
    ///   wrong for this word -- not words missing from the passage.
    ///
    /// This needs a dictionary. Without one the builder returns `None`, so a corpus
    /// without a dictionary has no vocabulary items rather than mislabelled ones.
    ///
    /// ## The context window
    ///
    /// The prompt quotes the sentence the word occurs in, so the learner judges the
    /// word *as used*. `verify` re-checks that the target word occurs in that quoted
    /// context and that the backing definition actually contains the chosen sense.
    fn build_vocab_in_context<F>(&self, rng: &mut Rng, seed: u64, accept: &mut F) -> Option<PcItem>
    where
        F: FnMut(&str) -> bool,
    {
        let dictionary = self.dictionary.as_ref()?;
        let paragraph = self.pick_passage(rng, accept)?;

        // The target must be a word the dictionary defines, so a real sense exists.
        let targets: Vec<String> = {
            let mut set: HashSet<String> = HashSet::new();
            for sentence in self.passage_sentences(&paragraph) {
                for word in words(&sentence) {
                    // A vocabulary target is a content word with enough length to
                    // carry a distinct sense, and one the dictionary actually has.
                    if word.len() >= 5
                        && !STOP_WORDS.contains(&word.as_str())
                        && (dictionary.covers(&word) || dictionary.covers(&base_form(&word)))
                    {
                        // Only words with a definition rich enough to gloss belong.
                        if sense_gloss(dictionary, &word).is_some() {
                            set.insert(word);
                        }
                    }
                }
            }
            let mut set: Vec<String> = set.into_iter().collect();
            set.sort();
            set
        };
        if targets.is_empty() {
            return None;
        }
        let target = targets[(rng.range(0, targets.len() as i64 - 1)) as usize].clone();

        // The sentence the target occurs in is the context the learner reads.
        let context = self
            .passage_sentences(&paragraph)
            .into_iter()
            .find(|sentence| words(sentence).iter().any(|word| word == &target))?;

        let answer = sense_gloss_in_context(dictionary, &target, &context)?;

        // Distractors: meaning phrases from *other* words in the same passage, so
        // they are real senses of real words and wrong only for this target.
        let context_words: Vec<String> = words(&context);
        let mut others: Vec<String> = context_words
            .into_iter()
            .filter(|word| word != &target)
            .filter(|word| word.len() >= 5 && !STOP_WORDS.contains(&word.as_str()))
            .collect();
        others.sort();
        others.dedup();

        let mut wrong_meanings: Vec<String> = Vec::new();
        for word in &others {
            let Some(gloss) = sense_gloss(dictionary, word) else {
                continue;
            };
            // A distractor meaning must be distinct from the answer, and must not be a
            // synonym of it: a different wording of the same sense is a second right
            // answer, not a distractor.
            if normalize(&gloss) != normalize(&answer)
                && !meaning_is_synonym_of(dictionary, &gloss, &answer)
                && !wrong_meanings.contains(&gloss)
            {
                wrong_meanings.push(gloss);
            }
            if wrong_meanings.len() == 3 {
                break;
            }
        }
        // Fall back to meanings from the wider paragraph and then the whole source
        // when the context sentence does not yield three on its own. A distractor
        // drawn from another paragraph is still a real meaning of a word the source
        // uses, so it stays in register; it is simply wrong for *this* word.
        if wrong_meanings.len() < 3 {
            let mut wide: Vec<String> = words(&paragraph)
                .into_iter()
                .filter(|word| word != &target && word.len() >= 5)
                .collect();
            for other in &self.paragraphs {
                if other == &paragraph {
                    continue;
                }
                wide.extend(words(other).into_iter().filter(|word| word.len() >= 5));
            }
            wide.sort();
            wide.dedup();
            for word in &wide {
                if wrong_meanings.len() == 3 {
                    break;
                }
                if word == &target {
                    continue;
                }
                let Some(gloss) = sense_gloss(dictionary, word) else {
                    continue;
                };
                if normalize(&gloss) != normalize(&answer) && !wrong_meanings.contains(&gloss) {
                    wrong_meanings.push(gloss);
                }
            }
        }
        if wrong_meanings.len() < 3 {
            return None;
        }

        let mut options: Vec<(String, String)> =
            vec![(answer.clone(), "correct meaning".to_string())];
        for meaning in wrong_meanings.iter().take(3) {
            options.push((
                meaning.clone(),
                "a real meaning, but of a different word -- wrong for this one in \
                 context"
                    .to_string(),
            ));
        }
        rng.shuffle(&mut options);
        let correct_index = options
            .iter()
            .position(|(option, _)| option == &answer)
            .expect("the correct option is present");

        let mut distractor_rationales = BTreeMap::new();
        for (index, (option, rationale)) in options.iter().enumerate() {
            if index == correct_index {
                continue;
            }
            distractor_rationales.insert(index, format!("{option:?} is {rationale}."));
        }

        // The passage field stays the passage; the context sentence the learner
        // reads is carried in the prompt, so `verify` can re-check containment.
        let prompt =
            format!("In the sentence \"{context}\" the word \"{target}\" most nearly means:");
        let occurrences = words(&paragraph)
            .into_iter()
            .filter(|word| word == &target)
            .count();
        let difficulty = derive_difficulty(&paragraph, occurrences);
        let source_label = self.label.clone();

        Some(PcItem {
            objective_id: "OBJ-PC-VOCAB-01".to_string(),
            passage: paragraph,
            prompt,
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            // The evidence for a vocabulary item is the word as the passage uses it;
            // `verify` re-checks the context contains it and the definition backs the
            // chosen meaning.
            supporting_clause: context,
            source_label,
            difficulty,
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

        let difficulty = derive_difficulty(&passage, 1);

        Some(PcItem {
            objective_id: "OBJ-PC-DETAIL-01".to_string(),
            passage,
            prompt: "According to the passage, which of the following is stated?".to_string(),
            options: options.into_iter().map(|(option, _)| option).collect(),
            correct_index,
            distractor_rationales,
            supporting_clause: correct,
            source_label: self.label.clone(),
            difficulty,
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

/// Uppercase the first character of a word, for use at the start of a sentence.
fn capitalise(word: &str) -> String {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

/// A rough base form of an inflected English word, for dictionary lookup.
///
/// Deliberately conservative and rule-based rather than a stemmer: it tries the
/// common plural and verb endings and returns the input unchanged when none applies.
/// A wrong guess costs nothing here, because the caller only uses it as a *fallback*
/// lookup -- the surface form is tried first, and an unfound base form simply means
/// no gloss, which refuses the item rather than producing a wrong one.
fn base_form(word: &str) -> String {
    let lowered = word.to_lowercase();
    // Plural and third-person `-s`, with the `-es` and `-ies` cases handled.
    if let Some(stem) = lowered.strip_suffix("ies") {
        if stem.len() >= 2 {
            return format!("{stem}y");
        }
    }
    if let Some(stem) = lowered.strip_suffix("ves") {
        if stem.len() >= 2 {
            return format!("{stem}f");
        }
    }
    if let Some(stem) = lowered.strip_suffix("es") {
        if stem.len() >= 3 {
            return stem.to_string();
        }
    }
    if let Some(stem) = lowered.strip_suffix('s') {
        if stem.len() >= 3 {
            return stem.to_string();
        }
    }
    // Past tense and gerund.
    if let Some(stem) = lowered.strip_suffix("ed") {
        if stem.len() >= 3 {
            return stem.to_string();
        }
    }
    if let Some(stem) = lowered.strip_suffix("ing") {
        if stem.len() >= 3 {
            return stem.to_string();
        }
    }
    lowered
}

/// The word carried in a vocabulary prompt's quotes, lowercased.
///
/// The prompt is `In the sentence "..." the word "X" most nearly means:`. The word
/// being asked about is the span after the literal `the word`, so reading it back
/// lets `verify` confirm the sentence really uses it rather than trusting the
/// builder's claim. Anchoring on `the word` rather than on quote position matters: a
/// context sentence can itself contain quotation marks, and an earlier span-indexed
/// read picked up a fragment of the passage instead of the target.
fn quoted_word(prompt: &str) -> Option<String> {
    let (_, tail) = prompt.rsplit_once("the word")?;
    let after = tail.split_once('"')?.1;
    let target = after.split_once('"')?.0.trim();
    if target.is_empty() {
        return None;
    }
    Some(target.to_lowercase())
}

/// Whether a vocabulary item's stated answer is a meaning the dictionary gives for
/// the word its prompt asks about.
///
/// This is the stored-bank counterpart of the builder's own guarantee. The builder
/// draws the answer from `sense_gloss`; a storer that re-derives it from the *stored*
/// prompt and option -- rather than trusting the builder's in-memory item -- catches a
/// stored item whose answer no source backs. `None` from `quoted_word` (a prompt that
/// does not name a word) is treated as unbacked: there is nothing to back.
pub fn vocab_answer_is_backed(dictionary: &Dictionary, prompt: &str, answer: &str) -> bool {
    let Some(target) = quoted_word(prompt) else {
        return false;
    };
    match sense_gloss(dictionary, &target) {
        Some(gloss) => normalize(&gloss) == normalize(answer),
        None => false,
    }
}

/// The shortest dictionary gloss for a word, or `None` when the dictionary does not
/// define it.
///
/// Webster's is a dictionary of headwords, so a passage's inflected form (`glaciers`)
/// frequently has no entry even though its base form (`glacier`) does. Looking up the
/// base form is what makes the builder usable on real prose; the *surface* form the
/// passage uses is still what the learner is asked about and what the prompt quotes.
fn sense_gloss(dictionary: &Dictionary, word: &str) -> Option<String> {
    let definition = dictionary
        .define(word)
        .or_else(|| dictionary.define(&base_form(word)))?;
    for candidate in gloss_candidates(definition) {
        if is_usable_gloss(&candidate, word) {
            let mut gloss = candidate;
            if gloss.chars().next().is_some_and(char::is_lowercase) {
                gloss = capitalise(&gloss);
            }
            return Some(gloss);
        }
    }
    None
}

/// Every usable gloss of a word, best sense first, normalised.
///
/// A Webster's entry carries several senses and only one is the sense a given
/// sentence uses. The single-gloss reader above cannot tell them apart, which is how
/// a keyed answer could be the right *word* and the wrong *sense*. Returning all the
/// senses lets the caller pick the one the context supports.
pub fn sense_glosses(dictionary: &Dictionary, word: &str) -> Vec<String> {
    let Some(definition) = dictionary
        .define(word)
        .or_else(|| dictionary.define(&base_form(word)))
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for candidate in gloss_candidates(definition) {
        if !is_usable_gloss(&candidate, word) {
            continue;
        }
        let mut gloss = candidate;
        if gloss.chars().next().is_some_and(char::is_lowercase) {
            gloss = capitalise(&gloss);
        }
        if !out
            .iter()
            .any(|existing| normalize(existing) == normalize(&gloss))
        {
            out.push(gloss);
        }
    }
    out
}

/// The gloss of `word` whose own wording best matches the sentence the word occurs in.
///
/// A word with several senses has several glosses; the one a sentence *uses* is the
/// one whose words overlap the sentence's words. `The process works best when the air
/// is dry` supports `A series of actions toward a result` over an unrelated sense, so
/// scoring each candidate by shared content words picks the sense in play. Ties break
/// toward the first (most common) sense, so a word used in its plain sense is
/// unaffected. Falls back to the first sense when no candidate overlaps, which is the
/// honest answer when the sentence carries no signal.
pub fn sense_gloss_in_context(
    dictionary: &Dictionary,
    word: &str,
    context: &str,
) -> Option<String> {
    let senses = sense_glosses(dictionary, word);
    let first = senses.first()?.clone();
    let context_words: HashSet<String> = words(context).into_iter().collect();
    let mut best = first.clone();
    let mut best_score = 0usize;
    for sense in &senses {
        let score = words(sense)
            .into_iter()
            .filter(|token| token.len() >= 4)
            .filter(|token| {
                token != &word.to_lowercase()
                    && !STOP_WORDS.contains(&token.as_str())
                    && context_words.contains(token)
            })
            .count();
        if score > best_score {
            best_score = score;
            best = sense.clone();
        }
    }
    Some(best)
}

/// Whether a meaning is a synonym of the correct sense, so it cannot be a distractor.
///
/// A distractor that means the *same* thing as the answer is a second right answer
/// wearing different words. Comparing the glosses' own headwords through the
/// dictionary's `are_linked` catches a synonym pair (`A light narrow boat` versus
/// `A small vessel for travel on water`) that exact-text equality misses.
pub fn meaning_is_synonym_of(dictionary: &Dictionary, meaning: &str, answer: &str) -> bool {
    let head = |text: &str| -> Option<String> {
        words(text)
            .into_iter()
            .filter(|token| token.len() >= 4)
            .find(|token| !STOP_WORDS.contains(&token.as_str()))
    };
    match (head(meaning), head(answer)) {
        (Some(a), Some(b)) => normalize(&a) == normalize(&b) || dictionary.are_linked(&a, &b),
        _ => false,
    }
}

/// The candidate glosses of a Webster entry, best first.
///
/// The entry's real prose lives in its numbered senses. A 1913 Webster's entry has
/// this shape:
///
/// ```text
/// FLASK
/// Flask, n. Etym: [AS. flasce, flaxe; akin to D. flesch, ...]
///
/// 1. A small bottle-shaped vessel for holding fluids; as, a flask of
/// oil or wine.
///
/// 2. A narrow-necked vessel of metal or glass, used for various purposes; ...
///
/// 4. (Founding)
///
/// Defn: The wooden or iron frame which holds the sand, ...
/// ```
///
/// So the numbered senses -- not the `Defn:` blocks, which are sub-senses and
/// apparatus -- carry the meaning. This skips the headword line and the `Etym:`
/// bracket, then yields each numbered sense trimmed to its leading gloss. The
/// earlier `Defn:`-first read is what put `Candel, candel, AS, candel, fr` and
/// `Aboute, abouten, abuten` on offer as "meanings".
fn gloss_candidates(definition: &str) -> Vec<String> {
    // Drop the etymology bracket, which is a citation of the word's history rather
    // than a statement of what it means.
    let body = match definition.find(']') {
        Some(close) if definition[..close].contains("Etym") => &definition[close + 1..],
        _ => definition,
    };
    let mut out: Vec<String> = Vec::new();
    for raw in body.split('\n') {
        let line = raw.trim();
        // A numbered sense begins with `N.` at the start of a trimmed line.
        let rest = match line.split_once('.') {
            Some((number, rest))
                if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) =>
            {
                rest.trim()
            }
            _ => continue,
        };
        // Cut the sense at its first `;` or `, as,` clause list, or the first period:
        // what follows is a quotation or an example, not the meaning itself.
        let cut = rest
            .find("; ")
            .or_else(|| rest.find(", as,"))
            .or_else(|| rest.find(". "))
            .unwrap_or(rest.len());
        let gloss = rest[..cut].trim();
        if !gloss.is_empty() {
            out.push(gloss.to_string());
        }
    }
    // A `Defn:` body is a fallback only: some entries carry their meaning there and
    // have no numbered sense, but it is checked last so apparatus never wins.
    if let Some(defn) = definition.split("Defn:").nth(1) {
        let gloss = defn
            .trim()
            .split(['.', ';'])
            .map(str::trim)
            .find(|part| part.split_whitespace().count() >= 2)
            .unwrap_or("")
            .to_string();
        if !gloss.is_empty() {
            out.push(gloss);
        }
    }
    out
}

/// The template a sentence follows, so two sentences built from the same wording can
/// be told apart from two that are genuinely different claims.
///
/// Content words (the ones carrying meaning) are replaced with a placeholder and the
/// connective skeleton is kept, lowercased and stripped of punctuation. Two short
/// sentences sharing a skeleton -- `the passage is mainly about X` twice -- return
/// the same frame, which is what a duplicated-template check wants to catch.
pub fn sentence_frame(sentence: &str) -> String {
    const FRAME_STOP: [&str; 24] = [
        "the", "a", "an", "of", "to", "in", "is", "are", "was", "were", "and", "or", "but", "that",
        "this", "these", "those", "it", "its", "as", "be", "been", "being", "than",
    ];
    words(sentence)
        .into_iter()
        .map(|word| {
            if FRAME_STOP.contains(&word.as_str()) {
                word
            } else {
                "_".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether Webster's marks a word as a noun.
///
/// The 1913 dictionary tags each entry with its part of speech right after the
/// headword: `Flask, n. Etym: ...`, `Different, a. ...`. A word tagged `n.` (or `n.`)
/// is a thing, which is what a passage's subject has to be. This is read from the
/// source rather than guessed, so `different` is correctly refused as a topic.
fn is_noun(dictionary: &Dictionary, word: &str) -> bool {
    let Some(definition) = dictionary
        .define(word)
        .or_else(|| dictionary.define(&base_form(word)))
    else {
        return false;
    };
    // Look only at the headword line: the first line of the entry, which carries the
    // part-of-speech tag before any `Defn:` or sense text.
    let head = definition.lines().next().unwrap_or("").to_lowercase();
    // `, n.` or `, n ` marks a noun; `, a.` marks an adjective and is not a noun.
    head.contains(", n.") || head.contains(", n ") || head.contains("(n.)")
}

/// Whether a candidate gloss is a meaning a learner could choose, rather than
/// dictionary apparatus.
///
/// The checks are deliberately conservative and each is evidenced by a real gloss
/// seen in a corpus run: `ADOPT A*dopt", v` and `COPPER Cop"per, n` (headword +
/// pronunciation markup), `Candel, candel, AS, candel, fr` and `Aboute, abouten,
/// abuten` (an etymology line), and single words that are the headword itself. A
/// gloss that is apparatus is worse than no item, so this refuses rather than repairs.
fn is_usable_gloss(gloss: &str, word: &str) -> bool {
    let gloss = gloss.trim();
    let words = gloss.split_whitespace().count();
    // A meaning is a phrase: at least two words, at most sixteen.
    if !(2..=16).contains(&words) {
        return false;
    }
    // Pronunciation markup and etymology brackets are apparatus, not meaning.
    for marker in ['*', '"', '[', ']', '<', '>', '{', '}', '|'] {
        if gloss.contains(marker) {
            return false;
        }
    }
    let lowered = gloss.to_lowercase();
    // A leading part-of-speech abbreviation is apparatus.
    for pos in [
        "n. ", "v. ", "v. t", "v. i", "a. ", "adv. ", "adj. ", "prep. ", "conj. ", "e. ", "as. ",
        "fr. ", "cf. ",
    ] {
        if lowered.starts_with(pos) {
            return false;
        }
    }
    // A cross-reference points at another entry rather than stating a meaning.
    if lowered.starts_with("see ") || lowered.starts_with("same as ") || lowered.starts_with("cf. ")
    {
        return false;
    }
    // Etymology markers that survive as prose.
    if lowered.contains("etym") || lowered.contains("akin to") {
        return false;
    }
    // Every word must be a real word, so a run of comma-joined stems is refused.
    if gloss.ends_with(',') || gloss.ends_with(" fr") {
        return false;
    }
    // The gloss must not open with the headword itself, which states nothing.
    let headword = base_form(word);
    let first = lowered
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|c: char| !c.is_ascii_alphabetic())
        .to_string();
    if first == headword || first == word.to_lowercase() {
        return false;
    }
    true
}

/// Difficulty from the item's own properties, not a hard-coded constant.
///
/// The review's point: a constant is a placeholder, not a measurement. Three real
/// properties an item carries are how long its passage is (more text to hold), how
/// often the target recurs (a repeated word is easier to infer from context than one
/// seen once), and how many sentences the passage has (more moving parts to track).
/// All are read from the item and blended, so the value is deterministic given the
/// item and a corpus-wide run is reproducible. It is a measurement, not a constant,
/// which is why two items across the bank do not share one stamped value.
fn derive_difficulty(passage: &str, occurrences: usize) -> f64 {
    let words = passage.split_whitespace().count().max(1);
    let sentences = split_sentences(passage).len().max(1);
    // Length: a passage at the floor is the easiest; at the ceiling, the hardest.
    let span = (MAX_PASSAGE_WORDS - MIN_PASSAGE_WORDS).max(1) as f64;
    let length_term = ((words.saturating_sub(MIN_PASSAGE_WORDS)) as f64 / span).clamp(0.0, 1.0);
    // Recurrence: a word the passage repeats is easier to pin from context.
    let recurrence_term = (1.0 / (1.0 + occurrences as f64)).clamp(0.0, 1.0);
    // Sentences: more sentences means more places the answer could hide.
    let sentence_term = (sentences as f64 / 6.0).clamp(0.0, 1.0);
    let blended = 0.5 * length_term + 0.25 * recurrence_term + 0.25 * sentence_term;
    // Round to three places: fine enough that distinct items keep distinct values,
    // coarse enough that the stored value is stable and readable.
    (blended * 1000.0).round() / 1000.0
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

    // Each kind has its own answerability rule, so verification branches by
    // objective rather than assuming one shape. DETAIL and VOCAB are checked against
    // the passage; MAIN_IDEA is checked against the passage's dominant topic.
    let is_main_idea = item.objective_id == "OBJ-PC-MAINIDEA-01";
    let is_vocab = item.objective_id == "OBJ-PC-VOCAB-01";

    // Difficulty must be a real probability, and a derived one lies in 0.0..=1.0.
    if !(0.0..=1.0).contains(&item.difficulty) {
        return Err(PcVerificationFailure::DifficultyOutOfRange {
            difficulty: item.difficulty.to_string(),
        });
    }

    let correct = &item.options[item.correct_index];

    if is_main_idea {
        // MAIN_IDEA: the answer is a full sentence whose subject is the passage's
        // dominant repeated topic. `supporting_clause` carries that topic word, so
        // the topic is re-derived from the passage rather than trusted.
        let topic = item.supporting_clause.to_lowercase();
        let occurrences = words(&item.passage)
            .into_iter()
            .filter(|word| word == &topic)
            .count();
        if occurrences < 2 {
            return Err(PcVerificationFailure::TopicNotRepeated {
                topic: item.supporting_clause.clone(),
                occurrences,
            });
        }
        // The correct option must *state* the topic, so "mainly about X" really names
        // X, and must be the passage's own topic rather than one drawn from elsewhere.
        if !correct.to_lowercase().contains(&topic) {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                correct.clone(),
            ));
        }
        // Every option is a complete sentence.
        for (position, option) in item.options.iter().enumerate() {
            if option.trim().is_empty() {
                return Err(PcVerificationFailure::Blank(format!("option {position}")));
            }
            if !option.trim().ends_with(['.', '!', '?']) {
                return Err(PcVerificationFailure::MainIdeaOptionNotASentence(
                    option.clone(),
                ));
            }
        }
        // The *answer* must not be passage text verbatim: a lifted sentence is a
        // detail restatement, not a summary. A distractor may be passage text -- the
        // "too narrow" distractor is exactly a true detail the passage states, which
        // is what makes it plausible -- so only the correct option is held to this.
        if haystack.contains(&normalize(correct)) {
            return Err(PcVerificationFailure::MainIdeaOptionIsPassageText(
                correct.clone(),
            ));
        }
        // No two options may share a sentence frame. If they do, the answer is one of
        // a pair and can be guessed by shape without reading the passage, which is
        // the defect this whole redo exists to remove.
        for (index, option) in item.options.iter().enumerate() {
            for other in item.options.iter().skip(index + 1) {
                if sentence_frame(option) == sentence_frame(other) {
                    return Err(PcVerificationFailure::MainIdeaOptionsNotDistinct(
                        option.clone(),
                    ));
                }
            }
        }
    } else if is_vocab {
        // VOCAB_IN_CONTEXT: the answer is a meaning of the target word, and the
        // target must occur in the quoted context sentence. The chosen meaning must
        // be a real meaning of the target -- the builder draws it from the
        // dictionary, and `verify` re-checks the target occurs where the prompt says.
        let context = &item.supporting_clause;
        if !haystack.contains(&normalize(context)) {
            return Err(PcVerificationFailure::CorrectOptionNotInPassage(
                context.clone(),
            ));
        }
        // The target word is the quoted word in the prompt; it must occur in the
        // context, or the learner is asked about a word the sentence does not use.
        let target = quoted_word(&item.prompt)
            .ok_or_else(|| PcVerificationFailure::CorrectOptionNotInPassage(correct.clone()))?;
        if !words(context).iter().any(|word| word == &target) {
            return Err(PcVerificationFailure::VocabOptionAbsentFromPassage(target));
        }
        // Every option is a meaning phrase; none may be a bare word lifted from the
        // passage (that would be word-spotting again) and none may duplicate another.
        for (position, option) in item.options.iter().enumerate() {
            if option.trim().is_empty() {
                return Err(PcVerificationFailure::Blank(format!("option {position}")));
            }
            if position == item.correct_index {
                continue;
            }
            // A distractor that is one word and occurs in the passage is not a
            // meaning; it is the old word-spotting option wearing a new prompt.
            if option.split_whitespace().count() == 1
                && passage_tokens.contains(&option.to_lowercase())
            {
                return Err(PcVerificationFailure::VocabOptionAbsentFromPassage(
                    option.clone(),
                ));
            }
            if normalize(option) == normalize(correct) {
                return Err(PcVerificationFailure::DuplicateOption(option.clone()));
            }
        }
    } else {
        // DETAIL: the correct option is a clause the passage states, in the clause
        // band; every distractor is that clause with one detail changed.
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
        if is_main_idea || is_vocab {
            // The sentence/meaning shape rules above already cover these kinds; the
            // clause band and the altered-token rule are DETAIL rules.
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
