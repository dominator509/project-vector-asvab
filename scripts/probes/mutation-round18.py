"""Prove the content-reading rules are load-bearing.

A rule that no test notices is a rule that can be deleted in the next refactor. This script
disables one rule at a time in the source, runs the test that is supposed to catch it, and
requires that test to fail. A mutation whose test still passes is reported as NOT CAUGHT,
which means the rule is decoration.

The rules are the ones the corpus reader and the ingestion loops were given: the nineteen in
round 18 (what a purpose is, what a tool's name is, what damage is refused, one question asked
once, a refused item leaving no draft), the two in round 19 (a thin module not taking a run
down, and a run that produces nothing still failing), and the three in round 28 (a named
objective narrowing the servable read, an objective with no content falling back to its
subtest, and the interface actually asking for the objective the plan named), and the browser
rule in round 28 (the built bundle serves the objective's item rather than the subtest's first).

A rule enforced in two places is one rule: the mutation disables *every* site of it, because
disabling one site and watching the test still pass would say nothing about the rule.

Each mutation is applied to a copy of the file and restored afterwards, so the working tree
is left exactly as it was found. The script exits non-zero if any mutation went uncaught.

Run it alone. It edits the source in place, so any other test command running at the same time
compiles whatever mutation is applied at that moment and reports a failure that belongs to this
script, not to the code -- observed in round 28, when a concurrent `cargo test` failed on
`a_letter_from_another_alphabet_is_refused` because that rule was mutated at the time.

Usage:
    python3 scripts/probes/mutation-round18.py
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PURPOSES = ROOT / "crates/vector-questions/src/purposes.rs"
CONTENT = ROOT / "crates/vector-application/src/content.rs"
PERSISTENCE = ROOT / "crates/vector-persistence/src/content.rs"
PACK_REGISTRY = ROOT / "crates/vector-persistence/src/repo.rs"
BACKUP = ROOT / "crates/vector-persistence/src/backup.rs"
TOOLS = ROOT / "tools/vector-tools/src/content.rs"
PACKS = ROOT / "crates/vector-application/src/packs.rs"
SERVICE = ROOT / "crates/vector-application/src/service.rs"
DESKTOP = ROOT / "apps/desktop"
USE_PRACTICE_ITEMS = DESKTOP / "src/state/usePracticeItems.ts"
APP_TSX = DESKTOP / "src/App.tsx"
STUB_BACKEND = DESKTOP / "e2e/stubBackend.ts"

NAME_STOP_WORDS_IN_THE_NAME = """    if words
        .iter()
        .any(|word| NAME_STOP_WORDS.contains(&word.to_lowercase().as_str()))
    {
        return false;
    }"""

NAME_STOP_WORDS_IN_THE_SUBJECT = """    if chosen
        .iter()
        .any(|word| NAME_STOP_WORDS.contains(&word.to_lowercase().as_str()))
    {
        return None;
    }"""

# (id, file, edit or [edits], crate, test filter, what the rule is). An edit is
# (text to replace, replacement).
MUTATIONS = [
    (
        "closed-verb-list",
        PURPOSES,
        (
            "    if !PURPOSE_VERBS.contains(&first.as_str()) {\n        return false;\n    }",
            "    if false && !PURPOSE_VERBS.contains(&first.as_str()) {\n        return false;\n    }",
        ),
        "vector-questions",
        "an_adjective_is_not_a_purpose",
        "a purpose has to open with a verb the manuals use",
    ),
    (
        "connector-follows-the-word",
        PURPOSES,
        (
            """    purpose
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase()""",
            """    purpose
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase()""",
        ),
        "vector-questions",
        "a_comma_after_the_first_word_does_not_hide_its_ending",
        "punctuation is not part of the word that follows it",
    ),
    (
        "no-full-stop-in-a-purpose",
        PURPOSES,
        (
            "    if contains_full_stop(purpose) || purpose.contains(';') {\n        return false;\n    }",
            "    if false && (contains_full_stop(purpose) || purpose.contains(';')) {\n        return false;\n    }",
        ),
        "vector-questions",
        "a_purpose_does_not_span_a_sentence_break",
        "a purpose is a clause and holds no full stop",
    ),
    (
        "no-full-stop-behind-a-quote",
        PURPOSES,
        (
            """            .find(|c| !matches!(c, '”' | '"' | '’' | '\\'' | ')' | ']'))""",
            """            .find(|c| !matches!(c, '”' | '"' | '’' | '\\'' | ')' | ']'))
            .filter(|_| false)""",
        ),
        "vector-questions",
        "a_full_stop_hidden_by_a_closing_quote_still_ends_the_clause",
        "the stop survives the quotation mark in front of it",
    ),
    (
        "latin-letters-only-in-a-name",
        PURPOSES,
        (
            "    if !uses_only_latin(tool) {\n        return false;\n    }",
            "    if false && !uses_only_latin(tool) {\n        return false;\n    }",
        ),
        "vector-questions",
        "a_letter_from_another_alphabet_is_refused",
        "a tool's name is written in the Latin alphabet",
    ),
    (
        "latin-letters-only-in-a-purpose",
        PURPOSES,
        (
            "    if !uses_only_latin(purpose) {\n        return false;\n    }",
            "    if false && !uses_only_latin(purpose) {\n        return false;\n    }",
        ),
        "vector-questions",
        "a_letter_from_another_alphabet_is_refused",
        "a purpose is written in the Latin alphabet",
    ),
    (
        "no-categorising-head-noun",
        PURPOSES,
        (
            "    !NOT_A_TOOL_HEAD.contains(&words[words.len() - 1].to_lowercase().as_str())",
            "    true || !NOT_A_TOOL_HEAD.contains(&words[words.len() - 1].to_lowercase().as_str())",
        ),
        "vector-questions",
        "a_class_or_a_generalisation_is_not_a_tool",
        "a name headed by `type` names a class, not a tool",
    ),
    (
        "no-pronoun-in-a-name",
        PURPOSES,
        # The list is enforced at both ends of the pipeline: once where the subject is cut out
        # of the sentence, and once where the name is accepted. Disabling one leaves the other
        # standing, so the mutation disables the rule, both of its sites at once.
        [
            (
                NAME_STOP_WORDS_IN_THE_NAME,
                NAME_STOP_WORDS_IN_THE_NAME.replace("if words", "if false\n        && words"),
            ),
            (
                NAME_STOP_WORDS_IN_THE_SUBJECT,
                NAME_STOP_WORDS_IN_THE_SUBJECT.replace("if chosen", "if false\n        && chosen"),
            ),
        ],
        "vector-questions",
        "a_class_or_a_generalisation_is_not_a_tool",
        "a pronoun or a connective means the name is a clause",
    ),
    (
        "no-gerund-in-a-function-option",
        PURPOSES,
        (
            """            ItemKind::Function => self
                .entries
                .iter()
                .filter(|entry| !opens_with_a_gerund(&entry.purpose))
                .collect(),""",
            """            ItemKind::Function => self.entries.iter().collect(),""",
        ),
        "vector-questions",
        "a_gerund_description_is_not_asked_as_a_function",
        "an Auto Information option is an infinitive, not a gerund framed as one",
    ),
    (
        "no-scan-damage-in-prose",
        PURPOSES,
        (
            """    // Damage the scan left inside the sentence: see `carries_scan_damage`.
    if carries_scan_damage(purpose) {
        return false;
    }""",
            """    // Damage the scan left inside the sentence: see `carries_scan_damage`.
    if false && carries_scan_damage(purpose) {
        return false;
    }""",
        ),
        "vector-questions",
        "damage_the_scan_left_inside_a_sentence_is_refused",
        "the damage these scans leave inside a sentence is refused",
    ),
    (
        "no-possessive-in-a-name",
        PURPOSES,
        (
            """    if tool.contains(['\\'', '\\u{2019}']) {
        return false;
    }""",
            """    if false && tool.contains(['\\'', '\\u{2019}']) {
        return false;
    }""",
        ),
        "vector-questions",
        "a_class_or_a_generalisation_is_not_a_tool",
        "a possessive names a document, not an object",
    ),
    (
        "no-lost-space-in-a-compound",
        CONTENT,
        (
            """            for part in trimmed.split('-') {
                if part.len() >= 5 && comes_apart_into_two_words(dictionary, part) {
                    return Some(token.to_string());
                }
            }""",
            """            for part in trimmed.split('-') {
                let _ = part;
            }""",
        ),
        "vector-application",
        "a_lost_space_is_seen_in_the_prose_a_learner_reads",
        "a hyphenated compound can lose a space inside its parts",
    ),
    (
        "no-leading-boundary-word-exemption",
        PURPOSES,
        (
            "        .filter(|(index, _)| *index > 0)",
            "        .filter(|(_, _)| true)",
        ),
        "vector-questions",
        "a_name_that_starts_with_a_boundary_word_keeps_it",
        "a name is not ended by its own first word",
    ),
    (
        "no-question-dedupe",
        CONTENT,
        (
            """        ContentItemRepo::new(self.db).question_exists(subtest, stem, correct, passage)""",
            """        let _ = (subtest, stem, correct, passage);
        Ok(false)""",
        ),
        "vector-application",
        "two_editions_of_one_manual_do_not_ask_the_same_question_twice",
        "one question per stem and answer, whoever asked it first",
    ),
    (
        "no-draft-cleanup",
        PERSISTENCE,
        (
            """        self.db.connection().execute(
            "DELETE FROM content_items WHERE id = ?1 AND state = 'draft'",
            params![item_id],
        )?;
        Ok(())""",
            """        let _ = item_id;
        Ok(())""",
        ),
        "vector-application",
        "ingestion_refuses_a_module_the_vault_has_not_recorded",
        "a refused item leaves no draft behind",
    ),
    (
        "no-lost-space-in-prose",
        CONTENT,
        (
            """                has_joined_words(request.dictionary, &item.prompt).is_none()
                    && item
                        .options
                        .iter()
                        .all(|option| option_words_are_known(request.dictionary, option))""",
            """                item
                    .options
                    .iter()
                    .all(|option| option_words_are_known(request.dictionary, option))""",
        ),
        "vector-application",
        "a_purpose_with_a_lost_space_is_refused",
        "two words run together are refused",
    ),
    (
        "no-named-relation",
        PURPOSES,
        (
            "    describe_named_relation(sentence).or_else(|| describe_by_verb(sentence))",
            "    describe_by_verb(sentence)",
        ),
        "vector-questions",
        "a_relation_the_sentence_names_is_read_the_other_way_round",
        "a sentence that names the relation puts its subject after the phrase",
    ),
    (
        "no-gerund-subject-guard",
        PURPOSES,
        (
            "    if opens_with_a_gerund(subject) {\n        return None;\n    }",
            "    if false && opens_with_a_gerund(subject) {\n        return None;\n    }",
        ),
        "vector-questions",
        "a_relation_the_sentence_names_is_read_the_other_way_round",
        "a subject that opens with a gerund is an action, not a thing",
    ),
    (
        "no-per-manual-tolerance",
        TOOLS,
        (
            "        let report = match pipeline.ingest_purposes(&source, &request) {",
            "        let report = match Ok::<_, anyhow::Error>(pipeline.ingest_purposes(&source, &request)?) {",
        ),
        "vector-tools",
        "a_manual_that_describes_no_tool_does_not_take_the_run_down",
        "a manual the reader finds no tools in does not end the run",
    ),
    (
        "no-noun-frame",
        PURPOSES,
        (
            "        (\" serve as \", Link::As),\n        (\" serves as \", Link::As),\n        (\" act as \", Link::As),\n        (\" acts as \", Link::As),\n        (\" function as \", Link::As),\n        (\" functions as \", Link::As),",
            "        (\" serve as \", Link::To),\n        (\" serves as \", Link::To),\n        (\" act as \", Link::To),\n        (\" acts as \", Link::To),\n        (\" function as \", Link::To),\n        (\" functions as \", Link::To),",
        ),
        "vector-questions",
        "a_noun_complement_is_asked_what_the_component_serves_as",
        "a noun complement carries the noun link and is asked in its own frame",
    ),
    (
        "no-option-form-check",
        PURPOSES,
        (
            "    if !item\n        .options\n        .iter()\n        .all(|option| option_matches_link(option, link))\n    {",
            "    if false\n        && !item\n            .options\n            .iter()\n            .all(|option| option_matches_link(option, link))\n    {",
        ),
        "vector-questions",
        "verification_refuses_options_in_the_wrong_form",
        "an item's options are written in the form its question asks in",
    ),
    (
        "no-semicolon-rule",
        PURPOSES,
        (
            "    if contains_full_stop(purpose) || purpose.contains(';') {",
            "    if contains_full_stop(purpose) {",
        ),
        "vector-questions",
        "a_purpose_does_not_span_a_sentence_break",
        "a purpose holds no semicolon: it is one clause",
    ),
    (
        "no-measurement-head-rule",
        PURPOSES,
        (
            "    !NOT_A_TOOL_HEAD.contains(&words[words.len() - 1].to_lowercase().as_str())",
            "    true || !NOT_A_TOOL_HEAD.contains(&words[words.len() - 1].to_lowercase().as_str())",
        ),
        "vector-questions",
        "a_measurement_or_a_reference_back_is_not_a_name",
        "a measurement or a reference back is not a name",
    ),
    (
        "no-figure-label-rule",
        PURPOSES,
        (
            "        if bare.chars().count() == 1\n            && bare.chars().all(|c| c.is_lowercase())\n            && bare != \"a\"\n            && bare != \"i\"\n        {",
            "        if false {",
        ),
        "vector-questions",
        "a_measurement_or_a_reference_back_is_not_a_name",
        "a single letter in a sentence is a figure's label",
    ),
    (
        "no-new-relation-shapes",
        PURPOSES,
        (
            "        (\" are arranged to \", Link::To),\n        (\" is arranged to \", Link::To),",
            "        (\" are arranged to \", Link::To),",
        ),
        "vector-questions",
        "the_relation_shapes_the_sources_state_are_read",
        "the shapes the manuals state are read",
    ),
    (
        "no-servable-filter",
        SERVICE,
        (
            "            .filter(|estimate| self.can_serve(&estimate.subtest))",
            "            .filter(|_| true)",
        ),
        "vector-application",
        "a_plan_does_not_send_a_learner_to_an_empty_subtest",
        "a plan only schedules subtests the installation can serve",
    ),
    (
        "no-prerequisite-threshold",
        SERVICE,
        (
            "                        .filter(|prerequisite| score(prerequisite) < PREREQUISITE_MET)",
            "                        .filter(|_| false)",
        ),
        "vector-application",
        "objective_mastery_comes_from_the_attempts_on_that_objective",
        "an unmet prerequisite is named, so the objective it unlocks waits",
    ),
    (
        "no-objective-focus",
        SERVICE,
        (
            "    mastery\n        .iter()\n        .filter(|objective| objective.subtest == subtest && objective.waiting_on.is_empty())",
            "    mastery\n        .iter()\n        .filter(|objective| objective.subtest == subtest)",
        ),
        "vector-application",
        "a_plan_names_the_objective_to_work_on",
        "an objective whose prerequisite is unmet is not offered",
    ),
    (
        "no-prerequisite-ordering",
        SERVICE,
        (
            "    if pairs.is_empty() || drills.len() < 2 {",
            "    if true || pairs.is_empty() || drills.len() < 2 {",
        ),
        "vector-application",
        "a_plan_follows_the_curriculum_the_installed_pack_declares",
        "a plan follows the prerequisites the installed pack declares",
    ),
    (
        "no-pack-objectives",
        PACKS,
        (
            "    let Ok(payload) = serde_json::from_str::<PackPayload>(manifest_json) else {",
            "    if true {\n        return Vec::new();\n    }\n    let Ok(payload) = serde_json::from_str::<PackPayload>(manifest_json) else {",
        ),
        "vector-application",
        "an_installed_pack_reports_what_it_teaches",
        "an installed pack reports the objectives its manifest declares",
    ),
    (
        "no-curriculum-check",
        PACKS,
        (
            "    verify_curriculum(document)?;",
            "    let _ = verify_curriculum(document);",
        ),
        "vector-application",
        "a_pack_whose_item_teaches_an_undeclared_objective_is_refused",
        "an item's objective has to be declared in the curriculum",
    ),
    (
        "no-centred-derivation",
        PACKS,
        (
            "                expected_correct: 1.0 / (1.0 + (-mean).exp()),",
            "                expected_correct: -1.0,",
        ),
        "vector-application",
        "a_pack_built_without_a_curriculum_declares_what_it_teaches",
        "a derived calibration is a share between 0 and 1",
    ),
    (
        "no-source-kind",
        TOOLS,
        (
            "    let (url, licence) = match source.trim().to_lowercase().as_str() {",
            "    let (url, licence) = match \"archive\" {",
        ),
        "vector-tools",
        "a_tool_source_cites_the_page_its_bytes_came_from",
        "a source's kind decides the page its citation names",
    ),
    (
        "no-per-module-tolerance",
        TOOLS,
        (
            "        if glossary.is_empty() {\n            let reason = format!(",
            "        if glossary.is_empty() {\n            anyhow::bail!(\"no glossary entries\");\n            #[allow(unreachable_code)]\n            let reason = format!(",
        ),
        "vector-tools",
        "a_module_that_yields_nothing_does_not_take_the_run_down",
        "a module that contributes nothing does not end the run",
    ),
    (
        "no-run-level-guard",
        TOOLS,
        (
            """    if total_activated == 0 && total_already_present == 0 {
        let first = modules
            .iter()
            .find_map(|module| module.skipped.as_deref())
            .unwrap_or("no reason recorded");
        anyhow::bail!(
            "no module in this run produced an item: {} module(s) were skipped or refused; \\
             first reason: {first}",
            modules.iter().filter(|m| m.skipped.is_some()).count()
        );
    }""",
            """    let _ = (total_activated, total_already_present);""",
        ),
        "vector-tools",
        "a_run_in_which_no_module_produced_an_item_fails",
        "a run that produced nothing at all is still a failure",
    ),
    # Round 28: the plan names an objective, and practice has to serve it. The rule
    # spans three layers, so it gets three mutations -- a narrowing the store does
    # not apply, a fallback the pipeline does not take, and a filter the reader
    # never asks for would each look correct from outside their own layer.
    (
        "objective-narrowing-ignored",
        PERSISTENCE,
        (
            "AND (?2 IS NULL OR objective_id = ?2)",
            "AND (?2 IS NULL OR 1 = 1)",
        ),
        "vector-desktop",
        "a_named_objective_serves_its_own_items_and_not_a_neighbour",
        "a named objective narrows the servable read to its own items",
    ),
    (
        "objective-fallback-removed",
        CONTENT,
        (
            """        let items = repo.servable(subtest)?;
        Ok(pick(&items, seen).map(ItemDto::from))""",
            """        Ok(None)""",
        ),
        "vector-desktop",
        "an_objective_with_no_content_falls_back_to_the_subtest",
        "an objective with no content falls back to the whole subtest",
    ),
    # Round 29-30: the corpus the pack ships, the way back from a rollback, and a restore
    # over a database too damaged to open.
    (
        "generated-questions-not-deduplicated",
        CONTENT,
        (
            """            if self.content_hash_exists(&content_hash)?
                || self.question_is_stored(&item.subtest, &item.stem, &item.answer, None)?""",
            """            if self.content_hash_exists(&content_hash)?""",
        ),
        "vector-desktop",
        "a_generated_batch_asks_each_question_once",
        "a question a generated batch draws twice is asked once",
    ),
    (
        "activation-quarantines-the-active-version",
        PACK_REGISTRY,
        (
            """            conn.execute(
                "UPDATE content_packs SET status = 'superseded'
                 WHERE name = ?1 AND status = 'active'",
                params![name],
            )?;
            conn.execute(
                "UPDATE content_packs SET status = 'active' WHERE id = ?1",
                params![target.id],
            )?;""",
            """            conn.execute(
                "UPDATE content_packs SET status = 'quarantined'
                 WHERE name = ?1 AND status = 'active'",
                params![name],
            )?;
            conn.execute(
                "UPDATE content_packs SET status = 'active' WHERE id = ?1",
                params![target.id],
            )?;""",
        ),
        "vector-application",
        "an_activated_version_returns_to_service_and_a_reinstall_does_not",
        "the version an activation leaves behind can still be returned to",
    ),
    (
        "no-path-based-restore",
        BACKUP,
        (
            # The rule is "a store too damaged to open is still restorable". Displacing the
            # damaged file is not the rule -- on Windows a rename over an existing file
            # replaces it, so mutating that guard left the test passing and the mutation was
            # reported NOT CAUGHT until it was rewritten to refuse the restore outright.
            "        let remove_sidecars = || {",
            "        anyhow::bail!(\"the path-based restore is disabled\");\n        #[allow(unreachable_code)]\n        let remove_sidecars = || {",
        ),
        "vector-persistence",
        "a_truncated_store_is_restored_from_its_archive",
        "a store too damaged to open is still restorable from its archive",
    ),
]

