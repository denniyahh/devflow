# Phase 48 fix review — three code-review remediations

You are an adversarial reviewer. A prior review of Phase 48 found three defects; they were fixed in three
commits. Attack the fixes, and — more important — look for **other instances of the same defect class**
that the fixes did not touch. A previous round of this phase fixed one instance of a class three times
before the class was closed; assume that is happening again until the code shows otherwise.

## Where everything is (paths relative to the directory containing this file)

- `snapshot/` — `git archive` of the current tip (`f745bd5`). No `.git`. **Do not modify it**; copy to a
  sibling directory if you want to build or experiment.
- `fix.diff` — `git diff 3f6669b..9a33c70 -- crates` (445 lines): exactly the three fixes.
- Evidence of the original findings: `snapshot/.planning/reviews/48-code-review-2026-09-22/VERIFIED.md`.

## The three fixes

- **A (`fb2c4f8`) — class: a state deleter that does not take the per-phase lock.** The implicit
  `recover --clean` sweep (`crates/devflow-core/src/recover.rs` `clean_report`) deleted state judged stale by
  agent pid alone while a gate-waiting monitor held the per-phase lock. It now takes the lock and keeps a
  contended phase. The phase's own design says correctness depends on "every state writer takes the lock".
  **Find every other code path that writes, deletes or replaces `.devflow/state-*.json`, gate files or cron
  records without holding the per-phase lock**, and say for each whether a live lock holder can be harmed.
- **E (`f1accee`) — class: a gate-answer writer that applies a weaker holder rule than `stop`.**
  `gate approve|reject` (`crates/devflow-cli/src/commands.rs` `gate_respond`) skipped the holder check at
  Ship, so a Recycled holder got a response. Rule now: write only if the holder may be waiting
  (Live/Unconfirmable), or at Ship with NoHolder. **Enumerate every writer of a gate response**
  (`Gates::respond`, `Gates::reap`, auto-responses, `gate sweep`, `stop`, `--yes-ship`, anything else) and
  check each applies a consistent holder rule, including at Ship.
- **D (`9a33c70`) — class: an operator-facing message that claims an action which did not happen.**
  `recover --clean` printed "cleaned up …" and exited 0 when it deleted nothing. `clean_phase` now returns
  `RecoverError::Lock(Contended)`; the sweep reports cleared phases via the new additive `clean_report`.
  **Find other commands that print success, or exit 0, after a refusal or a no-op** (`stop`, `gate sweep`,
  `gate approve|reject`, `ship`, `resume`, `doctor --fix` if present, `cleanup`, `recover`).

Also check that each fix is itself correct: lock scope and ordering (does A hold the lock across everything
it deletes? does it leave or remove the lock file correctly?), error mapping (can `clean_phase` now surface a
contended lock in a way another caller mishandles?), and whether the tests would catch a regression.

## Already known — do not report

Recycled pid blocks lock-taking verbs (999.128); Ship finalization-retry recovery unverified (999.129);
split-lock / flock / fsync / gate-waiter registration (999.130); stale gate answers not bound to a gate
incarnation, including the `resume` and late-`respond` paths (999.130); non-PATH test env mutations
(999.131); `gate list` vs `status` (999.132); Supervise re-scan rejection reusing its response for the
fall-through gate (intended, pinned by `rejecting_the_rescan_gate_records_nothing_and_falls_through`).

## Output format

```
## CONFIRMED
### C-n. <title>
**File:** path:line (every file:line involved)
**Mechanism:** step by step
**Reproduction:** concrete state and wrong outcome; commands and output if you ran any
**Severity:** critical | high | medium | low — and why

## SUSPECTED
### S-n. <title>
Same fields; say what evidence would confirm or refute it.

## CHECKED AND CLEAN
One line per writer / message / path you enumerated and found sound, with its file:line.
```

CONFIRMED needs exact lines and a concrete path from the current code. Cite `file:line` for every claim.
