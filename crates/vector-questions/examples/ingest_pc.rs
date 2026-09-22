//! Ingest Paragraph Comprehension items from public-domain prose.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_pc -- <label>=<path> [<label>=<path> ...] [count]
//!
//! Each text is a Project Gutenberg work, public domain in the USA. The Gutenberg
//! header and footer are stripped so their boilerplate cannot become a passage.
//!
//! Every item is verified against its own passage before it is counted, so the
//! numbers printed are of *provable* items rather than attempted ones.

use std::path::PathBuf;

use vector_questions::passages::{parse_gutenberg, verify, Text};

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    let mut count = 500usize;
    for argument in args.by_ref() {
        match argument.split_once('=') {
            Some((label, path)) => sources.push((label.to_string(), PathBuf::from(path))),
            None => {
                count = argument.parse().unwrap_or(500);
            }
        }
    }
    if sources.is_empty() {
        eprintln!(
            "usage: ingest_pc <label>=<path> [...] [count]\n\
             fetch a text with:\n  \
             curl -L -o sources/gutenberg/faraday-candle.txt \\\n    \
             https://www.gutenberg.org/cache/epub/14474/pg14474.txt"
        );
        std::process::exit(2);
    }

    let mut total_paragraphs = 0usize;
    let mut total_items = 0usize;
    let mut total_verified = 0usize;
    let mut total_failed = 0usize;
    let mut samples: Vec<_> = Vec::new();

    println!();
    for (label, path) in &sources {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let parsed: Text = parse_gutenberg(&text, label);
        if parsed.is_empty() {
            eprintln!("{label}: no paragraphs parsed; the markers may have changed");
            continue;
        }
        total_paragraphs += parsed.paragraph_count();

        // The parser already drops chapter headings and contents lists. This is the
        // caller's own bar, kept separate so the module does not have to know what
        // the application considers acceptable prose.
        let items = parsed.build_items(count, 20_260_922, |passage| {
            !passage.contains("Project Gutenberg")
                && !passage.contains("Gutenberg")
                && passage.split_whitespace().count() >= 40
        });

        let mut verified = 0usize;
        let mut failed = 0usize;
        for item in &items {
            match verify(item) {
                Ok(()) => verified += 1,
                Err(failure) => {
                    failed += 1;
                    if failed <= 3 {
                        eprintln!("  REJECTED: {failure}");
                    }
                }
            }
        }
        total_items += items.len();
        total_verified += verified;
        total_failed += failed;

        println!(
            "{label:<22} sha256={}… paragraphs={:<5} built={:<5} verified={:<5} failed={}",
            &digest(&text)[..12],
            parsed.paragraph_count(),
            items.len(),
            verified,
            failed
        );
        if samples.len() < 2 {
            samples.extend(items.into_iter().take(2 - samples.len()));
        }
    }

    println!();
    println!("paragraphs parsed : {total_paragraphs}");
    println!("items built       : {total_items}");
    println!("items verified    : {total_verified}");
    println!("items failed      : {total_failed}");

    println!("\n--- sample items ---");
    for item in &samples {
        println!("\nPASSAGE: {}", item.passage);
        println!("{}", item.prompt);
        for (index, option) in item.options.iter().enumerate() {
            let marker = if index == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            println!("  {}. {option}{marker}", (b'A' + index as u8) as char);
        }
        let rationale = item
            .distractor_rationales
            .values()
            .next()
            .cloned()
            .unwrap_or_default();
        println!("  why a distractor is wrong: {rationale}");
    }

    if total_failed > 0 {
        eprintln!("\ncorpus is not clean: {total_failed} item(s) failed verification");
        std::process::exit(1);
    }
    println!("\nall {total_verified} item(s) verified against their passages");
}
