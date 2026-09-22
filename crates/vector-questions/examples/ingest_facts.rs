//! Ingest General Science items from public-domain question-and-answer texts.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_facts -- <label>=<path> [...] [count]
//!
//! Every item is verified against its own source before it is counted, so the
//! numbers printed are of *provable* items rather than attempted ones. The answers
//! a text gives are its own; this prints what they look like so the register can be
//! judged rather than assumed.

use std::path::PathBuf;

use vector_questions::facts::{
    parse_faq, verify, FactItem, Faq, MAX_OPTION_WORDS, MIN_OPTION_WORDS,
};

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn main() {
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    let mut count = 500usize;
    for argument in std::env::args().skip(1) {
        match argument.split_once('=') {
            Some((label, path)) => sources.push((label.to_string(), PathBuf::from(path))),
            None => count = argument.parse().unwrap_or(500),
        }
    }
    if sources.is_empty() {
        eprintln!("usage: ingest_facts <label>=<path> [...] [count]");
        std::process::exit(2);
    }

    let mut samples: Vec<FactItem> = Vec::new();
    let mut total_questions = 0usize;
    let mut total_built = 0usize;
    let mut total_verified = 0usize;
    let mut total_failed = 0usize;

    println!();
    for (label, path) in &sources {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let faq: Faq = parse_faq(&text, label);
        let items = faq.build_items(
            "GS",
            count,
            20_260_922,
            MIN_OPTION_WORDS,
            MAX_OPTION_WORDS,
            |_| true,
        );

        let mut verified = 0usize;
        let mut failed = 0usize;
        for item in &items {
            match verify(item, &faq) {
                Ok(()) => verified += 1,
                Err(failure) => {
                    failed += 1;
                    if failed <= 3 {
                        eprintln!("  REJECTED: {failure}");
                    }
                }
            }
        }
        total_questions += faq.entry_count();
        total_built += items.len();
        total_verified += verified;
        total_failed += failed;

        println!(
            "{label:<26} sha256={}… questions={:<5} built={:<5} verified={:<5} failed={}",
            &digest(&text)[..12],
            faq.entry_count(),
            items.len(),
            verified,
            failed
        );
        if samples.len() < 3 {
            samples.extend(items.into_iter().take(3 - samples.len()));
        }
    }

    println!();
    println!("questions parsed : {total_questions}");
    println!("items built      : {total_built}");
    println!("items verified   : {total_verified}");
    println!("items failed     : {total_failed}");

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
        for (index, rationale) in &item.distractor_rationales {
            println!("  why {index} is wrong: {rationale}");
        }
    }

    if total_failed > 0 {
        eprintln!("\ncorpus is not clean: {total_failed} item(s) failed verification");
        std::process::exit(1);
    }
    println!("\nall {total_verified} item(s) verified against their sources");
}
