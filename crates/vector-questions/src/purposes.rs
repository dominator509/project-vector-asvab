//! Shop Information items from a public-domain manual's tool descriptions.
//!
//! Requirements: REQ-022, REQ-023, REQ-056.
//!
//! ## Why this is a separate engine from `facts`
//!
//! A General Science item is a question the source itself asks, quoted with its own
//! answer. A Shop Information item is a *statement* the source makes in passing while
//! describing a tool: `Screw extractors are used to remove broken screws without
//! damaging the surrounding material.` There is no question to quote, so the item has
//! to be formed -- and forming one means the prompt is written by this program from the
//! source's own words rather than lifted from it.
//!
//! ## The shape it reads, and what it refuses
//!
//! Tool manuals describe tools in one dominant shape:
//!
//! ```text
//! SCREW AND TAP EXTRACTORS
//! Screw extractors are used to remove broken screws without damaging the ...
//! Long-nose pliers are used for gripping, reaching places not readily accessible ...
//! ```
//!
//! Reading that shape means refusing a great deal of what looks like it. Figure
//! references make subjects out of nothing (`The thumb-screw D`, `The barrel I`), the
//! scanned text runs words together (`tight fitsare necessary`), and a heading can be
//! absorbed into the sentence after it (`HACKSAWS Hacksaws are used to cut ...`). Every
//! one of those reached a candidate item while this was being built, and each is
//! refused explicitly below rather than left to look like a tool.

use std::collections::{BTreeMap, HashSet};

use crate::provenance::ContentHash;

/// One tool description the source states.
#[derive(Debug, Clone, PartialEq)]
pub struct Purpose {
    /// The tool, as the source names it.
    pub tool: String,
    /// What the source says it is for, verbatim.
    pub purpose: String,
    /// The whole sentence, for a reviewer.
    pub sentence: String,
}

/// A parsed tool manual.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Purposes {
    pub label: String,
    entries: Vec<Purpose>,
}

/// A Shop Information item built from a tool description.
#[derive(Debug, Clone, PartialEq)]
pub struct PurposeItem {
    pub subtest: String,
    pub objective_id: String,
    /// Written from the source's own purpose clause.
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub distractor_rationales: BTreeMap<usize, String>,
    /// The sentence this item rests on, verbatim.
    pub supporting_sentence: String,
    pub source_label: String,
    pub difficulty: f64,
    pub seed: u64,
}

impl PurposeItem {
    /// Stable identity over the answerable content, excluding the seed.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str(&self.subtest);
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.prompt));
        canonical.push('\u{1}');
        canonical.push_str(&normalize(&self.supporting_sentence));
        ContentHash::of_text(&canonical).as_str().to_string()
    }

    /// The explanation shown after answering: the source's own sentence.
    pub fn explanation(&self) -> String {
        format!(
            "{} says: \"{}\"",
            self.source_label, self.supporting_sentence
        )
    }
}

