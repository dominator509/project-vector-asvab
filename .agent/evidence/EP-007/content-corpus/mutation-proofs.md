# Mutation proofs for the Paragraph Comprehension path

HARNESS_LAWS.md law 9: a test that does not fail when the behaviour it names is
broken is not evidence. These are the mutations run this round, what they broke, and
what the suite did about it.

## 1. The source-containment check in `ContentPipeline::store_pc_verified`

The check:

```rust
if !text.contains_passage(&item.passage) {
    anyhow::bail!(
        "the item's passage does not occur in {:?}, so the item is not source-backed",
        request.label
    );
}
```

Mutation: the whole block replaced by a comment, nothing else changed.

Result — `cargo test -p vector-application --test pc_ingestion`:

```
test storing_refuses_an_item_whose_passage_is_not_in_the_source ... FAILED
test result: FAILED. 9 passed; 1 failed
```

Restored: `test result: ok. 10 passed; 0 failed`.

**A trap worth recording.** The first restore reported the *same* failure, which
looked like the check had not been restored. It had: `Copy-Item` put the backup
back with its original modification time, which was older than the artifact built
from the mutated source, so cargo considered the build fresh and re-ran the mutated
binary. Touching the file and re-running gave 10 passed. A mutation proof is only
worth something if the restore is proven to be in effect, and mtime is not proof of
that.

## 2. The store's refusal to keep a Paragraph Comprehension item without a passage

`migrations/005_content_passage.sql` installs the rule as a trigger. The proof is
inside the test rather than beside it:
`crates/vector-persistence/tests/content_lifecycle.rs::the_passage_rule_is_load_bearing`
drops `content_items_pc_requires_passage_insert` in the test database and asserts
that the insert the guard refused then succeeds. If the trigger were removed from
the migration, the earlier test in the same file — which expects a refusal — fails.

## 3. The passage builder's verbatim guarantee

Not a mutation but the same kind of evidence, and it is what found the defect:
`Text::contains_passage` refused 3 items during a real ingestion run. Auditing the
built corpus with

```
cargo run -p vector-questions --example pc_passage_audit -- \
    book-of-wonders=sources/gutenberg/book-of-wonders.txt \
    triumph=sources/gutenberg/triumphs-wonders.txt \
    doe=sources/gutenberg/doe-nuclear-1.txt \
    beagle=sources/gutenberg/darwin-beagle.txt \
    moby=sources/gutenberg/melville-moby.txt 1
```

reported `0 of N built item(s) have a passage the work does not contain` for every
work *before* the splitter was fixed, and the before-state is preserved in
`pc-corpus-scan-before-passage-fix.txt` for comparison.

## 4. The provenance probe

`scripts/probes/verify-gutenberg-provenance.py` was written to be able to fail, and
it did on its first run: `darwin-origin.txt` was cited as Project Gutenberg #2009
while the file is #1228. A check that only ever agrees with the ingester proves
nothing; this one disagreed, and the discrepancy was real.
