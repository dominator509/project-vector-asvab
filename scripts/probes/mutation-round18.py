"""Prove the round-18 reading rules are load-bearing.

A rule that no test notices is a rule that can be deleted in the next refactor. This script
disables one rule at a time in the source, runs the test that is supposed to catch it, and
requires that test to fail. A mutation whose test still passes is reported as NOT CAUGHT,
which means the rule is decoration.

A rule enforced in two places is one rule: the mutation disables *every* site of it, because
disabling one site and watching the test still pass would say nothing about the rule.

Each mutation is applied to a copy of the file and restored afterwards, so the working tree
is left exactly as it was found. The script exits non-zero if any mutation went uncaught.

Usage:
    python3 scripts/probes/mutation-round18.py
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PURPOSES = ROOT / "crates/vector-questions/src/purposes.rs"
CONTENT = ROOT / "crates/vector-application/src/content.rs"
PERSISTENCE = ROOT / "crates/vector-persistence/src/content.rs"

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
            "    if contains_full_stop(purpose) {\n        return false;\n    }",
            "    if false && contains_full_stop(purpose) {\n        return false;\n    }",
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
]


def test_passes(crate: str, test: str) -> bool:
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", crate, test],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return result.returncode == 0


def apply(path: Path, edits: list[tuple[str, str]]) -> str | None:
    """Write the mutated file, returning the original text, or None if a pattern is absent."""
    original = path.read_text(encoding="utf-8")
    mutated = original
    for old, new in edits:
        if mutated.count(old) != 1:
            return None
        mutated = mutated.replace(old, new)
    if mutated == original:
        return None
    path.write_text(mutated, encoding="utf-8")
    return original


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
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
            path.write_text(original, encoding="utf-8")
        if not caught:
            uncaught += 1
        print(f"{name:<32} {'caught' if caught else 'NOT CAUGHT':<11} {test}")
        print(f"    rule: {description}")
    print()
    if uncaught:
        print(f"{uncaught} mutation(s) went uncaught: the rule is not load-bearing")
        return 1
    print(f"all {len(MUTATIONS)} mutation(s) caught: every rule is load-bearing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
