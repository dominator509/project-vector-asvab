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
    /// Which of the relation's forms the source used, because the question has to be asked in
    /// the same one: a manual that writes `the relay serves as a switch` states a complement
    /// that is a noun, and "Which tool is used to a switch?" is not a question.
    pub link: Link,
}

/// The form a description states its relation in.
///
/// The manuals use three, and they take different complements -- which is why the frame cannot
/// be imposed by the reader:
///
/// * `is used to cut`, `serves to control`, `is employed to bind`, `is arranged to pull`: an
///   infinitive;
/// * `is used for cutting`, `is employed for facing`: a gerund;
/// * `serves as a switch`, `acts as a thrust bearing`, `is used as a wedge`: a noun phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Link {
    /// `is used to <verb>`.
    To,
    /// `is used for <gerund>`.
    For,
    /// `serves as <noun>`, `acts as <noun>`.
    As,
}

impl Link {
    /// The words a purpose in this form is written after, as a question.
    fn tool_question(self, purpose: &str) -> String {
        match self {
            // A gerund after `to` is not English, and a bare verb after `for` is not either.
            Link::To => format!("Which tool is used to {purpose}"),
            Link::For => format!("Which tool is used for {purpose}"),
            Link::As => format!("Which tool serves as {purpose}"),
        }
    }
}

/// A parsed tool manual.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Purposes {
    pub label: String,
    entries: Vec<Purpose>,
}

