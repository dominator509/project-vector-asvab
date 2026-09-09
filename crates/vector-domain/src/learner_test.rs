#[cfg(test)]
mod tests {
    use crate::profile::LearnerProfile;
    use crate::subtests::AsvabSubtest;
    use uuid::Uuid;

    #[test]
    fn test_asvab_subtests_modeled_separately() {
        let subtests = vec![
            AsvabSubtest::GeneralScience,
            AsvabSubtest::ArithmeticReasoning,
            AsvabSubtest::WordKnowledge,
            AsvabSubtest::ParagraphComprehension,
            AsvabSubtest::MathematicsKnowledge,
            AsvabSubtest::ElectronicsInformation,
            AsvabSubtest::AutoInformation,
            AsvabSubtest::ShopInformation,
            AsvabSubtest::MechanicalComprehension,
            AsvabSubtest::AssemblingObjects,
        ];

        assert_eq!(subtests.len(), 10);
        let afqt_count = subtests.iter().filter(|s| s.is_afqt()).count();
        assert_eq!(afqt_count, 4);
    }

    #[test]
    fn test_learner_profile_privacy_minimal() {
        let profile = LearnerProfile {
            id: Uuid::new_v4(),
            name: "LocalLearner".to_string(),
            target_score: 65,
        };
        assert_eq!(profile.target_score, 65);
    }
}
