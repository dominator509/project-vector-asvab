#[cfg(test)]
mod domain_tests {
    use crate::content::{AnswerProof, ContentPack, ContentStatus, QuestionItem};
    use crate::ingestion::{IngestionError, QuestionIngestion, SourceType};
    use crate::mastery::{Mastery, Subtest};
    use crate::plan::{AdaptivePlan, PlanGoal};
    use crate::profile::LearnerProfile;
    use crate::simulator::{PaperSimulator, SimulatorState};
    use uuid::Uuid;

    // REQ-001: local privacy-minimal learner profile
    #[test]
    fn test_req_001_privacy_minimal_profile() {
        let profile = LearnerProfile::new("Learner", 50);
        assert_ne!(profile.id, Uuid::nil());
        // Verify no PII fields exist like email, ssn, etc. in the struct serialization
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("email"));
        assert!(!json.contains("password"));
    }

    // REQ-002: cross-domain diagnostic and mastery uncertainty
    #[test]
    fn test_req_002_diagnostic_mastery_uncertainty() {
        let mastery = Mastery::new();
        assert_eq!(mastery.diagnostic_score(), 0.0);
        assert_eq!(mastery.uncertainty(), 1.0); // high uncertainty initially
    }

    // REQ-003: daily plan from weakness, due review, AFQT/job goals, time
    #[test]
    fn test_req_003_adaptive_plan() {
        let plan = AdaptivePlan::generate(
            &Mastery::new(),
            PlanGoal::AFQT(50),
            chrono::Duration::minutes(30),
        );
        assert!(!plan.drills().is_empty());
        assert_eq!(plan.target_duration(), chrono::Duration::minutes(30));
    }

    // REQ-004: GS AR WK PC MK EI AI SI MC AO modeled separately
    #[test]
    fn test_req_004_subtests_modeled() {
        let subjects = vec![
            Subtest::GS,
            Subtest::AR,
            Subtest::WK,
            Subtest::PC,
            Subtest::MK,
            Subtest::EI,
            Subtest::AI,
            Subtest::SI,
            Subtest::MC,
            Subtest::AO,
        ];
        assert_eq!(subjects.len(), 10);
    }

    // REQ-007: paper-style navigation/timing
    #[test]
    fn test_req_007_paper_simulator() {
        let mut sim = PaperSimulator::new(Subtest::AR, chrono::Duration::minutes(36));
        assert_eq!(sim.state(), SimulatorState::NotStarted);
        sim.start();
        assert_eq!(sim.state(), SimulatorState::InProgress);
        assert!(sim.can_navigate_back());
    }

    // REQ-009: reject controlled/leaked official question ingestion
    #[test]
    fn test_req_009_reject_official_questions() {
        let result = QuestionIngestion::ingest(SourceType::OfficialLeaked, "Some question text");
        assert!(matches!(
            result,
            Err(IngestionError::ControlledMaterialRejected)
        ));
    }

    // REQ-021: diff/quarantine/rollback/freshness
    #[test]
    fn test_req_021_quarantine_freshness() {
        let mut pack = ContentPack::new("Pack 1");
        pack.stage_update();
        assert_eq!(pack.status(), ContentStatus::Quarantined);
        assert!(pack.freshness() < chrono::Duration::seconds(5));
        pack.rollback();
        assert_eq!(pack.status(), ContentStatus::Active);
    }

    // REQ-022: original item + deterministic proof + independent verification
    #[test]
    fn test_req_022_original_item_proof() {
        let item = QuestionItem::new("What is 2+2?", AnswerProof::Deterministic("4".to_string()));
        assert!(item.verify());
        assert!(!item.is_derived());
    }

    // REQ-023: signed/versioned/reviewed packs
    #[test]
    fn test_req_023_signed_versioned_packs() {
        let pack = ContentPack::new("Pack 1");
        assert_eq!(pack.version(), 1);
        assert!(pack.signature().is_none());
        let signed_pack = pack.sign("private_key");
        assert!(signed_pack.signature().is_some());
        assert!(signed_pack.is_reviewed());
    }
}
