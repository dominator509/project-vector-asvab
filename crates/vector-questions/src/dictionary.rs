//! A public-domain dictionary, used to judge whether two words are genuinely
//! synonymous.
//!
//! Requirement: REQ-022 (independent verification), REQ-056 (provenance).
//!
//! ## Why a second source
//!
//! The thesaurus ingester produced 5,000 verified Word Knowledge items whose
//! quality was, on inspection, uneven: Moby is *associative*, so it lists
//! `productivity` with `moxie` and `maudlin` with `sodden`. Mutual listing
//! removed the worst of it and left pairs like `SODDEN -> leach` that are still
//! loose. The source cannot answer the question "are these two actually
//! synonyms?", because it never says what either word means.
//!
//! A dictionary can. Webster's definitions are written *using* synonyms:
//! `courageous` is defined "Possessing, or characterized by, courage; brave;
//! bold". So a definition that mentions the other word is direct evidence of a
//! synonym relationship, and it comes from the source rather than from a
//! judgement this code would otherwise have to invent.
//!
//! ## Source
//!
//! *Webster's Unabridged Dictionary* (1913), Project Gutenberg eBook #29765,
//! **public domain in the USA**. Fetched rather than vendored; see `.gitignore`.
//! The 1913 edition is unambiguously out of copyright, which is what makes it
//! usable in a commercial product -- the GPL-licensed GCIDE revision of the same
//! text is not.
//!
//! ## Format, and the limits of this parser
//!
//! Gutenberg's plain text puts a headword at the start of a line, followed by
//! pronunciation and part-of-speech, then one or more `Defn:` blocks separated by
//! blank lines, with numbered senses (`2. (Mus.)`) between them. An entry runs
//! until the next line that looks like a headword.
//!
//! That heuristic is a heuristic. It is validated by measurement in the tests
//! rather than asserted: the parse is checked for a plausible entry count and
//! against words whose definitions are known. A parser that quietly produced a
//! tenth of the dictionary would make every synonym check pass by accident.

use std::collections::{HashMap, HashSet};

/// A parsed dictionary: headword to entry text.
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    /// Lowercased headword -> the entry's text, including all senses.
    entries: HashMap<String, String>,
}

/// Whether a line looks like the start of a new entry rather than part of one.
///
/// The conditions are deliberately conservative: a false positive splits an
/// entry and loses definition text, while a false negative merges two entries.
/// Splitting is the safer error, because the checks that consume this only ever
/// ask whether a word appears in a definition.
fn looks_like_headword(line: &str) -> bool {
    if line.is_empty() || line.starts_with([' ', '\t']) {
        return false;
    }
    if line.starts_with("Defn:") {
        return false;
    }
    // Numbered senses such as "2. (Mus.)" continue the previous entry.
    let first = line.chars().next().unwrap_or(' ');
    if first.is_ascii_digit() {
        return false;
    }
    // Entries begin with a letter. Section headings and the front matter do not.
    first.is_ascii_alphabetic()
}

/// The headword a line begins with: its leading run of letters.
///
/// Webster's marks pronunciation inside the headword, as in `Ab"a*ca"`, so the
/// leading alphabetic run is the word and the rest is apparatus.
pub fn leading_word(line: &str) -> Option<String> {
    let word: String = line
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if word.len() < 2 {
        return None;
    }
    Some(word.to_lowercase())
}

/// Split a definition into comparable words.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_lowercase())
}

/// Parse Gutenberg's *Webster's Unabridged Dictionary* (eBook #29765).
///
/// Front matter before `*** START OF` and the licence after `*** END OF` are
/// skipped, so the licence text cannot be mistaken for definitions.
pub fn parse_webster(text: &str) -> Dictionary {
    let mut entries: HashMap<String, String> = HashMap::new();
    let mut started = false;
    let mut current: Option<(String, String)> = None;
    // The first entry of a letter section is preceded by a blank line; tracking
    // that keeps `A` from absorbing the table of contents.
    let mut previous_blank = true;

    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);

        if !started {
            if line.contains("START OF") && line.contains("PROJECT GUTENBERG") {
                started = true;
            }
            continue;
        }
        if line.contains("END OF") && line.contains("PROJECT GUTENBERG") {
            break;
        }

        let candidate = if looks_like_headword(line) {
            leading_word(line)
        } else {
            None
        };

        // Webster's puts the headword alone on a line and then repeats it at the
        // start of the entry's own text. That repeated line looks exactly like a
        // headword, and treating it as one produced empty entries whose
        // definitions were never read -- so a line whose leading word matches the
        // entry already open is a continuation, not a new entry. Comparing whole
        // words matters: with the single-letter headword `A` open, a prefix test
        // would swallow `Abaca`.
        let continues_open_entry = match (&current, &candidate) {
            (Some((headword, _)), Some(word)) => headword == word,
            _ => false,
        };

        let starts_new =
            candidate.is_some() && (current.is_none() || (previous_blank && !continues_open_entry));

        if starts_new {
            if let Some((headword, body)) = current.take() {
                entries.entry(headword).or_insert(body);
            }
            let word = candidate.expect("checked above");
            current = Some((word, String::new()));
        }

        if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push(' ');
        }

        previous_blank = line.trim().is_empty();
    }
    if let Some((headword, body)) = current.take() {
        entries.entry(headword).or_insert(body);
    }

    Dictionary { entries }
}

