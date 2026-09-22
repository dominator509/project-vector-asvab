//! Ingest Word Knowledge items using both public-domain sources, and measure
//! what the dictionary filter actually buys.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_wk -- <thesaurus> <dictionary> [count]
//!
//! Both sources are fetched rather than committed (see `.gitignore`):
//!   Moby Thesaurus II, Grady Ward      — public domain, Gutenberg #3202
//!   Webster's Unabridged Dictionary    — public domain, Gutenberg #29765
//!
//! The dictionary is not decoration. It answers the question the thesaurus
//! cannot: are these two words actually synonyms? Reporting a corpus size without
//! it would repeat the mistake of the first ingestion run, which announced 5,000
//! verified items whose pairs included `SODDEN -> maudlin`.

use std::collections::HashSet;
use std::path::PathBuf;

use vector_questions::dictionary::parse_webster;
use vector_questions::thesaurus::{parse_moby, verify, Thesaurus, WkItem};

fn default_thesaurus() -> PathBuf {
    PathBuf::from("sources/moby-thesaurus/words.txt")
}

fn default_dictionary() -> PathBuf {
    PathBuf::from("sources/webster-1913/pg29765.txt")
}

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn read(path: &PathBuf, what: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {what} at {}: {error}", path.display());
            std::process::exit(2);
        }
    }
}

fn audit(items: &[WkItem], thesaurus: &Thesaurus, label: &str) -> usize {
    let mut verified = 0usize;
    let mut failures = 0usize;
    let mut hashes = HashSet::new();
    let mut duplicates = 0usize;
    for item in items {
        match verify(item, thesaurus) {
            Ok(()) => verified += 1,
            Err(failure) => {
                failures += 1;
                if failures <= 3 {
                    eprintln!("  REJECTED {}: {failure}", item.headword);
                }
            }
        }
        if !hashes.insert(item.content_hash()) {
            duplicates += 1;
        }
    }
    println!(
        "{label:<22} built={:<6} verified={:<6} failed={:<4} duplicates={:<4} distinct={}",
        items.len(),
        verified,
        failures,
        duplicates,
        hashes.len()
    );
    failures + duplicates
}

fn main() {
    let mut args = std::env::args().skip(1);
    let thesaurus_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_thesaurus);
    let dictionary_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_dictionary);
    let wanted: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_000);

    let thesaurus_text = read(&thesaurus_path, "thesaurus");
    let dictionary_text = read(&dictionary_path, "dictionary");

    println!("thesaurus   : {}", thesaurus_path.display());
    println!("  sha256    : {}", digest(&thesaurus_text));
    println!("dictionary  : {}", dictionary_path.display());
    println!("  sha256    : {}", digest(&dictionary_text));

    let started = std::time::Instant::now();
    let thesaurus = parse_moby(&thesaurus_text);
    println!(
        "\nthesaurus parsed in {} ms: {} root words, {} distinct plain words",
        started.elapsed().as_millis(),
        thesaurus.root_count(),
        thesaurus.vocabulary_size()
    );

    let started = std::time::Instant::now();
    let dictionary = parse_webster(&dictionary_text);
    println!(
        "dictionary parsed in {} ms: {} entries",
        started.elapsed().as_millis(),
        dictionary.len()
    );

    // Validate the parser against words whose definitions are known, rather than
    // trusting the entry count alone. A parser that silently produced a tenth of
    // the dictionary would make every link check pass by accident.
    println!("\n--- parser validation ---");
    for word in ["brave", "courageous", "sodden", "leach", "moxie", "lucid"] {
        match dictionary.define(word) {
            Some(definition) => {
                let preview: String = definition.chars().take(90).collect();
                println!("  {word:<12} => {preview}…");
            }
            None => println!("  {word:<12} => ABSENT"),
        }
    }

    println!("\n--- link evidence for the pairs that prompted this ---");
    for (a, b) in [
        ("courageous", "brave"),
        ("sodden", "leach"),
        ("moxie", "animation"),
        ("moxie", "productivity"),
    ] {
        println!(
            "  {a} <-> {b}: mentions({a},{b})={} mentions({b},{a})={} linked={}",
            dictionary.mentions(a, b),
            dictionary.mentions(b, a),
            dictionary.are_linked(a, b)
        );
    }

    println!("\n--- yield ---");
    let started = std::time::Instant::now();
    let unfiltered = thesaurus.build_items(wanted, 20_260_922);
    let unfiltered_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    let filtered = thesaurus.build_items_filtered(wanted, 20_260_922, |head, candidate| {
        dictionary.are_linked(head, candidate)
    });
    let filtered_ms = started.elapsed().as_millis();

    let mut dirty = audit(&unfiltered, &thesaurus, "thesaurus only");
    dirty += audit(&filtered, &thesaurus, "with dictionary");
    println!(
        "\nbuild times     : thesaurus-only {unfiltered_ms} ms, dictionary-linked {filtered_ms} ms"
    );

    // How much of the unfiltered corpus the dictionary refuses. This is the
    // measurement that justifies the second source.
    let endorsed = unfiltered
        .iter()
        .filter(|item| dictionary.are_linked(&item.headword, &item.options[item.correct_index]))
        .count();
    if !unfiltered.is_empty() {
        println!(
            "dictionary endorses {} of {} thesaurus-only pairs ({:.0}%)",
            endorsed,
            unfiltered.len(),
            100.0 * endorsed as f64 / unfiltered.len() as f64
        );
    }

    println!("\n--- sample items (dictionary-linked) ---");
    for item in filtered.iter().take(5) {
        println!("\n{}", item.prompt);
        for (index, option) in item.options.iter().enumerate() {
            let marker = if index == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            // Whether Webster's defines the word at all. A distractor the
            // dictionary has never heard of is one the source does not treat as
            // ordinary English, which is what makes an option eliminable.
            let defined = if dictionary.covers(option) {
                "defined"
            } else {
                "NOT DEFINED"
            };
            println!(
                "  {}. {option:<18} [{defined}]{marker}",
                (b'A' + index as u8) as char
            );
        }
    }

    // The measurement behind the distractor filter: how often an offered
    // distractor is a word the dictionary does not carry.
    let mut distractors = 0usize;
    let mut undefined = 0usize;
    for item in &filtered {
        for (index, option) in item.options.iter().enumerate() {
            if index == item.correct_index {
                continue;
            }
            distractors += 1;
            if !dictionary.covers(option) {
                undefined += 1;
            }
        }
    }
    if distractors > 0 {
        println!(
            "\ndistractors: {distractors}, of which {undefined} ({:.0}%) are words \
             Webster's does not define",
            100.0 * undefined as f64 / distractors as f64
        );
    }

    if dirty > 0 {
        eprintln!("\ncorpus is not clean");
        std::process::exit(1);
    }
    println!("\nall items verified against the source");
}
