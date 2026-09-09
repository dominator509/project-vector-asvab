#[cfg(test)]
mod tests {
    use super::super::mastery::{MasteryState, SkillObservation};
    use vector_domain::subtests::AsvabSubtest;

    #[test]
    fn test_diagnostic_mastery_uncertainty_reduction() {
        let mut state = MasteryState::new(AsvabSubtest::ArithmeticReasoning);
        assert_eq!(state.uncertainty, 0.5);

        for _ in 0..10 {
            state.update(&SkillObservation {
                subtest: AsvabSubtest::ArithmeticReasoning,
                is_correct: true,
                response_time_ms: 15000,
                timestamp: "2026-09-09T01:00:00Z".to_string(),
            });
        }

        assert!(state.mastery_level > 0.8);
        assert!(state.uncertainty < 0.2);
    }
}
