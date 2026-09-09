use serde::{Deserialize, Serialize};
use vector_domain::subtests::AsvabSubtest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExamMode {
    Cat,   // Computer Adaptive Test (no backtracking)
    Paper, // Paper-style (backtracking allowed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExamState {
    pub mode: ExamMode,
    pub subtest: AsvabSubtest,
    pub current_item_index: usize,
    pub total_items: usize,
    pub time_remaining_seconds: u32,
    pub answered_items: Vec<Option<usize>>,
}

impl ExamState {
    pub fn new(
        mode: ExamMode,
        subtest: AsvabSubtest,
        total_items: usize,
        duration_seconds: u32,
    ) -> Self {
        Self {
            mode,
            subtest,
            current_item_index: 0,
            total_items,
            time_remaining_seconds: duration_seconds,
            answered_items: vec![None; total_items],
        }
    }

    pub fn submit_answer(
        &mut self,
        item_index: usize,
        answer_index: usize,
    ) -> Result<(), &'static str> {
        if self.mode == ExamMode::Cat && item_index != self.current_item_index {
            return Err("CAT_NO_BACKTRACK_VIOLATION: Cannot answer non-current item in CAT mode.");
        }
        if self.mode == ExamMode::Cat && self.answered_items[item_index].is_some() {
            return Err(
                "CAT_NO_BACKTRACK_VIOLATION: Cannot modify already submitted item in CAT mode.",
            );
        }

        self.answered_items[item_index] = Some(answer_index);
        if self.current_item_index < self.total_items - 1 {
            self.current_item_index += 1;
        }

        Ok(())
    }

    pub fn navigate_previous(&mut self) -> Result<(), &'static str> {
        if self.mode == ExamMode::Cat {
            return Err(
                "CAT_NO_BACKTRACK_VIOLATION: Navigation to previous item prohibited in CAT mode.",
            );
        }
        if self.current_item_index > 0 {
            self.current_item_index -= 1;
        }
        Ok(())
    }
}
