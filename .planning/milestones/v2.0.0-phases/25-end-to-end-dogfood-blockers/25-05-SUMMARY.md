---
phase: 25-end-to-end-dogfood-blockers
plan: 05
subsystem: infra
tags: [rust, git, preflight, staleness, self-dogfood]

# Dependency graph
requires:
  - phase: 25-03
    provides: the hoisted `enforce_build_staleness` call site in `commands::start` (shares `commands.rs` and `start()`, no file-content overlap)
provides:
  - "base_ref_currency / ensure_base_ref_current (preflight.rs): a currency probe for a base branch against its remote-tracking ref, fetch-then-compare, ordered before the existing phase-reachability guard"
  - "the Behind arm's operator-adjudicated fast-forward-when-safe-else-refuse-loudly behavior (999.51/D-18a)"
affects: [25-06, 25-07]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Currency probe as a sibling of an existing reachability probe: same enum shape (Current/Ahead/Behind{count}/Diverged/Undeterminable), same fail-open-where-blind Undeterminable contract, same pure-message-builder + enforcement-wrapper split"
    - "Fetch-then-compare with a soft-failing fetch: `git fetch --quiet <remote> <base>` updates only the remote-tracking ref (never the local branch/working tree), and its failure degrades to a warning rather than blocking — the comparison then proceeds against whatever the remote-tracking ref currently resolves to"
    - "Safe fast-forward via `git update-ref refs/heads/<base> refs/remotes/<remote>/<base>` — advances a ref without touching the working tree, gated on the base not being the currently checked-out branch"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "D-18a (operator, 2026-07-27, pre-adjudicated by the plan's <resolved_decision> block — no checkpoint in this plan): Behind fast-forwards when the base is not currently checked out (lossless — Behind already establishes strict ancestry), else refuses loudly. Diverged always refuses. Current/Ahead/Undeterminable all proceed."
  - "Remote name hardcoded to `origin`, matching devflow_core::git::origin_main_ancestor_status's existing convention — this project has no remote-name configuration knob."
  - "The two 'NO git fetch' doc comments at commands.rs (release --check's module doc and check_divergence's doc comment) are updated in place to record that the no-fetch property is scoped to release --check and is reversed for the start path by 999.51's ensure_base_ref_current, per the plan's explicit instruction not to leave the two comments reading as a contradiction."

patterns-established:
  - "A currency probe precedes a reachability probe at the same call site, because a stale base is the most common *cause* of what a reachability probe reports as a *symptom* (phase heading absent) — ordering is load-bearing, not incidental."

requirements-completed: ["25a"]

coverage:
  - id: D1
    description: "devflow start closes 999.51's silent 'heading present but code stale' case: ensure_base_ref_current runs before ensure_phase_reachable_on_base, fast-forwards a safely-behind base and proceeds unattended, and refuses loudly on divergence or an unsafe fast-forward"
    requirement: "25a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_is_current_when_local_equals_remote"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_behind_and_not_checked_out_fast_forwards_and_proceeds"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_behind_and_checked_out_refuses_with_actionable_message"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_behind_fast_forward_failure_falls_through_to_refusal"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_is_ahead_for_unpushed_local_work"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_is_diverged_when_local_and_remote_both_moved_independently"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_is_undeterminable_with_no_remote_configured"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_fetch_failure_falls_back_to_existing_remote_ref"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::currency_message_contains_no_absolute_path"
        status: pass
    human_judgment: false

# Metrics
duration: ~45min
completed: 2026-07-27
status: complete
---

# Phase 25 Plan 05: Base-Ref Currency Probe Summary

**Added `base_ref_currency`/`ensure_base_ref_current` to `preflight.rs` (siblings of the existing phase-reachability probe) and wired the call immediately before `ensure_phase_reachable_on_base` in `commands::start`, closing 999.51's silent "heading present but code stale" case — the Behind arm fast-forwards a safely-behind base and proceeds unattended, or refuses loudly per the operator's 2026-07-27 adjudication (D-18a).**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-07-27 (session start)
- **Completed:** 2026-07-28T01:04:33Z
- **Tasks:** 1 of 1 completed
- **Files modified:** 2 (`preflight.rs`, `commands.rs`)

## Accomplishments

