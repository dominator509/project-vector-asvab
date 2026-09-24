//! Audit: count and sample the PC kinds produced from real prose.
//!
//! Run:
//!   cargo run -p vector-questions --release --example pc_kind_audit -- \
//!       <label>=<path> [...] dict=<webster.txt> [count]

use std::path::PathBuf;

use vector_questions::dictionary::parse_webster;
use vector_questions::passages::{parse_gutenberg, verify};

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
    let dictionary = dictionary_path
        .as_ref()
        .map(|p| parse_webster(&std::fs::read_to_string(p).expect("read dictionary")));

    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut failed = 0usize;
    let mut examples: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (label, path) in &sources {
        let text = std::fs::read_to_string(path).expect("read source");
        let mut parsed = parse_gutenberg(&text, label);
        if let Some(d) = &dictionary {
            parsed = parsed.with_dictionary(d.clone());
        }
        for item in parsed.build_items(count, 20_260_922, |p| {
            !p.to_lowercase().contains("gutenberg") && p.split_whitespace().count() >= 40
        }) {
            *kinds.entry(item.objective_id.clone()).or_insert(0) += 1;
            if let Err(e) = verify(&item) {
                failed += 1;
                eprintln!("REJECTED {label} {}: {e}", item.objective_id);
            }
            let entry = examples.entry(item.objective_id.clone()).or_default();
            if entry.len() < 2 {
                entry.push(format!(
                    "{}\n{}",
                    item.prompt,
                    item.options
                        .iter()
                        .enumerate()
                        .map(|(i, o)| format!(
                            "   {}{o}",
                            if i == item.correct_index { "*" } else { " " }
                        ))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
    }
    println!("\nkind counts: {kinds:?}");
    println!("failed: {failed}");
    for (kind, samples) in &examples {
        println!("\n===== {kind} =====");
        for s in samples {
            println!("{s}\n");
        }
    }
}
