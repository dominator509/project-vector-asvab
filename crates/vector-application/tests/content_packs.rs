//! Signed content packs end to end: assembly, installation, rollback.
//!
//! Requirements: REQ-023, REQ-032, REQ-048, REQ-056.
//!
//! This file drives the public path only: a store with items, a pack built from it,
//! the bytes written and read back, and the effect read out of the second store
//! through the serving path rather than from the installer's return value. The
//! refusals that need a pack whose stored fields disagree with its signature live in
//! `src/packs.rs`, where the test-only hooks are compiled, because producing those
//! states through a public API would mean shipping setters for invalid packs.

use std::path::Path;

use ed25519_dalek::SigningKey;
use vector_application::content::{ContentPipeline, GenerateRequest};
use vector_application::packs::{
    activate_pack, build_pack, install_pack, installed_packs, pack_bytes, parse_pack,
    rollback_pack, verify_pack, BuildPackRequest, PackRefusal, PACK_FORMAT, PACK_SCHEMA_VERSION,
};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};

mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "vector-packs-{}-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            path.push(unique);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

const RUNNING_VERSION: &str = "0.1.0";

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

fn database(tag: &str) -> (tempdir::TempDir, Database) {
    let dir = tempdir::TempDir::new(tag);
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    (dir, db)
}

/// A store holding generated items, with a licence the corpus permits.
fn store_with_items(tag: &str, subtest: &str, count: usize) -> (tempdir::TempDir, Database) {
    store_with_licence(tag, subtest, count, "Public domain in the USA")
}

fn store_with_licence(
    tag: &str,
    subtest: &str,
    count: usize,
    licence: &str,
) -> (tempdir::TempDir, Database) {
    let (dir, db) = database(tag);
    let source = EvidenceRepo::new(&db)
        .put(&NewEvidence::new(
            "https://www.officialasvab.com/applicants/sample-questions/",
            "ASVAB subtest constructs (facts only)",
            "sha256:pack-test-source",
            licence,
            "2026-09-22",
            0.9,
            "retrieved",
        ))
        .expect("record source");
    ContentPipeline::new(&db)
        .generate_and_activate(&GenerateRequest {
            subtest,
            count,
            seed: 20_260_922,
            reviewer: "content-reviewer",
            source_id: &source,
            generator: "factory",
        })
        .expect("generate");
    (dir, db)
}

/// A fixed test key. Nothing outside this file trusts it.
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7_u8; 32])
}

fn other_key() -> SigningKey {
    SigningKey::from_bytes(&[9_u8; 32])
}

fn trusted() -> Vec<u8> {
    key().verifying_key().to_bytes().to_vec()
}

