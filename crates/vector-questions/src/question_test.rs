#[cfg(test)]
mod tests {
    use super::super::question::{QuestionIngestionValidator, QuestionTemplate};
    use vector_domain::subtests::AsvabSubtest;

    #[test]
    fn test_question_deterministic_answer_proof() {
        let q = QuestionTemplate {
            id: "q-101".to_string(),
            subtest: AsvabSubtest::ArithmeticReasoning,
            stem: "If 3 x = 12, what is x?".to_string(),
            options: vec![
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
            ],
            correct_option_index: 2,
            explanation: "Divide 12 by 3 to get 4.".to_string(),
            is_official_leaked: false,
        };

        assert!(q.validate_answer_proof(2));
        assert!(!q.validate_answer_proof(0));
        assert_eq!(q.compute_hash().len(), 64);
    }

    #[test]
    fn test_rejects_controlled_leaked_official_question_ingestion() {
        let leaked_q = QuestionTemplate {
            id: "leaked-01".to_string(),
            subtest: AsvabSubtest::GeneralScience,
            stem: "Official leaked ASVAB item text".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            correct_option_index: 0,
            explanation: "Leaked explanation".to_string(),
            is_official_leaked: true,
        };

        let result = QuestionIngestionValidator::validate_and_quarantine(&leaked_q);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("REJECTED_LEAKED_OFFICIAL_QUESTION"));
    }
}