# Rules enforced in the interface rather than in Rust. The same contract applies: the
# mutation disables the rule and the test that is supposed to depend on it must fail. The
# runner is vitest, so these are kept apart from the cargo list above -- one list per
# runner rather than a command string in every entry.
#
# (id, file, edit, test file, name filter, what the rule is).
FRONTEND_MUTATIONS = [
    (
        "objective-not-sent-to-the-backend",
        USE_PRACTICE_ITEMS,
        (
            "const item = await client.contentNext(subtest, seen, objectiveId);",
            "const item = await client.contentNext(subtest, seen, null);",
        ),
        "src/views/dataViews.test.tsx",
        "the plan's objective reaches practice",
        "the reader asks the backend for the objective the plan named",
    ),
    (
        "objective-not-carried-into-practice",
        APP_TSX,
        (
            "<PracticeContentView learnerId={profile?.id} request={practice} />",
            "<PracticeContentView learnerId={profile?.id} request={null} />",
        ),
        "src/views/dataViews.test.tsx",
        "the plan's objective reaches practice",
        "the shell carries the plan's request into the practice view",
    ),
]

# Rules the browser-level smoke spec relies on. Its runner rebuilds the bundle first, so the
# result is about the rule and not about whatever build happened to be lying around. Each list
# is checked against the runner that executes it: a cargo entry naming a crate that is not a
# workspace member, or a frontend entry naming a file that does not exist, stops the run.
#
# (id, file, edit, test file, name filter, what the rule is).
E2E_MUTATIONS = [
    (
        "e2e-stub-serves-the-whole-subtest",
        STUB_BACKEND,
        (
            "const candidates = narrowed.length > 0 ? narrowed : servable;",
            "const candidates = servable;",
        ),
        "e2e/smoke.spec.ts",
        "the plan's objective reaches the session it starts",
        "a session is served the objective's item, not the subtest's first",
    ),
]