/// Build a pack file, signed by whichever key the caller names.
fn pack_file(db: &Database, version: i64, signing: &SigningKey) -> Vec<u8> {
    let document = build_pack(
        db,
        &BuildPackRequest {
            name: "core-asvab",
            version,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        signing,
    )
    .expect("build");
    pack_bytes(&document).expect("serialize")
}

fn build(db: &Database, version: i64) -> Vec<u8> {
    pack_file(db, version, &key())
}

/// Build a pack file under a chosen name, so two packs of different names can be
/// installed side by side.
fn named_pack_file(db: &Database, name: &str, version: i64) -> Vec<u8> {
    let document = build_pack(
        db,
        &BuildPackRequest {
            name,
            version,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect("build");
    pack_bytes(&document).expect("serialize")
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

#[test]
fn a_built_pack_carries_its_items_its_ledger_and_a_signature() {
    let (_dir, db) = store_with_items("build", "AR", 12);
    let document = build_pack(
        &db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect("build");

    assert_eq!(document.format(), PACK_FORMAT);
    assert_eq!(document.schema_version(), PACK_SCHEMA_VERSION);
    assert_eq!(document.name(), "core-asvab");
    assert_eq!(document.items().len(), 12);
    assert_eq!(document.sources().len(), 1, "one cited source");
    assert!(
        document
            .licences()
            .contains(&"Public domain in the USA".to_string()),
        "{:?}",
        document.licences()
    );
    assert!(!document.signature.is_empty() && !document.signer.is_empty());
    assert!(
        document.content_hash.starts_with("sha256:"),
        "the signature covers a content digest: {}",
        document.content_hash
    );

    // Provenance completeness, as the pack itself carries it: every item cites a
    // source in its own ledger and names its reviewer.
    let ledger: Vec<&str> = document.sources().iter().map(|s| s.id.as_str()).collect();
    for item in document.items() {
        assert!(!item.sources.is_empty(), "{} cites nothing", item.id);
        for cited in &item.sources {
            assert!(
                ledger.contains(&cited.as_str()),
                "{} cites {cited}",
                item.id
            );
        }
        assert!(
            !item.reviewer.trim().is_empty(),
            "{} arrives with no reviewer",
            item.id
        );
    }
}

#[test]
fn a_built_pack_survives_a_round_trip_through_its_bytes() {
    let (_dir, db) = store_with_items("roundtrip", "MK", 6);
    let bytes = build(&db, 1);
    let parsed = parse_pack(&bytes).expect("parse");
    assert_eq!(parsed.items().len(), 6);
    verify_pack(&parsed, &trusted(), RUNNING_VERSION).expect("a fresh pack must verify");
}

/// The content hash is computed over the serialized payload, and `verify_pack`
/// recomputes it from what was parsed back off the bytes. If any payload field does
/// not survive serde's text round trip byte-for-byte -- a raw `f64` sigmoid was the
/// actual offender, `0.24973989440488234` printing more digits than it parses back
/// as -- then re-serializing a parsed pack yields different bytes and the pack
/// refuses itself. AR carries the difficulty spread that produced that value, so it
/// is the subtest most likely to catch a regression here.
#[test]
fn a_parsed_pack_re_serializes_to_identical_bytes() {
    for subtest in ["AR", "MK"] {
        let (_dir, db) = store_with_items(&format!("restable-{subtest}"), subtest, 24);
        let bytes = build(&db, 1);
        let parsed = parse_pack(&bytes).expect("parse");

        // Timestamp, signature and the hash itself are outside the hashed payload's
        // stability guarantee for a re-serialization test only in that they are
        // recomputed; the payload they cover must be identical. Comparing the whole
        // document is stronger, so blank the three fields that legitimately differ
        // and compare the rest.
        let scrub = |raw: &[u8]| -> String {
            String::from_utf8_lossy(raw)
                .lines()
                .filter(|line| {
                    !line.contains("created_at")
                        && !line.contains("content_hash")
                        && !line.contains("signature")
                        && !line.contains("signer")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let rebuilt = pack_bytes(&parsed).expect("re-serialize");
        assert_eq!(
            scrub(&bytes),
            scrub(&rebuilt),
            "{subtest}: the payload does not survive a text round trip, so the content \
             hash a pack records can differ from the one verify_pack recomputes"
        );
        assert_eq!(
            parsed.content_hash,
            parsed.computed_content_hash(),
            "{subtest}: the recorded hash must equal the recomputed one on the parsed pack"
        );
        verify_pack(&parsed, &trusted(), RUNNING_VERSION)
            .unwrap_or_else(|e| panic!("{subtest}: re-serialized pack must still verify: {e}"));
    }
}

#[test]
fn building_a_pack_from_an_empty_store_is_refused() {
    let (_dir, db) = database("empty");
    let error = build_pack(
        &db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect_err("an empty store has nothing to attest");
    assert!(
        error.to_string().contains("no active items"),
        "the refusal should say why: {error}"
    );
}

#[test]
fn a_pack_signed_by_another_key_is_refused() {
    let (_dir, db) = store_with_items("signer", "AR", 4);
    let bytes = pack_file(&db, 1, &other_key());

    let document = parse_pack(&bytes).expect("parse");
    match verify_pack(&document, &trusted(), RUNNING_VERSION) {
        Err(PackRefusal::UntrustedSigner { .. }) => {}
        other => panic!("expected an untrusted-signer refusal, got {other:?}"),
    }

    // And installation refuses it too, whichever entry point a caller uses.
    let (_target_dir, target_db) = database("signer-target");
    install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION)
        .expect_err("an untrusted signer must not install");
    assert!(installed_packs(&target_db).expect("packs").is_empty());
}

#[test]
fn a_pack_that_requires_a_newer_application_is_refused() {
    let (_dir, db) = store_with_items("apprange", "AR", 4);
    let document = build_pack(
        &db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: "9.0.0",
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect("build");
    match verify_pack(&document, &trusted(), RUNNING_VERSION) {
        Err(PackRefusal::IncompatibleApp { running, .. }) => assert_eq!(running, RUNNING_VERSION),
        other => panic!("expected a compatibility refusal, got {other:?}"),
    }
}

/// The prohibited-source scan, reached through the public path: a store whose own
/// vault records a licence the corpus does not permit produces a pack carrying it.
#[test]
fn a_pack_built_from_a_prohibited_source_is_refused() {
    let (_dir, db) = store_with_licence(
        "licence",
        "AR",
        4,
        "Copyright 2020. All rights reserved. Licensed for study use only.",
    );
    let bytes = build(&db, 1);
    let document = parse_pack(&bytes).expect("parse");
    match verify_pack(&document, &trusted(), RUNNING_VERSION) {
        Err(PackRefusal::SourceNotPermitted { licence, .. }) => {
            assert!(licence.contains("All rights reserved"), "{licence}")
        }
        other => panic!("expected a prohibited-source refusal, got {other:?}"),
    }
}

#[test]
fn a_file_that_is_not_a_pack_is_refused_rather_than_guessed_at() {
    match parse_pack(b"{\"not\": \"a pack\"}") {
        Err(PackRefusal::Malformed(_)) => {}
        other => panic!("expected a malformed refusal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Installation and rollback
// ---------------------------------------------------------------------------

#[test]
fn installing_a_pack_delivers_its_items_and_records_its_identity() {
    let (_source_dir, source_db) = store_with_items("install-src", "AR", 10);
    let bytes = build(&source_db, 1);

    let (_target_dir, target_db) = database("install-target");
    let report = install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION).expect("install");

    assert_eq!(report.name, "core-asvab");
    assert_eq!(report.installed, 10);
    assert_eq!(report.already_present, 0);
    assert_eq!(
        report.sources_added, 1,
        "the pack's ledger enters the vault"
    );
    assert_eq!(report.items, report.installed);

    // Read the effect back through the serving path, not the return value.
    let repo = ContentItemRepo::new(&target_db);
    let served = repo.servable("AR").expect("servable");
    assert_eq!(served.len(), 10);
    for item in &served {
        assert_eq!(item.state, "active");
        assert!(
            !repo.sources(&item.id).expect("sources").is_empty(),
            "{}: an installed item must cite its source",
            item.id
        );
        assert!(
            !item.reviewer.trim().is_empty(),
            "{}: and record who reviewed it",
            item.id
        );
    }

    // The audit trail an installed item carries is the shape an ingested one has:
    // four transitions, the last of them activation.
    let history = repo.history(&served[0].id).expect("history");
    assert_eq!(history.len(), 4, "{history:?}");
    assert_eq!(history[0].from_state, "draft");
    assert_eq!(history[3].to_state, "active");

    let packs = installed_packs(&target_db).expect("packs");
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].status, "active");
    assert_eq!(packs[0].item_count, 10);
    assert!(
        packs[0].signature_valid,
        "the registry must verify the identity it stored"
    );
    assert_eq!(packs[0].schema_version, PACK_SCHEMA_VERSION);
}

#[test]
fn installing_the_same_pack_twice_changes_nothing() {
    let (_source_dir, source_db) = store_with_items("reinstall-src", "MK", 8);
    let bytes = build(&source_db, 1);
    let (_target_dir, target_db) = database("reinstall-target");

    let first = install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION).expect("first");
    let second = install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION).expect("second");

    assert_eq!(first.installed, 8);
    assert_eq!(second.installed, 0, "nothing is reinstalled");
    assert_eq!(second.already_present, 8);
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("MK")
            .expect("servable")
            .len(),
        8,
        "and nothing is duplicated"
    );
}

#[test]
fn a_refused_pack_writes_nothing_at_all() {
    let (_source_dir, source_db) = store_with_licence(
        "refuse-src",
        "AR",
        6,
        "Copyright 2020. All rights reserved.",
    );
    let bytes = build(&source_db, 1);

    let (_target_dir, target_db) = database("refuse-target");
    let error = install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION)
        .expect_err("a prohibited licence must refuse the pack");
    assert!(
        error.to_string().contains("does not permit"),
        "the refusal should say why: {error}"
    );

    assert!(
        installed_packs(&target_db).expect("packs").is_empty(),
        "a refused install must leave no pack row"
    );
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("servable")
            .len(),
        0,
        "and no items"
    );
    assert!(
        EvidenceRepo::new(&target_db)
            .list()
            .expect("vault")
            .is_empty(),
        "and no vault rows"
    );
}

#[test]
fn rollback_restores_the_previous_pack_and_its_items() {
    let (_first_dir, first_db) = store_with_items("rollback-a", "AR", 5);
    let v1 = build(&first_db, 1);

    let (_second_dir, second_db) = store_with_items("rollback-b", "MK", 3);
    let v2 = build(&second_db, 2);

    let (_target_dir, target_db) = database("rollback-target");
    install_pack(&target_db, &v1, &trusted(), RUNNING_VERSION).expect("install v1");
    install_pack(&target_db, &v2, &trusted(), RUNNING_VERSION).expect("install v2");

    let repo = ContentItemRepo::new(&target_db);
    assert_eq!(
        repo.servable("AR").expect("AR").len(),
        0,
        "v1 is superseded"
    );
    assert_eq!(repo.servable("MK").expect("MK").len(), 3, "v2 serves");

    let rolled = rollback_pack(&target_db, "core-asvab").expect("rollback");
    assert_eq!(rolled.version, 1);
    assert_eq!(rolled.status, "active");

    // The items follow the pack, because serving is derived from pack status.
    assert_eq!(
        repo.servable("AR").expect("AR").len(),
        5,
        "the previous pack's items serve again"
    );
    assert_eq!(
        repo.servable("MK").expect("MK").len(),
        0,
        "the withdrawn pack's items do not"
    );

    let packs = installed_packs(&target_db).expect("packs");
    assert_eq!(packs.len(), 2, "both versions stay for a further rollback");
    assert!(packs.iter().all(|pack| pack.signature_valid));
    assert_eq!(
        packs.iter().filter(|pack| pack.status == "active").count(),
        1,
        "exactly one version is active"
    );
}

#[test]
fn rolling_back_a_pack_that_has_no_earlier_version_is_refused() {
    let (_source_dir, source_db) = store_with_items("rollback-none", "AR", 4);
    let bytes = build(&source_db, 1);
    let (_target_dir, target_db) = database("rollback-none-target");
    install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION).expect("install");

    let error = rollback_pack(&target_db, "core-asvab").expect_err("nothing to roll back to");
    assert!(
        error.to_string().contains("no earlier pack"),
        "the refusal should say why: {error}"
    );
}

/// The items an install delivers are the items the pack carried, not a regeneration.
#[test]
fn installed_items_are_the_ones_the_pack_carried() {
    let (_source_dir, source_db) = store_with_items("identity-src", "AR", 4);
    let document = build_pack(
        &source_db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect("build");
    let packed: Vec<(String, String)> = document
        .items()
        .iter()
        .map(|item| (item.id.clone(), item.stem.clone()))
        .collect();
    let bytes = pack_bytes(&document).expect("serialize");

    let (_target_dir, target_db) = database("identity-target");
    install_pack(&target_db, &bytes, &trusted(), RUNNING_VERSION).expect("install");

    let repo = ContentItemRepo::new(&target_db);
    for (id, stem) in packed {
        let stored = repo.get(&id).expect("get").expect("the item is present");
        assert_eq!(stored.stem, stem, "{id}: the stem must be the packed one");
        assert_eq!(stored.state, "active");
    }
}

/// A version that repackages content an earlier pack delivered must keep it serving.
///
/// Serving follows `pack_id`, so an item left pointing at the superseded pack is
/// withdrawn the moment the new version activates. Found by installing a second real
/// pack over the first: the corpus went from 7,016 servable items to none, because
/// the new pack happened to carry the same items and none of them were re-parented.
#[test]
fn repackaging_the_same_content_keeps_it_serving() {
    let (_source_dir, source_db) = store_with_items("repackage-src", "AR", 6);
    let v1 = build(&source_db, 1);
    // The same store, the same items, published as a new version.
    let v2 = build(&source_db, 2);

    let (_target_dir, target_db) = database("repackage-target");
    install_pack(&target_db, &v1, &trusted(), RUNNING_VERSION).expect("install v1");
    let report = install_pack(&target_db, &v2, &trusted(), RUNNING_VERSION).expect("install v2");

    assert_eq!(
        report.installed, 0,
        "the items themselves are already present"
    );
    assert_eq!(report.already_present, 6);
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("servable")
            .len(),
        6,
        "repackaged content must not be withdrawn when the earlier pack is superseded"
    );

    // And a rollback to v1 leaves them serving too, because v1 now holds them again.
    rollback_pack(&target_db, "core-asvab").expect("rollback");
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("servable")
            .len(),
        6
    );
}

/// A withdrawn version is not a rollback target.
///
/// A quarantined pack was set aside deliberately -- by a rollback, or by a reviewer --
/// and reinstating it automatically would undo that decision. Found by rolling back a
/// store holding three versions: the rollback landed on the quarantined one instead of
/// the version that had merely been superseded.
#[test]
fn rollback_does_not_reinstate_a_quarantined_pack() {
    let (_first_dir, first_db) = store_with_items("quarantine-a", "AR", 4);
    let v1 = build(&first_db, 1);
    let (_second_dir, second_db) = store_with_items("quarantine-b", "MK", 2);
    let v2 = build(&second_db, 2);
    // A third version carrying content of its own, from a subtest the factory serves.
    let (_third_dir, third_db) = store_with_items("quarantine-c", "MC", 3);
    let v3 = build(&third_db, 3);

    let (_target_dir, target_db) = database("quarantine-target");
    install_pack(&target_db, &v1, &trusted(), RUNNING_VERSION).expect("v1");
    install_pack(&target_db, &v2, &trusted(), RUNNING_VERSION).expect("v2");
    // v2 is withdrawn deliberately.
    rollback_pack(&target_db, "core-asvab").expect("rollback to v1");
    let statuses = installed_packs(&target_db).expect("packs");
    assert_eq!(
        statuses
            .iter()
            .find(|pack| pack.version == 2)
            .expect("v2 is registered")
            .status,
        "quarantined"
    );

    install_pack(&target_db, &v3, &trusted(), RUNNING_VERSION).expect("v3");
    let rolled = rollback_pack(&target_db, "core-asvab").expect("rollback from v3");
    assert_eq!(
        rolled.version, 1,
        "the rollback must land on the superseded version, not the quarantined one"
    );
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("AR")
            .len(),
        4,
        "v1's items serve again"
    );
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("MK")
            .expect("MK")
            .len(),
        0,
        "the quarantined version's items stay withdrawn"
    );
}

