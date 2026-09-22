//! Print generated items so a human can review their quality.
//!
//! The factory's tests prove that every item is internally consistent and
//! provable. They cannot judge whether a question is *well written*, which is a
//! judgement `QUESTION_FACTORY.md` step 10 assigns to content review. This
//! example exists so that judgement has something to look at.
//!
//! Usage:
//!     cargo run -p vector-questions --example generate_items -- AR 5
//!
//! Arguments are `<subtest> [count]`, defaulting to `AR` and 5.

use vector_questions::factory;

fn main() {
    let mut args = std::env::args().skip(1);
    let subtest = args.next().unwrap_or_else(|| "AR".to_string());
    let count: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);

    let templates = factory::templates_for(&subtest);
    if templates.is_empty() {
        eprintln!("no templates for subtest {subtest:?}; available: AR, MK");
        std::process::exit(2);
    }

    println!(
        "{subtest}: {} template(s) -> {templates:?}\n",
        templates.len()
    );

    for (index, item) in factory::generate_many(&subtest, count, 20_260_922)
        .iter()
        .enumerate()
    {
        match factory::verify(item) {
            Ok(()) => {}
            Err(failure) => {
                eprintln!("item {index} FAILED verification: {failure}");
                std::process::exit(1);
            }
        }

        println!(
            "--- item {} [{}] objective={}",
            index + 1,
            item.template_id,
            item.objective_id
        );
        println!("{}", item.stem);
        for (option_index, option) in item.options.iter().enumerate() {
            let marker = if option_index == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            println!(
                "  {}. {option}{marker}",
                (b'A' + option_index as u8) as char
            );
        }
        println!("  proof: {} = {}", item.expression, item.answer);
        for (option_index, rationale) in &item.distractor_rationales {
            println!(
                "  why {} is wrong: {rationale}",
                (b'A' + *option_index as u8) as char
            );
        }
        println!("  content_hash: {}", item.content_hash());
        println!();
    }

    println!("all {count} item(s) verified independently");
}