CARGO_CRATES = {
    "vector-application",
    "vector-content",
    "vector-desktop",
    "vector-domain",
    "vector-llm",
    "vector-mcp",
    "vector-observability",
    "vector-persistence",
    "vector-platform",
    "vector-questions",
    "vector-repair",
    "vector-study",
    "vector-tools",
}


def test_passes(crate: str, test: str) -> bool:
    """Whether a Rust test still passes under a mutation.

    Refuses to answer when the command could not have run the test at all: a crate that is not
    a workspace member, or a filter that matched nothing, are configuration mistakes, and
    reporting either as a result is how a mutation ends up "caught" by a test that never ran.
    """
    if crate not in CARGO_CRATES:
        raise SystemExit(f"mutation names crate {crate!r}, which is not a workspace member")
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", crate, test],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    output = result.stdout + result.stderr
    if result.returncode != 0 and "could not compile" in output:
        return False
    # Both numbers, not just `passed`: a mutation that *is* caught makes its test fail, and a
    # failing binary reports "0 passed; 1 failed". Counting only passes would read the caught
    # case as "this filter matched nothing" and stop the run -- which it did, once, on the very
    # first mutation (`an_adjective_is_not_a_purpose`).
    results = re.findall(r"test result: \w+\. (\d+) passed; (\d+) failed", output)
    ran = sum(int(passed) + int(failed) for passed, failed in results)
    if not results or ran == 0:
        raise SystemExit(
            f"mutation filter {test!r} matched no test in {crate}: the rule cannot be judged"
        )
    return result.returncode == 0


