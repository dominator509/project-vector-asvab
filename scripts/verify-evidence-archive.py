#!/usr/bin/env python3
"""Verify that the committed evidence archives restore the committed evidence chain.

The raw output of every gate run is git-ignored (see `*.log` in `.gitignore`),
so what the repository actually commits is a `.log.sha256` digest and a
`.exitcode` per gate. A digest is only worth committing if the bytes it digests
can still be produced: otherwise it is an attestation nobody can re-derive.

Each closed epoch therefore carries `.agent/evidence/EP-XXX/logs.tar.gz`, written
once at epoch close. This script proves those archives are sufficient: it
extracts every archive into a temporary directory and re-hashes each restored log
against the digest committed in the repository.

It exits 0 only when every committed digest is satisfied by a restored log. A
missing archive, a missing member or a mismatched digest is a failure. Running
this in a fresh clone is the evidence that a local working copy can be discarded
and reconstituted from GitHub without losing the verification record.

Usage:
    python3 scripts/verify-evidence-archive.py [root]

`root` defaults to the repository root inferred from this file's location.
"""

from __future__ import annotations

import hashlib
import pathlib
import subprocess
import sys
import tarfile
import tempfile

EVIDENCE_DIR = pathlib.Path(".agent") / "evidence"
ARCHIVE_NAME = "logs.tar.gz"
DIGEST_SUFFIX = ".log.sha256"
MANIFEST_NAME = "MANIFEST.txt"


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tracked_digests(root: pathlib.Path) -> dict[pathlib.Path, str]:
    """Map every committed `<gate>.log` path to the digest committed beside it."""
    result = subprocess.run(
        ["git", "ls-files", "-z", EVIDENCE_DIR.as_posix()],
        cwd=root,
        capture_output=True,
    )
    if result.returncode != 0:
        raise SystemExit("verify-evidence-archive: not a git working tree")
    digests: dict[pathlib.Path, str] = {}
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        rel = pathlib.Path(raw.decode("utf-8", "surrogateescape"))
        if rel.name.endswith(DIGEST_SUFFIX):
            log_rel = rel.with_name(rel.name[: -len(".sha256")])
            text = (root / rel).read_text(encoding="utf-8", errors="replace")
            fields = text.split()
            if not fields:
                raise SystemExit(f"verify-evidence-archive: empty digest file {rel}")
            digests[log_rel] = fields[0].strip().lower()
    return digests


def safe_extract(archive: pathlib.Path, destination: pathlib.Path) -> list[str]:
    """Extract an archive, refusing members that escape the destination."""
    with tarfile.open(archive) as tar:
        names = tar.getnames()
        for name in names:
            pure = pathlib.PurePosixPath(name)
            if pure.is_absolute() or ".." in pure.parts:
                raise SystemExit(
                    f"verify-evidence-archive: unsafe member {name!r} in {archive}"
                )
        tar.extractall(destination)
    return names


def main(argv: list[str]) -> int:
    if len(argv) > 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        return 2
    root = pathlib.Path(argv[1]).resolve() if len(argv) == 2 else pathlib.Path(__file__).resolve().parent.parent
    evidence = root / EVIDENCE_DIR
    if not evidence.is_dir():
        raise SystemExit(f"verify-evidence-archive: {evidence} not found")

    digests = tracked_digests(root)
    if not digests:
        print("verify-evidence-archive: no committed evidence digests to check")
        return 0

    archives = sorted(evidence.glob(f"EP-*/{ARCHIVE_NAME}"))
    if not archives:
        raise SystemExit(
            "verify-evidence-archive: no epoch archives found, so no committed "
            "digest can be re-derived"
        )

    verified = 0
    missing: list[pathlib.Path] = []
    mismatched: list[tuple[pathlib.Path, str, str]] = []
    with tempfile.TemporaryDirectory(prefix="vector-evidence-") as tmp:
        staging = pathlib.Path(tmp)
        restored: dict[str, pathlib.Path] = {}
        for archive in archives:
            epoch = archive.parent.name
            destination = staging / epoch
            destination.mkdir(parents=True, exist_ok=True)
            safe_extract(archive, destination)
            restored[epoch] = destination

        for log_rel, expected in sorted(digests.items()):
            try:
                relative = log_rel.relative_to(EVIDENCE_DIR)
            except ValueError:
                missing.append(log_rel)
                continue
            epoch = relative.parts[0]
            staged = restored.get(epoch)
            candidate = staged / pathlib.Path(*relative.parts[1:]) if staged else None
            if candidate is None or not candidate.is_file():
                missing.append(log_rel)
                continue
            actual = sha256_file(candidate)
            if actual == expected:
                verified += 1
            else:
                mismatched.append((log_rel, expected, actual))

    for log_rel in missing:
        print(f"MISSING FROM ARCHIVE  {log_rel.as_posix()}")
    for log_rel, expected, actual in mismatched:
        print(f"DIGEST MISMATCH       {log_rel.as_posix()}")
        print(f"  committed {expected}")
        print(f"  restored  {actual}")

    print(
        f"verify-evidence-archive: {verified} of {len(digests)} committed digests "
        f"re-derived from {len(archives)} epoch archive(s); "
        f"{len(missing)} missing, {len(mismatched)} mismatched"
    )
    if missing or mismatched:
        print("verify-evidence-archive: FAIL")
        return 1
    print("verify-evidence-archive: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
