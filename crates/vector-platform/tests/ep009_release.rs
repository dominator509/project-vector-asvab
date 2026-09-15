//! EP-009 acceptance: artifact identity (REQ-037, REQ-053) and SBOM (REQ-041).

use std::path::PathBuf;

use vector_platform::release::{
    ArtifactComponent, ArtifactIdentity, IdentityError, MeasuredComponent, Sbom, SbomEntry,
    SbomError, Sha256Digest, PERMITTED_LICENSES,
};

fn component(kind: ArtifactComponent, text: &str) -> MeasuredComponent {
    MeasuredComponent {
        component: kind,
        digest: Sha256Digest::of_text(text),
        bytes: Some(text.len() as u64),
    }
}

/// A complete component set covering every bound component.
fn complete_set() -> Vec<MeasuredComponent> {
    ArtifactComponent::ALL
        .iter()
        .map(|kind| component(*kind, &format!("content-of-{}", kind.label())))
        .collect()
}

// ---------------------------------------------------------------------------
// REQ-053: identity binds every component
// ---------------------------------------------------------------------------

#[test]
fn an_identity_binds_every_required_component() {
    let identity = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");
    for required in ArtifactComponent::ALL {
        assert!(
            identity.component_kinds().contains(required),
            "identity must bind {}",
            required.label()
        );
    }
}

#[test]
fn a_missing_component_is_a_hard_error() {
    // An earlier implementation substituted the literal "MISSING" for an absent
    // file and hashed that, so a manifest built from nothing still produced a
    // confident-looking digest.
    let mut set = complete_set();
    set.retain(|c| c.component != ArtifactComponent::Sbom);

    assert_eq!(
        ArtifactIdentity::build("0.1.0", set),
        Err(IdentityError::MissingComponent(ArtifactComponent::Sbom))
    );
}

#[test]
fn an_empty_component_is_refused() {
    // A present-but-empty component binds nothing while appearing to bind
    // something, so an absent digest must be a hard error.
    let mut set = complete_set();
    for item in set.iter_mut() {
        if item.component == ArtifactComponent::Content {
            item.digest = Sha256Digest::default_empty();
        }
    }
    assert_eq!(
        ArtifactIdentity::build("0.1.0", set),
        Err(IdentityError::EmptyComponent(ArtifactComponent::Content))
    );
}

#[test]
fn the_identity_digest_changes_when_any_component_changes() {
    // This is what makes the identity meaningful: a change to any bound
    // component must invalidate it.
    let baseline = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");

    for kind in ArtifactComponent::ALL {
        let mut set = complete_set();
        for item in set.iter_mut() {
            if item.component == *kind {
                item.digest = Sha256Digest::of_text("tampered");
            }
        }
        let changed = ArtifactIdentity::build("0.1.0", set).expect("builds");
        assert_ne!(
            baseline.identity_digest,
            changed.identity_digest,
            "changing {} must change the identity digest",
            kind.label()
        );
    }
}

#[test]
fn the_identity_digest_does_not_depend_on_component_order() {
    // Two callers measuring the same artifact must get the same identity.
    let forward = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");

    let mut reversed = complete_set();
    reversed.reverse();
    let backward = ArtifactIdentity::build("0.1.0", reversed).expect("builds");

    assert_eq!(forward.identity_digest, backward.identity_digest);
}

#[test]
fn an_identical_artifact_produces_an_identical_identity() {
    let a = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");
    let b = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");
    assert_eq!(a, b, "identity must be reproducible");
}

