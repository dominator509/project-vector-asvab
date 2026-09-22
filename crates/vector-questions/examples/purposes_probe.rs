//! Print the tool descriptions a text yields, for debugging the miner.
//!
//! Usage:
//!     cargo run -p vector-questions --example purposes_probe -- <file.txt>

use vector_questions::purposes::parse_purposes;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: purposes_probe <file.txt>");
    let text = std::fs::read_to_string(&path).expect("read the file");
    let source = parse_purposes(&text, "probe");
    println!("{} description(s)", source.entry_count());
    for entry in source.entries() {
        println!("  tool   : {:?}", entry.tool);
        println!("  purpose: {:?}", entry.purpose);
        println!("  sentence: {}", entry.sentence);
        println!();
    }
}
