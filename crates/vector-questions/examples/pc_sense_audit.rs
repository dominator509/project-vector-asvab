//! Measure the sense-matching and stored-bank checks on real prose.
//!
//! Run:
//!   cargo run -p vector-questions --release --example pc_sense_audit -- \
//!       dict=<webster.txt> <label>=<path> [...] [count]
//!
//! This answers the reviewer's two measurement requests on the PC redo:
//!
//! 1. **How often is a non-first sense chosen?** For every vocabulary item built, the
//!    builder keys a sense from the word's gloss list. This reports the share of items
//!    whose keyed sense is not the first (most common) sense, and prints ten sampled
//!    items as `(sentence, word, keyed sense)` so the reviewer can judge whether the
//!    context signal is real or a coincidence.
//!
//! 2. **Does the stored-bank check agree with the builder?** Every built vocabulary
//!    item is re-checked with `vocab_answer_is_backed` on its stored prompt and keyed
//!    option -- the same call the ingest makes. Before the fix this aborted the run on
//!    any item whose in-context sense was not the first; this reports zero mismatches
//!    when the two agree.
//!
//! It reads only corpus text and a dictionary. It does not touch the database.

use std::path::PathBuf;

use vector_questions::dictionary::parse_webster;
use vector_questions::passages::{
    parse_gutenberg, quoted_word, sense_choice_in_context, sense_glosses, verify,
    vocab_answer_is_backed,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    let mut dictionary_path: Option<PathBuf> = None;
    let mut count = 500usize;
    for argument in args.by_ref() {
        match argument.split_once('=') {
            Some(("dict", path)) => dictionary_path = Some(PathBuf::from(path)),
            Some((label, path)) => sources.push((label.to_string(), PathBuf::from(path))),
            None => count = argument.parse().unwrap_or(500),
        }
    }
    let dictionary = {
        let path = dictionary_path.expect("dict=<webster.txt> is required for this audit");
        parse_webster(&std::fs::read_to_string(&path).expect("read dictionary"))
    };

    let mut vocab_items = 0usize;
    let mut non_first = 0usize;
    let mut scored = 0usize;
    let mut mismatched = 0usize;
    let mut verify_failed = 0usize;
    let mut samples: Vec<String> = Vec::new();

    for (label, path) in &sources {
        let text = std::fs::read_to_string(path).expect("read source");
        let parsed = parse_gutenberg(&text, label).with_dictionary(dictionary.clone());
        for item in parsed.build_items(count, 20_260_922, |p| {
            !p.to_lowercase().contains("gutenberg") && p.split_whitespace().count() >= 40
        }) {
            if item.objective_id != "OBJ-PC-VOCAB-01" {
                continue;
            }
            vocab_items += 1;
            if verify(&item).is_err() {
                verify_failed += 1;
            }

            // Re-derive the sense the same way the builder did, and see whether it is
            // the first sense or a later one.
            let target = quoted_word(&item.prompt).unwrap_or_default();
            let context = item.supporting_clause.clone();
            let senses = sense_glosses(&dictionary, &target);
            let (chosen, score) = sense_choice_in_context(&senses, &target, &context);
            if score > 0 {
                scored += 1;
            }
            let chosen = chosen.unwrap_or_default();
            let is_first = senses.first().map(|first| first == &chosen).unwrap_or(true);
            if !is_first {
                non_first += 1;
                if samples.len() < 10 {
                    samples.push(format!(
                        "word={target:<16} score={score:<2} keyed_gloss={chosen:?}\n    sentence: {context}"
                    ));
                }
            }

            // The stored-bank check the ingest performs.
            let answer = &item.options[item.correct_index];
            if !vocab_answer_is_backed(&dictionary, &item.prompt, answer) {
                mismatched += 1;
                if mismatched <= 5 {
                    eprintln!(
                        "MISMATCH [{label}] word={target:?} keyed={answer:?}\n    prompt: {}",
                        item.prompt
                    );
                }
            }
        }
    }

    println!("\nvocab items built              : {vocab_items}");
    println!("items with a scored sense       : {scored}");
    println!("items keyed to a NON-first sense: {non_first}");
    if vocab_items > 0 {
        println!(
            "share non-first                 : {:.1}%",
            100.0 * non_first as f64 / vocab_items as f64
        );
    }
    println!("stored-bank check mismatches    : {mismatched}");
    println!("verify() failures               : {verify_failed}");

    println!("\n--- 10 sampled non-first-sense items (sentence, word, keyed sense) ---");
    for sample in &samples {
        println!("{sample}\n");
    }

    if mismatched > 0 {
        eprintln!("stored check disagrees with the builder on {mismatched} item(s)");
        std::process::exit(1);
    }
    println!("stored check agrees with the builder on all {vocab_items} item(s)");
}
