# SPEC-001 Core Domain

Entities: LearnerProfile, GoalProfile, DomainSkill, SkillObservation, StudyPlan, StudySession, QuestionTemplate, QuestionInstance, DistractorRationale, Explanation, ContentPack, EvidenceSource, SourceClaim, MasteryState, ReviewSchedule, PracticeExam, ExamAttempt, ReadinessEstimate, CompositeTarget, ProviderTransport, ModelRun, CrashBundle, RepairCase.

Invariants: question answers are deterministic from an answer specification; generated variants are validated independently of the generating model; mastery updates retain prior observation provenance; FSRS scheduling stores parameters and review history; official-policy claims carry effective/fetched dates; content packs are signed and versioned.