/// A rollback has a way back, and only on purpose.
///
/// `install` leaves a registered pack's status alone so a stray reinstall cannot undo a rollback
/// -- and that is the right safety property, but on its own it makes a rollback a one-way door.
/// Found in round 29 by rolling a real installation back and reinstalling the same signed pack:
/// the install reported success and changed nothing, leaving the learner's content withdrawn with
/// no supported way to bring it back. Activating a version is that way, and this pins both halves:
/// the reinstall stays inert, and the activation moves the content.
#[test]
fn an_activated_version_returns_to_service_and_a_reinstall_does_not() {
    let (_first_dir, first_db) = store_with_items("activate-a", "AR", 4);
    let v1 = build(&first_db, 1);
    let (_second_dir, second_db) = store_with_items("activate-b", "MK", 3);
    let v2 = build(&second_db, 2);

    let (_target_dir, target_db) = database("activate-target");
    install_pack(&target_db, &v1, &trusted(), RUNNING_VERSION).expect("v1");
    install_pack(&target_db, &v2, &trusted(), RUNNING_VERSION).expect("v2");
    rollback_pack(&target_db, "core-asvab").expect("rollback to v1");

    // The reinstall is deliberately inert.
    install_pack(&target_db, &v2, &trusted(), RUNNING_VERSION).expect("reinstall v2");
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("MK")
            .expect("MK")
            .len(),
        0,
        "reinstalling a registered version must not quietly undo the rollback"
    );

    // The activation is the deliberate act, and it moves the content back into service.
    let activated = activate_pack(&target_db, "core-asvab", 2).expect("activate v2");
    assert_eq!(activated.version, 2);
    assert_eq!(activated.status, "active");
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("MK")
            .expect("MK")
            .len(),
        3,
        "v2's items serve again once v2 is active"
    );
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("AR")
            .len(),
        0,
        "and v1's items are withdrawn, because v1 is not the active pack"
    );
    let statuses = installed_packs(&target_db).expect("packs");
    assert_eq!(
        statuses
            .iter()
            .find(|pack| pack.version == 1)
            .expect("v1 is registered")
            .status,
        "superseded",
        "the version left behind is superseded, not quarantined: a later transition can return to it"
    );

    // Activating what is already active is a no-op rather than an error, so a scripted
    // transition can state its intent without first asking what the state is.
    let again = activate_pack(&target_db, "core-asvab", 2).expect("activate v2 again");
    assert_eq!(again.status, "active");
    // And a version the installation does not hold is refused rather than invented.
    let missing = activate_pack(&target_db, "core-asvab", 9).expect_err("no v9");
    assert!(
        missing.to_string().contains("no pack named core-asvab"),
        "{missing}"
    );
}