/// A Shop Information item built from a tool description.
///
/// The same sentence carries two questions, and which one is asked depends on the subtest.
/// A tool manual and an automotive manual both write `X is used to Y`, and the two subtests
/// want opposite halves of it:
///
///   * Shop Information asks **which tool** does Y, so the options are tools.
///   * Auto Information asks **what X is for**, so the options are functions.
///
/// Both are formed from the source's own words; neither is quoted as a question, because
/// these manuals describe rather than ask.
#[derive(Debug, Clone, PartialEq)]
pub struct PurposeItem {
    pub subtest: String,
    pub kind: ItemKind,
    pub objective_id: String,
    /// Written from the source's own clause.
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

/// Which half of the description an item asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// `Which tool is used to ...?` -- options are the tools.
    Tool,
    /// `What is the <component> used for?` -- options are the functions.
    Function,
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
    /// An option is written in a different form from the one the prompt asks in.
    OptionFormMismatch {
        prompt: String,
    },
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
            PurposeVerificationFailure::OptionFormMismatch { prompt } => write!(
                f,
                "the options are not written in the form the prompt asks in: {prompt:?}"
            ),
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
    // A letter from another alphabet is a scan this reader cannot repair: see
    // `uses_only_latin`. `Oй filters` and `Tһe bulkhead receptacle` are what it looks like,
    // and each would have reached a learner as an option.
    if !uses_only_latin(tool) {
        return false;
    }
    // A possessive names a document rather than an object: TM 9-8000 offered
    // `automotive manufacturers’ requirements` beside `Fuel pumps` and `relief valve`.
    if tool.contains(['\'', '\u{2019}']) {
        return false;
    }
    // Damage in the name itself: `Oil seals be- tween` and `EVAPORATOR CORE CAPILLARY TUBE`.
    if carries_scan_damage(tool) {
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
    if words
        .iter()
        .any(|word| NAME_STOP_WORDS.contains(&word.to_lowercase().as_str()))
    {
        return false;
    }
    // A name headed by a categorising noun describes a class rather than a thing. The
    // manuals write `Setscrews are used to fasten together parts`, and the sentence before
    // it -- `Two types of fasteners are used: the more permanent type is used to ...` --
    // named `the more permanent type` as the tool, which became an option a learner could
    // choose. Nothing a shop stocks is called a type, a kind, a method, or a means.
    !NOT_A_TOOL_HEAD.contains(&words[words.len() - 1].to_lowercase().as_str())
}

/// Head nouns that mean a "name" describes a class, a method or a measurement rather than an
/// object.
///
/// The measurement words are here for the same reason as the class nouns: *Modern Machine-Shop
/// Practice* writes `softened sheet copper about 1/32 inch thick is used to make joints on
/// surfaces that have been planed`, whose subject ends in the thickness of the copper rather
/// than in a thing, and the reader named `about inch thick` -- an option no learner can pick
/// as a tool.
const NOT_A_TOOL_HEAD: [&str; 46] = [
    "type",
    "types",
    "kind",
    "kinds",
    "method",
    "methods",
    "means",
    "way",
    "ways",
    "manner",
    "purpose",
    "purposes",
    "sort",
    "example",
    "examples",
    "instance",
    "class",
    "classes",
    // Measurements: `softened sheet copper about 1/32 inch thick` named `about inch thick`.
    "thick",
    "thickness",
    "wide",
    "width",
    "deep",
    "depth",
    "long",
    "high",
    "broad",
    "diameter",
    "inch",
    "inches",
    "foot",
    "feet",
    "yard",
    "yards",
    "pound",
    "pounds",
    "gallon",
    "gallons",
    // Not things: a treatise's own words. `the work is designed to form a complete manual of
    // reference` named `work`, and `prevent radiation, as, for example, felt, mineral wool,
    // asbestos` named `non conducting substances`.
    "work",
    "works",
    "substance",
    "substances",
    "material",
    "materials",
    "tool",
    "tools",
];

/// Words that mean a run of text is a clause or a generalisation rather than a tool's name.
///
/// The pronouns and determiners are as disqualifying as the verbs: `Whatever method is used
/// to secure the sleeve` and `both hands are used to set the micrometer` both match the
/// description pattern, and neither subject is a tool.
const NAME_STOP_WORDS: &[&str] = &[
    "does",
    "do",
    "did",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "has",
    "have",
    "had",
    "can",
    "may",
    "will",
    "would",
    "should",
    "could",
    "not",
    "no",
    "any",
    "all",
    "each",
    "both",
    "some",
    "every",
    "either",
    "neither",
    "whatever",
    "whichever",
    "such",
    "other",
    "another",
    "this",
    "that",
    "these",
    "those",
    "it",
    "they",
    "you",
    "we",
    "he",
    "she",
    "which",
    "what",
    "who",
    // `as` leads the same kind of fragment: `The differential, as explained in the preceding
    // paragraph, is to provide for differences in speed` named `as explained`.
    "as",
    // A qualifier before a measurement: `about 1/32 inch thick`.
    "about",
    // `the same device`, `in connection with`, `third class`: the subject is a reference back
    // into the paragraph rather than a name.
    "same",
    "third",
    "second",
    "first",
    // The connectives. A name containing one is the tail of a clause the scanner ran
    // together: `catch trough then is used to collect the oil and return it to the sump` names
    // `catch trough then`, and the word `then` is what says so.
    "then",
    "than",
    "when",
    "whenever",
    "where",
    "whereas",
    "while",
    "because",
    "since",
    "if",
    "so",
    "also",
    "thus",
    "hence",
    "therefore",
    "however",
    "although",
    "unless",
    "until",
];

/// Whether a candidate purpose reads as a thing a tool does.
fn reads_as_a_purpose(purpose: &str, link: Link) -> bool {
    let words: Vec<&str> = purpose.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    // A noun complement is a different kind of phrase, and the verb test below would refuse
    // every one of them: `the relay serves as a switch` states `a switch`, which is what the
    // relay *is*, not what it does. What makes such a complement readable is the opposite
    // test -- it must not open with a verb from the list, and it must be a noun phrase rather
    // than a bare word: `serves as a switch` and `acts as a thrust bearing` are descriptions,
    // while `serves as good` and `serves as the following` are not.
    if link == Link::As {
        let first = words[0]
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_lowercase();
        if PURPOSE_VERBS.contains(&first.as_str())
            || NAME_STOP_WORDS.contains(&first.as_str())
            || words.len() < 2
        {
            return false;
        }
        return !contains_full_stop(purpose)
            && !purpose.contains(';')
            && uses_only_latin(purpose)
            && !carries_scan_damage(purpose)
            && !purpose.contains("(fig")
            && !purpose.contains("figure ")
            && !purpose.contains("Fig.");
    }
    // A purpose starts with a verb: `to cut ...`, `cutting ...`, `regulate ...`. Punctuation
    // around the word is the sentence's, not the word's -- `gripping, reaching places not
    // readily accessible` is a purpose whose first word is `gripping`, and reading the comma
    // as part of it dropped the description.
    //
    // The test is a list of the verbs these manuals use, and it is deliberately a list of
    // *verbs* rather than a list of rejections. An earlier revision tried to refuse only what
    // is obviously not a verb -- a function word, an adverb, a hyphenated participle, a known
    // leading noun -- and permit the rest, which let every adjective through: the shop
    // manuals say `Six and eight point wrenches are used for heavy, for medium, and for light
    // duty only`, and the purpose `heavy, for medium, and for light duty only` became
    // `Which tool is used to heavy, for medium, and for light duty only?`. English has no
    // morphological test that separates `heavy` from `cut`, `general` from `generate`, or the
    // attributive `cutting tools` from the gerund `cutting metal`, so the only honest test is
    // the lexical one. Recall is what pays for it: a purpose opening with a verb the list has
    // not seen is dropped, and the item is not built. `scripts/probes/purpose-verbs.py` prints
    // every distinct first word the sources produce, which is how this list was written.
    let first = words[0]
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase();
    if !PURPOSE_VERBS.contains(&first.as_str()) {
        return false;
    }
    // A purpose is a clause, and a clause holds no full stop: see `contains_full_stop`. It
    // holds no semicolon either, and that one matters more since the noun-complement forms were
    // read: `the pressure valve also serves as a safety valve to relieve extra pressure within
    // the system; the vacuum valve opens only when the pressure drops` is two sentences, and
    // the first half is the item while the second is the next one.
    if contains_full_stop(purpose) || purpose.contains(';') {
        return false;
    }
    // A purpose containing a letter from another alphabet is a scan this reader cannot
    // repair, and quoting it would teach the learner a word that is not in any language.
    if !uses_only_latin(purpose) {
        return false;
    }
    // Damage the scan left inside the sentence: see `carries_scan_damage`.
    if carries_scan_damage(purpose) {
        return false;
    }
    // A purpose with a bracketed figure reference asks the learner to look at a
    // drawing that is not in the item.
    if purpose.contains("(fig") || purpose.contains("figure ") || purpose.contains("Fig.") {
        return false;
    }
    true
}

/// Verbs these manuals open a purpose with, in the form the sentence uses.
///
/// Both the base form and the `-ing` form are listed separately because the sources use both
/// (`cut` and `cutting`, `drive` and `driving`) and deriving one from the other in code gets
/// the spelling wrong for the verbs that double a consonant or drop a silent `e`.
const PURPOSE_VERBS: &[&str] = &[
    "absorb",
    "accomplish",
    "adhere",
    "adjust",
    "aid",
    "allow",
    "apply",
    "applying",
    "attach",
    "attaching",
    "avoid",
    "balance",
    "bend",
    "bind",
    "block",
    "bore",
    "boring",
    "break",
    "burn",
    "carry",
    "change",
    "charge",
    "check",
    "checking",
    "circulate",
    "clean",
    "cleaning",
    "close",
    "collect",
    "combine",
    "compensate",
    "compress",
    "connect",
    "contain",
    "control",
    "convert",
    "cool",
    "coordinate",
    "cut",
    "cutting",
    "deliver",
    "dent",
    "describe",
    "designate",
    "detect",
    "determine",
    "develop",
    "direct",
    "distribute",
    "dope",
    "drain",
    "dress",
    "dressing",
    "drill",
    "drilling",
    "drive",
    "driving",
    "enable",
    "enlarge",
    "ensure",
    "exert",
    "express",
    "extract",
    "fasten",
    "file",
    "fill",
    "finish",
    "fit",
    "flare",
    "float",
    "follow",
    "force",
    "form",
    "generate",
    "get",
    "grasp",
    "grind",
    "grinding",
    "grip",
    "gripping",
    "group",
    "guide",
    "guiding",
    "handle",
    "heat",
    "help",
    "hold",
    "holding",
    "hone",
    "honing",
    "increase",
    "indicate",
    "induce",
    "inspect",
    "inspecting",
    "install",
    "installing",
    "insulate",
    "interconnect",
    "join",
    "joining",
    "keep",
    "knock",
    "knocking",
    "lay",
    "laying",
    "level",
    "lift",
    "limit",
    "line",
    "link",
    "load",
    "loading",
    "locate",
    "lock",
    "lower",
    "lubricate",
    "lubricating",
    "maintain",
    "make",
    "mark",
    "marking",
    "match",
    "measure",
    "measuring",
    "meet",
    "mix",
    "move",
    "nail",
    "nailing",
    "obtain",
    "observe",
    "open",
    "operate",
    "operating",
    "penetrate",
    "perform",
    "place",
    "plan",
    "planing",
    "position",
    "pour",
    "pressurize",
    "prevent",
    "produce",
    "project",
    "protect",
    "provide",
    "pull",
    "push",
    "put",
    "raise",
    "reach",
    "ream",
    "reduce",
    "regulate",
    "reinforce",
    "remove",
    "resist",
    "retain",
    "retaining",
    "rethread",
    "rotate",
    "run",
    "scatter",
    "scavenge",
    "scavenging",
    "secure",
    "securing",
    "set",
    "setting",
    "shape",
    "shaping",
    "sharpen",
    "smooth",
    "smoothing",
    "solder",
    "space",
    "splice",
    "start",
    "starting",
    "steer",
    "store",
    "strike",
    "striking",
    "support",
    "suppress",
    "take",
    "tap",
    "test",
    "thread",
    "threading",
    "tie",
    "transfer",
    "transmit",
    "trim",
    "trigger",
    "turn",
    "turning",
    "unite",
    "use",
    "whittle",
    "wire",
    "wiring",
    "withstand",
];

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
        let Some((tool, purpose, link)) = describe(&sentence) else {
            continue;
        };
        if !names_a_tool(&tool) || !reads_as_a_purpose(&purpose, link) {
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
            link,
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
    clean(&normalize_homoglyphs(&joined))
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
/// Two shapes are read, and the shape decides only *where* the subject is written, not what
/// counts as a tool or as a purpose: both end in `finish`, which is where the guards live.
fn describe(sentence: &str) -> Option<(String, String, Link)> {
    // A sentence that names the relation puts the subject after the phrase -- `The function of
    // the carburetor is to provide an air-fuel mixture` -- and reading what precedes the verb
    // would name `function`.
    describe_named_relation(sentence).or_else(|| describe_by_verb(sentence))
}

/// The tool and the purpose when the sentence names the relation instead of the verb.
///
/// These manuals write the relation both ways, and this one is common: `The function of the
/// recouperator is to transfer heat from the exhaust gases to the air entering the engine`,
/// `One important function of the power train is to transmit the power of the engine to the
/// wheels`, `The basic function of a suspension lockout system is to bypass the suspension
/// system`. `scripts/probes/description-shapes.py` counts them before they are read, and in
/// TM 9-8000 the shape appears nineteen times against 134 sentences written `is used to`.
///
/// `purpose of` is read for the same reason, and the same two link words: these manuals write
/// `is to` and, less often, `is for`.
fn describe_named_relation(sentence: &str) -> Option<(String, String, Link)> {
    const NAMED: &[&str] = &["function of ", "purpose of "];
    const LINK: &[&str] = &[" is to ", " is for "];
    let lower = sentence.to_lowercase();
    let (after, _) = NAMED
        .iter()
        .filter_map(|phrase| lower.find(phrase).map(|at| (at + phrase.len(), *phrase)))
        .min_by_key(|(after, _)| *after)?;
    // The link has to come after the phrase: `... is a function of the pressure, temperature,
    // and time` states no purpose, and reading the phrase's position alone would cut the
    // sentence in the wrong place.
    let (link_at, link) = LINK
        .iter()
        .filter_map(|link| lower[after..].find(link).map(|at| (after + at, *link)))
        .min_by_key(|(at, _)| *at)?;
    let subject = sentence[after..link_at].trim();
    let purpose = sentence[link_at + link.len()..].trim();
    // A subject that opens with a gerund is an action, not a thing: `The purpose of burning
    // fuel in the priming cup is to thoroughly heat the vaporizing chamber` describes burning
    // fuel, and reading the noun phrase out of it named `burning fuel`, then `idle system` from
    // `The purpose of shutting off the idle system with the engine is to ...`. The verb-before
    // shapes cannot make this mistake, because they ask what stands before the verb rather
    // than what stands after the phrase.
    if opens_with_a_gerund(subject) {
        return None;
    }
    // `The function of the X is to Y` and `The purpose of the X is for Y`: the complement is
    // the clause the link announces, so the form is `to` unless the source wrote `for`.
    let link = if sentence[..link_at].to_lowercase().ends_with(" is for") {
        Link::For
    } else {
        Link::To
    };
    finish(subject, purpose, link)
}

/// The tool and the purpose a sentence states about its subject, from the verb that follows it.
///
/// The subject is taken from *before* the verb, so a heading absorbed into the
/// sentence (`HACKSAWS Hacksaws are used to cut metal`) leaves the last word before the
/// verb as the tool rather than the whole run.
///
/// The relation is stated in more than one set of words, and every shape the reader cannot
/// see is content already in the corpus that never becomes an item. `scripts/probes/
/// description-shapes.py` counts them: in TM 9-8000 alone, beside the 134 sentences written
/// as `is used to`, the manual writes `is designed to` 66 times, `serves to` 23 times and
/// `is intended to` three times.
///
/// Every shape read here puts a bare verb after the phrase, which is the form the question
/// frame needs -- `The governor serves to control engine speed` asks and answers exactly as
/// `is used to control engine speed` does. `scripts/probes/relation-shapes.py` counts the ones
/// that do not, and this list is what survived it: `is employed to` appears 52 times across the
/// Shop and Auto sources, `is made to` 43 and `is adapted to` 11, while `serves as` (94),
/// `acts as` (139) and `is provided with` (87) take a *noun* complement and are deliberately
/// not read -- the frames this module builds take a purpose clause.
///
/// `is made to` is deliberately absent despite its count: the manuals also write
/// `A provision usually is made to install a fuel gage`, whose subject is a provision rather
/// than a tool, and the reader has no noun test that separates them. A shape that produces
/// "Which tool is used to install a fuel gage? -- a provision" is worse than an unread one.
fn describe_by_verb(sentence: &str) -> Option<(String, String, Link)> {
    /// The phrases the manuals state the relation with, and the form each one takes.
    const VERBS: &[(&str, Link)] = &[
        (" are used for ", Link::For),
        (" are used to ", Link::To),
        (" is used for ", Link::For),
        (" is used to ", Link::To),
        (" are used as ", Link::As),
        (" is used as ", Link::As),
        (" are designed to ", Link::To),
        (" is designed to ", Link::To),
        (" are intended to ", Link::To),
        (" is intended to ", Link::To),
        (" serve to ", Link::To),
        (" serves to ", Link::To),
        (" are employed to ", Link::To),
        (" is employed to ", Link::To),
        (" are employed for ", Link::For),
        (" is employed for ", Link::For),
        (" are utilized to ", Link::To),
        (" is utilized to ", Link::To),
        (" are adapted to ", Link::To),
        (" is adapted to ", Link::To),
        (" are adapted for ", Link::For),
        (" is adapted for ", Link::For),
        (" are arranged to ", Link::To),
        (" is arranged to ", Link::To),
        // The noun-complement forms. `scripts/probes/relation-shapes.py` counts 94 `serves as`,
        // 139 `acts as` and 33 `is used as` across the Shop and Auto sources -- more than any
        // other single form -- and none of them was readable before the link was recorded.
        (" serve as ", Link::As),
        (" serves as ", Link::As),
        (" act as ", Link::As),
        (" acts as ", Link::As),
        (" function as ", Link::As),
        (" functions as ", Link::As),
    ];
    let lower = sentence.to_lowercase();
    let (position, verb, link) = VERBS
        .iter()
        .filter_map(|(verb, link)| lower.find(verb).map(|position| (position, *verb, *link)))
        .min_by_key(|(position, _, _)| *position)?;

    let subject = sentence[..position].trim();
    let purpose = sentence[position + verb.len()..].trim();
    finish(subject, purpose, link)
}

/// The guards both shapes share, and the reading of the purpose itself.
fn finish(subject: &str, purpose: &str, link: Link) -> Option<(String, String, Link)> {
    if subject.split_whitespace().count() > MAX_SUBJECT_WORDS {
        return None;
    }
    // A relative clause makes the match belong to the wrong subject. In
    // `The yardstick that is used to measure the ignition quality of a diesel fuel is
    // the cetane-number scale`, the thing being used is the yardstick and the answer is
    // the cetane-number scale: reading the verb where it stands names the wrong object,
    // and the sentence is a definition rather than a description.
    let lower_subject = subject.to_lowercase();
    if [" that", " which", " who"]
        .iter()
        .any(|relative| lower_subject.ends_with(relative))
    {
        return None;
    }
    // A second ` is ` after the purpose is the same definitional frame.
    if purpose.to_lowercase().contains(" is the ") {
        return None;
    }
    let tool = subject_as_tool(subject)?;
    let purpose = lower_scan_capitals(purpose.trim_end_matches('.').trim());
    if purpose.is_empty() {
        return None;
    }
    Some((tool, purpose, link))
}

/// Repair the scan's habit of setting ordinary words in capitals.
///
/// These scans render small capitals as capitals and break lines mid-sentence, so a purpose
/// reads `used for Joining the towing vehicle` and `rising too high In hot weather`. A word
/// whose remaining letters are lower case and whose lower-case form is an ordinary
/// function word is lowered wherever it appears; the first word is lowered too, because a
/// line break at the start of a clause capitalises it. A name such as `Ford` is left alone:
/// it is not a function word and it is not the first word of a purpose.
fn lower_scan_capitals(text: &str) -> String {
    lower_words(text, true)
}

/// The same repair for a name, whose first word keeps its capital.
///
/// `Vises` is the manual's own capitalisation of a tool's name and stays; `slip Joint` is a
/// scanner artefact inside the name and does not.
fn lower_name_capitals(text: &str) -> String {
    lower_words(text, false)
}

fn lower_words(text: &str, lower_first: bool) -> String {
    let mut words: Vec<String> = Vec::new();
    for (position, word) in text.split_whitespace().enumerate() {
        let mut characters = word.chars();
        let lowered = match characters.next() {
            Some(first) if first.is_uppercase() => {
                let rest: String = characters.collect();
                if rest.chars().any(|c| c.is_uppercase()) {
                    // An acronym or a heading keeps its capitals.
                    None
                } else {
                    let candidate = format!("{}{}", first.to_lowercase(), rest);
                    let ordinary = FUNCTION_WORDS.contains(&candidate.as_str());
                    if (lower_first && position == 0) || ordinary {
                        Some(candidate)
                    } else if position > 0 {
                        // An interior capital inside a name is the scanner's small capitals,
                        // as in `slip Joint`; the first word is left as the manual wrote it.
                        Some(candidate)
                    } else {
                        None
                    }
                }
            }
            _ => None,
        };
        words.push(lowered.unwrap_or_else(|| word.to_string()));
    }
    words.join(" ")
}

/// Words a scan may set in capitals mid-sentence without meaning a name.
const FUNCTION_WORDS: [&str; 24] = [
    "in", "and", "the", "to", "for", "with", "from", "of", "or", "on", "at", "by", "is", "are",
    "was", "were", "as", "if", "it", "its", "into", "than", "then", "when",
];

/// Prepositions that end a tool's name.
///
/// `Nails with large flat heads are used for ...` names one tool: `Nails`. Taking the
/// words next to the verb instead produced `large flat heads`, which is part of a nail
/// rather than a tool.
const ENDS_THE_NAME: [&str; 49] = [
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
    // Prepositions the first list missed. `A power takeoff installed at the left side of a
    // transmission` named "the left side" until they were added: the first boundary word it
    // recognised was `of`, so everything between `at` and `of` became the name.
    "at",
    "under",
    "over",
    "near",
    "between",
    "through",
    "during",
    "after",
    "before",
    "against",
    "along",
    "across",
    "around",
    "behind",
    "below",
    "beneath",
    "beside",
    "beyond",
    "inside",
    "outside",
    "toward",
    "towards",
    "within",
    "without",
    "upon",
    // A second verb means a second clause, and the description in that clause belongs to
    // its own subject rather than to anything in this one.
    "is",
    "are",
    "was",
    "were",
    "has",
    "have",
    "had",
];

/// Words that mean the "subject" is a continuation of the previous clause.
const NOT_A_SUBJECT_START: [&str; 4] = ["or", "and", "but", "nor"];

/// Whether a run of prose carries damage these scans leave inside a sentence.
///
/// Each shape below reached a learner-facing purpose or option, and none can be repaired
/// without this program writing the manual's words, so the description is refused:
///
/// * a word the scan broke -- `prevent leakage be- tween rotating and nonrotating members`
///   (TM 9-8000). The scan kept the hyphen and dropped the line break;
/// * a figure's label read into the sentence -- `absorb heat from the airstream directed into
///   the EVAPORATOR CORE CAPILLARY TUBE, ...`. `lower_scan_capitals` repairs small capitals,
///   which is one word set in capitals; a run of them is a label from a drawing;
/// * a measurement whose number the scan dropped -- `provide a white light of to candlepower
///   at a distance of feet directly in front of the lamp`, `at a rate of at least gallons
///   liters) per minute`, `provide approximately a to air-fuel ratio`, `it has percent of the
///   strength of the rope`. A unit with no number in front of it, or the bigram `of to`,
///   states a quantity the learner cannot know;
/// * a bracket that is opened and never closed, which is the same dropped text.
///
/// The test is a refusal, not a repair: a purpose that carries any of them is left to the
/// source, because the alternative is an item that teaches a sentence nobody wrote.
fn carries_scan_damage(text: &str) -> bool {
    // A broken word: a letter, a hyphen, a space, a lower-case letter.
    let characters: Vec<char> = text.chars().collect();
    for window in characters.windows(4) {
        if window[0].is_alphabetic()
            && window[1] == '-'
            && window[2].is_whitespace()
            && window[3].is_lowercase()
        {
            return true;
        }
    }

    // A figure's label: a lower-case letter standing on its own. The manuals number the parts
    // of a drawing and then refer to them -- `the tool e would cut the [V]-shaped groove i` --
    // and a sentence that names its own parts that way is describing a picture, not the tool.
    // `a` and `i` are words, so they are not counted.
    for word in text.split_whitespace() {
        let bare = word.trim_matches(|c: char| !c.is_alphabetic());
        if bare.chars().count() == 1
            && bare.chars().all(|c| c.is_lowercase())
            && bare != "a"
            && bare != "i"
        {
            return true;
        }
    }

    // A run of capitals: two or more words of three letters or more, all capitals.
    let mut capitals_in_a_row = 0;
    for word in text.split_whitespace() {
        let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
        let is_label = letters.len() >= 3 && letters.iter().all(|c| c.is_uppercase());
        capitals_in_a_row = if is_label { capitals_in_a_row + 1 } else { 0 };
        if capitals_in_a_row >= 2 {
            return true;
        }
    }

    // A unit with no number before it, and the two bigrams a dropped word leaves behind.
    const UNITS: &[&str] = &[
        "mph", "km/h", "psi", "rpm", "percent", "gallons", "liters", "amperes", "volts", "ohms",
        "watts", "degrees",
    ];
    let lower = text.to_lowercase();
    if lower.contains(" of to ") || lower.contains(" a to ") {
        return true;
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/');
        if !UNITS.contains(&bare) {
            continue;
        }
        let has_a_number_before = index > 0 && words[index - 1].chars().any(|c| c.is_ascii_digit());
        if !has_a_number_before {
            return true;
        }
    }

    // An unbalanced bracket.
    let opens = text.matches(['(', '[']).count();
    let closes = text.matches([')', ']']).count();
    opens != closes
}

/// Whether a run of text uses only the Latin letters these manuals are written in.
///
/// The scanners substitute letters from other alphabets, and `normalize_homoglyphs` undoes
/// the ones that are shape-identical. What is left cannot be undone by guessing: an archived
/// copy of TM 9-8000 offers `дeaг-ana rotor-type pumps`, `Oй filters` and `Tһe bulkhead
/// receptacle`, where `й` stands for two letters and `юг` for `for`. Replacing them would be
/// this program writing the manual rather than reading it, so the description is refused.
/// Quotation marks and dashes carry no alphabet and are not affected.
fn uses_only_latin(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii() || !c.is_alphabetic())
}

/// Whether a run of prose carries a full stop inside it.
///
/// A purpose is a clause, and a clause holds no full stop. One inside it means either that
/// two sentences were read as one or that the scanner set a stop in the middle of a line.
/// Both happened: `abrasive paper is used to cut this fuzz from the wood. “Sandpaper”
/// consists of small particles of flint glued to a paper backing` became a single answer
/// because the opening quote after the stop kept `split_sentences` from ending the sentence,
/// and `when setting in. panes of glass` is the same damage without a quote. A decimal point
/// is followed by a digit and an abbreviation by a letter, so a stop followed by whitespace,
/// or by nothing at all, is punctuation rather than a measurement.
fn contains_full_stop(text: &str) -> bool {
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        if *character != '.' {
            continue;
        }
        let next = characters[index + 1..]
            .iter()
            .find(|c| !matches!(c, '”' | '"' | '’' | '\'' | ')' | ']'))
            .copied();
        match next {
            None => return true,
            Some(character) if character.is_whitespace() => return true,
            Some(_) => {}
        }
    }
    false
}