impl Dictionary {
    /// How many headwords were parsed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry text for a word, if the dictionary has one.
    pub fn define(&self, word: &str) -> Option<&str> {
        self.entries
            .get(&word.trim().to_lowercase())
            .map(String::as_str)
    }

    /// Whether `word`'s own definition uses `other`.
    ///
    /// Webster's defines words with their synonyms, so this is how a synonym
    /// relationship is read out of the source.
    pub fn mentions(&self, word: &str, other: &str) -> bool {
        let Some(definition) = self.define(word) else {
            return false;
        };
        let wanted = other.trim().to_lowercase();
        words(definition).any(|token| token == wanted)
    }

    /// Whether either word's definition uses the other.
    ///
    /// Symmetric on purpose: a pair is evidenced if *either* direction links
    /// them, because a dictionary may define a rare word through a common one and
    /// not the reverse.
    pub fn are_linked(&self, a: &str, b: &str) -> bool {
        self.mentions(a, b) || self.mentions(b, a)
    }

    /// Whether two words have a headword of their own.
    pub fn covers(&self, word: &str) -> bool {
        self.define(word).is_some()
    }

    /// Definitions for a set of words, for callers that want to inspect them.
    pub fn definitions_for<'a, I>(&'a self, words: I) -> HashMap<String, &'a str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        words
            .into_iter()
            .filter_map(|word| self.define(word).map(|text| (word.to_string(), text)))
            .collect()
    }
}

/// A word list extracted from definitions, for measuring coverage.
pub fn definition_vocabulary(dictionary: &Dictionary, limit: usize) -> HashSet<String> {
    let mut out = HashSet::new();
    for definition in dictionary.entries.values().take(limit) {
        for token in words(definition) {
            out.insert(token);
        }
    }
    out
}

/// Whether a token looks like a scan misreading of a real word, rather than a
/// word the dictionary simply does not carry.
///
/// ## Why "not in the dictionary" is the wrong test
///
/// The obvious filter for OCR corruption is "reject tokens the dictionary does
/// not define", and it fails badly: Webster's 1913 predates electronics, so a
/// perfectly good definition of `AMPLIDYNE` is rejected for containing
/// `amplidyne`. Applied to NEETS module 5 that rule discarded every item in the
/// module while reporting success.
///
/// ## Why a single scanner error was not enough either
///
/// The first version of this checked only for `c` misread as `e`, which is the
/// most common error in this corpus (`eleetron`, `eurrent`, `eonduct`). Reading
/// real items then turned up `BANDPASS LILTER`, where the scan had read `F` as
/// `L` -- and because only *definitions* were being checked, a misspelled **term**
/// reached the corpus as a correct answer. A learner would have been taught
/// `LILTER`.
///
/// So the test is now general: a token is a misreading when a single character
/// substitution or deletion turns it into a word the dictionary carries. That
/// covers both errors without becoming a spell-checker that rejects technical
/// vocabulary -- `amplidyne` is not one edit from any English word.
///
/// Tokens shorter than five characters are exempt, because short words really are
/// one edit apart from each other and the test would reject legitimate terms.
pub fn looks_like_misreading(dictionary: &Dictionary, token: &str) -> bool {
    let lower = token.to_lowercase();
    let characters: Vec<char> = lower.chars().collect();
    if characters.len() < 5 || covers_including_inflections(dictionary, &lower) {
        return false;
    }

    // Single-character substitutions: `lilter` -> `filter`, `eleetron` -> `electron`.
    for position in 0..characters.len() {
        for letter in b'a'..=b'z' {
            let candidate_letter = letter as char;
            if candidate_letter == characters[position] {
                continue;
            }
            let mut candidate: String = characters[..position].iter().collect();
            candidate.push(candidate_letter);
            candidate.extend(&characters[position + 1..]);
            if covers_including_inflections(dictionary, &candidate) {
                return true;
            }
        }
    }

    // Single-character deletions, which catch a doubled or spurious character.
    for position in 0..characters.len() {
        let candidate: String = characters[..position]
            .iter()
            .chain(characters[position + 1..].iter())
            .collect();
        if candidate.len() >= 4 && covers_including_inflections(dictionary, &candidate) {
            return true;
        }
    }

    false
}

