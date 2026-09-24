# The corpus source tree

`sources/` is **not committed** (`.gitignore:54`): it is roughly 250 MB of third-party prose
spanning dozens of works, and a repository that carries it carries a copy of Project
Gutenberg. But the corpus is *ingested, not authored* — `scripts/rebuild-corpus.py` rebuilds
every bank from this tree — so the tree has to be reproducible, and for a long time no script
made it. That gap is what this file and `scripts/fetch-sources.py` close.

## Rebuild from a fresh checkout

```sh
python3 scripts/fetch-sources.py        # ~2–4 min, ~250 MB, records the manifest below
cargo run -q -p vector-tools -- db setup --db-path vector.db
python3 scripts/rebuild-corpus.py vector.db
```

`fetch-sources.py` fetches each work from its **canonical public distribution** and records the
URL, SHA-256 and size of every file it writes into
`.agent/evidence/EP-007/content-corpus/source-manifest/MANIFEST.sha256.json`
(a copy is also left at `sources/MANIFEST.sha256.json`). It refuses a
Gutenberg download whose own `<title> [eBook #<id>]</title>` header names a different id than the
one it was asked for — the check that a wrong number cannot become an authoritative-looking
citation (ten Paragraph Comprehension items cited #2009 for a file that is #1228).

## Provenance check

After fetching, re-derive the Gutenberg citations:

```sh
python3 scripts/probes/verify-gutenberg-provenance.py \
  --manifest .agent/evidence/EP-007/content-corpus/gutenberg-manifest.txt
```

Each work reports OK (the URL re-downloads to the bytes it is cited as), MISMATCH, or MISSING.

## What is fetched, and from where

| Family | Subtest(s) | Source | Form |
| --- | --- | --- | --- |
| Paragraph Comprehension prose | PC | 25 Project Gutenberg works | `gutenberg.org/cache/epub/<id>/pg<id>.txt` |
| Word Knowledge dictionary | WK, EI, SI, AI | Webster's Unabridged 1913 | Gutenberg #29765 |
| Word Knowledge thesaurus | WK | Moby Thesaurus II (Grady Ward) | PG #3202's linked `files/mthesaur.txt` |
| Electronics Information | EI | 24 NEETS modules | Internet Archive item `neetsmodules_202003` |
| Shop / Auto manuals | SI, AI | US Army manuals + Gutenberg | Internet Archive + Gutenberg |
| General Science | GS | The Book of Wonders | Gutenberg #75948 |

## Two source facts a reviewer should know

1. **The thesaurus is a data file, not an ebook.** Project Gutenberg #3202 is a documentation
   page; the 30,259-line comma-separated data the ingester parses is linked from it as
   `https://www.gutenberg.org/files/3202/files/mthesaur.txt`. Fetching `pg3202.txt` instead —
   which is what a naive `cache/epub/<id>` fetch does — yields a 486-line readme, and the
   Word Knowledge ingestion then rejects every candidate with *"no Word Knowledge items survived
   the dictionary filter"*, because there is no root line to build from.

2. **The upstream bytes are not frozen.** The thesaurus file at that URL has changed since the
   original corpus was built: the vault records digest `63fcdcec…` at **30,259** root words,
   while a fetch on 2026-09-24 yields `7c9742b1…` at **30,260**. The Webster dictionary is
   unchanged (`86fb9c28…`, identical). A rebuild today therefore produces slightly different
   Word Knowledge counts (1,961 distinct items against the pack's 1,949) from the same seed.
   This is a property of the upstream source, not of the ingester, and it is why the manifest is
   committed: the next maintainer can see exactly which bytes a build used.
