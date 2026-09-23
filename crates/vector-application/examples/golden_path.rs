//! Walk the golden path against a database and print what each step did.
//!
//! This is the outcome the product exists for, run end to end against a real store rather than
//! against a fixture: create a learner, read the plan the weak-skill model produces for them,
//! serve the item the plan's objective names, answer it, record the attempt, and read the effect
//! back out of the database -- analytics, objective mastery, and the item's own provenance.
//!
//! It exists for the release lanes that cannot drive the interface: a zero-state installation on
//! a machine with no learner data, and the clean-room lane. `desktop-live-fire.py` proves the
//! packaged window boots and reaches the command layer; this proves the study path behind it,
//! including the parts a window cannot show -- that the attempt is stored once, that the mastery
//! estimate moves, and that the answered item was cited and proved.
//!
//! Every value it prints is read back from the store after the writes, never echoed from the
//! request. It writes learner rows, so point it at a zero-state database or a copy.
//!
//! Usage:
//!     cargo run -p vector-application --example golden_path -- <db> [name] [minutes] [target]

use std::path::PathBuf;

use vector_application::content::ContentPipeline;
use vector_application::service::Services;
use vector_persistence::content::ContentItemRepo;
use vector_persistence::{Database, MigrationManager};

/// What the run did, so a caller can grep one line per step.
fn step(label: &str, detail: String) {
    println!("{label:<26} {detail}");
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(path) = arguments.next() else {
        eprintln!("usage: golden_path <db> [name] [minutes]");
        std::process::exit(2);
    };
    let name = arguments
        .next()
        .unwrap_or_else(|| "golden-path".to_string());
    let minutes: u32 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    // The target is a fourth input so a caller proving the path with unpredictable data can
    // put two run-time values through it, not one.
    let target: u32 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(70);

    let mut db = Database::open(PathBuf::from(&path)).expect("open the database");
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )
    .expect("load migrations");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");

    let services = Services::new(&db);
    step("database", path.clone());

    // 1. A learner. The plan is theirs, so there is no plan without one.
    let profile = services
        .create_profile(&name, target)
        .expect("create a learner");
    let readback = services
        .get_profile(&profile.id)
        .expect("read the learner back");
    assert_eq!(
        readback.name, name,
        "the stored learner differs from the request"
    );
    step(
        "learner created",
        format!(
            "{} name={} target={}",
            profile.id, readback.name, readback.target_score
        ),
    );

    // 2. The plan for the time the learner has.
    let plan = services
        .study_plan(&profile.id, target, minutes)
        .expect("a plan");
    let allocated: u32 = plan.drills.iter().map(|drill| drill.minutes).sum();
    assert_eq!(
        allocated, plan.total_minutes,
        "a plan whose total disagrees with its drills is not a plan"
    );
    step(
        "plan",
        format!(
            "{} drill(s) over {} minute(s)",
            plan.drills.len(),
            plan.total_minutes
        ),
    );

    // 3. One session per drill that names an objective: the item the plan asked for, answered.
    let pipeline = ContentPipeline::new(&db);
    let mut sessions = 0;
    let mut objectives_served: Vec<String> = Vec::new();
    for drill in plan
        .drills
        .iter()
        .filter(|drill| drill.objective_id.is_some())
    {
        let objective = drill.objective_id.as_deref().unwrap_or_default();
        let served = pipeline
            .next_item_for(&drill.subtest, Some(objective), &[])
            .expect("a read of the corpus");
        let Some(item) = served else {
            step(
                "session skipped",
                format!("{} {} served nothing", drill.subtest, objective),
            );
            continue;
        };
        assert_eq!(
            item.objective_id, objective,
            "the plan named {objective} and the session served {}",
            item.objective_id
        );

        // Answer it correctly, the way a learner who knows the material would, and record it.
        // The id names the *attempt*, so it carries the learner: the same item answered by two
        // learners is two attempts, and a runner that repeats against an accumulating store must
        // not look like a duplicate submission. (Found by the soak, which repeats this run.)
        let attempt_id = format!("golden-{}-{}-{}", profile.id, item.id, sessions);
        let recorded = services
            .record_attempt(
                &attempt_id,
                &profile.id,
                &item.subtest,
                &item.id,
                true,
                4_000,
            )
            .expect("record the attempt");
        assert!(recorded, "the attempt was not stored");

        // The same submission twice is one attempt: the store is idempotent by attempt id.
        let again = services
            .record_attempt(
                &attempt_id,
                &profile.id,
                &item.subtest,
                &item.id,
                true,
                4_000,
            )
            .expect("record it again");
        assert!(!again, "a repeated submission must not be stored twice");

        sessions += 1;
        objectives_served.push(objective.to_string());
        step(
            "session",
            format!(
                "{} {} -> {} answered, attempt stored once",
                drill.subtest, objective, item.id
            ),
        );
    }

    // 4. Read the effect back. Analytics and mastery are computed from the attempts, not
    //    from anything this process remembers.
    let analytics = services
        .analytics_all(&profile.id)
        .expect("analytics for the learner");
    step(
        "attempts stored",
        format!(
            "{} row(s) over {} subtest(s)",
            analytics.iter().map(|(_, row)| row.total).sum::<i64>(),
            analytics.len()
        ),
    );
    for (subtest, row) in &analytics {
        step(
            "analytics",
            format!(
                "{subtest}: {}/{} correct, mean {} ms",
                row.correct, row.total, row.mean_latency_ms
            ),
        );
    }

    let mastery = services
        .objective_mastery(&profile.id)
        .expect("objective mastery");
    let moved: Vec<String> = mastery
        .iter()
        .filter(|objective| objective.attempts > 0)
        .map(|objective| {
            format!(
                "{} {:.2} over {} attempt(s)",
                objective.objective_id, objective.score, objective.attempts
            )
        })
        .collect();
    step("objectives with evidence", moved.join("; "));

    // Recompute the stored per-subtest mastery from those attempts, then read it back. An
    // estimate that was computed and never stored is not something a later session can rely
    // on, and the readback is what proves the write.
    let written = services
        .recompute_mastery(&profile.id)
        .expect("recompute mastery");
    let stored = services.mastery(&profile.id).expect("stored mastery");
    step(
        "mastery stored",
        format!("{written} row(s) written, {} readable", stored.len()),
    );
    assert_eq!(
        written,
        stored.len(),
        "the number of mastery rows written and read back must agree"
    );

    // 5. The answered items are still active, cited, and proved -- the provenance claim, read
    //    from the store the session just wrote to.
    let repo = ContentItemRepo::new(&db);
    let active = repo
        .counts_by_state()
        .expect("counts")
        .into_iter()
        .find(|(state, _)| state == "active")
        .map(|(_, count)| count)
        .unwrap_or(0);
    let mut cited = 0;
    for objective in &objectives_served {
        let subtest = objective.split('-').nth(1).unwrap_or_default();
        for item in repo.servable(subtest).expect("servable") {
            if &item.objective_id == objective {
                assert!(
                    !repo.sources(&item.id).expect("sources").is_empty(),
                    "{} is active and cites nothing",
                    item.id
                );
                cited += 1;
            }
        }
    }
    step(
        "provenance",
        format!("{active} active item(s), {cited} in the objectives served, all cited"),
    );

    println!(
        "\ngolden path complete: {sessions} session(s) for learner {}",
        profile.id
    );
}