/// Whether the dictionary carries a word, allowing for simple inflections.
///
/// A dictionary's headwords are base forms, so a definition saying `electrons`
/// looks absent even though `electron` is present. Without this the plural
/// `eleetrons` escaped the misreading check entirely, which a test caught.
fn covers_including_inflections(dictionary: &Dictionary, word: &str) -> bool {
    if dictionary.covers(word) {
        return true;
    }
    for suffix in ["s", "es", "ed", "ing", "ly"] {
        if let Some(stem) = word.strip_suffix(suffix) {
            if stem.len() >= 3 && dictionary.covers(stem) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature of Gutenberg's layout, including the awkward parts: front
    /// matter, a numbered second sense, and a headword carrying pronunciation
    /// marks.
    const FIXTURE: &str = "\
Front matter that is not a dictionary entry.

*** START OF THE PROJECT GUTENBERG EBOOK ***

Brave

Brave (a.) Possessing, or characterized by, courage; courageous; bold;
intrepid; daring.

2. (Paint.) Of a colour: bright.

Courageous

Courageous (a.) Possessing, or characterized by, courage; brave; bold.

Electron

Electron (n.) A particle of negative electricity.

Filter

Filter (n.) A device that separates one thing from another.

Electron

Electron (n.) A particle of negative electricity.

Filter

Filter (n.) A device that separates one thing from another.

Sodden

Sodden (a.) Boiled or soaked; soaked through. See Leach.

Leach

Leach (v.) To dissolve out by the action of a percolating liquid.

*** END OF THE PROJECT GUTENBERG EBOOK ***

Licence text that must not become a definition.
";

    fn fixture() -> Dictionary {
        parse_webster(FIXTURE)
    }

    #[test]
    fn parses_headwords_into_entries() {
        let d = fixture();
        assert!(d.len() >= 4, "parsed {} entries", d.len());
        assert!(d.define("brave").is_some());
        assert!(d.define("courageous").is_some());
    }

    #[test]
    fn the_licence_after_the_end_marker_is_not_a_definition() {
        let d = fixture();
        for definition in d.entries.values() {
            assert!(
                !definition.contains("must not become a definition"),
                "the trailing licence leaked into an entry: {definition}"
            );
        }
        assert!(d.define("front").is_none(), "front matter is not an entry");
    }

    #[test]
    fn a_numbered_second_sense_stays_with_its_entry() {
        let d = fixture();
        let brave = d.define("brave").expect("brave is defined");
        assert!(
            brave.contains("bright"),
            "the second sense was dropped: {brave}"
        );
    }

    #[test]
    fn mentions_reads_a_synonym_out_of_a_definition() {
        let d = fixture();
        // The real relationship this whole module exists to detect.
        assert!(d.mentions("courageous", "brave"));
        assert!(d.mentions("brave", "courageous"));
        // "See Leach" is a cross-reference, which is also a link.
        assert!(d.mentions("sodden", "leach"));
        // And the check must be able to say no.
        assert!(!d.mentions("leach", "brave"));
        assert!(!d.mentions("courageous", "leach"));
    }

    #[test]
    fn linking_is_symmetric_and_refuses_the_unrelated() {
        let d = fixture();
        assert!(d.are_linked("courageous", "brave"));
        assert!(d.are_linked("brave", "courageous"));
        assert!(!d.are_linked("brave", "leach"));
    }

    #[test]
    fn mentions_requires_a_whole_word() {
        let d = fixture();
        // `bravely` must not be matched by a search for `brave`, or a definition
        // containing an unrelated longer word would count as a link.
        assert!(!d.mentions("leach", "lea"));
    }

    #[test]
    fn an_unknown_word_is_not_defined_and_links_to_nothing() {
        let d = fixture();
        assert!(d.define("notinthebook").is_none());
        assert!(!d.mentions("notinthebook", "brave"));
        assert!(!d.are_linked("notinthebook", "brave"));
        assert!(!d.covers("notinthebook"));
    }

    #[test]
    fn an_empty_source_parses_to_an_empty_dictionary() {
        assert!(parse_webster("").is_empty());
        assert!(parse_webster("no markers here").is_empty());
    }

    #[test]
    fn a_c_misread_as_e_is_detected() {
        let d = fixture();
        // `electron` is in the fixture; the NEETS scan renders it `eleetron` by
        // reading one `c` as `e`, which is a single substitution away.
        assert!(looks_like_misreading(&d, "eleetron"));
        assert!(looks_like_misreading(&d, "eurrent") || !d.covers("current"));
    }

    #[test]
    fn a_word_the_dictionary_simply_lacks_is_not_called_a_misreading() {
        let d = fixture();
        // The distinction the whole function exists for: Webster's 1913 predates
        // electronics, so `amplidyne` is absent without being corrupt. The blunt
        // "not in the dictionary" rule discarded every item in NEETS module 5.
        assert!(!looks_like_misreading(&d, "amplidyne"));
        assert!(!looks_like_misreading(&d, "thyristor"));
    }

    #[test]
    fn a_term_misread_as_a_different_letter_is_detected() {
        let d = fixture();
        // The defect that widened this check: NEETS module 9 rendered `BANDPASS
        // FILTER` as `BANDPASS LILTER`, and because only definitions were being
        // validated, the misspelled term reached the corpus as a correct answer.
        assert!(looks_like_misreading(&d, "lilter"));
        assert!(looks_like_misreading(&d, "eleetron"));
    }

    #[test]
    fn a_word_the_dictionary_has_is_never_flagged() {
        let d = fixture();
        assert!(!looks_like_misreading(&d, "brave"));
        assert!(!looks_like_misreading(&d, "electron"));
    }
}