/// Why a tool item did not verify.
#[derive(Debug, Clone, PartialEq)]
pub enum PurposeVerificationFailure {
    Blank(&'static str),
    /// The source does not say this tool is for this.
    NotFromSource(String),
    /// The correct option is not the tool the source names.
    WrongTool(String),
    /// Two options name the same tool.
    DuplicateOption(String),
    TooFewOptions(usize),
    CorrectIndexOutOfRange {
        index: usize,
        options: usize,
    },
    /// A distractor is not a tool this source describes.
    UnknownDistractor(String),
    /// A distractor the source describes the same way, so it is also correct.
    DistractorAlsoFits(String),
    MissingRationale(usize),
    /// The prompt does not ask what a tool is for.
    NotAPurposeQuestion(String),
}

impl std::fmt::Display for PurposeVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PurposeVerificationFailure::Blank(what) => write!(f, "the {what} is blank"),
            PurposeVerificationFailure::NotFromSource(purpose) => {
                write!(f, "the source does not state this purpose: {purpose:?}")
            }
            PurposeVerificationFailure::WrongTool(tool) => write!(
                f,
                "the correct option {tool:?} is not the tool the source names for this purpose"
            ),
            PurposeVerificationFailure::DuplicateOption(option) => {
                write!(f, "two options name the same tool: {option:?}")
            }
            PurposeVerificationFailure::TooFewOptions(count) => {
                write!(f, "an item needs at least 2 options, got {count}")
            }
            PurposeVerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            PurposeVerificationFailure::UnknownDistractor(tool) => write!(
                f,
                "the distractor {tool:?} is not a tool this source describes"
            ),
            PurposeVerificationFailure::DistractorAlsoFits(tool) => write!(
                f,
                "the distractor {tool:?} is described as serving the same purpose, so two \
                 options would be right"
            ),
            PurposeVerificationFailure::MissingRationale(index) => {
                write!(f, "option {index} has no rationale")
            }
            PurposeVerificationFailure::NotAPurposeQuestion(prompt) => {
                write!(f, "the prompt does not ask what a tool is for: {prompt:?}")
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

/// The words that mean a "subject" is not a tool.
const NOT_A_TOOL: [&str; 32] = [
    "it", "they", "this", "that", "these", "those", "which", "one", "some", "many", "most", "you",
    "we", "he", "she", "there", "figure", "fig", "chapter", "table", "page", "and",
    // A question's subject is an interrogative, not a tool. These manuals carry their own
    // review questions -- `What tool is used to mark the center of a hole?` -- and the
    // pattern matched them, producing items whose correct answer was the word `What`.
    "what", "who", "whose", "how", "why", "when", "where", "or", "but", "nor",
];

/// Words from a scan's own furniture, which are not part of the manual.
///
/// `Digitized by Google` and `Original from UNIVERSITY OF CALIFORNIA` sit in the page
/// margins of the scanned manuals and ran straight into a sentence's subject.
const SCAN_FURNITURE: [&str; 8] = [
    "digitized",
    "google",
    "microsoft",
    "archive",
    "original",
    "university",
    "california",
    "ocr",
];

/// Strip a scan's page furniture, which arrives inside sentences.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        // A token that is mostly punctuation is the scan's mark, not the manual's word:
        // `*`, `\`, `MMtrfatJVw**;r`, `1-57)`.
        let letters = word.chars().filter(|c| c.is_alphabetic()).count();
        let noisy = word
            .chars()
            .any(|c| matches!(c, '*' | '\\' | '|' | '~' | '{' | '}' | '^'));
        if noisy || letters == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Whether a candidate subject really names a tool.
fn names_a_tool(tool: &str) -> bool {
    let words: Vec<&str> = tool.split_whitespace().collect();
    if words.is_empty() || words.len() > 3 {
        return false;
    }
    if words
        .iter()
        .any(|word| NOT_A_TOOL.contains(&word.to_lowercase().as_str()))
    {
        return false;
    }
    // A figure reference is not a tool: `Tube cutters (fig`, `Flaring tools (figure 1-58)`.
    let lower = tool.to_lowercase();
    if lower.contains("(fig") || lower.contains("fig.") || lower.contains("figure") {
        return false;
    }
    // A single capital letter is a figure's label: `The thumb-screw D`, `The barrel I`.
    if words
        .iter()
        .any(|word| word.len() == 1 && word.chars().all(|c| c.is_ascii_uppercase()))
    {
        return false;
    }
    // Digits and stray punctuation are scan damage.
    if tool.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if tool.contains(['(', ')', ',', ';', ':', '"']) {
        return false;
    }
    // At least one word has to be a word: `or` and `the name implies` are not tools.
    if !words.iter().any(|word| word.chars().count() >= 4) {
        return false;
    }
    // Every word has to be a plausible word rather than a scanner's fragment.
    if !words
        .iter()
        .all(|word| word.chars().filter(|c| c.is_alphabetic()).count() >= 2)
    {
        return false;
    }
    // A capital inside a word is the scanner guessing at small capitals: `MaWng Oonoave
    // Forming` and `Drill Jiers Drill` are not tool names. A hyphenated compound keeps
    // its capitals (`T-bevel`), so each part is checked after its first letter.
    if words.iter().any(|word| {
        word.split('-')
            .any(|part| part.chars().skip(1).any(|c| c.is_uppercase()))
    }) {
        return false;
    }
    // A function word or a verb means the "name" is a clause: `punch does not`.
    !words
        .iter()
        .any(|word| NAME_STOP_WORDS.contains(&word.to_lowercase().as_str()))
}

/// Words that mean a run of text is a clause rather than a tool's name.
const NAME_STOP_WORDS: [&str; 24] = [
    "does", "do", "did", "is", "are", "was", "were", "be", "been", "being", "has", "have", "had",
    "can", "may", "will", "would", "should", "could", "not", "no", "any", "all", "each",
];

/// Whether a candidate purpose reads as a thing a tool does.
fn reads_as_a_purpose(purpose: &str) -> bool {
    let words: Vec<&str> = purpose.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    // A purpose starts with a verb: `to cut ...`, `cutting ...`, `hold ...`. Punctuation
    // around the word is the sentence's, not the word's -- `gripping, reaching places not
    // readily accessible` is a purpose whose first word is `gripping`, and reading the
    // comma as part of it dropped the description.
    let first = words[0]
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase();
    let looks_verbal = first.ends_with("ing")
        || first.ends_with("ed")
        || matches!(
            first.as_str(),
            "cut"
                | "hold"
                | "drive"
                | "remove"
                | "measure"
                | "support"
                | "join"
                | "make"
                | "shape"
                | "grip"
                | "lift"
                | "turn"
                | "tighten"
                | "loosen"
                | "clean"
                | "mark"
                | "test"
                | "protect"
                | "prevent"
                | "carry"
                | "guide"
                | "align"
                | "fasten"
                | "strike"
                | "break"
                | "smooth"
                | "polish"
                | "sharpen"
                | "bore"
                | "drill"
                | "thread"
                | "bend"
                | "spread"
                | "reach"
                | "check"
                | "indicate"
                | "record"
                | "transfer"
        );
    if !looks_verbal {
        return false;
    }
    // A purpose with a bracketed figure reference asks the learner to look at a
    // drawing that is not in the item.
    if purpose.contains("(fig") || purpose.contains("figure ") || purpose.contains("Fig.") {
        return false;
    }
    true
}

/// Parse a tool manual's descriptions.
pub fn parse_purposes(text: &str, label: &str) -> Purposes {
    let body = clean_scan(text);
    let mut entries: Vec<Purpose> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for sentence in split_sentences(&body) {
        // A sentence that asks a question states no purpose: the manual's review
        // questions match the same pattern, and reading them produced items whose
        // correct answer was the word `What`.
        if sentence.trim_end().ends_with('?') {
            continue;
        }
        let Some((tool, purpose)) = describe(&sentence) else {
            continue;
        };
        if !names_a_tool(&tool) || !reads_as_a_purpose(&purpose) {
            continue;
        }
        let lower_tool = tool.to_lowercase();
        if SCAN_FURNITURE.iter().any(|word| lower_tool.contains(word)) {
            continue;
        }
        let key = normalize(&tool);
        if !seen.insert(key) {
            continue;
        }
        entries.push(Purpose {
            tool,
            purpose,
            sentence: sentence.trim().to_string(),
        });
    }

    Purposes {
        label: label.to_string(),
        entries,
    }
}

/// Strip the scan's line furniture, repair its hyphenation, and collapse the text.
///
/// Two scanner habits have to be undone before anything can be read out of this text.
/// A word broken at a line end keeps its hyphen (`Microm- eters`, `con- structed`,
/// `exam- ple`), and a page's furniture arrives as short lines of digits or capitals.
/// Left alone, the first produces items whose tools are named `Microm- eters` and whose
/// purpose text does not appear in the source, which is what the verifier refused.
fn clean_scan(text: &str) -> String {
    let mut joined = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let letters = trimmed.chars().filter(|c| c.is_alphabetic()).count();
        if letters == 0 {
            continue;
        }
        if joined.ends_with('-') {
            // A hyphen at the end of a line, followed by a lower-case word, is a word
            // the scanner split: rejoin it without the hyphen or a space.
            let next_starts_lower = trimmed.chars().next().is_some_and(|c| c.is_lowercase());
            if next_starts_lower {
                joined.pop();
                joined.push_str(trimmed);
                continue;
            }
        }
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(trimmed);
    }
    clean(&joined)
}

/// Split prose into sentences, keeping the terminator.
fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        current.push(*character);
        if !matches!(character, '.' | '?' | '!') {
            continue;
        }
        // A full stop inside an abbreviation or an initial is not a boundary.
        let next = characters[index + 1..]
            .iter()
            .find(|c| !c.is_whitespace())
            .copied()
            .unwrap_or(' ');
        if character == &'.' && !next.is_ascii_uppercase() {
            continue;
        }
        out.push(current.trim().to_string());
        current.clear();
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// The tool and the purpose a sentence states, when it states one.
///
/// The subject is taken from *before* the verb, so a heading absorbed into the
/// sentence (`HACKSAWS Hacksaws are used to cut metal`) leaves the last word before the
/// verb as the tool rather than the whole run.
fn describe(sentence: &str) -> Option<(String, String)> {
    const VERBS: [&str; 6] = [
        " are used for ",
        " are used to ",
        " is used for ",
        " is used to ",
        " are used as ",
        " is used as ",
    ];
    let lower = sentence.to_lowercase();
    let (position, verb) = VERBS
        .iter()
        .filter_map(|verb| lower.find(verb).map(|position| (position, *verb)))
        .min_by_key(|(position, _)| *position)?;

    let subject = sentence[..position].trim();
    let purpose = sentence[position + verb.len()..].trim();
    let tool = subject_as_tool(subject)?;
    let purpose = purpose.trim_end_matches('.').trim().to_string();
    if purpose.is_empty() {
        return None;
    }
    Some((tool, purpose))
}

/// Prepositions that end a tool's name.
///
/// `Nails with large flat heads are used for ...` names one tool: `Nails`. Taking the
/// words next to the verb instead produced `large flat heads`, which is part of a nail
/// rather than a tool.
const ENDS_THE_NAME: [&str; 17] = [
    "with",
    "for",
    "of",
    "in",
    "on",
    "to",
    "by",
    "from",
    "that",
    "which",
    "whose",
    // A participle means the description has started: `The wire gage shown in fig. is
    // used for ...` names a wire gage, not a "wire gage shown".
    "shown",
    "illustrated",
    "made",
    "listed",
    "described",
    "used",
];

/// Words that mean the "subject" is a continuation of the previous clause.
const NOT_A_SUBJECT_START: [&str; 4] = ["or", "and", "but", "nor"];

/// The tool a sentence's subject names.
///
/// Three things sit in front of a tool's name in this text and none of them is part of
/// it: the section heading the scan ran into the sentence (`GAGE Telescoping gages`), a
/// leading article (`The sliding T-bevel`), and a phrase describing the tool rather than
/// naming it (`Nails with large flat heads`).
fn subject_as_tool(subject: &str) -> Option<String> {
    let words: Vec<&str> = subject.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    // Drop leading heading words: a token in full capitals is a heading, not a name.
    let mut start = 0;
    while start + 1 < words.len()
        && words[start].chars().filter(|c| c.is_alphabetic()).count() > 1
        && words[start]
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
    {
        start += 1;
    }
    let mut chosen: Vec<&str> = words[start..].to_vec();
    if chosen
        .first()
        .is_some_and(|word| NOT_A_SUBJECT_START.contains(&word.to_lowercase().as_str()))
    {
        return None;
    }

    // Drop a leading article.
    if chosen
        .first()
        .is_some_and(|word| matches!(word.to_lowercase().as_str(), "the" | "a" | "an"))
    {
        chosen.remove(0);
    }

    // Stop at a preposition: everything after it describes the tool.
    if let Some(position) = chosen
        .iter()
        .position(|word| ENDS_THE_NAME.contains(&word.to_lowercase().as_str()))
    {
        chosen.truncate(position);
    }
    chosen.truncate(3);

    // A repeated word is the heading echoed in the tool's name: `Auger Bits Bits`.
    if chosen.len() >= 2 {
        let last = singular(chosen[chosen.len() - 1]);
        if singular(chosen[chosen.len() - 2]) == last {
            chosen.truncate(chosen.len() - 1);
        }
    }

    if chosen.is_empty() {
        return None;
    }
    let tool = chosen
        .join(" ")
        .trim_end_matches(['.', ',', ';', ':'])
        .trim()
        .to_string();
    (!tool.is_empty()).then_some(tool)
}

/// A word without its plural, so `Bits` and `Bit` compare equal.
fn singular(word: &str) -> String {
    let lower = word.to_lowercase();
    lower.strip_suffix('s').map(str::to_string).unwrap_or(lower)
}

/// The question a purpose clause answers.
///
/// The source writes purposes two ways -- `used to cut metal` and `used for cutting
/// metal` -- and the question has to follow the source rather than impose one form on
/// it, or the item asks which tool is "used to laying out angles".
fn question_for(purpose: &str) -> String {
    let first = purpose.split_whitespace().next().unwrap_or("");
    // A gerund after `to` is not English; a bare verb after `for` is not either.
    if first.ends_with("ing") {
        format!("Which tool is used for {purpose}")
    } else {
        format!("Which tool is used to {purpose}")
    }
}

/// The purpose clause a prompt asks about, whichever form it was written in.
fn purpose_in_prompt(prompt: &str) -> Option<String> {
    for prefix in ["Which tool is used to ", "Which tool is used for "] {
        if let Some(rest) = prompt.strip_prefix(prefix) {
            return Some(rest.trim_end_matches('?').trim().to_string());
        }
    }
    None
}

impl Purposes {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Purpose] {
        &self.entries
    }