/// Replace the letters a scanner substituted from another alphabet.
///
/// The scans render a Latin `A` as a Cyrillic `А` often enough to matter: one sentence read
/// `... moves up and down. А center bearing generally is used to support the drive shaft`,
/// and because the splitter looks for an ASCII capital to start a sentence, the boundary was
/// missed and the centre bearing's function was attributed to the slip joint. Normalising
/// the homoglyphs restores the boundary, and with it the right subject.
fn normalize_homoglyphs(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\u{0410}' => 'A', // Cyrillic A
            '\u{0412}' => 'B',
            '\u{0415}' => 'E',
            '\u{041a}' => 'K',
            '\u{041c}' => 'M',
            '\u{041d}' => 'H',
            '\u{041e}' => 'O',
            '\u{0420}' => 'P',
            '\u{0421}' => 'C',
            '\u{0422}' => 'T',
            '\u{0423}' => 'Y',
            '\u{0425}' => 'X',
            '\u{0430}' => 'a', // Cyrillic a
            '\u{0435}' => 'e',
            '\u{043e}' => 'o',
            '\u{0440}' => 'p',
            '\u{0441}' => 'c',
            '\u{0443}' => 'y',
            '\u{0445}' => 'x',
            // Ukrainian and Belarusian letters the same scanners substitute, including the
            // `і` that turned `in their product` into `іп their product`.
            '\u{0456}' => 'i',
            '\u{0455}' => 's',
            '\u{0458}' => 'j',
            '\u{043f}' => 'n',
            '\u{0432}' => 'b',
            '\u{043a}' => 'k',
            '\u{043c}' => 'm',
            '\u{043d}' => 'h',
            '\u{0442}' => 't',
            other => other,
        })
        .collect()
}

