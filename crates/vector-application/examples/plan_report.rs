//! Print the plan a database produces, with the objectives the installed pack names.
//!
//! This is the readback the service tests cannot give: it opens a *real* store -- the one an
//! installation uses, with the pack it installed and the corpus that was ingested into it --
//! creates a learner, and prints what the planner says for them. The service tests prove the
//! behaviour against a fixture; this proves the fixture is not the only thing it works on.
//!
//! It writes a profile, so point it at a copy rather than at a live database.
//!
//! Usage:
//!     cargo run -p vector-application --example plan_report -- <db> [name] [minutes]

use std::path::PathBuf;

use vector_application::content::ContentPipeline;
use vector_application::service::Services;
use vector_persistence::{Database, MigrationManager};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(path) = arguments.next() else {
        eprintln!("usage: plan_report <db> [name] [minutes]");
        std::process::exit(2);
    };
    let name = arguments.next().unwrap_or_else(|| "plan-probe".to_string());
    let minutes: u32 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(45);

    let mut db = Database::open(PathBuf::from(&path)).expect("open the database");
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )
    .expect("load migrations");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");

    let services = Services::new(&db);
    let profile = services
        .create_profile(&name, 70)
        .expect("create a learner");

    let mastery = services
        .objective_mastery(&profile.id)
        .expect("objective mastery");
    println!("objectives the installed pack declares: {}", mastery.len());
    for objective in &mastery {
        println!(
            "  {:<26} {:<4} score {:.2} over {} attempt(s){}",
            objective.objective_id,
            objective.subtest,
            objective.score,
            objective.attempts,
            if objective.waiting_on.is_empty() {
                String::new()
            } else {
                format!(", waiting on {}", objective.waiting_on.join(", "))
            }
        );
    }

    let plan = services
        .study_plan(&profile.id, 70, minutes)
        .expect("a plan");
    println!("\nplan for {} minute(s):", minutes);
    for drill in &plan.drills {
        println!(
            "  {:<4} {:>3} min  objective {:<26} {}",
            drill.subtest,
            drill.minutes,
            drill.objective_id.as_deref().unwrap_or("-"),
            drill.reason
        );
    }

    // The plan names an objective; this is what the practice surface actually asks the store
    // for, run against the real corpus rather than a fixture. Round 27's readback caught a drill
    // that could not start, so the same readback is applied to the item read: a plan naming an
    // objective no item carries would send the learner to a narrowed read that falls back, and
    // only the real store can say whether that is what happens.
    let pipeline = ContentPipeline::new(&db);
    println!("\nwhat a session for each named objective is served:");
    for drill in plan.drills.iter().filter(|d| d.objective_id.is_some()) {
        let objective = drill.objective_id.as_deref().unwrap_or_default();
        let served = pipeline
            .next_item_for(&drill.subtest, Some(objective), &[])
            .expect("a read of the real corpus");
        match served {
            Some(item) => println!(
                "  {:<4} {:<26} -> {} [{}] {}",
                drill.subtest,
                objective,
                item.id,
                item.objective_id,
                shorten(&item.stem)
            ),
            None => println!(
                "  {:<4} {:<26} -> nothing to serve",
                drill.subtest, objective
            ),
        }
    }

    // The profile is written, so the caller is told what to clean up when it did not use a copy.
    println!("\nwrote learner {} to {}", profile.id, path);
}

/// The stem, cut to one line so the readback stays readable.
fn shorten(stem: &str) -> String {
    let flat = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 72 {
        return flat;
    }
    let cut: String = flat.chars().take(72).collect();
    format!("{cut}…")
}
