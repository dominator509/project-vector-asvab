//! Report which items the OCR check refuses, and which word refused them.
//!
//! The check is worth having only if it refuses damage rather than vocabulary. This
//! prints the offending word next to the dictionary word it is one edit from, so the
//! judgement can be made from evidence instead of from the refusal count.
//!
//! Usage:
//!     cargo run -p vector-application --example facts_ocr_report -- <dictionary.txt> <work.txt>

use std::collections::BTreeSet;

use vector_questions::dictionary::{looks_like_misreading, parse_webster};
use vector_questions::facts::{parse_faq, FactItem, MAX_OPTION_WORDS, MIN_OPTION_WORDS};

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|word| word.len() >= 4)
        .map(str::to_string)
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dictionary_path = args.next().expect("usage: <dictionary.txt> <work.txt>");
    let work_path = args.next().expect("usage: <dictionary.txt> <work.txt>");

    let dictionary_text = std::fs::read_to_string(&dictionary_path).expect("read the dictionary");
    let dictionary = parse_webster(&dictionary_text);
    println!("dictionary entries: {}", dictionary.len());

    let work = std::fs::read_to_string(&work_path).expect("read the work");
    let faq = parse_faq(&work, "work");
    let items: Vec<FactItem> = faq.build_items(
        "GS",
        500,
        20_260_922,
        MIN_OPTION_WORDS,
        MAX_OPTION_WORDS,
        |_| true,
    );
    println!("items built: {}", items.len());

    let mut refused = 0usize;
    let mut offending: BTreeSet<String> = BTreeSet::new();
    for item in &items {
        let mut bad: Vec<String> = Vec::new();
        for text in std::iter::once(&item.prompt).chain(item.options.iter()) {
            for word in words(text) {
                if looks_like_misreading(&dictionary, &word) {
                    bad.push(word);
                }
            }
        }
        if bad.is_empty() {
            continue;
        }
        refused += 1;
        for word in &bad {
            offending.insert(word.clone());
        }
        if refused <= 12 {
            println!("\nrefused: {}", item.prompt);
            println!("   words: {bad:?}");
        }
    }

    println!("\nrefused {refused} of {} item(s)", items.len());
    println!("distinct offending words: {}", offending.len());
    let listed: Vec<&String> = offending.iter().take(40).collect();
    println!("first forty: {listed:?}");
}