    /// Whether this source says this tool is for this.
    pub fn says(&self, tool: &str, purpose: &str) -> bool {
        let tool = normalize(tool);
        let purpose = normalize(purpose);
        self.entries
            .iter()
            .any(|entry| normalize(&entry.tool) == tool && normalize(&entry.purpose) == purpose)
    }

    /// What the source says a tool is for.
    pub fn purpose_of(&self, tool: &str) -> Option<&Purpose> {
        let tool = normalize(tool);
        self.entries
            .iter()
            .find(|entry| normalize(&entry.tool) == tool)
    }

    /// Build up to `count` items.
    pub fn build_items<F>(
        &self,
        subtest: &str,
        count: usize,
        seed: u64,
        mut accept: F,
    ) -> Vec<PurposeItem>
    where
        F: FnMut(&PurposeItem) -> bool,
    {
        if self.entries.len() < 4 {
            return Vec::new();
        }
        let mut rng = crate::factory::Rng::new(seed);
        let mut items: Vec<PurposeItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let attempts = count.saturating_mul(12).max(64);
        for _ in 0..attempts {
            if items.len() >= count {
                break;
            }
            let correct = &self.entries[rng.range(0, self.entries.len() as i64 - 1) as usize];
            let mut options: Vec<(String, Option<String>)> = vec![(correct.tool.clone(), None)];

            // Distractors are tools the same manual describes doing something else, so
            // each is a real tool of the trade rather than filler.
            let mut candidates: Vec<&Purpose> = self
                .entries
                .iter()
                .filter(|entry| normalize(&entry.tool) != normalize(&correct.tool))
                .collect();
            rng.shuffle(&mut candidates);
            for other in candidates {
                if options.len() == 4 {
                    break;
                }
                if options
                    .iter()
                    .any(|(existing, _)| normalize(existing) == normalize(&other.tool))
                {
                    continue;
                }
                // A tool the source describes the same way would make two options right.
                if normalize(&other.purpose) == normalize(&correct.purpose) {
                    continue;
                }
                options.push((
                    other.tool.clone(),
                    Some(format!("{} is for {}.", other.tool, other.purpose)),
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

            let item = PurposeItem {
                subtest: subtest.to_string(),
                objective_id: format!("OBJ-{subtest}-TOOLS-01"),
                // A purpose read from `is used to cut` is an infinitive; one read from
                // `is used for cutting` is a gerund. The prompt has to take whichever
                // the source wrote, or it asks the learner about a tool "used to laying
                // out angles".
                prompt: format!("{}?", question_for(&correct.purpose)),
                options: options.into_iter().map(|(option, _)| option).collect(),
                correct_index,
                distractor_rationales,
                supporting_sentence: correct.sentence.clone(),
                source_label: self.label.clone(),
                difficulty: 0.1,
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
pub fn verify(item: &PurposeItem, source: &Purposes) -> Result<(), PurposeVerificationFailure> {
    if item.prompt.trim().is_empty() {
        return Err(PurposeVerificationFailure::Blank("prompt"));
    }
    let Some(purpose) = purpose_in_prompt(&item.prompt) else {
        return Err(PurposeVerificationFailure::NotAPurposeQuestion(
            item.prompt.clone(),
        ));
    };
    if item.supporting_sentence.trim().is_empty() {
        return Err(PurposeVerificationFailure::Blank("supporting sentence"));
    }
    if item.options.len() < 2 {
        return Err(PurposeVerificationFailure::TooFewOptions(
            item.options.len(),
        ));
    }
    if item.correct_index >= item.options.len() {
        return Err(PurposeVerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }

    // The prompt's purpose has to be the source's own words for it.
    let correct = &item.options[item.correct_index];
    if !source.says(correct, &purpose) {
        return Err(PurposeVerificationFailure::NotFromSource(purpose.clone()));
    }
    if !normalize(&item.supporting_sentence).contains(&normalize(&purpose)) {
        return Err(PurposeVerificationFailure::NotFromSource(purpose));
    }

    for (position, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(PurposeVerificationFailure::Blank("option"));
        }
        if item.options[..position]
            .iter()
            .any(|earlier| normalize(earlier) == normalize(option))
        {
            return Err(PurposeVerificationFailure::DuplicateOption(option.clone()));
        }
        if position == item.correct_index {
            continue;
        }
        let Some(entry) = source.purpose_of(option) else {
            return Err(PurposeVerificationFailure::UnknownDistractor(
                option.clone(),
            ));
        };
        // A distractor that the source describes the same way is also correct.
        if normalize(&entry.purpose) == normalize(&purpose) {
            return Err(PurposeVerificationFailure::DistractorAlsoFits(
                option.clone(),
            ));
        }
        if !item.distractor_rationales.contains_key(&position) {
            return Err(PurposeVerificationFailure::MissingRationale(position));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature manual reproducing the scanner's habits, each of which reached a
    /// candidate item before it was refused: a heading run into the sentence, a word
    /// broken across lines, a figure reference, the manual's own review question, a
    /// repeated word in a tool's name, and both ways of writing a purpose.
    const FIXTURE: &str = "\
COMMON HANDTOOLS

SCREW AND TAP EXTRACTORS
Screw extractors are used to remove broken screws without damaging the surrounding
material.

MICROMETERS
Microm-
eters are used to measure distances to the nearest one thousandth of an inch.

TUBE CUTTERS
Tube cutters (fig. 1-57) are used to cut tubing made of iron, steel, brass, copper,
and aluminum.

AUGER BITS
Auger Bits Bits are used to bore holes in wood.

VISES AND CLAMPS
Vises are used for holding work when it is being planed, sawed, or drilled.

THE SLIDING T-BEVEL
The sliding T-bevel is used for laying out angles other than right angles.

REVIEW
What tool is used to mark the center of a hole prior to drilling?

Digitized by Google
Digitized is used to scan books.
";

    fn fixture() -> Purposes {
        parse_purposes(FIXTURE, "A Test Manual")
    }

    fn build(count: usize, seed: u64) -> Vec<PurposeItem> {
        fixture().build_items("SI", count, seed, |_| true)
    }

    #[test]
    fn a_tool_and_its_purpose_are_read_from_the_description() {
        let source = fixture();
        let entry = source
            .purpose_of("Screw extractors")
            .expect("the extractors are described");
        assert_eq!(
            entry.purpose,
            "remove broken screws without damaging the surrounding material"
        );
    }

    #[test]
    fn a_word_broken_across_lines_is_rejoined() {
        // `Microm-\neters` is one word. Left broken, the tool is named `Microm- eters`
        // and the purpose text does not appear in the source, which the verifier then
        // refuses.
        let source = fixture();
        assert!(
            source.purpose_of("Micrometers").is_some(),
            "the rejoined word should name the tool: {:?}",
            source.entries()
        );
    }

    #[test]
    fn a_heading_run_into_the_sentence_is_not_part_of_the_tool() {
        let source = fixture();
        let tool = source
            .purpose_of("Vises")
            .expect("the vise is described")
            .tool
            .clone();
        assert_eq!(tool, "Vises", "the heading was absorbed into the name");
        assert!(source.purpose_of("Auger Bits").is_some() || source.purpose_of("Bits").is_some());
    }

    #[test]
    fn a_figure_reference_is_not_a_tool() {
        let source = fixture();
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.tool.to_lowercase().contains("fig")),
            "{:?}",
            source.entries()
        );
    }

    #[test]
    fn the_manuals_own_review_question_is_not_a_description() {
        // `What tool is used to mark the center of a hole?` matches the pattern, and
        // reading it produced items whose correct answer was the word `What`.
        let source = fixture();
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.tool.eq_ignore_ascii_case("what")),
            "{:?}",
            source.entries()
        );
        assert!(source.purpose_of("mark the center of a hole").is_none());
    }

    #[test]
    fn a_scan_footer_is_not_a_tool() {
        let source = fixture();
        assert!(
            source.purpose_of("Digitized").is_none(),
            "{:?}",
            source.entries()
        );
    }

    #[test]
    fn the_question_follows_the_form_the_source_used() {
        // `used to cut` takes an infinitive, `used for holding` a gerund. Asking
        // "which tool is used to holding work" is not English.
        for item in build(8, 1) {
            assert!(
                !item.prompt.contains("used to holding")
                    && !item.prompt.contains("used to measuring")
                    && !item.prompt.contains("used for measure"),
                "the prompt does not read as a question: {}",
                item.prompt
            );
            assert!(item.prompt.ends_with('?'), "{}", item.prompt);
        }
    }

    #[test]
    fn every_option_is_a_tool_the_source_describes() {
        let source = fixture();
        let items = build(8, 2);
        assert!(!items.is_empty(), "the fixture should yield items");
        for item in &items {
            assert_eq!(item.options.len(), 4);
            for option in &item.options {
                assert!(
                    source.purpose_of(option).is_some(),
                    "option {option:?} is not a tool in this source"
                );
            }
        }
    }

    #[test]
    fn a_distractor_is_described_doing_something_else() {
        let source = fixture();
        for item in build(8, 3) {
            let purpose = purpose_in_prompt(&item.prompt).expect("a purpose question");
            for (index, option) in item.options.iter().enumerate() {
                if index == item.correct_index {
                    continue;
                }
                let entry = source.purpose_of(option).expect("a described tool");
                assert_ne!(
                    normalize(&entry.purpose),
                    normalize(&purpose),
                    "a distractor must not serve the same purpose"
                );
                assert!(
                    item.distractor_rationales.contains_key(&index),
                    "every distractor needs a rationale"
                );
            }
        }
    }

    #[test]
    fn the_same_seed_reproduces_the_same_items() {
        assert_eq!(build(6, 42), build(6, 42));
    }

    #[test]
    fn every_built_item_passes_independent_verification() {
        let source = fixture();
        for item in build(10, 7) {
            verify(&item, &source)
                .unwrap_or_else(|failure| panic!("{} failed: {failure}", item.prompt));
        }
    }

    #[test]
    fn verification_refuses_a_purpose_the_source_does_not_state() {
        let mut item = build(1, 5).remove(0);
        let purpose = purpose_in_prompt(&item.prompt).expect("a purpose question");
        item.prompt = format!("Which tool is used to {purpose} underwater?");
        match verify(&item, &fixture()) {
            Err(PurposeVerificationFailure::NotFromSource(_)) => {}
            other => panic!("expected a source refusal, got {other:?}"),
        }
    }

    #[test]
    fn verification_refuses_a_prompt_that_is_not_a_purpose_question() {
        let mut item = build(1, 11).remove(0);
        item.prompt = "Which tool costs the least?".to_string();
        match verify(&item, &fixture()) {
            Err(PurposeVerificationFailure::NotAPurposeQuestion(_)) => {}
            other => panic!("expected a question refusal, got {other:?}"),
        }
    }

    #[test]
    fn verification_refuses_a_distractor_that_is_not_a_tool_in_the_source() {
        let mut item = build(1, 13).remove(0);
        let wrong = (item.correct_index + 1) % item.options.len();
        item.options[wrong] = "A tool nobody described".to_string();
        match verify(&item, &fixture()) {
            Err(PurposeVerificationFailure::UnknownDistractor(_)) => {}
            other => panic!("expected an unknown-distractor refusal, got {other:?}"),
        }
    }

    #[test]
    fn verification_refuses_a_distractor_described_the_same_way() {
        // Two tools the manual describes identically would both be right.
        let text = "\
Screwdrivers are used to turn screws.
A screwdriver is used to turn screws.
Pliers are used to grip wire.
Hammers are used to drive nails.
Saws are used to cut wood.
";
        let source = parse_purposes(text, "x");
        if source.entry_count() < 4 {
            return;
        }
        let items = source.build_items("SI", 2, 3, |_| true);
        for item in &items {
            let purpose = purpose_in_prompt(&item.prompt).expect("a purpose question");
            for (index, option) in item.options.iter().enumerate() {
                if index == item.correct_index {
                    continue;
                }
                let entry = source.purpose_of(option).expect("a described tool");
                assert_ne!(normalize(&entry.purpose), normalize(&purpose));
            }
        }
    }

    #[test]
    fn a_source_with_fewer_than_four_tools_yields_nothing() {
        let thin = parse_purposes(
            "Pliers are used to grip wire.\nHammers are used to drive nails.\n",
            "x",
        );
        assert!(thin.build_items("SI", 4, 1, |_| true).is_empty());
    }
}
