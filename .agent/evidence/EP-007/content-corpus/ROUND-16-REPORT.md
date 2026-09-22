# Study corpus round 16: packs in the product

Node: EP-007. Requirements touched: REQ-023 (signed, versioned packs), REQ-032
(rollback), REQ-048, REQ-056.

## What this round adds

Round 15 built the pack engine and left it reachable only from `vector-tools`. The
objective asks for a content manager with **pack install and quarantine**, so this
round puts packs behind the command boundary and in front of a reviewer:

1. **Three Tauri commands** — `content_packs`, `content_pack_install`,
   `content_pack_rollback` — registered in `generate_handler!`, each with a testable
   `*_impl`. `InstallReport` gained `Serialize` because it crosses the boundary.
2. **The trusted signing key is configuration, not an argument.**
   `content_pack_install` reads `VECTOR_PACK_TRUSTED_SIGNER` from the environment; a
   caller that could name the key it trusts could install a pack it had signed itself,
   which is the one thing the signature exists to prevent. With no key configured,
   installation refuses every pack and says so.
3. **The interface**: the content manager has a Content packs panel — name, version,
   status, item count, whether the stored signature still verifies, a Roll back button
   on the active version, and an install box that takes a path on this machine. The
   install button is disabled until a path is given, the path is cleared after a
   successful install so a second click cannot reinstall the same file, and a refusal
   is shown as an error rather than as a success.
4. **The IPC layer**: `InstalledPackDto` and `InstallReportDto` types, validating
   readers (`readInstalledPack`, `readPacks`, `readInstallReport`), the three client
   methods, and the fake client's model of the registry.

## The reader check that is easy to leave out

`readInstallReport` refuses a report claiming more installs than the pack has items.
Verification happens before storage, so `installed + already_present > items` is not a
report an honest installer can produce, and a surface showing it would be showing a
reassuring number over content that was not delivered. It is the same shape of check
as the `content_generate` reader's "more activations than verifications".

The fake client's install refuses a path containing `untrusted` or `unsigned` for the
same reason the backend refuses an untrusted pack: a fake that accepted everything
would let the panel pass its tests while the real installer refused the same click.

## Tests

- **Rust, command boundary**: 28 tests in `apps/desktop/src-tauri/tests/content_commands.rs`,
  five of them new — an install through the command is served back through
  `content_next_impl`; no configured key refuses everything; a malformed key is refused
  before any file is read; another key's pack is refused and leaves nothing to serve; a
  rollback with no earlier version is refused.
- **Frontend**: 210 tests, five new for the panel — the empty state, a successful
  install and its report, a refused install shown as an error with no success notice,
  a rollback through two versions with the rows' statuses checked, and a registry that
  cannot be read degrading to the empty state without breaking the corpus half.
- **E2E against the built bundle**: 36 tests, two new — the content manager renders an
  installed pack with its signature state and refuses an install this stub cannot
  verify; a fresh installation says there are no packs.

The E2E stub had to learn `content_manager` as well as `content_packs`. Without it the
view sat in its error state and the panel was never reached — which is how these two
tests failed the first time, and a reminder that a stub is a description of the whole
surface a view touches.

## Gates

All eight exit 0: format-check, lint, typecheck, unit, integration, e2e,
generated-pack, anti-gaming. 880 Rust tests across 69 binaries, 0 failed; 210 frontend
tests; 36 E2E. The packaged artifact was rebuilt (`sh scripts/build.sh`) and the
packaged live-fire passed: `"verdict": "pass"`, webview reached the command layer,
markers 21 → 22.

## What is still not done

- **Shop Information and Auto Information still have no items.** Round 14's findings
  stand unchanged: about thirty purpose sentences unmined in *Tools and Their Uses*, and
  no public-domain automotive source with usable structure found.
- **The pack schema carries neither the curriculum graph nor the calibration metadata**
  listed in `CONTENT_PACK_SPEC.md`.
- **The interface installs from a path typed by hand.** A file picker would need the
  Tauri dialog plugin, which is a new dependency; typing a path is honest but not
  friendly.
- **Word Knowledge distractor plausibility** remains the oldest open gap: Moby's
  associations are not all synonyms and the dictionary filter reduces the rate rather
  than eliminating it.
