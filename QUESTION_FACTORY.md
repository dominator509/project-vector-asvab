# Original question factory

## State machine
`DRAFT -> MACHINE_VALIDATED -> INDEPENDENT_VERIFIED -> CONTENT_REVIEWED -> ACTIVE`; any state may enter `QUARANTINED`.

## Pipeline
1. Select a versioned learning objective and evidence claims.
2. Build item spec: subtest, skill, prerequisite, cognitive operation, difficulty target, units/constants, answer form and misconception targets.
3. Draft an original stem/options without seeding from protected/current official question text.
4. Produce deterministic answer proof. Use executable/symbolic checks for math/physics/electronics when practical; source-backed rubric for verbal/science.
5. Generate distractors from named misconceptions.
6. Run duplicate/near-duplicate and contamination-similarity checks; quarantine suspicious similarity.
7. Independent verifier uses a distinct route and may not simply echo generator output.
8. Validate ambiguity, option uniqueness, units, language/accessibility and calculator-free method.
9. Assign provisional internal difficulty; later calibration requires adequate sample size/versioned statistics.
10. Human/content review high-impact items and sampled generated batches.
11. Sign pack manifest with item/source/generator/verifier/reviewer hashes and versions.

If a learner pastes a claimed real ASVAB question, default behavior stores neither recoverable text nor embedding and instead teaches the underlying concept using a new original example.

Never market psychometric equivalence to official ASVAB items without a validation study.
