use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vector_domain::subtests::AsvabSubtest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionTemplate {
    pub id: String,
    pub subtest: AsvabSubtest,
    pub stem: String,
    pub options: Vec<String>,
    pub correct_option_index: usize,
    pub explanation: String,
    pub is_official_leaked: bool, // REQ-009 safety flag
}

impl QuestionTemplate {
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(self.stem.as_bytes());
        for opt in &self.options {
            hasher.update(opt.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn validate_answer_proof(&self, selected_index: usize) -> bool {
        selected_index == self.correct_option_index
    }
}

pub struct QuestionIngestionValidator;

impl QuestionIngestionValidator {
    pub fn validate_and_quarantine(template: &QuestionTemplate) -> Result<(), &'static str> {
        if template.is_official_leaked {
            return Err("REJECTED_LEAKED_OFFICIAL_QUESTION: Protected/leaked ASVAB question text is strictly forbidden.");
        }
        if template.options.len() < 2 {
            return Err("INVALID_QUESTION_FORMAT: At least 2 options required.");
        }
        if template.correct_option_index >= template.options.len() {
            return Err("INVALID_QUESTION_FORMAT: Correct index out of bounds.");
        }
        Ok(())
    }
}
