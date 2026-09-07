# Bounded Execution Loops

## Build loop
Reproduce failure -> capture command and output -> classify environment/code/test/spec -> make smallest reversible correction -> rerun the narrow failing proof -> rerun affected suite -> record evidence. Maximum five candidate attempts per node before blocker classification.

## Integration loop
Verify official contract and installed client version -> probe capability -> exercise positive operation -> exercise negative/fail-closed operation -> independent readback -> cleanup -> hash evidence. Never substitute a mock for final integration evidence.

## Crash-repair loop
Redact crash bundle -> deduplicate signature -> reproduce in isolated worktree -> create regression test -> repair -> mutation proof -> full affected gates -> independent audit -> open PR only if all gates pass. No autonomous merge or release.

## Blocker ladder
1. Confirm the exact failure twice when deterministic.
2. Inspect direct logs/artifacts and nearest dependency.
3. Try the smallest local remedy.
4. Challenge the architectural assumption against specs/ADR.
5. Try an alternate standards-compliant route.
If still blocked, emit `.agent/blocked/EP-xxx.blocked.json` with evidence, unblock condition, affected requirements, and independent work that remains executable.
