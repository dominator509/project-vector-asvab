#[cfg(test)]
mod tests {
    use super::super::exam::{ExamMode, ExamState};
    use vector_domain::subtests::AsvabSubtest;

    #[test]
    fn test_cat_no_backtracking_enforcement() {
        let mut exam = ExamState::new(ExamMode::Cat, AsvabSubtest::ArithmeticReasoning, 5, 600);
        assert_eq!(exam.current_item_index, 0);

        // Submit first item
        assert!(exam.submit_answer(0, 1).is_ok());
        assert_eq!(exam.current_item_index, 1);

        // Attempting to backtrack or re-submit item 0 in CAT mode must fail
        assert!(exam.navigate_previous().is_err());
        assert!(exam.submit_answer(0, 2).is_err());
    }

    #[test]
    fn test_paper_mode_allows_navigation_and_review() {
        let mut exam = ExamState::new(ExamMode::Paper, AsvabSubtest::WordKnowledge, 5, 600);
        assert!(exam.submit_answer(0, 1).is_ok());

        // Navigation back is allowed in Paper mode
        assert!(exam.navigate_previous().is_ok());
        assert_eq!(exam.current_item_index, 0);

        // Re-submitting in Paper mode is allowed
        assert!(exam.submit_answer(0, 3).is_ok());
        assert_eq!(exam.answered_items[0], Some(3));
    }
}
