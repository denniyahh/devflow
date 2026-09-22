# Phase 48 adversarial review — production code

You are an adversarial reviewer. Attack the implementation, not its intent. Your job is to find
defects that the phase's own tests, plan gates, security audit (SECURED, 0 open threats) and
Nyquist validation (compliant) all missed.

## Where everything is

All paths below are relative to the review root: the directory that contains this file.

- `snapshot/` — a `git archive` export of commit `3f6669b` (the tip of `feature/phase-48`). It has no
  `.git`. Read source here. **Do not modify anything under `snapshot/`**: it is the review base. If you
  want to build or experiment, copy what you need to a new sibling directory first.
- `phase48-src.diff` — `git diff fbf6e98..3f6669b` over `clippy.toml`, `crates/devflow-core/src` and
  `crates/devflow-cli/src` (11,087 lines). `fbf6e98` is the commit before Phase 48 began.
- `phase48-commits.txt` — the 47 Phase 48 commits that touched code, oldest first.
- Requirements and decisions: `snapshot/.planning/REQUIREMENTS.md` (SURV-01, SURV-02, CHKPT-01,
  CHKPT-02, TEST-01), `snapshot/.planning/ROADMAP.md` § "Phase 48" (success criteria 1-7), and
  `snapshot/.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md`
  (decisions D-01..D-10, D-03b).

## What the phase claims (verify, do not trust)

- **SURV-01:** two processes writing state for the same phase cannot silently lose an update.
  Mechanisms: unique temp names for state writes (`devflow-core/src/workflow.rs`), first-answer-wins
  gate publish via hard link (`gates.rs`), per-phase lock with bounded blocking acquire and serialized
  stale reclaim (`lock.rs`), `start` and `stop` taking the lock (`devflow-cli/src/commands.rs`,
  `main.rs`), `advance` bound to its launched stage (`monitor.rs`, `pipeline_launch.rs`).
- **SURV-02:** a gate whose consumer is gone reports the recovery that exists instead of asserting a
  waiter that does not. Mechanisms: `lock::holder_status` classification
  (NoHolder / Live / Recycled / Unconfirmable, identity = pid + start time); `gate approve`,
  `gate reject`, `stop` and `gate sweep` check for a live waiter before writing any answer; no answer
  file at a non-Ship gate with no live waiter; Ship's stored-response exception only for `NoHolder`;
  `recover --clean` (`recover.rs`); `doctor` findings (`commands.rs`).
- **CHKPT-01:** preflight and the resume route share one parsed checkpoint predicate (`verify.rs`).
- **CHKPT-02:** a human-only checkpoint that is new or changed relative to the set recorded at Code's
  preflight parks at a human gate before the resume decision can auto-decide it; a state file with no
  recorded set gates. D-03b: unchanged recorded checkpoints auto-decide only in Auto mode; Supervise
  gets a human gate. A rejected Auto re-scan parks the phase in Supervise, keeps the prior approval
  and requires repair plus explicit resume (`pipeline_launch.rs`, `preflight.rs`, `state.rs`).
- **TEST-01:** tests that replace `PATH` run in a child process; a clippy `disallowed-methods` lint
  rejects new process-global env mutation (`clippy.toml`, `devflow-core/src/test_support.rs`). Review
  the harness for ways a test could pass without running its body.

## Already known — do not report these as findings

These are filed as backlog items or already fixed. Mention one only if you find it is **worse** than
described (for example, it causes data loss rather than a blocked recovery).

- A live pid with a different start time (Recycled) blocks every lock-taking verb, because lock
  reclaim checks pid liveness only; the repair `stop`/`doctor` name then refuses until that pid exits
  (backlog 999.128, accepted Phase 48 limit).
- Ship finalization-retry gate recovery is unverified (999.129).
- No `flock` around state writes / no `update_state` API / no fsync of file plus directory (999.130).
- About 40 non-`PATH` env mutations in tests remain under `#[expect]` and `ENV_MUTEX` (999.131).
- `gate list` and `status` disagree after an unwaited Ship answer (999.132).
- `stop` used to print "lock holder was recycled; treating it as no waiter"; removed in `395feb4`.
- Several plan `<automated>` gate scripts are stale or use `rg -c` (prints nothing on zero). Those are
  planning artifacts, not production code — out of scope.

## How to attack

Trace complete state machines, not individual functions:
- every path from `start` → `advance` → gate wait → `gate approve|reject` / `stop` / `gate sweep` /
  `recover --clean` / `resume` / `doctor`, per stage and per mode (Auto vs Supervise), including Ship;
- process-identity races (pid reuse, a successor holder taking the lock between two reads, a holder
  exiting mid-command), time-of-check/time-of-use between `holder_status`, `holder_identity`, lock
  acquisition and file writes;
- crash windows: a process killed between writing a temp file and renaming or linking it, between
  saving state and writing or cleaning a gate file;
- operator-facing messages that claim more than the code established (a message saying something
  was stopped, answered, or picked up when it was not);
- checkpoint parsing edge cases that make preflight and resume disagree, or let a changed checkpoint
  auto-decide.

## Output format

Report in this exact structure:

```
## CONFIRMED
### C-n. <title>
**File:** path:line (and every other file:line involved)
**Mechanism:** the exact code path, step by step
**Reproduction:** concrete inputs/state and the observed wrong outcome. If you ran something, give the
command and its output.
**Severity:** critical | high | medium | low — and why

## SUSPECTED
### S-n. <title>
Same fields; say exactly what evidence would confirm or refute it.

## CHECKED AND CLEAN
One line per area you traced and found sound, naming the file:line that bounds the claim.
```

Only put an item under CONFIRMED if you can point to the exact lines and a concrete path that
produces the wrong outcome from the current code. Cite `file:line` for every claim. "No findings" is
a legitimate result only if CHECKED AND CLEAN shows what you actually traced.
