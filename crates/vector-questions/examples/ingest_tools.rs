//! Ingest Shop Information items from public-domain tool manuals.
//!
//! Usage:
//!     cargo run -p vector-questions --example ingest_tools -- <label>=<path> [...] [count]
//!
//! Every item is verified against its own source before it is counted, so the numbers
//! printed are of *provable* items rather than attempted ones.

use std::path::PathBuf;

use vector_questions::purposes::{parse_purposes, verify, ItemKind, PurposeItem, Purposes};

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn main() {
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    let mut count = 200usize;
    let mut kind = ItemKind::Tool;
    let mut list_names = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "functions" => kind = ItemKind::Function,
            "tools" => kind = ItemKind::Tool,
            // The manuals' own vocabulary decides what counts as a tool, so the survey
            // needs every name the miner accepted rather than four samples of it.
            "names" => list_names = true,
            other => match other.split_once('=') {
                Some((label, path)) => sources.push((label.to_string(), PathBuf::from(path))),
                None => count = other.parse().unwrap_or(200),
            },
        }
    }
    let subtest = match kind {
        ItemKind::Tool => "SI",
        ItemKind::Function => "AI",
    };
    if sources.is_empty() {
        eprintln!("usage: ingest_tools <label>=<path> [...] [count]");
        std::process::exit(2);
    }

    let mut samples: Vec<PurposeItem> = Vec::new();
    let mut names: Vec<(String, String, String)> = Vec::new();
    let mut total_statements = 0usize;
    let mut total_built = 0usize;
    let mut total_verified = 0usize;
    let mut total_failed = 0usize;

    println!();
    for (label, path) in &sources {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let source: Purposes = parse_purposes(&text, label);
        let items = match kind {
            ItemKind::Tool => source.build_items(subtest, count, 20_260_922, |_| true),
            ItemKind::Function => source.build_function_items(subtest, count, 20_260_922, |_| true),
        };

        let mut verified = 0usize;
        let mut failed = 0usize;
        for item in &items {
            match verify(item, &source) {
                Ok(()) => verified += 1,
                Err(failure) => {
                    failed += 1;
                    if failed <= 3 {
                        eprintln!("  REJECTED: {failure}");
                        eprintln!("    prompt : {}", item.prompt);
                        eprintln!("    correct: {}", item.options[item.correct_index]);
                        eprintln!("    source : {}", item.supporting_sentence);
                    }
                }
            }
        }
        total_statements += source.entry_count();
        total_built += items.len();
        total_verified += verified;
        total_failed += failed;

        println!(
            "{label:<22} sha256={}… statements={:<4} built={:<4} verified={:<4} failed={}",
            &digest(&text)[..12],
            source.entry_count(),
            items.len(),
            verified,
            failed
        );
        if samples.len() < 4 {
            samples.extend(items.into_iter().take(4 - samples.len()));
        }
        if list_names {
            for entry in source.entries() {
                names.push((label.clone(), entry.tool.clone(), entry.purpose.clone()));
            }
        }
    }

    println!();
    println!("descriptions parsed : {total_statements}");
    println!("items built         : {total_built}");
    println!("items verified      : {total_verified}");
    println!("items failed        : {total_failed}");

    if list_names {
        println!("\n--- every accepted description ---");
        for (label, tool, purpose) in &names {
            println!("{label:<22} {tool:<34} -> {purpose}");
        }
    }

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
        println!("  source: {}", item.supporting_sentence);
    }

    if total_failed > 0 {
        eprintln!("\ncorpus is not clean: {total_failed} item(s) failed verification");
        std::process::exit(1);
    }
    println!("\nall {total_verified} item(s) verified against their sources");
}