#[test]
fn verification_detects_a_tampered_file() {
    let dir = std::env::temp_dir().join(format!("vec-rel-tamper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    let binary = dir.join("app.bin");
    std::fs::write(&binary, b"original").expect("write");

    let identity = ArtifactIdentity {
        components: complete_set(),
        identity_digest: ArtifactIdentity::derive_digest(&complete_set()),
        version: "0.1.0".to_string(),
    };

    // Replace the binary content with something else.
    std::fs::write(&binary, b"tampered").expect("write");

    let result = identity.verify_against(&[(ArtifactComponent::Binary, binary.clone())]);
    assert!(
        matches!(result, Err(IdentityError::DigestMismatch { .. })),
        "a tampered file must fail verification, got {result:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verification_detects_a_missing_file() {
    let identity = ArtifactIdentity {
        components: complete_set(),
        identity_digest: ArtifactIdentity::derive_digest(&complete_set()),
        version: "0.1.0".to_string(),
    };
    let missing = PathBuf::from("definitely-not-a-real-artifact-path");
    assert!(matches!(
        identity.verify_against(&[(ArtifactComponent::Binary, missing)]),
        Err(IdentityError::ArtifactMissing(_))
    ));
}

#[test]
fn verification_detects_a_mismatched_binding_digest() {
    // The components are intact but the recorded binding digest is wrong, which
    // is what a forged manifest looks like.
    let mut identity = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");
    identity.identity_digest = Sha256Digest::of_text("forged");

    assert!(matches!(
        identity.verify_against(&[]),
        Err(IdentityError::BindingMismatch { .. })
    ));
}

#[test]
fn a_text_digest_ignores_line_ending_style() {
    // Otherwise an artifact built on Windows could not be verified on Linux.
    assert_eq!(
        Sha256Digest::of_text("a\nb"),
        Sha256Digest::of_text("a\r\nb")
    );
}

#[test]
fn measuring_a_real_file_records_its_size() {
    let dir = std::env::temp_dir().join(format!("vec-rel-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("thing.bin");
    std::fs::write(&file, b"0123456789").expect("write");

    let measured =
        ArtifactIdentity::measure_file(ArtifactComponent::Binary, &file).expect("measure");
    assert_eq!(measured.bytes, Some(10));
    assert_eq!(measured.digest, Sha256Digest::of(b"0123456789"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_identity_round_trips_through_serde() {
    let identity = ArtifactIdentity::build("0.1.0", complete_set()).expect("builds");
    let json = serde_json::to_string(&identity).expect("serialize");
    let back: ArtifactIdentity = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(identity, back);
}

// ---------------------------------------------------------------------------
// REQ-041: SBOM and license policy
// ---------------------------------------------------------------------------

fn entry(name: &str, license: Option<&str>) -> SbomEntry {
    SbomEntry {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        license: license.map(str::to_string),
        ecosystem: "cargo".to_string(),
    }
}

#[test]
fn a_permissive_sbom_validates() {
    let sbom = Sbom {
        entries: vec![
            entry("serde", Some("MIT OR Apache-2.0")),
            entry("sha2", Some("MIT OR Apache-2.0")),
            entry("rusqlite", Some("MIT")),
        ],
    };
    assert!(sbom.validate().is_ok());
}

#[test]
fn an_empty_sbom_is_refused() {
    assert_eq!(Sbom { entries: vec![] }.validate(), Err(SbomError::Empty));
}

#[test]
fn an_unstated_license_is_refused() {
    // Unstated terms are not permission, the same rule ingestion applies.
    let sbom = Sbom {
        entries: vec![entry("mystery", None)],
    };
    assert_eq!(
        sbom.validate(),
        Err(SbomError::UnknownLicense("mystery".to_string()))
    );

    let blank = Sbom {
        entries: vec![entry("blank", Some("   "))],
    };
    assert!(matches!(
        blank.validate(),
        Err(SbomError::UnknownLicense(_))
    ));
}

#[test]
fn a_copyleft_license_is_refused_for_commercial_distribution() {
    // A license that would impose obligations on VECTOR's own source is a
    // blocking question for a human, not something to wave through.
    let sbom = Sbom {
        entries: vec![entry("gpl-thing", Some("GPL-3.0-only"))],
    };
    assert_eq!(
        sbom.validate(),
        Err(SbomError::IncompatibleLicense {
            name: "gpl-thing".to_string(),
            license: "GPL-3.0-only".to_string(),
        })
    );
}

#[test]
fn the_permitted_list_excludes_strong_copyleft() {
    for forbidden in ["GPL-3.0-only", "AGPL-3.0-only", "SSPL-1.0", "BUSL-1.1"] {
        assert!(
            !PERMITTED_LICENSES.contains(&forbidden),
            "{forbidden} must not be a permitted release license"
        );
    }
}

#[test]
fn attribution_licenses_are_permitted_but_reported() {
    // CC-BY-4.0 is permissive but carries a notice obligation. Admitting it
    // silently would hide a duty the release has to discharge.
    let sbom = Sbom {
        entries: vec![
            entry("caniuse-lite", Some("CC-BY-4.0")),
            entry("serde", Some("MIT")),
        ],
    };
    sbom.validate()
        .expect("CC-BY-4.0 is permitted with attribution");

    let attributed = sbom.attribution_required();
    assert_eq!(attributed.len(), 1);
    assert_eq!(attributed[0].name, "caniuse-lite");
}

#[test]
fn a_copyleft_license_is_not_hidden_by_the_attribution_path() {
    let sbom = Sbom {
        entries: vec![entry("gpl", Some("GPL-3.0-only"))],
    };
    assert!(sbom.validate().is_err());
    assert!(sbom.attribution_required().is_empty());
}

#[test]
fn notices_list_every_dependency_and_flag_unstated_licenses() {
    let sbom = Sbom {
        entries: vec![
            entry("alpha", Some("MIT")),
            entry("beta", None),
            entry("gamma", Some("CC-BY-4.0")),
        ],
    };
    let notices = sbom.render_notices();

    for name in ["alpha", "beta", "gamma"] {
        assert!(notices.contains(name), "notices must list {name}");
    }
    assert!(
        notices.contains("LICENSE NOT DECLARED"),
        "an unstated license must be visible, not omitted"
    );
    assert!(
        notices.contains("Attribution-required licenses"),
        "the attribution duty must be called out"
    );
}

#[test]
fn the_licenses_in_use_are_reported_distinctly() {
    let sbom = Sbom {
        entries: vec![
            entry("a", Some("MIT")),
            entry("b", Some("MIT")),
            entry("c", Some("Apache-2.0")),
        ],
    };
    assert_eq!(
        sbom.licenses(),
        vec!["Apache-2.0".to_string(), "MIT".to_string()]
    );
}

#[test]
fn spdx_rendering_is_deterministic_and_ordered() {
    let sbom = Sbom {
        entries: vec![entry("zeta", Some("MIT")), entry("alpha", Some("MIT"))],
    };
    let first = sbom.render_spdx("vector", "0.1.0");
    let second = sbom.render_spdx("vector", "0.1.0");
    assert_eq!(first, second, "SBOM rendering must be reproducible");

    let alpha_at = first.find("alpha").expect("alpha present");
    let zeta_at = first.find("zeta").expect("zeta present");
    assert!(
        alpha_at < zeta_at,
        "entries must be sorted for a stable digest"
    );
}

#[test]
fn spdx_rendering_includes_the_document_header_and_licenses() {
    let sbom = Sbom {
        entries: vec![entry("serde", Some("MIT"))],
    };
    let rendered = sbom.render_spdx("vector", "0.1.0");
    assert!(rendered.contains("SPDXVersion: SPDX-2.3"));
    assert!(rendered.contains("DocumentName: vector-0.1.0"));
    assert!(rendered.contains("PackageLicenseDeclared: MIT"));
    assert!(rendered.contains("PackageName: serde"));
}

#[test]
fn spdx_rendering_is_stable_under_entry_reordering() {
    let forward = Sbom {
        entries: vec![entry("a", Some("MIT")), entry("b", Some("Apache-2.0"))],
    };
    let mut reversed_entries = forward.entries.clone();
    reversed_entries.reverse();
    let reversed = Sbom {
        entries: reversed_entries,
    };
    assert_eq!(
        forward.render_spdx("vector", "0.1.0"),
        reversed.render_spdx("vector", "0.1.0"),
        "the SBOM digest must not depend on input order"
    );
}

// ---------------------------------------------------------------------------
// Directory components (REQ-053)
//
// Migrations and content packs are sets of files. Binding one representative
// file would mean adding a migration did not change the artifact identity, so
// these tests pin the properties a directory digest must have.
// ---------------------------------------------------------------------------

/// A disposable directory tree.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "vector-identity-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create scratch");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write");
    }

    fn path(&self) -> &std::path::Path {
        &self.root
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_directory_digest_covers_every_file_in_it() {
    let scratch = Scratch::new("covers");
    scratch.write("001_initial.sql", "CREATE TABLE a (id TEXT);\n");
    scratch.write("002_more.sql", "CREATE TABLE b (id TEXT);\n");

    let before = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
        .expect("measure directory");
    assert_eq!(before.bytes, Some(52), "both files are counted");

    // Adding a file must change the digest; this is the whole reason a
    // directory is measured rather than one representative file.
    scratch.write("003_more.sql", "CREATE TABLE c (id TEXT);\n");
    let after = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
        .expect("measure again");
    assert_ne!(before.digest, after.digest);
}

#[test]
fn a_directory_digest_does_not_depend_on_enumeration_order() {
    let first = Scratch::new("order-a");
    let second = Scratch::new("order-b");
    for scratch in [&first, &second] {
        scratch.write("b/second.sql", "SELECT 2;\n");
        scratch.write("a/first.sql", "SELECT 1;\n");
        scratch.write("top.sql", "SELECT 0;\n");
    }

    let a = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, first.path())
        .expect("measure a");
    let b = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, second.path())
        .expect("measure b");
    assert_eq!(
        a.digest, b.digest,
        "the digest must depend on the set of files, not on read order"
    );
}

#[test]
fn a_directory_digest_ignores_line_ending_differences() {
    // The same rule the text digest follows: an artifact built on Windows must
    // verify on a machine that checks the same content out with LF.
    let lf = Scratch::new("lf");
    let crlf = Scratch::new("crlf");
    lf.write("001.sql", "CREATE TABLE a (id TEXT);\nSELECT 1;\n");
    crlf.write("001.sql", "CREATE TABLE a (id TEXT);\r\nSELECT 1;\r\n");

    let a = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, lf.path()).expect("lf");
    let b =
        ArtifactIdentity::measure_path(ArtifactComponent::Migrations, crlf.path()).expect("crlf");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn a_renamed_file_changes_the_digest() {
    // The relative path is part of the digest, so renaming a migration is a
    // change rather than a no-op.
    let scratch = Scratch::new("rename");
    scratch.write("001_initial.sql", "SELECT 1;\n");
    let before = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
        .expect("before");

    std::fs::rename(
        scratch.path().join("001_initial.sql"),
        scratch.path().join("001_renamed.sql"),
    )
    .expect("rename");

    let after = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
        .expect("after");
    assert_ne!(before.digest, after.digest);
}

#[test]
fn an_empty_directory_is_refused_rather_than_measured() {
    // A digest of nothing would let an absent component look present.
    let scratch = Scratch::new("empty");
    let error = ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
        .expect_err("an empty directory must not measure");
    assert!(
        matches!(error, IdentityError::ArtifactMissing(_)),
        "got {error:?}"
    );
}

#[test]
fn a_file_still_measures_through_the_same_entry_point() {
    let scratch = Scratch::new("file");
    scratch.write("single.sql", "SELECT 1;\n");
    let measured = ArtifactIdentity::measure_path(
        ArtifactComponent::Migrations,
        &scratch.path().join("single.sql"),
    )
    .expect("measure a file");
    assert!(measured.digest.to_string().len() == 64);
    assert_eq!(measured.bytes, Some(10));
}

#[test]
fn verification_re_derives_a_directory_component_from_disk() {
    let scratch = Scratch::new("verify-dir");
    scratch.write("001.sql", "SELECT 1;\n");
    scratch.write("002.sql", "SELECT 2;\n");

    let identity = ArtifactIdentity::build(
        "0.1.0",
        vec![
            ArtifactIdentity::measure_path(ArtifactComponent::Migrations, scratch.path())
                .expect("measure"),
            component(ArtifactComponent::Binary, "binary"),
            component(ArtifactComponent::Content, "content"),
            component(ArtifactComponent::Sbom, "sbom"),
            component(ArtifactComponent::Licenses, "licenses"),
            MeasuredComponent {
                component: ArtifactComponent::GitSha,
                digest: Sha256Digest::of_text("deadbeef"),
                bytes: None,
            },
        ],
    )
    .expect("build");

    identity
        .verify_against(&[(ArtifactComponent::Migrations, scratch.path().to_path_buf())])
        .expect("an unchanged directory verifies");

    // Changing one file inside the directory must break verification.
    scratch.write("002.sql", "SELECT 3;\n");
    let error = identity
        .verify_against(&[(ArtifactComponent::Migrations, scratch.path().to_path_buf())])
        .expect_err("a changed file must be detected");
    assert!(
        matches!(error, IdentityError::DigestMismatch { .. }),
        "got {error:?}"
    );
}