- `BaseRefCurrency` enum (`Current`/`Ahead`/`Behind{count}`/`Diverged`/`Undeterminable`), `base_ref_currency` (the pure-ish probe — its only side effect is the soft-failing `git fetch`), `stale_base_message` (pure message builder, WR-02-clean), and `ensure_base_ref_current` (the enforcement wrapper) all added to `preflight.rs`, mirroring `PhaseReachability`/`phase_reachability_on_base`/`unreachable_message`/`ensure_phase_reachable_on_base`'s existing shape.
- `base_ref_currency` fetches (`git fetch --quiet origin <base>`, soft-failing — a spawn error or non-zero exit prints a warning and the comparison proceeds against whatever `origin/<base>` currently resolves to) then compares in both directions via `git merge-base --is-ancestor`, distinguishing Current/Ahead/Behind/Diverged, with the Behind commit count from `git rev-list --count`.
- `ensure_base_ref_current` implements the operator's 2026-07-27 D-18a adjudication verbatim: `Current`/`Ahead` proceed silently, `Undeterminable` proceeds with a warning (fail-open-where-blind, matching the sibling probe's contract), `Diverged` always refuses (message names both refs), and `Behind` fast-forwards via `git update-ref refs/heads/<base> refs/remotes/origin/<base>` only when `<base>` is not the currently checked-out branch — any failure (checked out, spawn error, non-zero exit) falls through to the same actionable refusal message (`stale_base_message`), which names the base, the remote-tracking ref, the commit count, and a runnable repair command, with no absolute filesystem path.
- Wired `ensure_base_ref_current(project_root, DEVELOP)?;` into `commands::start`, immediately before the existing `ensure_phase_reachable_on_base(project_root, phase, DEVELOP)?;` call — extended the doc comment above the reachability call (now preceded by the new currency call) to record the ordering and why it is load-bearing (a stale base is the most common cause of a phase heading appearing absent).
- Updated the two "NO `git fetch`" doc comments in `commands.rs` (`release --check`'s module doc, and `check_divergence`'s doc comment) to record that the no-fetch property they describe is scoped to `release --check` and is reversed for the `start` path by `ensure_base_ref_current` (999.51) — per the plan's explicit instruction not to leave a future reader seeing the two doc blocks as contradicting each other.
- 9 new tests against a real clone+remote git fixture (`currency_fixture`, built with `devflow_core::test_support::git_command`, never a bare git command): Current, Behind+not-checked-out (fast-forwards, asserts the local ref now equals the remote-tracking ref by value), Behind+checked-out (refuses, asserts the message names both refs, the count, and contains no absolute path), Behind+failed-fast-forward (a pre-seeded `.lock` file forces `git update-ref` to fail — falls through to the same refusal), Ahead (unpushed local work, not staleness), Diverged (mutual non-ancestors, refuses naming both refs), Undeterminable (no remote configured, proceeds), fetch-failure-falls-back (the remote directory is deleted after clone so the live fetch fails, but the already-resolved `origin/develop` ref is still used — proves the check doesn't collapse to Undeterminable on a fetch failure), and `stale_base_message`'s own no-absolute-path assertion.

## Task Commits

Each task was committed atomically:

1. **Task 1: Base-ref currency probe, wired ahead of the reachability guard** - `266ecab` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified

- `crates/devflow-cli/src/preflight.rs` — Added `BaseRefCurrency`, `base_ref_currency`, `stale_base_message`, `ensure_base_ref_current` (siblings of the existing phase-reachability probe), plus 9 new tests and their shared fixture helpers (`currency_fixture`, `advance_remote`, `run_git`)
- `crates/devflow-cli/src/commands.rs` — Imported `ensure_base_ref_current`; inserted the call in `start()` immediately before `ensure_phase_reachable_on_base`, with an extended doc comment recording the ordering rationale; updated the `release --check` module doc and `check_divergence`'s doc comment to record the no-fetch property's start-path reversal (999.51)

## Decisions Made

- **D-18a** (from the plan's pre-resolved `<resolved_decision>` block, operator 2026-07-27): `Behind` fast-forwards when the base is not the currently checked-out branch (losslessly — `Behind` already establishes strict ancestry, no divergence, no local commits at risk), else refuses loudly. This plan carried no `checkpoint:decision` — the operator's adjudication was already recorded in the plan before execution began, so Task 1 implemented it directly.
- Remote name hardcoded to `"origin"` — matches `devflow_core::git::origin_main_ancestor_status`'s existing convention; this project has no remote-name configuration knob to consult instead.
- The fast-forward uses `git update-ref` (not `git merge --ff-only` or a checkout+merge), because it advances the ref without touching the working tree — the reason the "not currently checked out" precondition alone is sufficient for safety, matching the plan's action instructions exactly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test naming collision with the plan's own literal source-assertion command**
- **Found during:** Task 1, running the plan's `<acceptance_criteria>` source assertion `rg -c 'fn base_ref_currency' crates/devflow-cli/src/preflight.rs` (expected: returns 1)
- **Issue:** My first draft named the new tests and their fixture helper with a `base_ref_currency_*` prefix (e.g. `fn base_ref_currency_is_current_when_local_equals_remote`, `fn base_ref_currency_fixture`). Because `rg -c 'fn base_ref_currency'` matches on substring (not a whole-identifier boundary), every one of those test/helper function definitions also matched the pattern, inflating the count to 10 instead of the expected 1 for the production `base_ref_currency` probe function itself.
- **Fix:** Renamed all 8 new test functions and the fixture helper to a `currency_*` prefix (e.g. `currency_is_current_when_local_equals_remote`, `currency_fixture`) — following the sibling reachability probe's own test-naming convention, which similarly uses a shortened `reachability_*` prefix rather than repeating the full `phase_reachability_on_base` function name.
- **Files modified:** `crates/devflow-cli/src/preflight.rs` (test names only, no production code changed)
- **Verification:** `rg -c 'fn base_ref_currency' crates/devflow-cli/src/preflight.rs` now returns exactly `1`; `rg -c 'fn ensure_base_ref_current'` returns exactly `1`; all 27 `preflight::` tests still pass after the rename.
- **Committed in:** `266ecab` (Task 1 commit — caught and fixed before the commit, not as a follow-up)

---

**Total deviations:** 1 auto-fixed (test-naming collision against the plan's own literal grep-based acceptance criterion). No scope creep — the fix touched only test identifiers, not behavior.

## Issues Encountered

**Third, previously-undocumented clippy failure discovered (out of scope, not fixed):** `cargo clippy --workspace --all-targets -- -D warnings` fails on a THIRD line beyond the two documented in this plan's `<known_red_baseline>` — `crates/devflow-cli/src/pipeline_gate.rs:836`, a call to the now-`#[deprecated]` `devflow_core::version::count_git_tags` (`superseded by reachable_semver_baseline (D-07)`) with no `#[allow(deprecated)]`. Confirmed via `git diff --stat HEAD -- crates/devflow-cli/src/pipeline_gate.rs` (empty — I never touched this file) and via `git log --oneline -- crates/devflow-cli/src/pipeline_gate.rs crates/devflow-core/src/version.rs`, which shows `9d33489 feat(25-01): wire D-10 refusal, deprecate superseded tag-count helpers` deprecated `count_git_tags` without migrating this call site. This is a genuine pre-existing gap inherited from Wave 1 (25-01), landed in the base commit (`34fa488c`) before I started — not caused by this plan, and `pipeline_gate.rs` is outside this plan's `files_modified` scope, so per the scope-boundary rule it was not fixed here. Reported per this plan's own instruction ("If you see any third failure... report explicitly"): **clippy has 2 pre-existing failures, not 1** — `commands.rs:3380` (`looks_like_devflow_process`, owned by 25-07 per this plan's baseline note) and `pipeline_gate.rs:836` (`count_git_tags`, apparently unowned by any plan in this phase's Wave 2/3 as currently planned — worth flagging to the orchestrator/25-06/25-07 executors or a future gap-closure plan, since NEITHER of those two plans' file lists include `pipeline_gate.rs` per their own `files_modified`).
- `cargo test --workspace --no-fail-fast`: only the documented pre-existing failure remains — `pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` (196 passed / 1 failed in that target; 0 new failures introduced; total workspace: 0 failed elsewhere).
- No subprocess timeout idiom exists in this codebase for the `git fetch` call (per the plan's request to record this rather than invent one) — `base_ref_currency`'s fetch has no bounded timeout; on a genuinely hung network call it would block `devflow start` indefinitely. This mirrors the residual already accepted for `origin_main_ancestor_status`'s sibling call pattern in `devflow-core/src/git.rs` and is not addressed by this plan.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None.

## Threat Flags

None beyond what this plan's own `<threat_model>` already registered (T-25-40 through T-25-44, all dispositioned in the plan itself).

## Next Phase Readiness

- 25-06 and 25-07 (Wave 3) are unblocked by this plan — neither's declared `files_modified` overlaps `preflight.rs`, and this plan's one addition to `commands.rs` (the `start()` function body plus two unrelated doc comments) does not touch either plan's target lines (`pipeline_gate.rs:860`'s fixture, or `commands.rs`'s `looks_like_devflow_process` call site near line 3380).
- **Flag for 25-06/25-07 or a future gap-closure plan:** `pipeline_gate.rs:836`'s deprecated `count_git_tags` call (Wave 1/25-01 gap) is a clippy-blocking failure not currently owned by any planned Wave 2/3 plan's file list — confirm before Ship that this line gets migrated to `reachable_semver_baseline` (or explicitly `#[allow(deprecated)]`'d with a tracked follow-up) alongside the two already-known failures, or `cargo clippy --workspace --all-targets -- -D warnings` will still be red after 25-06 and 25-07 both land.
- No blockers for downstream plans from this plan's own diff.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-27*

## Self-Check: PASSED

- FOUND: `.planning/phases/25-end-to-end-dogfood-blockers/25-05-SUMMARY.md`
- FOUND commit: `266ecab` (Task 1: base-ref currency probe, wired ahead of reachability)
- Verified `cargo test --package devflow --bin devflow preflight::` → 27 passed, 0 failed
- Verified `cargo test --workspace --no-fail-fast` → only the documented pre-existing failure (`finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`) remains; 0 new failures
- Verified `cargo clippy --workspace --all-targets -- -D warnings` → only the two pre-existing deprecated-function errors remain (`pipeline_gate.rs:836`, `commands.rs:3380`), neither touched by this plan's diff (confirmed via `git diff --stat HEAD -- crates/devflow-cli/src/pipeline_gate.rs` = empty)
- Verified `cargo fmt --check` → clean
- Verified all acceptance-criteria source assertions (`fn base_ref_currency` count=1, `fn ensure_base_ref_current` count=1, ordering, `Undeterminable` Ok arm, WR-02 no-path in `stale_base_message`, fetch-qualification doc comments)