/// The longest a subject may be before it is treated as a missed sentence boundary.
///
/// A subject in these manuals is a noun phrase: `The open hooks on either side at the front
/// end of the frame` is a long one at eleven words. A "subject" of twenty words is two
/// sentences the scanner ran together.
const MAX_SUBJECT_WORDS: usize = 14;

/// The tool or component a sentence's subject names.
///
/// Four things sit in front of a name in these texts and none of them is part of it: the
/// section heading the scan ran into the sentence (`GAGE Telescoping gages`), a leading
/// article (`The sliding T-bevel`), a phrase describing the thing rather than naming it
/// (`Nails with large flat heads`), and a whole clause from an earlier part of the sentence
/// (`The pressure developed in the hoist cylinder ... , it also`). Reading the head of the
/// noun phrase -- the last words before the verb, in the sentence's last clause -- is what
/// gets the name; taking the first words produced `pressure developed` and
/// `Ammeter The ammeter`.
fn subject_as_tool(subject: &str) -> Option<String> {
    // The last clause: a comma means the sentence has moved on from its subject.
    let clause = subject.rsplit(',').next().unwrap_or(subject);
    let words: Vec<&str> = clause.split_whitespace().collect();
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

    // Drop a leading article.
    if chosen
        .first()
        .is_some_and(|word| matches!(word.to_lowercase().as_str(), "the" | "a" | "an"))
    {
        chosen.remove(0);
    }

    // A prepositional phrase standing first is not the name: `with the micrometer` names the
    // micrometer. A preposition cannot *end* a name by standing first in it, though, and
    // these manuals name tools `Inside micrometers` and `Outside calipers` -- truncating at
    // the leading `inside` emptied the name and lost the tool.
    while chosen.len() > 1
        && ENDS_THE_NAME.contains(&chosen[0].to_lowercase().as_str())
        && matches!(chosen[1].to_lowercase().as_str(), "the" | "a" | "an")
    {
        chosen.remove(0);
    }

    // Stop at a preposition: everything after it describes the thing. The first word is
    // exempt, because a name begins with its own first word.
    if let Some(position) = chosen
        .iter()
        .enumerate()
        .filter(|(index, _)| *index > 0)
        .find(|(_, word)| ENDS_THE_NAME.contains(&word.to_lowercase().as_str()))
        .map(|(index, _)| index)
    {
        chosen.truncate(position);
    }

    // Keep the head of the noun phrase, which is its end.
    if chosen.len() > 3 {
        chosen.drain(..chosen.len() - 3);
    }
    // An article can survive the truncation that put the head of the phrase in view.
    if chosen
        .first()
        .is_some_and(|word| matches!(word.to_lowercase().as_str(), "the" | "a" | "an"))
    {
        chosen.remove(0);
    }
    // An adverb comments on the verb rather than naming anything: `Electric motors generally`.
    while chosen.len() > 1
        && chosen
            .last()
            .is_some_and(|word| is_adverb(&word.to_lowercase()))
    {
        chosen.pop();
    }

    // A clause fragment is not a name: `the hydraulic cylinder is double acting`.
    if chosen
        .iter()
        .any(|word| NAME_STOP_WORDS.contains(&word.to_lowercase().as_str()))
    {
        return None;
    }

    // A repeated word is a heading the scan ran into the name. Three shapes occur:
    // `GAGE Telescoping gages` (the heading repeats a later word), `Auger Bits Bits` (the
    // heading and the name are the same words twice) and `Ammeter The ammeter` (the heading,
    // an article, then the name). Dropping a leading word that duplicates a later one,
    // collapsing consecutive duplicates, and dropping a leading article settles all three.
    loop {
        // Collapse a word the scan doubled: `Auger Bits Bits`.
        let mut deduped: Vec<&str> = Vec::new();
        for word in chosen.iter().copied() {
            if deduped
                .last()
                .is_some_and(|previous| singular(previous) == singular(word))
            {
                continue;
            }
            deduped.push(word);
        }
        let before = deduped.len();
        // Drop a heading that repeats the name's last word: `GAGE Telescoping gages`.
        if deduped.len() >= 2 && singular(deduped[0]) == singular(deduped[deduped.len() - 1]) {
            deduped.remove(0);
        }
        // Then an article the heading left behind: `Ammeter The ammeter`.
        if deduped.len() >= 2 && matches!(deduped[0].to_lowercase().as_str(), "the" | "a" | "an") {
            deduped.remove(0);
        }
        let shrunk = deduped.len() < chosen.len();
        chosen = deduped;
        if !shrunk || chosen.len() == before {
            break;
        }
    }

    if chosen.is_empty() {
        return None;
    }
    // A lone adverb or conjunction is the sentence's connective, not a name:
    // `... it also is used to hold or lower the dump body`.
    if chosen.len() == 1 {
        let only = chosen[0].to_lowercase();
        if is_adverb(&only) || NOT_A_SUBJECT_START.contains(&only.as_str()) {
            return None;
        }
    }
    let tool = lower_name_capitals(
        chosen
            .join(" ")
            .trim_end_matches(['.', ',', ';', ':'])
            .trim(),
    );
    (!tool.is_empty()).then_some(tool)
}

/// Whether a word is an adverb, which modifies the verb rather than naming anything.
fn is_adverb(word: &str) -> bool {
    word.ends_with("ly")
        || matches!(
            word,
            "generally"
                | "usually"
                | "normally"
                | "often"
                | "sometimes"
                | "also"
                | "always"
                | "almost"
                | "nearly"
                | "quite"
                | "very"
                | "only"
                | "still"
                | "even"
                | "just"
                | "particularly"
                | "especially"
                | "mainly"
                | "mostly"
        )
}

/// A word without its plural, so `Bits` and `Bit` compare equal.
fn singular(word: &str) -> String {
    let lower = word.to_lowercase();
    lower.strip_suffix('s').map(str::to_string).unwrap_or(lower)
}

/// A function as an Auto Information option states it.
///
/// The frame follows the source: `is used to prevent leakage` becomes the option "to prevent
/// leakage", and `serves as a thrust bearing` becomes "a thrust bearing", because the question
/// it answers is "What does the X serve as?".
fn function_option(entry: &Purpose) -> String {
    match entry.link {
        Link::As => entry.purpose.clone(),
        _ => format!("to {}", entry.purpose),
    }
}

/// The question an Auto Information item asks about a component.
fn function_question(component: &str, link: Link) -> String {
    match link {
        Link::As if is_plural(component) => format!("What do the {component} serve as?"),
        Link::As => format!("What does the {component} serve as?"),
        _ => component_question(component),
    }
}

/// Whether a component's name is plural, which decides whether its question says do or does.
fn is_plural(component: &str) -> bool {
    component.split_whitespace().last().is_some_and(|word| {
        let lower = word.to_lowercase();
        lower.ends_with('s') && !lower.ends_with("ss")
    })
}

