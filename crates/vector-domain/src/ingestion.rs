use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    OfficialLeaked,
    PublicDomain,
    Generated,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IngestionError {
    ControlledMaterialRejected,
    ParseError,
}

pub struct QuestionIngestion;

impl QuestionIngestion {
    pub fn ingest(source: SourceType, _text: &str) -> Result<(), IngestionError> {
        if source == SourceType::OfficialLeaked {
            return Err(IngestionError::ControlledMaterialRejected);
        }
        Ok(())
    }
}
