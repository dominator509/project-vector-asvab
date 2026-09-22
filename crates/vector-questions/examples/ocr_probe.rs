//! Ask the OCR detector about specific tokens, using the real dictionary.
//!
//! Usage:
//!     cargo run -p vector-questions --example ocr_probe -- <webster.txt> <token> [token...]
//!
//! Exists because "the check should have caught this" is a hypothesis, and the
//! only way to settle it is to run the check against the same dictionary the
//! ingester uses. Every token is reported with the first substitution that
//! matched, so a false negative says *why*.

use std::path::PathBuf;

use vector_questions::dictionary::{parse_webster, Dictionary};

/// The substitution that makes a token look like a misreading, if any.
fn explain(dictionary: &Dictionary, token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    let characters: Vec<char> = lower.chars().collect();
    if characters.len() < 5 {
        return Some(format!("too short ({} chars) to test", characters.len()));
    }
    if dictionary.covers(&lower) {
        return Some(format!("the dictionary carries `{lower}` itself"));
    }
    for position in 0..characters.len() {
        for letter in b'a'..=b'z' {
            let candidate_letter = letter as char;
            if candidate_letter == characters[position] {
                continue;
            }
            let mut candidate: String = characters[..position].iter().collect();
            candidate.push(candidate_letter);
            candidate.extend(&characters[position + 1..]);
            if dictionary.covers(&candidate) {
                return Some(format!(
                    "`{lower}` -> `{candidate}` by substituting position {position}"
                ));
            }
        }
    }
    for position in 0..characters.len() {
        let candidate: String = characters[..position]
            .iter()
            .chain(characters[position + 1..].iter())
            .collect();
        if candidate.len() >= 4 && dictionary.covers(&candidate) {
            return Some(format!(
                "`{lower}` -> `{candidate}` by deleting position {position}"
            ));
        }
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: ocr_probe <webster.txt> <token> [token...]");
        std::process::exit(2);
    };
    let tokens: Vec<String> = args.collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let dictionary = parse_webster(&text);
    println!("dictionary: {} entries\n", dictionary.len());

    for token in &tokens {
        let flagged = vector_questions::dictionary::looks_like_misreading(&dictionary, token);
        match explain(&dictionary, token) {
            Some(reason) => println!("  {token:<14} flagged={flagged:<6} {reason}"),
            None => println!("  {token:<14} flagged={flagged:<6} no near word found"),
        }
        println!(
            "                 covers={} covers(lower)={}",
            dictionary.covers(token),
            dictionary.covers(&token.to_lowercase())
        );
    }
}
