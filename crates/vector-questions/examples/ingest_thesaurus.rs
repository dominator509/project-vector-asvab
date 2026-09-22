//! Ingest a public-domain thesaurus and report what it yields.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_thesaurus -- <path> [count]
//!
//! The default path is `sources/moby-thesaurus/words.txt`, which is fetched
//! rather than committed (see `.gitignore`). Moby Thesaurus II by Grady Ward is
//! public domain in the USA — Project Gutenberg eBook #3202.
//!
//! Every built item is verified against the source before it is counted, so the
//! numbers this prints are of *provable* items rather than of attempted ones.

use std::collections::HashSet;
use std::path::PathBuf;

use vector_questions::thesaurus::{parse_moby, verify, Thesaurus};

const DEFAULT_SOURCE: &str = "sources/moby-thesaurus/words.txt";

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOURCE));
    let wanted: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_000);

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            eprintln!(
                "fetch it with:\n  \
                 curl -L -o sources/moby-thesaurus/words.txt \\\n    \
                 https://raw.githubusercontent.com/words/moby/master/words.txt"
            );
            std::process::exit(2);
        }
    };

    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    let started = std::time::Instant::now();
    let thesaurus: Thesaurus = parse_moby(&text);
    let parse_ms = started.elapsed().as_millis();

    println!("source      : {}", path.display());
    println!("sha256      : {digest}");
    println!("bytes       : {}", text.len());
    println!("parsed in   : {parse_ms} ms");
    println!("root words  : {}", thesaurus.root_count());
    println!(
        "vocabulary  : {} distinct plain words",
        thesaurus.vocabulary_size()
    );
    if thesaurus.is_empty() {
        eprintln!("the source parsed to nothing; refusing to report a corpus");
        std::process::exit(1);
    }

    let started = std::time::Instant::now();
    let items = thesaurus.build_items(wanted, 20_260_922);
    let build_ms = started.elapsed().as_millis();

    let mut verified = 0usize;
    let mut failures = 0usize;
    let mut hashes = HashSet::new();
    let mut duplicates = 0usize;
    for item in &items {
        match verify(item, &thesaurus) {
            Ok(()) => verified += 1,
            Err(failure) => {
                failures += 1;
                if failures <= 5 {
                    eprintln!("  REJECTED {}: {failure}", item.headword);
                }
            }
        }
        if !hashes.insert(item.content_hash()) {
            duplicates += 1;
        }
    }

    println!();
    println!("requested   : {wanted}");
    println!("built       : {} in {build_ms} ms", items.len());
    println!("verified    : {verified}");
    println!("failed      : {failures}");
    println!("duplicates  : {duplicates}");
    println!("distinct    : {}", hashes.len());

    println!("\n--- sample items ---");
    for item in items.iter().take(3) {
        println!("\n{}", item.prompt);
        for (index, option) in item.options.iter().enumerate() {
            let marker = if index == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            println!("  {}. {option}{marker}", (b'A' + index as u8) as char);
        }
        println!("  evidence: {}", item.supporting_line);
        println!("  hash: {}", item.content_hash());
    }

    if failures > 0 || duplicates > 0 {
        eprintln!("\nthe corpus is not clean: {failures} failed, {duplicates} duplicate");
        std::process::exit(1);
    }
    println!("\nall {} item(s) verified against the source", items.len());
}
