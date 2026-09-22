//! Find passages the builder produces that the source text does not contain.
//!
//! `Text::contains_passage` is the check that stops an item being called
//! source-backed when its passage is not in the work. When it refuses an item the
//! builder produced, the interesting question is *how* the two differ, and this
//! prints the passage beside the paragraph it came from so the difference is
//! visible rather than inferred.
//!
//! Usage:
//!     cargo run -p vector-questions --example pc_passage_audit -- <label>=<path> [limit]

use std::path::PathBuf;

use vector_questions::passages::{parse_gutenberg, split_sentences, Text};

/// The paragraph a passage came from, found by the sentence it starts with.
fn locate<'a>(text: &'a Text, passage: &str) -> Option<&'a str> {
    let first = passage.split_whitespace().next()?;
    text.paragraphs()
        .iter()
        .find(|paragraph| paragraph.split_whitespace().any(|word| word == first))
        .map(String::as_str)
}

/// Show the first point at which two strings differ, with context.
fn divergence(left: &str, right: &str) -> String {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    for index in 0..left_chars.len().min(right_chars.len()) {
        if left_chars[index] != right_chars[index] {
            let from = index.saturating_sub(40);
            let left_window: String = left_chars[from..(index + 40).min(left_chars.len())]
                .iter()
                .collect();
            let right_window: String = right_chars[from..(index + 40).min(right_chars.len())]
                .iter()
                .collect();
            return format!(
                "first difference at char {index}\n      item: {left_window:?}\n      para: {right_window:?}"
            );
        }
    }
    format!("no difference in the first {} characters", left_chars.len())
}

fn main() {
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    let mut limit = 5usize;
    for argument in std::env::args().skip(1) {
        match argument.split_once('=') {
            Some((label, path)) => sources.push((label.to_string(), PathBuf::from(path))),
            None => limit = argument.parse().unwrap_or(5),
        }
    }
    if sources.is_empty() {
        eprintln!("usage: pc_passage_audit <label>=<path> [...] [limit]");
        std::process::exit(2);
    }

    let mut total_rejected = 0usize;
    for (label, path) in &sources {
        let raw = std::fs::read_to_string(path).expect("read the work");
        let text = parse_gutenberg(&raw, label);
        let items = text.build_items(400, 20_260_922, |_| true);
        let mut rejected = 0usize;
        for item in &items {
            if text.contains_passage(&item.passage) {
                continue;
            }
            rejected += 1;
            if rejected > limit {
                continue;
            }
            println!("\n=== {label}: item passage is not in the work ===");
            println!("  passage: {:?}", item.passage);
            match locate(&text, &item.passage) {
                Some(paragraph) => {
                    println!("  paragraph: {paragraph:?}");
                    println!("  {}", divergence(&item.passage, paragraph));
                }
                None => println!("  no paragraph contains its first word either"),
            }
            println!("  supporting clause: {:?}", item.supporting_clause);
            println!("  sentences the paragraph splits into:");
            if let Some(paragraph) = locate(&text, &item.passage) {
                for sentence in split_sentences(paragraph) {
                    println!("    - {sentence:?}");
                }
            }
        }
        total_rejected += rejected;
        println!(
            "{label}: {} of {} built item(s) have a passage the work does not contain",
            rejected,
            items.len()
        );
    }
    println!("\ntotal: {total_rejected}");
}
