use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AsvabSubtest {
    GeneralScience,          // GS
    ArithmeticReasoning,     // AR
    WordKnowledge,           // WK
    ParagraphComprehension,  // PC
    MathematicsKnowledge,    // MK
    ElectronicsInformation,  // EI
    AutoInformation,         // AI
    ShopInformation,         // SI
    MechanicalComprehension, // MC
    AssemblingObjects,       // AO
}

impl AsvabSubtest {
    pub fn code(&self) -> &'static str {
        match self {
            Self::GeneralScience => "GS",
            Self::ArithmeticReasoning => "AR",
            Self::WordKnowledge => "WK",
            Self::ParagraphComprehension => "PC",
            Self::MathematicsKnowledge => "MK",
            Self::ElectronicsInformation => "EI",
            Self::AutoInformation => "AI",
            Self::ShopInformation => "SI",
            Self::MechanicalComprehension => "MC",
            Self::AssemblingObjects => "AO",
        }
    }

    pub fn is_afqt(&self) -> bool {
        matches!(
            self,
            Self::ArithmeticReasoning
                | Self::WordKnowledge
                | Self::ParagraphComprehension
                | Self::MathematicsKnowledge
        )
    }
}