/// The question a purpose clause answers.
///
/// The source writes purposes two ways -- `used to cut metal` and `used for cutting
/// metal` -- and the question has to follow the source rather than impose one form on
/// it, or the item asks which tool is "used to laying out angles".
fn question_for(purpose: &str, link: Link) -> String {
    // The link the source used decides the frame, and `For` is inferred from the purpose's own
    // form when the source did not state one: a purpose read from a description that named the
    // relation (`The function of the X is to Y`) carries its link, and one whose sentence was
    // split by the scanner may not match the table.
    let link = match link {
        Link::For => Link::For,
        Link::As => Link::As,
        Link::To if opens_with_a_gerund(purpose) => Link::For,
        Link::To => Link::To,
    };
    link.tool_question(purpose)
}

/// Whether a purpose opens with a gerund, which is the form `is used for cutting` states.
///
/// Punctuation belongs to the sentence, not to the word. The source writes `used for
/// gripping, reaching places not readily accessible`, and reading the comma as part of the
/// first word hid its `-ing` ending and produced `used to gripping` in the corpus.
fn opens_with_a_gerund(purpose: &str) -> bool {
    purpose
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase()
        .ends_with("ing")
}

/// The purpose clause a prompt asks about, whichever form it was written in.
fn purpose_in_prompt(prompt: &str) -> Option<String> {
    for prefix in [
        "Which tool is used to ",
        "Which tool is used for ",
        "Which tool serves as ",
    ] {
        if let Some(rest) = prompt.strip_prefix(prefix) {
            return Some(rest.trim_end_matches('?').trim().to_string());
        }
    }
    None
}

