#[cfg(test)]
mod tests {
    use super::super::mastery::MasteryState;
    use super::super::planner::{AdaptivePlanner, FsrsCard, FsrsState};
    use vector_domain::subtests::AsvabSubtest;

    #[test]
    fn test_fsrs_card_review_progression() {
        let mut card = FsrsCard::default();
        assert_eq!(card.reps, 0);
        assert_eq!(card.state, FsrsState::New);

        // Good review
        card.review(3);
        assert_eq!(card.reps, 1);
        assert_eq!(card.state, FsrsState::Review);
        assert!(card.scheduled_days >= 2);
    }

    #[test]
    fn test_adaptive_planner_allocates_time_by_weakness() {
        let mut m1 = MasteryState::new(AsvabSubtest::ArithmeticReasoning);
        m1.mastery_level = 0.2; // Weak
        let mut m2 = MasteryState::new(AsvabSubtest::WordKnowledge);
        m2.mastery_level = 0.9; // Strong

        let plan = AdaptivePlanner::generate_daily_plan(&[m1, m2], 60);
        assert_eq!(plan.len(), 2);
        // Arithmetic reasoning should get more minutes than word knowledge
        let ar_item = plan
            .iter()
            .find(|i| i.subtest == AsvabSubtest::ArithmeticReasoning)
            .unwrap();
        let wk_item = plan
            .iter()
            .find(|i| i.subtest == AsvabSubtest::WordKnowledge)
            .unwrap();
        assert!(ar_item.recommended_minutes > wk_item.recommended_minutes);
        assert_eq!(
            ar_item.recommended_minutes + wk_item.recommended_minutes,
            60
        );
    }
}
