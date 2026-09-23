# Round 33: the pull-request lane, driven against the real repository (REQ-032)

Node: EP-009 (release lanes). Requirements touched: REQ-032 (official `gh`; explicit approval; no
auto-merge).

`REQ-032` read `PARTIAL_EP003_RECORD_DONE_GH_LANE_PENDING`. The bookkeeping half existed —
`PullRequestRepo` records a proposal, refuses to mark it merged without a named approver, and the
schema makes an unapproved-but-merged row unrepresentable — but nothing ever *opened* a pull
request. This round adds that half and runs it against the real repository.

## The lane

`crates/vector-platform/src/gh.rs`. Three properties are structural rather than documented:

- **The official client only.** The lane invokes `gh` (or an absolute path a caller names) through
  the same shell-free `ProcessSpec` the rest of the platform uses: an argument vector, never a
  command line for a shell to re-parse.
- **Credentials are never handled.** The child environment comes from the platform's allowlist,
  which strips `GH_TOKEN`, `GITHUB_TOKEN` and their relatives, so the lane runs on whatever session
  `gh auth login` established. It cannot leak a token it never sees, and it cannot authenticate on
  its own. It does forward the *configuration-location* variables (`APPDATA`, `LOCALAPPDATA`,
  `XDG_CONFIG_HOME`, `XDG_DATA_HOME`), because that is where `gh` finds its session — and that was
  not a theoretical distinction: the first real run failed with *"To get started with GitHub CLI,
  please run: gh auth login"* precisely because `APPDATA` was missing, which is the failure a
  credential-passing design would have hidden behind a working call.
- **There is no merge.** One function builds every command the lane can express — `pr create` and
  `pr view` — and a test asserts that no command it can build contains `merge`, `close` or
  `delete`. Merging is a human act on the forge, not a capability of this code.

An approver is required and is refused before any process runs: `None`, `""` and `"   "` are all
`NotApproved`.

## The run against the real repository

| Step | Result |
|---|---|
| branch `lane/req-032-proof` pushed, carrying the lane itself and a release review checklist | `fc335b2` |
| `vector-tools repair gh-open --repo dominator509/project-vector-asvab --head lane/req-032-proof --base main --title … --body … --approver "dominator509 (session owner)"` | exit 0, **`https://github.com/dominator509/project-vector-asvab/pull/22`**, number 22, `"merged": false` in the lane's own report |
| independent readback with the official client: `gh pr view 22 --json …` | `state: OPEN`, `mergedAt: null`, `baseRefName: main`, `headRefName: lane/req-032-proof` |
| the diff, read back: `gh pr diff 22 --name-only` | four files — the lane, the crate root, the CLI and the checklist |
| cleanup: `gh pr close 22 --comment "Closed without merging: this was a lane proof…"` | closed; `state: CLOSED`, `mergedAt: null` |
| cleanup: branch deleted on the remote | `gh api …/branches/lane/req-032-proof` → `Branch not found (HTTP 404)` |

Transcripts and JSON in `.agent/evidence/EP-009/gh-lane/`. The default branch is unchanged by any of
it: the proof branch was deleted and the pull request closed unmerged, and the review checklist it
carried is the only content, kept locally as the branch's commit for whoever wants it.

Five unit tests cover the lane: the enumerated command set contains no merge, close or delete; an
unapproved request is refused before any process runs; the child environment carries no credentials
and does carry the configuration locations; an unusable repository, branch or title is refused; and
the lane runs a real program with an argument vector, parses the URL out of its stdout, and reports
a failing program's own stderr rather than swallowing it.

Two mutations are caught: `gh-lane-accepts-an-unapproved-request` and
`gh-lane-lets-a-token-through` (replace the allowlist-built environment with the parent's).

## State

`REQ-032` moves to `DONE_OFFICIAL_GH_LANE_EXPLICIT_APPROVAL_NO_AUTO_MERGE`; open requirements fall
from 5 to **4** — `REQ-036` (a signing certificate), `REQ-038` (a person using a screen reader),
`REQ-060` (trademark clearance), all three external, and `REQ-014` (a local GGUF model), which is
work here.

DoD unchanged at 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED; 21 gates exit 0; **46 mutations caught**
(43 cargo, 2 vitest, 1 Playwright); verdict `CONDITIONAL_EXTERNAL_GATES`.

One observation recorded rather than chased: GitHub's own Dependabot count on the default branch is
now **2 moderate** (from 17: 3 critical, 3 high, 11 moderate before the toolchain upgrade in round
31), while `pnpm audit` and `cargo deny check` both report nothing. The two views disagree about the
same tree, and the difference is worth a look before the next release rather than being assumed
away.