/// The link a prompt was written with, recovered from the prompt itself.
///
/// The frame is in the question, so a verifier reading the question needs nothing else -- and
/// an item whose prompt and options disagree about the form is caught by comparing them.
fn link_in_prompt(prompt: &str) -> Option<Link> {
    if prompt.starts_with("Which tool serves as ") {
        Some(Link::As)
    } else if prompt.starts_with("Which tool is used for ") {
        Some(Link::For)
    } else if prompt.starts_with("Which tool is used to ") {
        Some(Link::To)
    } else {
        None
    }
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
        self.build_items_asking(subtest, ItemKind::Tool, count, seed, &mut accept)
    }

    /// Build items that ask what a component is for.
    ///
    /// The mirror image of [`Purposes::build_items`] over the same sentences: the prompt
    /// names the component and the options are functions, which is what Auto Information
    /// asks. The source's sentences are the same ones a tool manual carries --
    /// `The throttle valve is used to regulate the speed and power output of the engine` --
    /// so the reason to ask the other way round is the subtest, not the text.
    pub fn build_function_items<F>(
        &self,
        subtest: &str,
        count: usize,
        seed: u64,
        mut accept: F,
    ) -> Vec<PurposeItem>
    where
        F: FnMut(&PurposeItem) -> bool,
    {
        self.build_items_asking(subtest, ItemKind::Function, count, seed, &mut accept)
    }

    fn build_items_asking<F>(
        &self,
        subtest: &str,
        kind: ItemKind,
        count: usize,
        seed: u64,
        accept: &mut F,
    ) -> Vec<PurposeItem>
    where
        F: FnMut(&PurposeItem) -> bool,
    {
        if self.entries.len() < 4 {
            return Vec::new();
        }
        // An Auto Information item offers functions as its options, and the options have to be
        // parallel or the item tests the reader's tolerance rather than their knowledge. The
        // manuals state some functions as `is used to secure` and others as `is used for
        // joining`, and framing the second kind with `to ` produced `to joining the towing
        // vehicle and the trailer with safety chains` -- the corpus had five of those. The
        // fix is not to conjugate the manual's words into a form it did not write: it is to
        // ask the Shop Information question about them instead, which is why they are dropped
        // here and not rewritten.
        let entries: Vec<&Purpose> = match kind {
            // A Shop Information item asks which *tool* does something, and a description
            // written `the relay serves as a switch` names something the tool is, not something
            // it does: the answer would be a component offered as a tool. Those descriptions are
            // asked the other way round, where they are exactly right.
            ItemKind::Tool => self
                .entries
                .iter()
                .filter(|entry| entry.link != Link::As)
                .collect(),
            ItemKind::Function => self
                .entries
                .iter()
                .filter(|entry| !opens_with_a_gerund(&entry.purpose))
                .collect(),
        };
        if entries.len() < 4 {
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
            let correct = entries[rng.range(0, entries.len() as i64 - 1) as usize];

            // The correct answer, and what the distractors are drawn from: the same
            // relation read from whichever end the question asks about.
            let (correct_option, prompt) = match kind {
                ItemKind::Tool => (
                    correct.tool.clone(),
                    format!("{}?", question_for(&correct.purpose, correct.link)),
                ),
                ItemKind::Function => (
                    function_option(correct),
                    function_question(&correct.tool, correct.link),
                ),
            };
            let mut options: Vec<(String, Option<String>)> = vec![(correct_option, None)];

            // Distractors are the other half of other descriptions in the same manual, so
            // each is a real answer to a real question rather than filler.
            let mut candidates: Vec<&Purpose> = entries
                .iter()
                .copied()
                .filter(|entry| normalize(&entry.tool) != normalize(&correct.tool))
                .collect();
            rng.shuffle(&mut candidates);
            for other in candidates {
                if options.len() == 4 {
                    break;
                }
                // A distractor has to be the same *kind* of answer as the correct one, or the
                // item offers an infinitive beside a noun phrase and the form gives the answer
                // away. This is the parallelism rule of the gerund fix, applied to the link.
                if kind == ItemKind::Function
                    && (other.link == Link::As) != (correct.link == Link::As)
                {
                    continue;
                }
                let (option, rationale) = match kind {
                    ItemKind::Tool => (
                        other.tool.clone(),
                        format!("{} is for {}.", other.tool, other.purpose),
                    ),
                    ItemKind::Function => (
                        function_option(other),
                        format!("That is what {} is for.", other.tool),
                    ),
                };
                if options
                    .iter()
                    .any(|(existing, _)| normalize(existing) == normalize(&option))
                {
                    continue;
                }
                options.push((option, Some(rationale)));
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
                kind,
                objective_id: match kind {
                    ItemKind::Tool => format!("OBJ-{subtest}-TOOLS-01"),
                    ItemKind::Function => format!("OBJ-{subtest}-FUNCTION-01"),
                },
                // A purpose read from `is used to cut` is an infinitive; one read from
                // `is used for cutting` is a gerund. The prompt has to take whichever
                // the source wrote, or it asks the learner about a tool "used to laying
                // out angles". An Auto Information item is the other way round: the
                // functions are its options, and the gerund descriptions are not offered
                // here at all -- see the filter above.
                prompt,
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
    match item.kind {
        ItemKind::Tool => verify_tool_item(item, source),
        ItemKind::Function => verify_function_item(item, source),
    }
}

/// The question a component's description answers.
///
/// The verb has to agree with the component: `What is the open hooks used for?` reads as a
/// program talking, while the source's own sentence says `hooks are used for`.
fn component_question(component: &str) -> String {
    if is_plural(component) {
        format!("What are the {component} used for?")
    } else {
        format!("What is the {component} used for?")
    }
}

/// The component a `What is/are the <component> used for?` prompt names.
fn component_in_prompt(prompt: &str) -> Option<String> {
    // Singular and plural, and both frames: `What is the relay used for?`, `What are the
    // brushes used for?`, `What does the relay serve as?`, `What do the pole shoes serve as?`.
    for (prefixes, suffix) in [
        (["What is the ", "What are the "], " used for?"),
        (["What does the ", "What do the "], " serve as?"),
    ] {
        for prefix in prefixes {
            if let Some(rest) = prompt.strip_prefix(prefix) {
                if let Some(rest) = rest.strip_suffix(suffix) {
                    return (!rest.trim().is_empty()).then(|| rest.trim().to_string());
                }
            }
        }
    }
    None
}

/// The function an option states, without the `to ` it is written with.
fn function_in_option(option: &str) -> String {
    option
        .trim()
        .strip_prefix("to ")
        .unwrap_or(option.trim())
        .to_string()
}

/// Whether an option is written in the form its question asks for.
///
/// A question that says `serve as` takes a noun phrase and one that says `used for` takes an
/// infinitive; an item whose options are in the other form offers the answer's shape as a clue.
/// The check is on the *form* only, because whether the words are right is what the rest of
/// verification is for.
fn option_matches_link(option: &str, link: Link) -> bool {
    match link {
        Link::As => !option.trim_start().starts_with("to "),
        _ => option.trim_start().starts_with("to "),
    }
}

fn verify_function_item(
    item: &PurposeItem,
    source: &Purposes,
) -> Result<(), PurposeVerificationFailure> {
    if item.prompt.trim().is_empty() {
        return Err(PurposeVerificationFailure::Blank("prompt"));
    }
    let Some(component) = component_in_prompt(&item.prompt) else {
        return Err(PurposeVerificationFailure::NotAPurposeQuestion(
            item.prompt.clone(),
        ));
    };
    // The question says which form its answers take. An item whose prompt says `serve as` and
    // whose options begin `to ` hands the learner the shape of the answer.
    let link = if item.prompt.trim_end().ends_with(" serve as?") {
        Link::As
    } else {
        Link::To
    };
    if !item
        .options
        .iter()
        .all(|option| option_matches_link(option, link))
    {
        return Err(PurposeVerificationFailure::OptionFormMismatch {
            prompt: item.prompt.clone(),
        });
    }
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

    let correct = function_in_option(&item.options[item.correct_index]);
    if !source.says(&component, &correct) {
        return Err(PurposeVerificationFailure::NotFromSource(correct));
    }

    let mut seen: HashSet<String> = HashSet::new();
    for (position, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(PurposeVerificationFailure::Blank("option"));
        }
        if !seen.insert(normalize(option)) {
            return Err(PurposeVerificationFailure::DuplicateOption(option.clone()));
        }
        if position == item.correct_index {
            continue;
        }
        let function = function_in_option(option);
        // A distractor has to be a function the source states for *another* component, or
        // it is not an answer to anything the manual says.
        let Some(entry) = source.entries.iter().find(|entry| {
            normalize(&entry.purpose) == normalize(&function)
                && normalize(&entry.tool) != normalize(&component)
        }) else {
            return Err(PurposeVerificationFailure::UnknownDistractor(
                option.clone(),
            ));
        };
        // Read back through the source: it must state this function for that component.
        if !source.says(&entry.tool, &function) {
            return Err(PurposeVerificationFailure::UnknownDistractor(
                option.clone(),
            ));
        }
        if normalize(&function) == normalize(&correct) {
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

fn verify_tool_item(
    item: &PurposeItem,
    source: &Purposes,
) -> Result<(), PurposeVerificationFailure> {
    if item.prompt.trim().is_empty() {
        return Err(PurposeVerificationFailure::Blank("prompt"));
    }
    let Some(purpose) = purpose_in_prompt(&item.prompt) else {
        return Err(PurposeVerificationFailure::NotAPurposeQuestion(
            item.prompt.clone(),
        ));
    };
    // A Shop Information item asks which tool *does* something, so a noun-complement frame --
    // `Which tool serves as a switch?` -- has no place here: the answer would be a component
    // offered as a tool.
    if link_in_prompt(&item.prompt) == Some(Link::As) {
        return Err(PurposeVerificationFailure::NotAPurposeQuestion(
            item.prompt.clone(),
        ));
    }
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
    fn a_letter_from_another_alphabet_is_refused() {
        // Verbatim from the archived TM 9-8000 and TM 9-2700 scans, except for the Cyrillic
        // letters, which are the scans' own. `normalize_homoglyphs` repairs the letters that
        // are shape-identical; `й` for `il`, `юг` for `for` and `һ` for `h` cannot be
        // repaired without this program inventing the manual's words.
        let source = parse_purposes(
            "Oй filters are used to help remove impurities from the oil.\n\
             Tһe bulkhead receptacle is used to penetrate a panel while maintaining a seal.\n\
             дeaг-ana rotor-type pumps are used to generate fluid pressure юг the lubrication system.\n\
             Oil filters are used to remove impurities юг the oil.\n\
             Oil filters are used to remove impurities from the oil.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        assert!(names.iter().all(|name| name.is_ascii()), "{names:?}");
        // The purpose is checked separately from the name: the fourth sentence names a clean
        // tool and states the damage in the purpose, so a rule that reads only names would
        // leave it standing.
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| entry.purpose.is_ascii()),
            "{:?}",
            source.entries()
        );
        assert!(
            names.contains(&"Oil filters"),
            "the clean description still parses: {names:?}"
        );
    }

    #[test]
    fn a_full_stop_hidden_by_a_closing_quote_still_ends_the_clause() {
        // Verbatim from TM 9-8000: the purpose runs into the next sentence because the stop
        // is inside the closing quote, so the splitter saw no capital to break on.
        let source = parse_purposes(
            "PISTONS\n\
             Special ribs are used to reinforce the piston-pin “bosses.” the radial-engine piston varies only slightly.\n\
             Piston pins are used to join the piston to the connecting rod.\n\
             Piston rings are used to seal the combustion chamber.\n\
             Connecting rods are used to connect the piston to the crankshaft.\n\
             Cylinder walls are used to guide the piston.\n",
            "A Test Manual",
        );
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.purpose.contains("radial-engine")),
            "{:?}",
            source.entries()
        );
        // The decimal point stays a measurement rather than a clause, so a purpose that
        // states a tolerance is still readable.
        assert!(!contains_full_stop("measure a clearance of 0.010 inch"),);
        assert!(contains_full_stop(
            "reach the piston-pin “bosses.” the next one"
        ));
        assert!(contains_full_stop("hold the work."));
    }

    #[test]
    fn a_class_or_a_generalisation_is_not_a_tool() {
        // Verbatim from `Tools and Their Uses`, where the sentence before the description
        // reads `Two types of fasteners are used: the more permanent type is used to fasten
        // together parts that may have to be taken apart later.` The miner took `the more
        // permanent type` for the tool, and it reached the corpus as an option.
        let source = parse_purposes(
            "FASTENERS\n\
             The more permanent type is used to fasten together parts that may have to be taken apart later.\n\
             Setscrews are used to secure small pulleys, gears, and cams to shafts.\n\
             Whatever method is used to secure the sleeve, it is very important that the sleeve fits tightly.\n\
             Both hands are used to set the micrometer for checking a diameter.\n\
             The catch trough then is used to collect the oil and return it to the sump.\n\
             The automotive manufacturers’ requirements are used to meet the demands of smaller engines.\n\
             Machinists use a micrometer to check a diameter.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        assert!(
            names.iter().all(|name| {
                let lower = name.to_lowercase();
                !lower.ends_with("type")
                    && !lower.starts_with("whatever")
                    && !lower.starts_with("both")
                    && !lower.ends_with("then")
                    && !name.contains(['\'', '\u{2019}'])
            }),
            "{names:?}"
        );
        // The positive control: the description of the setscrews still parses, so the rule
        // is not refusing every subject that follows one it refused.
        assert!(source.purpose_of("Setscrews").is_some(), "{names:?}");
    }

    #[test]
    fn an_adjective_is_not_a_purpose() {
        // Verbatim from `Tools and Their Uses`. The rule that refused only what was
        // *obviously* not a verb accepted this purpose -- `heavy` is not a function word, not
        // an adverb, and not hyphenated -- and built `Which tool is used to heavy, for
        // medium, and for light duty only?`. The line is also the reason `Six and eight` is
        // missing from the tool's name: the numerals are dropped, and only the last three
        // words survive.
        let source = parse_purposes(
            "WRENCHES\n\
             Six and eight point wrenches are used for heavy, for medium, and for light duty only.\n\
             Open end wrenches are used to turn nuts and bolts.\n",
            "A Test Manual",
        );
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.purpose.starts_with("heavy")),
            "{:?}",
            source.entries()
        );
        // The positive control: the same source still yields the purpose that does start
        // with a verb, so the test fails if the rule refuses both.
        assert!(
            source
                .entries()
                .iter()
                .any(|entry| entry.purpose == "turn nuts and bolts"),
            "{:?}",
            source.entries()
        );
    }

    #[test]
    fn a_comma_after_the_first_word_does_not_hide_its_ending() {
        // `Longnose pliers are used for gripping, reaching places not readily accessible to
        // the hand` reached the corpus as `Which tool is used to gripping, ...?` because the
        // comma made the first word look like a bare verb.
        assert_eq!(
            question_for("gripping, reaching places not readily accessible", Link::To),
            "Which tool is used for gripping, reaching places not readily accessible"
        );
        assert_eq!(
            question_for(
                "honing, which brings the cutting edge to keenness",
                Link::To
            ),
            "Which tool is used for honing, which brings the cutting edge to keenness"
        );
        assert_eq!(
            question_for("cut metal that is too heavy for snips", Link::To),
            "Which tool is used to cut metal that is too heavy for snips"
        );
    }

    #[test]
    fn a_purpose_does_not_span_a_sentence_break() {
        let source = parse_purposes(
            "ABRASIVE PAPER\n\
             Abrasive paper is used to cut this fuzz from the wood. “Sandpaper” consists of \n\
             small particles of flint glued to a paper backing.\n\
             PUTTY KNIVES\n\
             A putty knife is used for applying putty to window sash when setting in. panes \n\
             of glass are held in place.\n\
             MICROMETERS\n\
             Micrometers are used to measure distances to the nearest one thousandth of an \n\
             inch.\n",
            "A Test Manual",
        );
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.purpose.contains(". ")),
            "{:?}",
            source.entries()
        );
        // The positive control: the description with no stray stop still parses.
        assert!(
            source.purpose_of("Micrometers").is_some(),
            "{:?}",
            source.entries()
        );

        // A semicolon joins two clauses, so a purpose that holds one is two sentences. TM 9-2700
        // writes `the pressure valve also serves as a safety valve to relieve extra pressure
        // within the system; the vacuum valve opens only when the pressure drops`.
        assert!(
            !reads_as_a_purpose(
                "a safety valve to relieve extra pressure within the system; the vacuum valve \
                 opens only when the pressure drops",
                Link::As
            ),
            "a purpose holds no semicolon"
        );
        assert!(
            !reads_as_a_purpose("cut the thread; the die is turned back", Link::To),
            "a purpose holds no semicolon, whichever link it came in by"
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

#[cfg(test)]
mod function_tests {
    use super::*;

    /// An automotive manual's shape: components described by what they are for.
    const MANUAL: &str = "\
The throttle valve is used to regulate the speed and power output of the engine.
The fuel pump is used to deliver fuel from the tank to the carburetor.
The oil pump is used to circulate oil through the engine.
The radiator is used to cool the coolant heated by the engine.
The battery is used to store electrical energy for the starting motor.
";

    fn manual() -> Purposes {
        parse_purposes(MANUAL, "A Test Manual")
    }

    #[test]
    fn the_question_asks_what_the_component_is_for() {
        let items = manual().build_function_items("AI", 8, 7, |_| true);
        assert!(!items.is_empty(), "the fixture should yield items");
        for item in &items {
            assert_eq!(item.kind, ItemKind::Function);
            assert!(
                item.prompt.starts_with("What is the ") || item.prompt.starts_with("What are the "),
                "{}",
                item.prompt
            );
            assert!(item.prompt.ends_with(" used for?"), "{}", item.prompt);
            // The options are functions, written as the source writes them.
            for option in &item.options {
                assert!(
                    option.starts_with("to "),
                    "option is not a function: {option}"
                );
            }
        }
    }

    #[test]
    fn a_name_that_starts_with_a_boundary_word_keeps_it() {
        // `Inside micrometers` and `Outside calipers` are tools these manuals name, and
        // `inside` and `outside` are also prepositions that end a name (`the inside of the
        // tube`). Truncating at the leading one emptied the name and lost the tool; truncating
        // at a prepositional phrase that opens with an article still reduces to the noun.
        let source = parse_purposes(
            "Inside micrometers are used for measuring inside dimensions of a bored hole.\n\
             Outside calipers are used to measure the outside diameter of a shaft.\n\
             Nails with large flat heads are used for nailing roof paper and similar thin materials.\n\
             The sliding T-bevel is used for laying out angles other than right angles.\n\
             Micrometers are used to measure distances to the nearest one thousandth of an inch.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        assert!(names.contains(&"Inside micrometers"), "{names:?}");
        assert!(names.contains(&"Outside calipers"), "{names:?}");
        // The prepositions still end a name where they should: `Nails with large flat heads`
        // names nails rather than "large flat heads".
        assert!(source.purpose_of("Nails").is_some(), "{names:?}");
    }

    #[test]
    fn a_relation_the_sentence_names_is_read_the_other_way_round() {
        // Every sentence here is verbatim from TM 9-8000, TM 9-2700 or *Tools and Their Uses*.
        // These sentences put the subject *after* the phrase, so reading what precedes the verb
        // would name `function`, `purpose` or `system`.
        let source = parse_purposes(
            "The purpose of the piston skirt is to keep the piston from rocking in the cylinder.\n\
             The primary function of engine lubrication is to reduce the friction between moving parts.\n\
             An additional function of the hair spring is to pull the pointer back to zero when the engine stops.\n\
             The purpose of the suspension system of a vehicle is to support the weight of that vehicle.\n\
             The primary purpose of this system is to provide the necessary oxygen to the catalytic converter.\n\
             The purpose of burning fuel in the priming cup is to thoroughly heat the vaporizing chamber.\n\
             The purpose of shutting off the idle system with the engine is to help eliminate engine dieseling.\n\
             The spontaneous-ignition point of a diesel fuel is a function of the pressure and time.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        assert!(
            source.purpose_of("piston skirt").is_some(),
            "the shape names its subject after the phrase: {names:?}"
        );
        assert!(
            source.purpose_of("engine lubrication").is_some(),
            "{names:?}"
        );
        assert!(source.purpose_of("hair spring").is_some(), "{names:?}");
        assert!(
            source.purpose_of("suspension system").is_some(),
            "{names:?}"
        );
        // The refusals: the phrase's own word is not a tool; a pronoun is not a name; a
        // gerund subject is an action rather than a thing; and a sentence that merely uses
        // `function of` states no purpose at all.
        assert!(source.purpose_of("function").is_none(), "{names:?}");
        assert!(source.purpose_of("purpose").is_none(), "{names:?}");
        assert!(
            names.iter().all(|name| !name.starts_with("this")),
            "{names:?}"
        );
        assert!(
            names.iter().all(|name| !name.contains("burning")),
            "the gerund subject is an action: {names:?}"
        );
        // The second gerund: `The purpose of shutting off the idle system with the engine is to
        // help eliminate engine dieseling` (the page header the scan ran into the middle of the
        // sentence is dropped here, which is what `clean_scan` does). Its purpose does open
        // with a verb the manuals use, so this is the case the gerund rule exists for -- without
        // it the subject reads as `idle system`.
        assert!(names.iter().all(|name| *name != "idle system"), "{names:?}");
        assert!(source.purpose_of("diesel fuel").is_none(), "{names:?}");
    }

    #[test]
    fn a_measurement_or_a_reference_back_is_not_a_name() {
        // Verbatim from *Modern Machine-Shop Practice*, the source this round measured and
        // refused. Each sentence reached a learner-facing option before these rules.
        let source = parse_purposes(
            "Softened sheet copper about 1/32 inch thick is used to make joints on surfaces that have been planed.\n\
             The same device is used to test if the cross slide of the carriage is at a right angle.\n\
             The work is designed to form a complete manual of reference for all who handle tools.\n\
             A tool which would be used to cut a thread, the tool e would cut the [V]-shaped groove i.\n\
             Steel rings are used to line the cylinder openings.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        // A measurement is not a thing: the copper's *thickness* was named.
        assert!(
            names.iter().all(|name| !name.ends_with("thick")),
            "{names:?}"
        );
        // A reference back into the paragraph is not a name.
        assert!(
            names.iter().all(|name| !name.starts_with("same")),
            "{names:?}"
        );
        // A treatise's own words are not a thing either.
        assert!(names.iter().all(|name| *name != "work"), "{names:?}");
        // A sentence that labels the parts of a drawing is describing a picture.
        assert!(
            source
                .entries()
                .iter()
                .all(|entry| !entry.purpose.contains("would cut")),
            "{:?}",
            source.entries()
        );
        // The positive control: the description that names a tool still parses.
        assert!(source.purpose_of("Steel rings").is_some(), "{names:?}");

        // A semicolon joins two clauses, so a purpose that holds one is two sentences.
        assert!(
            !reads_as_a_purpose(
                "a safety valve to relieve extra pressure within the system; the vacuum valve \
                 opens only when the pressure drops",
                Link::As
            ),
            "a purpose holds no semicolon"
        );

        // The rules themselves, asserted where they live, because a sentence can be refused
        // for more than one reason and a test that only parses sentences cannot say which rule
        // did the refusing.
        assert!(
            !names_a_tool("about inch thick"),
            "a measurement is not a name"
        );
        assert!(
            !names_a_tool("the same device"),
            "a reference back is not a name"
        );
        assert!(
            !names_a_tool("steel work"),
            "a treatise's own word is not a name"
        );
        assert!(
            carries_scan_damage("hold the work at an angle g"),
            "a single letter is a figure's label"
        );
        assert!(
            !carries_scan_damage("hold the work at an angle"),
            "a sentence with no label is not damaged"
        );
    }

    #[test]
    fn the_relation_shapes_the_sources_state_are_read() {
        // Verbatim from TM 9-8000 and *Farm Mechanics*. `is employed to` and `is utilized to`
        // are how the older manuals state the same relation as `is used to`.
        let source = parse_purposes(
            "As the cutting edges are diagonally offset approximately degrees, diagonal pliers are adapted to cutting small objects flush with a surface.\n\
             A tractor is arranged to pull its load in two different ways, first by the draw bar.\n\
             Extension bits are used for boring holes larger than one inch.\n\
             Steel rings are used to line the cylinder openings.\n\
             Reamers are used to finish the holes after drilling.\n",
            "A Test Manual",
        );
        let names: Vec<&str> = source
            .entries()
            .iter()
            .map(|entry| entry.tool.as_str())
            .collect();
        assert!(
            source.purpose_of("diagonal pliers").is_some(),
            "`are adapted to` states a purpose: {names:?}"
        );
        assert!(
            source.purpose_of("tractor").is_some(),
            "`is arranged to` states a purpose: {names:?}"
        );
    }

    /// The noun-complement frame: `the relay serves as a switch` is a description of what a
    /// component *is*, and it is asked the other way round from a purpose.
    ///
    /// The frame has to follow the source. Asking "Which tool is used to a switch?" is not a
    /// question, and offering "a thrust bearing" beside "to prevent leakage" as options tells
    /// the learner which one is the odd answer out.
    #[test]
    fn a_noun_complement_is_asked_what_the_component_serves_as() {
        let source = parse_purposes(
            "The bimetallic strip serves as one of the contact points.\n\
             A one-way valve acts as a check against return flow.\n\
             The pole shoes serve as a core for the field coils to increase permeability.\n\
             The relay is used to switch the current.\n\
             The drive spring serves as a cushion while the engine is cranked.\n",
            "A Test Manual",
        );
        let links: Vec<(&str, Link)> = source
            .entries()
            .iter()
            .map(|entry| (entry.tool.as_str(), entry.link))
            .collect();
        assert!(
            links.contains(&("bimetallic strip", Link::As)),
            "a `serves as` description carries the noun link: {links:?}"
        );
        assert!(links.contains(&("pole shoes", Link::As)), "{links:?}");

        let items = source.build_function_items("AI", 8, 11, |_| true);
        assert!(!items.is_empty(), "the fixture should yield items");
        for item in &items {
            let noun_frame = item.prompt.ends_with(" serve as?");
            for option in &item.options {
                assert_eq!(
                    option.starts_with("to "),
                    !noun_frame,
                    "every option must be written in the form the question asks in: {} / {option}",
                    item.prompt
                );
            }
        }
        // At least one item is asked in the noun frame, and every one verifies.
        assert!(
            items.iter().any(|item| item.prompt.ends_with(" serve as?")),
            "{:?}",
            items.iter().map(|item| &item.prompt).collect::<Vec<_>>()
        );
        for item in &items {
            verify(item, &source)
                .unwrap_or_else(|failure| panic!("{} failed: {failure}", item.prompt));
        }

        // A Shop Information item asks which tool *does* something, so a component offered as
        // the answer is refused.
        let tool_items = source.build_items("SI", 8, 11, |_| true);
        assert!(
            tool_items
                .iter()
                .all(|item| !item.prompt.contains(" serves as ")),
            "{:?}",
            tool_items
                .iter()
                .map(|item| &item.prompt)
                .collect::<Vec<_>>()
        );
    }

    /// An item whose options are in the wrong form is refused rather than served.
    #[test]
    fn verification_refuses_options_in_the_wrong_form() {
        let source = parse_purposes(
            "The bimetallic strip serves as one of the contact points.\n\
             A one-way valve acts as a check against return flow.\n\
             The pole shoes serve as a core for the field coils to increase permeability.\n\
             The drive spring serves as a cushion while the engine is cranked.\n\
             The throttle return dashpot serves as a damper.\n",
            "A Test Manual",
        );
        let mut item = source.build_function_items("AI", 4, 5, |_| true).remove(0);
        item.prompt = "What does the bimetallic strip serve as?".to_string();
        // The options are the infinitive form the *other* frame uses.
        for option in item.options.iter_mut() {
            *option = format!("to {option}");
        }
        match verify(&item, &source) {
            Err(PurposeVerificationFailure::OptionFormMismatch { .. }) => {}
            other => panic!("expected an option-form refusal, got {other:?}"),
        }
    }

    #[test]
    fn damage_the_scan_left_inside_a_sentence_is_refused() {
        // Every sentence here is verbatim from TM 9-8000 or TM 9-2700, and every one of them
        // reached a learner-facing purpose before this rule.
        assert!(carries_scan_damage(
            "prevent leakage be- tween rotating and nonrotating members"
        ));
        assert!(carries_scan_damage(
            "absorb heat from the airstream directed into the EVAPORATOR CORE CAPILLARY TUBE"
        ));
        assert!(carries_scan_damage(
            "provide a white light of to candlepower at a distance of feet"
        ));
        assert!(carries_scan_damage(
            "allow their tanks or cells to be filled at a rate of at least gallons liters) per minute"
        ));
        assert!(carries_scan_damage(
            "provide approximately a to air-fuel ratio under normal steady speed conditions"
        ));
        assert!(carries_scan_damage(
            "it has percent of the strength of the rope"
        ));
        assert!(carries_scan_damage(
            "operate in high gear at speeds over mph km/h)"
        ));
        // The negative controls: sentences with a quantity, a single acronym, a balanced
        // bracket, and a hyphen that is part of a word.
        assert!(!carries_scan_damage(
            "measure distances to the nearest one thousandth of an inch"
        ));
        assert!(!carries_scan_damage(
            "operate the EGR valve through a vacuum amplifier"
        ));
        assert!(!carries_scan_damage(
            "perform four events: intake, compression, power, and exhaust"
        ));
        assert!(!carries_scan_damage("drain 3 gallons of oil from the sump"));
        assert!(!carries_scan_damage(
            "extract the kinetic energy (energy due to motion) from the gases"
        ));
        assert!(!carries_scan_damage(
            "hold work on a T-bevel and an air-over-hydraulic press"
        ));

        // The rule is wired into the reader rather than merely available to it: the
        // description below has a clean name and a damaged purpose, and it must not become an
        // entry. Without this the test would pass on a reader that never asked the question.
        let source = parse_purposes(
            "SEALS\n\
             Oil seals are used to prevent leakage be- tween rotating and nonrotating members.\n\
             Piston rings are used to seal the combustion chamber against gas leakage.\n\
             Valve springs are used to hold the valves closed.\n\
             Fuel pumps are used to deliver fuel to the carburetor.\n\
             Oil pumps are used to circulate oil through the engine.\n",
            "A Test Manual",
        );
        assert!(
            source.purpose_of("Oil seals").is_none(),
            "{:?}",
            source.entries()
        );
        assert!(
            source.purpose_of("Valve springs").is_some(),
            "the clean descriptions still parse: {:?}",
            source.entries()
        );
    }

    #[test]
    fn a_gerund_description_is_not_asked_as_a_function() {
        // Verbatim from TM 9-8000, where the functions are stated both ways: `open hooks are
        // used for joining the towing vehicle and the trailer` and `the tie rod ends are used
        // to form a flexible link`. Framing the first with `to ` produced the corpus's `to
        // joining the towing vehicle and the trailer with safety chains`.
        let source = parse_purposes(
            "The open hooks are used for joining the towing vehicle and the trailer.\n\
             The tie rod ends are used to form a flexible link between the tie rod and the steering arm.\n\
             The parking brake is used to keep the vehicle stationary.\n\
             The fuel pump is used to deliver fuel from the tank to the carburetor.\n\
             The oil pump is used to circulate oil through the engine.\n",
            "A Test Manual",
        );
        let items = source.build_function_items("AI", 8, 5, |_| true);
        assert!(!items.is_empty(), "the fixture should yield items");
        for item in &items {
            for option in &item.options {
                assert!(
                    !option.starts_with("to joining")
                        && !option.contains("to joining")
                        && !option.contains("for joining"),
                    "a gerund was framed as an infinitive: {option}"
                );
            }
            assert!(
                !item.prompt.contains("open hooks"),
                "the gerund description was asked about: {}",
                item.prompt
            );
        }
        // The positive control: the descriptions the manual wrote as infinitives still build,
        // and the Shop Information side still asks about the gerund one, which is the form
        // its own question can carry.
        assert!(
            items
                .iter()
                .any(|item| item.prompt.contains("tie rod ends")),
            "{:?}",
            items.iter().map(|item| &item.prompt).collect::<Vec<_>>()
        );
        let tool_items = source.build_items("SI", 8, 5, |_| true);
        assert!(
            tool_items
                .iter()
                .any(|item| item.prompt.contains("used for joining")),
            "{:?}",
            tool_items
                .iter()
                .map(|item| &item.prompt)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_verb_agrees_with_the_component() {
        // `What is the open hooks used for?` reads as a program talking; the source's own
        // sentence says `hooks are used for`.
        let plural = parse_purposes(
            "The open hooks are used to join the trailer to the towing vehicle.\n\
             The slip joint is used to take up end play in the shaft.\n\
             The ammeter is used to indicate the current flowing.\n\
             The brake shoes are used to stop the turning wheel.\n",
            "x",
        );
        for item in plural.build_function_items("AI", 6, 3, |_| true) {
            let component = component_in_prompt(&item.prompt).expect("a component");
            let ends_with_s = component
                .split_whitespace()
                .last()
                .is_some_and(|word| word.to_lowercase().ends_with('s'));
            if ends_with_s && !component.to_lowercase().ends_with("ss") {
                assert!(
                    item.prompt.starts_with("What are the "),
                    "{} should ask `What are the`",
                    item.prompt
                );
            }
        }
    }

    #[test]
    fn every_option_is_a_function_the_manual_states_for_another_component() {
        let source = manual();
        let items = source.build_function_items("AI", 6, 11, |_| true);
        assert!(!items.is_empty());
        for item in &items {
            let component = component_in_prompt(&item.prompt).expect("a component");
            for (index, option) in item.options.iter().enumerate() {
                let function = function_in_option(option);
                if index == item.correct_index {
                    assert!(
                        source.says(&component, &function),
                        "the right answer must be this component's function"
                    );
                    continue;
                }
                // A distractor has to be another component's stated function, not filler.
                assert!(
                    source
                        .entries()
                        .iter()
                        .any(|entry| normalize(&entry.purpose) == normalize(&function)
                            && normalize(&entry.tool) != normalize(&component)),
                    "distractor {option:?} is not another component's function"
                );
                assert!(item.distractor_rationales.contains_key(&index));
            }
        }
    }

    #[test]
    fn every_built_function_item_verifies() {
        let source = manual();
        for item in source.build_function_items("AI", 8, 13, |_| true) {
            verify(&item, &source)
                .unwrap_or_else(|failure| panic!("{} failed: {failure}", item.prompt));
        }
    }

    #[test]
    fn verification_refuses_a_function_the_manual_does_not_state() {
        let source = manual();
        let mut item = source.build_function_items("AI", 1, 5, |_| true).remove(0);
        item.options[item.correct_index] = "to do something nobody described".to_string();
        match verify(&item, &source) {
            Err(PurposeVerificationFailure::NotFromSource(_)) => {}
            other => panic!("expected a source refusal, got {other:?}"),
        }
    }

    #[test]
    fn verification_refuses_a_prompt_that_does_not_name_a_component() {
        let source = manual();
        let mut item = source.build_function_items("AI", 1, 17, |_| true).remove(0);
        item.prompt = "Which tool is used to regulate the engine?".to_string();
        match verify(&item, &source) {
            Err(PurposeVerificationFailure::NotAPurposeQuestion(_)) => {}
            other => panic!("expected a question refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_relative_clause_is_not_a_description_of_its_subject() {
        // `The yardstick that is used to measure the ignition quality ... is the
        // cetane-number scale` describes the cetane-number scale, and the thing being used is
        // the yardstick. Reading the verb where it stands named the wrong object.
        let source = parse_purposes(
            "The yardstick that is used to measure the ignition quality is the cetane scale.\n\
             The ammeter is used to indicate the current flowing to the battery.\n\
             The oil pump is used to circulate oil through the engine.\n\
             The radiator is used to cool the coolant heated by the engine.\n",
            "x",
        );
        assert!(
            source.purpose_of("yardstick").is_none(),
            "{:?}",
            source.entries()
        );
    }
}