def vitest_passes(test_file: str, name_filter: str) -> bool:
    """Whether a frontend test still passes, run the way the project runs it.

    Same refusal as `test_passes`: a file vitest cannot find, or a filter that selects
    nothing, must stop the run rather than count as a caught mutation. Two Rust mutations sat
    in the frontend list for a round and were reported "caught" by a runner that never loaded
    them, which is precisely the false confidence this harness exists to prevent.
    """
    npx = shutil.which("npx")
    if npx is None:
        raise SystemExit("npx is not on PATH; the interface mutations cannot be run")
    if not (DESKTOP / test_file).exists():
        raise SystemExit(f"mutation names {test_file!r}, which does not exist under apps/desktop")
    result = subprocess.run(
        [npx, "vitest", "run", test_file, "-t", name_filter],
        cwd=DESKTOP,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    output = result.stdout + result.stderr
    if "No test files found" in output:
        raise SystemExit(f"vitest found no test file at {test_file!r}")
    if re.search(r"Tests\s+no tests", output) or re.search(r"Tests\s+0 passed", output):
        raise SystemExit(
            f"mutation filter {name_filter!r} selected no test in {test_file}: "
            "the rule cannot be judged"
        )
    return result.returncode == 0


def apply(path: Path, edits: list[tuple[str, str]]) -> str | None:
    """Write the mutated file, returning the original text, or None if a pattern is absent.

    The file's own line endings are kept and the original bytes are returned so the restore is
    byte-exact. This matters: the first version read and wrote in text mode, which normalises
    CRLF to LF on read and expands LF back to CRLF on Windows writes. Every mutated file came
    back with different bytes than it went in with, and the sweep's format gate failed on two
    TypeScript files that prettier wants with LF -- a harness that edits the tree it is judging
    has to put it back exactly as it found it.
    """
    raw = path.read_bytes().decode("utf-8")
    normalized = raw.replace("\r\n", "\n")
    mutated = normalized
    for old, new in edits:
        if mutated.count(old) != 1:
            return None
        mutated = mutated.replace(old, new)
    if mutated == normalized:
        return None
    written = mutated.replace("\n", "\r\n") if "\r\n" in raw else mutated
    path.write_bytes(written.encode("utf-8"))
    return raw


def restore(path: Path, original: str) -> None:
    path.write_bytes(original.encode("utf-8"))


def all_mutations() -> list[tuple]:
    """Every mutation, from every runner, in one shape: (name, path, edits, rest...)."""
    combined: list[tuple] = []
    for entry in MUTATIONS + FRONTEND_MUTATIONS + E2E_MUTATIONS:
        name, path, edits, *rest = entry
        combined.append((name, path, edits, *rest))
    return combined


def leftovers() -> list[str]:
    """Mutation replacements already present in the working tree.

    A run that is killed -- by a timeout, by an interrupt, by the harness ending the turn --
    never reaches its `finally`, and the mutation it was applying stays in the source. That
    happened: two `if false && ...` guards sat in `purposes.rs` for a round, and they did not
    announce themselves as anything other than two tests failing for a reason that made no
    sense. Checking before mutating turns that into a refusal with a filename in it.
    """
    found: list[str] = []
    for (_name, path, edits, *_rest) in all_mutations():
        if isinstance(edits, tuple):
            edits = [edits]
        text = path.read_text(encoding="utf-8")
        for old, new in edits:
            # A leftover is the replacement *instead of* the original, not merely present:
            # several mutations delete one line from a pair, so the replacement is a substring
            # of the text they were applied to.
            if new and new in text and old not in text:
                found.append(f"{path.name}: {new.strip().splitlines()[0][:60]}")
    return found


def run_rust_mutations() -> int:
    uncaught = 0
    for (name, path, edits, crate, test, description) in MUTATIONS:
        if isinstance(edits, tuple):
            edits = [edits]
        original = apply(path, edits)
        if original is None:
            print(f"{name:<32} PATTERN NOT FOUND in {path.name}: mutation not applied")
            uncaught += 1
            continue
        try:
            caught = not test_passes(crate, test)
        finally:
            restore(path, original)
        if not caught:
            uncaught += 1
        print(f"{name:<32} {'caught' if caught else 'NOT CAUGHT':<11} {test}")
        print(f"    rule: {description}")
    return uncaught


def run_frontend_mutations() -> int:
    uncaught = 0
    for (name, path, edits, test_file, name_filter, description) in FRONTEND_MUTATIONS:
        if isinstance(edits, tuple):
            edits = [edits]
        original = apply(path, edits)
        if original is None:
            print(f"{name:<32} PATTERN NOT FOUND in {path.name}: mutation not applied")
            uncaught += 1
            continue
        try:
            caught = not vitest_passes(test_file, name_filter)
        finally:
            restore(path, original)
        if not caught:
            uncaught += 1
        print(f"{name:<32} {'caught' if caught else 'NOT CAUGHT':<11} {name_filter}")
        print(f"    rule: {description}")
    return uncaught


def run_e2e_mutations() -> int:
    uncaught = 0
    for (name, path, edits, test_file, name_filter, description) in E2E_MUTATIONS:
        if isinstance(edits, tuple):
            edits = [edits]
        original = apply(path, edits)
        if original is None:
            print(f"{name:<32} PATTERN NOT FOUND in {path.name}: mutation not applied")
            uncaught += 1
            continue
        try:
            caught = not playwright_passes(test_file, name_filter)
        finally:
            restore(path, original)
        if not caught:
            uncaught += 1
        print(f"{name:<32} {'caught' if caught else 'NOT CAUGHT':<11} {name_filter}")
        print(f"    rule: {description}")
    return uncaught


def playwright_passes(test_file: str, name_filter: str) -> bool:
    """Whether a browser-level spec still passes, against a bundle built from this source.

    The build is not optional: the spec runs against the production bundle, and without a
    build it would be judging whatever artifact happened to be lying in `dist` from an
    earlier round -- which is how a green E2E can describe code that no longer exists.
    """
    npx = shutil.which("npx")
    if npx is None:
        raise SystemExit("npx is not on PATH; the E2E mutation cannot be run")
    build = subprocess.run(
        [npx, "vite", "build"],
        cwd=DESKTOP,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if build.returncode != 0:
        raise SystemExit(f"the bundle did not build; the E2E mutation cannot be judged:\n{build.stderr[-2000:]}")
    result = subprocess.run(
        [npx, "playwright", "test", test_file, "-g", name_filter],
        cwd=DESKTOP,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return result.returncode == 0


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    stale = leftovers()
    if stale:
        print("a previous run left a mutation applied; restore the source before mutating:")
        for line in stale:
            print(f"  {line}")
        return 2
    uncaught = (
        run_rust_mutations() + run_frontend_mutations() + run_e2e_mutations()
    )
    print()
    if uncaught:
        print(f"{uncaught} mutation(s) went uncaught: the rule is not load-bearing")
        return 1
    total = len(MUTATIONS) + len(FRONTEND_MUTATIONS) + len(E2E_MUTATIONS)
    print(f"all {total} mutation(s) caught: every rule is load-bearing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
