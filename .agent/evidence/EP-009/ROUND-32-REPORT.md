# Round 32: the update path — signature, digest, atomic stage, rollback (REQ-037)

Node: EP-009 (release lanes). Requirements touched: REQ-037 (signature/hash/atomic stage/rollback),
REQ-036 (the certificate this leaves external).

`REQ-037` read `PARTIAL_DIGEST_VERIFY_AND_TAMPER_DETECTION_DONE_SIGNATURE_PENDING`: two of the four
things the requirement names existed (the digest and tamper detection, on packs) and two did not
(a signature for an *application* update, and a staged transition). This round adds the missing
half, and the end-to-end run of it found two design errors that unit tests had not.

## The verification, in the order it happens

`crates/vector-platform/src/update.rs`:

1. **Format and application** — a manifest of the wrong shape, or one for another product, is
   refused as itself.
2. **The signer**, before the signature: a valid signature by a key this installation does not
   trust and an invalid signature are different facts, and the refusal says which. The trusted key
   is a hex Ed25519 public key the installation pins.
3. **The signature**, over a payload rebuilt from the manifest's *current* fields, every field
   length-prefixed — the same construction the content pack uses for its identity. Editing the
   version, the artifact name, its digest, its size, the minimum base version or the publication
   date breaks it.
4. **The version relation**, after the cryptography, because a version mismatch is not an attack:
   `running < offered`, and `running >= min_running_version`.
5. **The artifact**, by SHA-256 and by length, before anything is written anywhere.

`stage` copies the verified artifact beside the installation under a versioned name and verifies
the *copy* again. `apply_staged` moves the previous artifact aside and puts the staged one in
place, restoring the old one if the rename fails; `rollback` puts it back. On Windows a running
executable cannot be replaced, so applying is a separate step from staging and this code never
replaces the file it is running from.

## What the end-to-end run found

The unit test used a manifest file name that happened to match the installed artifact name, so it
passed. Against real files the first run applied the update to `vector-desktop-0.2.0.exe` — the
*published* name — instead of the installed `vector-desktop.exe`, and then could not roll back,
having looked for the backup beside the installed one. Both were design errors, not typos: the
published file name and the installed file name are different things, and a rollback has to be able
to find what an apply displaced. Now `apply_staged` takes the installed target explicitly, derives
the backup path from it (`displaced_path`), returns `None` when there was nothing to displace (a
first install, which is legitimate), and `rollback` takes the same target and says plainly when
there is no previous artifact.

Measured on real files (`.agent/evidence/EP-009/update/`):

| Step | Command | Exit |
|---|---|---|
| keygen | `update update-keygen --key update.key` | 0 |
| manifest | `update update-manifest --artifact …-0.2.0.exe --version 0.2.0 --min-running 0.1.0` | 0 |
| sign | `update update-sign --manifest … --key … --out signed.json` | 0 |
| honest verify | `update update-verify … --trusted-signer <key> --running 0.1.0` | **0** |
| untrusted signer | same, `--trusted-signer 000…0` | **1** — *"signed by e35b82f9…, which is not the key this installation trusts"* |
| edited manifest | version changed to 9.9.9, original signature | **1** — *"signature error: Verification equation was not satisfied"* |
| tampered artifact | one word changed in the published bytes | **1** — *"the artifact hashes to sha256:ebc57d63…, and the manifest records sha256:cb44575b…"* |
| offer not newer | `--running 0.2.0` | **1** — *"version 0.2.0 is not newer than the running 0.2.0"* |
| stage and apply | `update update-stage … --install-dir install --target install/vector-desktop.exe` | **0** — target holds the published 0.2.0 bytes, backup holds the 0.1.0 bytes |
| rollback | `update update-rollback --target install/vector-desktop.exe` | **0** — target holds the 0.1.0 bytes again, backup consumed |

Five unit tests cover the same rules in the crate, including six separate edits to a signed
manifest and the first-install case. The mutation `update-signature-not-checked` (replace
`verify_strict(…) ?` with `let _ = verify_strict(…)`) is caught by
`an_edited_manifest_breaks_its_signature`, so the signature check is proven load-bearing rather
than merely present.

## What this closes, and what it does not

`REQ-037` is now `DONE_SIGNATURE_HASH_ATOMIC_STAGE_ROLLBACK_CERTIFICATE_EXTERNAL_GATE`. Five
requirements remain open: `REQ-036` (a code-signing certificate), `REQ-038` (a person using a
screen reader), `REQ-060` (trademark clearance) — all three external — plus `REQ-014` (a local GGUF
model) and `REQ-032` (the `gh` pull-request lane), which are work that can be done here.

Deliberately not implemented, and named rather than implied: **downloading**. The manifest carries
the artifact's digest and size; fetching the bytes is the operator's or the launcher's step, and
this crate takes no network dependency. The certificate a real distribution needs is `REQ-036`.

## State

DoD unchanged at **34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED**; gates **21 exit 0**; **44 mutations
caught** (41 cargo, 2 vitest, 1 Playwright); artifact digest `6eaa3027…`, epoch 22; open
requirements **5** (from 6). Corpus unchanged at 6,961 active items; pack `core-asvab v6` active;
every provenance invariant a zero. Verdict `CONDITIONAL_EXTERNAL_GATES`.