/// A pack whose content overlaps the corpus installs without conflict.
///
/// Two packs built independently from the same source carry the same questions under
/// different item ids. Installing the second one must recognise the content it already
/// has and record its own membership of those rows -- not try to insert them again, and
/// not point its membership at ids that do not exist locally. Found by installing a
/// 40-item pack into a store that already held the same 40 items among 7,016: the
/// install failed on a foreign key.
#[test]
fn a_pack_that_overlaps_the_corpus_installs_without_conflict() {
    let (_first_dir, first_db) = store_with_items("overlap-a", "AR", 5);
    let (_second_dir, second_db) = store_with_items("overlap-b", "AR", 5);
    let first = named_pack_file(&first_db, "pack-a", 1);
    let second = named_pack_file(&second_db, "pack-b", 1);

    // The two stores really do hold the same questions under different ids.
    let first_ids: Vec<String> = parse_pack(&first)
        .expect("parse a")
        .items()
        .iter()
        .map(|item| item.id.clone())
        .collect();
    let second_ids: Vec<String> = parse_pack(&second)
        .expect("parse b")
        .items()
        .iter()
        .map(|item| item.id.clone())
        .collect();
    assert_ne!(first_ids, second_ids, "the pack item ids differ");
    let mut first_hashes: Vec<String> = parse_pack(&first)
        .expect("parse a")
        .items()
        .iter()
        .map(|item| item.content_hash.clone())
        .collect();
    let mut second_hashes: Vec<String> = parse_pack(&second)
        .expect("parse b")
        .items()
        .iter()
        .map(|item| item.content_hash.clone())
        .collect();
    // Sorted, because each pack orders its items by its own ids, which differ.
    first_hashes.sort();
    second_hashes.sort();
    assert_eq!(
        first_hashes, second_hashes,
        "but the content they carry is the same"
    );

    let (_target_dir, target_db) = database("overlap-target");
    install_pack(&target_db, &first, &trusted(), RUNNING_VERSION).expect("install the first");
    let report = install_pack(&target_db, &second, &trusted(), RUNNING_VERSION)
        .expect("an overlapping pack must install");

    assert_eq!(report.installed, 0, "the content is already here");
    assert_eq!(report.already_present, 5);
    assert_eq!(
        ContentItemRepo::new(&target_db)
            .servable("AR")
            .expect("servable")
            .len(),
        5,
        "and the corpus is neither duplicated nor withdrawn"
    );
    let packs = installed_packs(&target_db).expect("packs");
    assert_eq!(packs.len(), 2, "both packs are registered");
    assert_eq!(
        packs.iter().filter(|pack| pack.status == "active").count(),
        2,
        "two packs of different names may both be active"
    );
}

