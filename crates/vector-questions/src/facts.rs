//! General Science and shop-knowledge items from public-domain explanatory texts.
//!
//! Requirements: REQ-022, REQ-023, REQ-056.
//!
//! ## Why this exists rather than a longer list of templates
//!
//! Arithmetic and mechanical items are computable, so `factory` generates them and
//! proves each answer by evaluating an expression. General Science is not
//! computable: its questions are about the world, and the only honest way to answer
//! one is to have a source that says so. This module therefore does not invent
//! questions. It finds the ones a public-domain text already asks and answers, and
//! builds an item whose correct option is that text's own answer, quoted.
//!
//! ## The shape it reads
//!
//! Some public-domain science books are written as questions with answers beneath
//! them: "Why Do We Count in Tens?" followed by the explanation. That structure is
//! what makes an item possible without writing content: the question is the stem,
//! the answer's first sentence is the correct option, and the first sentences of
//! *other* answers in the same book are the distractors -- each one a real answer to
//! a real question, rather than filler this program made up.
//!
//! ## What verification can and cannot establish
//!
//! `verify` re-derives the item from the source: the correct option must be the
//! first sentence of the answer that follows that question in the text, and every
//! distractor must be the first sentence of the answer to a *different* question. It
//! cannot establish that the book is right. A 1910s popular-science book is a
//! period source, and its answers are period answers; that limitation is recorded
//! here and in the evidence rather than hidden, and the citation names the work so a
//! reviewer can see where a claim comes from.

use std::collections::{BTreeMap, HashSet};

use crate::passages::split_sentences;
use crate::provenance::ContentHash;

/// One question the source asks, with the answer it gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub question: String,
    /// The first sentence of the answer, which is what an option quotes.
    pub answer: String,
    /// The whole answer, kept so a reviewer can read the context.
    pub full_answer: String,
}

/// A parsed question-and-answer text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faq {
    pub label: String,
    entries: Vec<Entry>,
}

/// A General Science (or shop-knowledge) item built from a question and its answer.
#[derive(Debug, Clone, PartialEq)]
pub struct FactItem {
    pub subtest: String,
    pub objective_id: String,
    /// The source's question.
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub distractor_rationales: BTreeMap<usize, String>,
    /// The answer this item rests on, verbatim from the source.
    pub supporting_answer: String,
    pub source_label: String,
    pub difficulty: f64,
    pub seed: u64,
}

impl FactItem {
    /// Stable identity over the answerable content, excluding the seed.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str(&self.subtest);
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.prompt));
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.supporting_answer));
        ContentHash::of_text(&canonical).as_str().to_string()
    }

    /// The explanation shown after answering: the source's own words.
    pub fn explanation(&self) -> String {
        format!(
            "{} answers this: \"{}\"",
            self.source_label, self.supporting_answer
        )
    }
}

/// Why an item could not be built or did not verify.
#[derive(Debug, Clone, PartialEq)]
pub enum FactVerificationFailure {
    Blank(&'static str),
    /// The option is not the answer this source gives to this question.
    AnswerNotFromSource(String),
    /// A distractor is an answer this source does not contain.
    DistractorNotFromSource(String),
    /// Too few options, or one of them empty.
    TooFewOptions(usize),
    /// An option outside the word band, so it is not comparable.
    OptionLength {
        option: String,
        words: usize,
    },
    CorrectIndexOutOfRange {
        index: usize,
        options: usize,
    },
    DuplicateOption(String),
    /// A distractor carries no rationale, so a learner is told nothing about why.
    MissingRationale(usize),
    /// The question does not end in a question mark.
    NotAQuestion(String),
}

impl std::fmt::Display for FactVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FactVerificationFailure::Blank(what) => write!(f, "the {what} is blank"),
            FactVerificationFailure::AnswerNotFromSource(option) => write!(
                f,
                "the correct option is not the answer this source gives: {option:?}"
            ),
            FactVerificationFailure::DistractorNotFromSource(option) => write!(
                f,
                "the distractor is not an answer this source contains: {option:?}"
            ),
            FactVerificationFailure::TooFewOptions(count) => {
                write!(f, "an item needs at least 2 options, got {count}")
            }
            FactVerificationFailure::OptionLength { option, words } => write!(
                f,
                "option {option:?} has {words} words, outside the comparable band"
            ),
            FactVerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            FactVerificationFailure::DuplicateOption(option) => {
                write!(f, "two options are the same: {option:?}")
            }
            FactVerificationFailure::MissingRationale(index) => {
                write!(f, "option {index} has no rationale")
            }
            FactVerificationFailure::NotAQuestion(prompt) => {
                write!(f, "the prompt is not a question: {prompt:?}")
            }
        }
    }
}

