# Study corpus round 15: signed content packs, install, and rollback

Node: EP-007 (the content item store). Requirements touched: REQ-023 (signed,
versioned packs), REQ-032 (rollback), REQ-048, REQ-056.

## What this round adds

`CONTENT_PACK_SPEC.md` requires a pack to carry a manifest, a schema version, a
compatible application range, a source ledger, licences, the items, and a detached
signature; and it requires installation to verify schema, signature, hashes,
compatibility, provenance completeness, a prohibited-source scan, answer consistency
and reviewer state before atomic activation, with the previous signed pack left
available for rollback. Before this round the registry table existed and **nothing
ever wrote to it**: `content_packs` had publish, activate and rollback, and no pack was
ever built, installed or rolled back in the product.

Now:

1. **`crates/vector-application/src/packs.rs`** assembles a pack from everything a
   store serves, verifies a received pack, installs it and rolls it back.
2. **`migrations/006_content_pack_manifest.sql`** gives the registry the identity
   installation verifies: signer, content hash, schema version, the manifest as
   received, and the item count. An `active` row without them is refused by trigger.
3. **`migrations/007_content_pack_items.sql`** makes membership its own relation (see
   the two defects below).
4. **Four tool commands**: `pack-keygen`, `pack-build`, `pack-install`, `pack-list`,
   `pack-rollback`, all in `COMMANDS.md`.

## The pack, built from the real corpus

`pack-build` over the 7,016-item corpus store produced a 13,741,945-byte signed pack:
7,016 items, 28 sources, licences `Public domain (US government work)` and
`Public domain in the USA`, Ed25519 signature by the key generated for this round.
Installed into an empty store it delivered 7,016 items, 28 vault rows, 9,016
citations, and the serving path offered every one of them.

Pack digests are in `packs/pack-digests.json`; the pack files themselves are not
committed, because a pack is derived from the store and the sources and can be rebuilt
with one command, while 27 MB of binaries in git cannot be reviewed.

**The signing key is not in the repository.** It lives at
`%APPDATA%\com.vector.app\keys\content-pack-signing.key`, its public half is
`86b0ed0e0524bd430b647b227df120f27adfedc8470ed65ee6dede641ac8df8c`, and the first
attempt did write it under `.agent/evidence/` — which would have committed a private
key to a public repository. It was removed and regenerated outside the tree.

## Two defects found by exercising packs against the real corpus

Neither is visible in a test that installs one pack into an empty store. Both appeared
the moment a second real pack met 7,016 real items.

1. **A version that repackaged content withdrew it.** Serving keyed off
   `content_items.pack_id`, the pack that *delivered* an item. A v2 containing the same
   7,016 questions therefore either stole them from v1 or left them behind, and either
   way the corpus went from 7,016 servable items to none. Membership is now its own
   relation (`content_pack_items`), so an item serves while any pack containing it is
   active: repackaging keeps content serving, and a version that adds content still
   withdraws it on rollback. `pack_id` stays as the provenance fact it always was.
2. **A pack that overlapped the corpus failed on a foreign key.** Two packs built
   independently from the same source carry the same questions under different item
   ids. Installing the second found the content already present, skipped it, and then
   recorded membership against ids that did not exist locally. Membership and citations
   now name the *local* row, mapped by content hash.

A third, smaller one: rollback chose the highest version below the active one
regardless of status, so it could reinstate a version that had been quarantined
deliberately. It now considers `superseded` versions only.

## Evidence

- `packs/pack-e2e.log` — the whole lifecycle against the real corpus: build, install,
  refusals, repackage, overlap, rollback, with the serving path read back after each.
- `packs/pack-refusals.log` — an untrusted signer, a pack edited at rest, and a pack
  requiring a newer application.
- `packs/pack-digests.json` — the SHA-256 of each pack built.
- Tests: 15 unit tests in `src/packs.rs` (the refusals that need a pack whose stored
  fields disagree with its signature) and 16 integration tests in
  `crates/vector-application/tests/content_packs.rs` (the public path: build, install,
  idempotence, overlap, repackage, rollback, and that a refused pack writes nothing).

Gate results: `gates/`. All eight exit 0. 875 Rust tests across 69 binaries, 0 failed;
205 frontend unit tests; 34 E2E tests.

## What is still not done

- **Packs are not in the interface.** The content manager lists items, sources and
  quarantine, and has no packs panel, no install action and no rollback button. The
  backend and the commands are finished and tested; the surface is next.
- **No app command wraps the pack functions.** `vector-tools` drives them; the Tauri
  command layer does not, so the desktop application cannot build, install or roll back
  a pack yet.
- **Shop Information and Auto Information still have no items** (round 14's findings
  stand: ~30 purpose sentences unmined in *Tools and Their Uses*, and no usable
  public-domain automotive source found).
- **The pack schema carries no curriculum graph or calibration metadata** that
  `CONTENT_PACK_SPEC.md` lists. What it does carry — manifest, semver, schema version,
  app range, source ledger, licences, items, signature — is implemented and verified;
  the two absent sections are named here rather than implied.