/// What a pack teaches survives installation and reaches the interface.
///
/// The curriculum and the calibration live inside the manifest the installer stores, so the
/// content manager can only show them if reading a pack back out of the registry carries them.
/// The counts matter as much as the values: `responses` is what tells a reader whether a
/// difficulty figure was measured or declared.
#[test]
fn an_installed_pack_reports_what_it_teaches() {
    let dir = tempdir::TempDir::new("pack-objectives");
    let mut db = Database::open(dir.path().join("vector.db")).expect("open");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let source = EvidenceRepo::new(&db)
        .put(&NewEvidence::new(
            "https://archive.org/details/micro_IA41153156_0308",
            "Tools and Their Uses (Internet Archive micro_IA41153156_0308)",
            "sha256:tools-fixture",
            "Public domain (US government work)",
            "2026-09-22",
            0.90,
            "retrieved",
        ))
        .expect("record the source");
    ContentPipeline::new(&db)
        .generate_and_activate(&GenerateRequest {
            subtest: "AR",
            count: 6,
            seed: 20_260_922,
            source_id: &source,
            reviewer: "content-reviewer",
            generator: "factory",
        })
        .expect("generate");

    // A curriculum with one prerequisite edge and one *measured* calibration entry, so the
    // interface has both a graph and a measurement to show.
    let base = build_pack(
        &db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        &key(),
    )
    .expect("build the base pack");
    let mut objectives: Vec<(String, String)> = base
        .curriculum()
        .iter()
        .map(|node| (node.objective_id.clone(), node.subtest.clone()))
        .collect();
    objectives.sort();
    let curriculum: Vec<vector_application::packs::CurriculumNode> = objectives
        .iter()
        .enumerate()
        .map(
            |(index, (objective_id, subtest))| vector_application::packs::CurriculumNode {
                objective_id: objective_id.clone(),
                subtest: subtest.clone(),
                title: format!("Objective {objective_id}"),
                prerequisites: if index == 0 {
                    Vec::new()
                } else {
                    vec![objectives[0].0.clone()]
                },
            },
        )
        .collect();
    let calibration: Vec<vector_application::packs::CalibrationEntry> = objectives
        .iter()
        .map(
            |(objective_id, _)| vector_application::packs::CalibrationEntry {
                objective_id: objective_id.clone(),
                expected_correct: 0.42,
                responses: 900,
                basis: "trial of 900 responses".to_string(),
            },
        )
        .collect();

    let document = build_pack(
        &db,
        &BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: RUNNING_VERSION,
            app_max: None,
            curriculum,
            calibration,
        },
        &key(),
    )
    .expect("build");
    let bytes = pack_bytes(&document).expect("serialize");
    install_pack(&db, &bytes, &trusted(), RUNNING_VERSION).expect("install");

    let packs = installed_packs(&db).expect("list");
    assert_eq!(packs.len(), 1);
    let listed = &packs[0];
    assert_eq!(listed.item_count as usize, document.items().len());
    assert_eq!(listed.objectives.len(), objectives.len());
    for objective in &listed.objectives {
        assert_eq!(objective.expected_correct, Some(0.42));
        assert_eq!(objective.responses, Some(900));
        assert_eq!(objective.basis.as_deref(), Some("trial of 900 responses"));
        assert_eq!(
            objective.title,
            format!("Objective {}", objective.objective_id)
        );
    }
    // The graph's edge survived, and the first objective is a root.
    let roots = listed
        .objectives
        .iter()
        .filter(|objective| objective.prerequisites.is_empty())
        .count();
    assert_eq!(roots, 1, "{:?}", listed.objectives);
}