/// Lowercase and collapse whitespace, for comparison.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A line that is a question: it ends with a question mark and asks something.
///
/// The interrogative word is required. A line that merely ends in a question mark
/// is usually a heading or a rhetorical aside, and one of those as a stem asks the
/// learner nothing.
fn is_question(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.ends_with('?') || trimmed.chars().count() < 12 {
        return false;
    }
    const OPENERS: [&str; 8] = [
        "why", "what", "how", "when", "where", "which", "who", "does",
    ];
    let first = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase();
    OPENERS.contains(&first.as_str())
}

/// Whether an answer reads as prose rather than as a heading or markup.
///
/// Some of these editions mark emphasis with `~...~` or `=...=`, and the first line
/// of an answer is sometimes a heading rather than a sentence. Both reached items as
/// options before this filter existed: one item offered `~WHERE RUBBER COMES FROM~
/// The first class, or wild rubbers, are collected from trees ...` as a choice.
fn looks_like_prose(answer: &str) -> bool {
    // `[...]` is the editions' illustration caption, and a caption reached an item as
    // an option: `[Illustration: This cut shows a section of a photo-engraving
    // screen enlarged ...`.
    if answer
        .chars()
        .any(|c| matches!(c, '~' | '=' | '_' | '|' | '*' | '[' | ']'))
    {
        return false;
    }
    let letters: Vec<char> = answer.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return false;
    }
    // A heading is mostly capitals; prose is not.
    let uppercase = letters.iter().filter(|c| c.is_uppercase()).count();
    if uppercase * 2 > letters.len() {
        return false;
    }
    answer.split_whitespace().count() >= 5
}

/// Parse a question-and-answer text.
///
/// `label` names the work, and goes on every item's explanation and its rubric.
pub fn parse_faq(text: &str, label: &str) -> Faq {
    // Project Gutenberg's header and licence are not part of the work. The markers
    // are matched in full: the bare words occur in ordinary prose.
    let body = match (
        text.find("*** START OF THE PROJECT GUTENBERG"),
        text.rfind("*** END OF THE PROJECT GUTENBERG"),
    ) {
        (Some(start), Some(end)) if end > start => {
            let after = &text[start..];
            let newline = after.find('\n').map(|o| start + o + 1).unwrap_or(start);
            &text[newline..end]
        }
        _ => text,
    };

    let mut entries: Vec<Entry> = Vec::new();
    let mut question: Option<String> = None;
    let mut answer = String::new();

    let flush = |question: &mut Option<String>, answer: &mut String, out: &mut Vec<Entry>| {
        if let Some(asked) = question.take() {
            let sentences = split_sentences(answer);
            if let Some(first) = sentences.first() {
                let cleaned = first.split_whitespace().collect::<Vec<_>>().join(" ");
                if looks_like_prose(&cleaned) {
                    out.push(Entry {
                        question: asked,
                        answer: cleaned,
                        full_answer: answer.split_whitespace().collect::<Vec<_>>().join(" "),
                    });
                }
            }
        }
        answer.clear();
    };

    for line in body.lines() {
        let trimmed = line.trim();
        if is_question(trimmed) {
            flush(&mut question, &mut answer, &mut entries);
            question = Some(trimmed.split_whitespace().collect::<Vec<_>>().join(" "));
            continue;
        }
        if question.is_some() {
            // The answer runs until the next question. Blank lines separate
            // paragraphs and do not end it.
            if !trimmed.is_empty() {
                if !answer.is_empty() {
                    answer.push(' ');
                }
                answer.push_str(trimmed);
            }
        }
    }
    flush(&mut question, &mut answer, &mut entries);

    // A question asked twice is one question; the text repeats some headings in its
    // table of contents, and those repetitions carry no answer of their own.
    let mut seen: HashSet<String> = HashSet::new();
    entries.retain(|entry| seen.insert(normalize(&entry.question)));

    Faq {
        label: label.to_string(),
        entries,
    }
}

impl Faq {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether this source answers `question` with `answer`.
    pub fn says(&self, question: &str, answer: &str) -> bool {
        let asked = normalize(question);
        let said = normalize(answer);
        self.entries
            .iter()
            .any(|entry| normalize(&entry.question) == asked && normalize(&entry.answer) == said)
    }

    /// The question a given answer belongs to, if this source contains it.
    pub fn question_for(&self, answer: &str) -> Option<&Entry> {
        let said = normalize(answer);
        self.entries
            .iter()
            .find(|entry| normalize(&entry.answer) == said)
    }

    /// Build up to `count` items.
    ///
    /// `accept` vets a built item: the application layer uses it for the OCR check,
    /// because the corpus includes scanned texts and an answer containing a
    /// misreading teaches the misreading.
    pub fn build_items<F>(
        &self,
        subtest: &str,
        count: usize,
        seed: u64,
        min_words: usize,
        max_words: usize,
        mut accept: F,
    ) -> Vec<FactItem>
    where
        F: FnMut(&FactItem) -> bool,
    {
        let usable: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|entry| {
                let words = entry.answer.split_whitespace().count();
                (min_words..=max_words).contains(&words)
            })
            .collect();
        if usable.len() < 4 {
            return Vec::new();
        }

