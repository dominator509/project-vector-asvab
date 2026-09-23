//! Generate factory items into a corpus database, the way the application does.
//!
//! The three computable subtests -- Arithmetic Reasoning, Mathematics Knowledge and Mechanical
//! Comprehension -- have templates rather than sources, so their items exist only when something
//! asks the factory for them. The application asks on demand, which means a fresh installation
//! holds none of them: the study plan can name no objective for those subtests (nothing declares
//! one), and the first practice session has to press "Prepare 40 questions" before it can serve
//! anything. This runner fills the corpus ahead of the pack build, through the same pipeline the
//! application uses -- verify first, then store, then activate -- so the items in the pack are the
//! ones the product would have made, with the same proofs, reviewers and citations.
//!
//! It is a corpus step, not a second implementation: `ContentPipeline::generate_and_activate` is
//! exactly what `content_generate` calls.
//!
//! Usage:
//!     cargo run -p vector-application --example generate_corpus -- <db> AR=100 MK=100 MC=100 [seed]

use std::path::PathBuf;

use vector_application::content::{ContentPipeline, GenerateRequest};
use vector_persistence::{Database, MigrationManager};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(path) = arguments.next() else {
        eprintln!("usage: generate_corpus <db> <SUBTEST=count>... [seed]");
        std::process::exit(2);
    };
    let rest: Vec<String> = arguments.collect();
    let seed: u64 = rest
        .last()
        .filter(|value| value.chars().all(|c| c.is_ascii_digit()))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let requests: Vec<(String, usize)> = rest
        .iter()
        .filter(|value| value.contains('='))
        .map(|value| {
            let (subtest, count) = value.split_once('=').expect("checked for =");
            (
                subtest.to_string(),
                count
                    .parse()
                    .unwrap_or_else(|_| panic!("count in {value:?}")),
            )
        })
        .collect();
    if requests.is_empty() {
        eprintln!("usage: generate_corpus <db> <SUBTEST=count>... [seed]");
        std::process::exit(2);
    }

    let mut db = Database::open(PathBuf::from(&path)).expect("open the database");
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )
    .expect("load migrations");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");

    let pipeline = ContentPipeline::new(&db);
    let source_id = pipeline
        .ensure_construct_source()
        .expect("construct source");

    for (subtest, count) in &requests {
        let request = GenerateRequest {
            subtest,
            count: *count,
            // One seed per subtest rather than one for the run, so adding a subtest to the
            // command does not change what the earlier subtests produce.
            seed: seed + subtest.bytes().map(u64::from).sum::<u64>(),
            reviewer: "machine-verifier",
            source_id: &source_id,
            generator: "factory",
        };
        let report = pipeline
            .generate_and_activate(&request)
            .unwrap_or_else(|error| panic!("generate {subtest}: {error}"));
        println!(
            "{}: generated {} verified {} activated {} already present {} rejected {}",
            report.subtest,
            report.generated,
            report.verified,
            report.activated,
            report.already_present,
            report.rejected.len()
        );
        for rejection in &report.rejected {
            println!(
                "  rejected {}: {}",
                rejection.template_id.as_deref().unwrap_or("-"),
                rejection.reason
            );
        }
    }

    // Read the per-objective spread back rather than trusting the report: the pack's curriculum
    // and calibration are written from the store, so this is what they will declare.
    let conn = db.connection();
    let mut statement = conn
        .prepare(
            "SELECT subtest, objective_id, COUNT(*) FROM content_items
             WHERE state = 'active' GROUP BY subtest, objective_id ORDER BY subtest, objective_id",
        )
        .expect("prepare");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .expect("query");
    println!("\nactive items by objective:");
    for row in rows {
        let (subtest, objective, count) = row.expect("row");
        println!("  {subtest:<3} {objective:<26} {count}");
    }
}
