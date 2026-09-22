//! Print Mechanical Comprehension items the factory generates, with their proofs.
use vector_questions::factory;

fn main() {
    let mut shown = 0;
    for seed in 1u64..400 {
        let Some(item) = factory::generate_one("MC", seed) else {
            continue;
        };
        if shown >= 6 {
            break;
        }
        shown += 1;
        println!("\n[{}] {}", item.template_id, item.stem);
        for (i, option) in item.options.iter().enumerate() {
            let mark = if i == item.correct_index {
                "  <-- correct"
            } else {
                ""
            };
            println!("   {}. {option}{mark}", (b'A' + i as u8) as char);
        }
        println!("   proof: {} = {}", item.expression, item.answer);
        for (index, why) in &item.distractor_rationales {
            println!("   distractor {index}: {why}");
        }
    }
    println!("\ntemplates for MC: {:?}", factory::templates_for("MC"));
}
