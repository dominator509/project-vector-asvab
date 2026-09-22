//! Ingest Electronics Information items from NEETS module glossaries.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_ei -- <dictionary> <module> [module...]
//!
//! Sources, both public domain:
//!   NEETS (Navy Electricity and Electronics Training Series), NETPDTC — the
//!     module text files, fetched from archive.org rather than committed.
//!   Webster's Unabridged Dictionary, 1913 — Gutenberg #29765, used here as a
//!     spell-checker rather than as a definition source.
//!
//! ## The OCR check, and why it is not optional
//!
//! The module text is OCR of a scanned page, and the scan's font made the reader
//! confuse `c` with `e`. The corpus contains `eleetron`, `eonduct`, `earry`,
//! `eleetrie`, `eurrent`, `reeiproeal` and `resistanee`. An item whose definition
//! reads "the amount of eleetron flow" teaches a learner that `eleetron` is a
//! word, which is worse than having no item at all.
//!
//! So a definition is used only if every alphabetic token of four or more
//! characters is either defined by Webster's or is a term the module's own
//! glossary defines. This is the second use of that dictionary and the reason its
//! 29 MB is worth carrying. Entries that fail are **skipped, not repaired**: a
//! misspelling is not something this program can correct, and guessing at
//! `eleetron` would be inventing source text.

use std::path::PathBuf;

use vector_questions::dictionary::{looks_like_misreading, parse_webster, Dictionary};
use vector_questions::neets::{parse_neets_glossary, verify, Glossary};

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Whether a definition is free of OCR corruption.
///
/// Every alphabetic token of four or more characters must be either a word
/// Webster's carries or a term the module's own glossary defines. The second half
/// matters: Webster's 1913 predates electronics, so a definition may legitimately
/// use a term it does not carry, and the check has to target misspellings rather
/// than technical language.
fn entry_is_legible(
    term: &str,
    definition: &str,
    dictionary: &Dictionary,
    glossary: &Glossary,
) -> bool {
    let check = |text: &str| {
        text.split(|c: char| !c.is_ascii_alphabetic())
            .filter(|token| token.len() >= 4)
            // A term the module defines itself is technical vocabulary this scan
            // rendered correctly; `looks_like_misread_c` targets the scan's actual
            // error rather than unknown words.
            .all(|token| glossary.defines(token) || !looks_like_misreading(dictionary, token))
    };
    check(term) && check(definition)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dictionary_path) = args.next().map(PathBuf::from) else {
        eprintln!(
            "usage: ingest_ei <webster.txt> <module.txt> [module.txt ...]\n\
             fetch the dictionary with:\n  \
             curl -L -o sources/webster-1913/pg29765.txt \\\n    \
             https://www.gutenberg.org/ebooks/29765.txt.utf-8"
        );
        std::process::exit(2);
    };
    let modules: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if modules.is_empty() {
        eprintln!("no NEETS modules given");
        std::process::exit(2);
    }

    let dictionary_text = std::fs::read_to_string(&dictionary_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dictionary_path.display()));
    let started = std::time::Instant::now();
    let dictionary = parse_webster(&dictionary_text);
    println!(
        "dictionary  : {} ({} entries, sha256 {}…, parsed in {} ms)",
        dictionary_path.display(),
        dictionary.len(),
        &digest(&dictionary_text)[..16],
        started.elapsed().as_millis()
    );

    let mut total_entries = 0usize;
    let mut total_built = 0usize;
    let mut total_verified = 0usize;
    let mut total_failed = 0usize;
    // Definitions the OCR check refuses, kept so the cost of that check is a
    // measured number rather than a claim.
    let mut refused_entries = 0usize;
    let mut samples: Vec<_> = Vec::new();

    println!();
    for path in &modules {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let module = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("NEETS")
            .to_string();
        let glossary = parse_neets_glossary(&text, &module);
        if glossary.is_empty() {
            println!("{module:<44} no glossary found");
            continue;
        }
        total_entries += glossary.entry_count();

        let mut refused = 0usize;
        let items = glossary.build_items(400, 20_260_922, 5, |term, definition| {
            let ok = entry_is_legible(term, definition, &dictionary, &glossary);
            if !ok {
                refused += 1;
            }
            ok
        });
        refused_entries += refused;
        total_built += items.len();

        let mut verified = 0usize;
        let mut failed = 0usize;
        for item in &items {
            match verify(item, &glossary) {
                Ok(()) => verified += 1,
                Err(failure) => {
                    failed += 1;
                    if failed <= 2 {
                        eprintln!("  REJECTED {}: {failure}", item.term);
                    }
                }
            }
        }
        total_verified += verified;
        total_failed += failed;

        println!(
            "{module:<44} entries={:<5} built={:<5} verified={:<5} refused_by_ocr={}",
            glossary.entry_count(),
            items.len(),
            verified,
            refused
        );
        if samples.len() < 3 {
            samples.extend(items.into_iter().take(3 - samples.len()));
        }
    }

    println!();
    println!("glossary entries : {total_entries}");
    println!("items built      : {total_built}");
    println!("items verified   : {total_verified}");
    println!("items failed     : {total_failed}");
    println!("definitions the OCR check refused: {refused_entries}");

    println!("\n--- sample items ---");
    for item in &samples {
        println!("\n{}", item.prompt);
        for (index, option) in item.options.iter().enumerate() {
            let marker = if index == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            println!("  {}. {option}{marker}", (b'A' + index as u8) as char);
        }
        println!("  evidence: {} — {}", item.term, item.supporting_definition);
    }

    if total_failed > 0 {
        eprintln!("\ncorpus is not clean: {total_failed} item(s) failed verification");
        std::process::exit(1);
    }
    println!("\nall {total_verified} item(s) verified against the source");
}
