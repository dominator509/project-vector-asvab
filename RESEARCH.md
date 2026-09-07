# Research synthesis - 2026-08-28

Canonical URLs are in `SOURCES.csv`.

## ASVAB facts that become data contracts
Official ASVAB documentation currently describes ten computer subtests: GS, AR, WK, PC, MK, EI, AI, SI, MC and AO. Paper administration combines Auto and Shop. CAT-ASVAB is adaptive, has subtest-specific public timing/question profiles, and calculators are currently not allowed. These facts are stored in signed/versioned source packs rather than scattered constants.

Official applicant guidance states ASVAB questions are controlled testing materials. VECTOR therefore does not harvest or reproduce leaked/current protected items. It creates original practice material from public objectives and commercially compatible sources.

AFQT is driven by the verbal/math subtests. Branch composites and job requirements are useful but change; the UI must show source version/freshness and require recruiter confirmation when the source is stale or the rule is not authoritative enough.

## Competitor baseline
Current prep products commonly offer large question banks, explanations, subject drills, mocks, daily goals/streaks, smart review, score/readiness estimates and AI mentors. VECTOR includes those useful capabilities, but its moat is provenance, offline-first personalization, calibrated uncertainty, original-item validation, source freshness, local/provider AI portability, MCP, and crash-to-PR engineering.

## Provider conclusion
A universal subscription-token broker is rejected. VECTOR launches or speaks only documented interfaces of installed official clients. xAI documents headless Grok CLI and ACP. Codex supports ChatGPT-plan sign-in and noninteractive `exec`. Claude Code supports subscription login/noninteractive use but remains terms-gated for third-party invocation. Google explicitly says Gemini CLI OAuth may not be used as a third-party bridge, so that route is disabled.

## Notebook conclusion
VECTOR's Evidence Vault is canonical. Gemini Notebook is an export/manual-import research companion. Gemini Notebook Enterprise may be integrated only through its documented licensed API behind an explicit cost/terms gate.

## Open-source conclusion
Prefer permissive components instead of inheriting a large LMS: Tauri/Rust, SQLite, llama.cpp, fsrs-rs, official Rust MCP SDK and GitHub CLI are current candidates. Exact versions, transitive licenses, model weights and content are independently audited before distribution.