        let mut rng = crate::factory::Rng::new(seed);
        let mut items: Vec<FactItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let attempts = count.saturating_mul(12).max(64);
        for _ in 0..attempts {
            if items.len() >= count {
                break;
            }
            let correct = usable[rng.range(0, usable.len() as i64 - 1) as usize];
            let mut options: Vec<(String, Option<String>)> = vec![(correct.answer.clone(), None)];

            // Distractors are answers to other questions in the same work, so each
            // one is a true statement about something else. That is what makes the
            // item a test of knowledge rather than of shape: a plausible distractor
            // here is a real answer.
            let mut candidates: Vec<&Entry> = usable
                .iter()
                .copied()
                .filter(|entry| normalize(&entry.question) != normalize(&correct.question))
                .collect();
            rng.shuffle(&mut candidates);
            for other in candidates {
                if options.len() == 4 {
                    break;
                }
                if options
                    .iter()
                    .any(|(existing, _)| normalize(existing) == normalize(&other.answer))
                {
                    continue;
                }
                options.push((
                    other.answer.clone(),
                    Some(format!(
                        "This answers a different question in {}: \"{}\".",
                        self.label, other.question
                    )),
                ));
            }
            if options.len() != 4 {
                continue;
            }

            rng.shuffle(&mut options);
            let correct_index = options
                .iter()
                .position(|(_, why)| why.is_none())
                .expect("the correct option is unlabelled");
            let mut distractor_rationales = BTreeMap::new();
            for (index, (_, why)) in options.iter().enumerate() {
                if let Some(why) = why {
                    distractor_rationales.insert(index, why.clone());
                }
            }

            let item = FactItem {
                subtest: subtest.to_string(),
                objective_id: format!("OBJ-{subtest}-EXPLAIN-01"),
                prompt: correct.question.clone(),
                options: options.into_iter().map(|(option, _)| option).collect(),
                correct_index,
                distractor_rationales,
                supporting_answer: correct.answer.clone(),
                source_label: self.label.clone(),
                difficulty: 0.2,
                seed,
            };
            if !accept(&item) {
                continue;
            }
            if seen.insert(item.content_hash()) {
                items.push(item);
            }
        }
        items
    }
}

/// Independently verify an item against the source it claims to quote.
///
/// Re-derives every fact from the source rather than trusting the builder: the
/// source must give this answer to this question, and every distractor must be an
/// answer the same source gives to a different question.
pub fn verify(item: &FactItem, faq: &Faq) -> Result<(), FactVerificationFailure> {
    if item.prompt.trim().is_empty() {
        return Err(FactVerificationFailure::Blank("prompt"));
    }
    if !is_question(&item.prompt) {
        return Err(FactVerificationFailure::NotAQuestion(item.prompt.clone()));
    }
    if item.supporting_answer.trim().is_empty() {
        return Err(FactVerificationFailure::Blank("supporting answer"));
    }
    if item.options.len() < 2 {
        return Err(FactVerificationFailure::TooFewOptions(item.options.len()));
    }
    if item.correct_index >= item.options.len() {
        return Err(FactVerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }

    let correct = &item.options[item.correct_index];
    if normalize(correct) != normalize(&item.supporting_answer) {
        return Err(FactVerificationFailure::AnswerNotFromSource(
            correct.clone(),
        ));
    }
    if !faq.says(&item.prompt, correct) {
        return Err(FactVerificationFailure::AnswerNotFromSource(
            correct.clone(),
        ));
    }

    for (position, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(FactVerificationFailure::Blank("option"));
        }
        if item.options[..position]
            .iter()
            .any(|earlier| normalize(earlier) == normalize(option))
        {
            return Err(FactVerificationFailure::DuplicateOption(option.clone()));
        }
        if position == item.correct_index {
            continue;
        }
        let Some(entry) = faq.question_for(option) else {
            return Err(FactVerificationFailure::DistractorNotFromSource(
                option.clone(),
            ));
        };
        // The distractor has to answer a *different* question, or two options are
        // true answers to this one.
        if normalize(&entry.question) == normalize(&item.prompt) {
            return Err(FactVerificationFailure::DistractorNotFromSource(
                option.clone(),
            ));
        }
        if !item.distractor_rationales.contains_key(&position) {
            return Err(FactVerificationFailure::MissingRationale(position));
        }
    }

    Ok(())
}

/// The band an option has to fit, for the application layer's own check.
pub const MIN_OPTION_WORDS: usize = 6;
pub const MAX_OPTION_WORDS: usize = 30;
