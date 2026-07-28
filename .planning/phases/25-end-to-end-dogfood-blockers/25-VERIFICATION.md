---
phase: 25-end-to-end-dogfood-blockers
verified: 2026-07-28T16:19:31Z
status: gaps_found
score: 8/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 6/9
  gaps_closed:
    - "25c (gate) — D-09 major-bump gate now fires in the default worktree execution path (25-08: execution_root threading + non-short-circuiting aggregation, both independently re-confirmed by direct source read and a passing targeted test run)"
    - "25c (anchor) — release_range_start now anchors correctly under the trunk-commit-between-tag-and-sync-merge topology (25-09: full ancestry-path walk, independently re-confirmed by source read and a passing targeted test run)"
  gaps_remaining: []
  regressions:
    - "25a — a NEW defect (CR-02, not present in the previous verification's scope) was found in the same unit: ensure_base_ref_current's fast-forward can itself corrupt the base ref it exists to keep current"
    - "25d — a NEW defect (CR-01, not present in the previous verification's scope, introduced by this phase's own 25d/999.44 work) was found: the doctor/gate-sweep stray-process surface can direct an operator to SIGKILL a live, registered process"
  note: >
    The two Critical findings this run treats as blocking (CR-01, CR-02 per the current
    25-REVIEW.md) are DIFFERENT defects from the two the previous 25-VERIFICATION.md tracked
    under the same CR-01/CR-02 labels (those were the D-09 gate short-circuit and the
    release_range_start anchor bug, both now fixed by 25-08/25-09 and closed above). The label
    reuse is REVIEW.md's own re-numbering after a full re-review of the post-gap-closure tree,
    not a re-verification of the same bugs. Additionally, this run supersedes the previous
    verification's truth 7 (25e), which was recorded as PRESENT_BEHAVIOR_UNVERIFIED on a
    "structurally removed" premise that 25-CI-OBSERVATION.md's 2-of-2 live reproduction later
    falsified. That premise has since been replaced by 25-11's measured site census + exec-
    visibility barrier, 25-12's production age floor, and 25-13's 11-observation CI-on-branch
    streak with explicit, verbatim human sign-off (2026-07-28) that engaged with the evidence's
    stated residual — the strongest form of closure this class of race admits, and this
    verifier's own independent corroboration (gh run view, git merge-base --is-ancestor) matches
    the artifact's claims. Truth 7 is recorded VERIFIED (human-verified, residual stated), not
    upgraded to "proven absent."
gaps:
  - truth: "25a — a run starts on a current base ref SAFELY: the fast-forward repair cannot itself corrupt the base ref it exists to keep current"
    status: failed
    reason: >
      Independently confirmed by direct source read of preflight.rs:424-481
      (`ensure_base_ref_current`'s `Behind` arm), not merely cited from 25-REVIEW.md's CR-02.
      The fast-forward is performed with `git update-ref refs/heads/{base}
      refs/remotes/{remote_ref}` and NO `<oldvalue>` argument — an unconditional write that can
      move `{base}` BACKWARDS onto a non-descendant if anything advances the local ref between
      `base_ref_currency`'s read and this write (a concurrent `devflow start`, an operator, a
      hook), while printing a false "N commit(s) fast-forwarded" success message. Separately,
      the precondition that guards the write — "not currently checked out" — is tested with
      `git symbolic-ref --quiet --short HEAD` run ONLY in `project_root`, i.e. it sees just the
      one worktree `devflow start` is invoked from. `git update-ref`, unlike `git branch -f`,
      carries no checked-out-branch protection of its own. This repository routinely runs
      several linked worktrees at once (confirmed live during this verification: a phase-07
      worktree at `/tmp/.tmpYzOOmM/.worktrees/phase-07` was actively running) — if `develop` is
      checked out in ANY worktree other than the one `project_root` resolves to, this code moves
      the ref out from under that worktree silently, leaving its HEAD, index and working tree
      out of sync. 25a exists precisely because a human previously had to repair the base ref by
      hand before `devflow start` would launch; a repair mechanism that can itself corrupt the
      ref does not close that requirement — it adds a second, silent way to reach the same
      broken state.
    artifacts:
      - path: "crates/devflow-cli/src/preflight.rs"
        issue: "ensure_base_ref_current (:445-479, Behind arm): git update-ref with no <oldvalue> guard (unconditional write); checked-out precondition (:447-454) probes only project_root's own HEAD, not other worktrees"
    missing:
      - "Pass <oldvalue> to git update-ref — the resolved local base SHA at check time — so the write refuses (rather than silently discarding) if base moved since base_ref_currency's own check"
      - "Replace the single-worktree symbolic-ref probe with a repository-wide check (git worktree list --porcelain, scanning for `branch refs/heads/<base>`), or attempt `git branch -f <base> <remote_ref>` first — it already refuses on its own when base is checked out in ANY worktree, which is exactly the precondition the current code is trying and failing to establish"
      - "A regression test with base checked out in a SECOND worktree (not project_root) — asserting the fast-forward refuses rather than silently moving the ref out from under it"
  - truth: "25d — a stalled run recovers without kill(1), and the recovery mechanism never SIGKILLs a live, registered (non-orphaned) process"
    status: failed
    reason: >
      Independently reproduced LIVE on this machine during this verification, not merely cited
      from 25-REVIEW.md's CR-01. `discover_stray_devflow_processes` (agent.rs:393-446) is a
      purely structural `/proc` census with no orphan test of any kind — it matches on argv
      shape and euid only. `build_stray_process_findings` (commands.rs:3023-3043) then asserts,
      for every match, that it "is running but reachable through no registry entry, lock file,
      or state file" and recommends `devflow gate sweep --reap-strays` (commands.rs:3039). Ran
      `devflow doctor --json` on this machine: 40 `stray_processes` findings. Cross-referenced
      pid 596367 (one of them, labelled "monitor wrapper") against
      `/tmp/.tmp4T6jFk/.devflow/state-12.json`: that file's `monitor_pid` field IS 596367 — a
      live, currently-running phase-12 monitor wrapper, reachable through a real state file.
      `doctor`'s claim that it is reachable through "no registry entry, lock file, or state
      file" is false for this specific process, right now. `gate_sweep`'s `--reap-strays` pass
      (commands.rs:1149-1232) feeds the SAME unfiltered census straight to
      `reap_stray_candidates` gated only by `agent::STRAY_MIN_AGE` (a 2-second age floor that
      exists to bound the fork/exec cmdline-inheritance race — 999.47's mechanism — and has no
      bearing on registry-reachability). A monitor wrapper minutes old sails past that floor.
      Following doctor's own printed repair would SIGKILL pid 596367; SIGKILL is uncatchable, so
      the wrapper's `trap cleanup TERM INT` never fires, orphaning its own trailing `devflow
      advance` child — manufacturing exactly the orphan class 999.44 exists to eliminate.
      Additionally confirmed: `gate_sweep`'s stray pass ignores `--root` entirely (comment at
      commands.rs:1143-1145 states this explicitly), so a sweep scoped to one root still reaps
      the whole machine's strays; and `main.rs:389-397`'s `--reap-strays` help text describes
      the behaviour as clearing "STATE-ORPHANED processes," which is not the semantics this
      code has. The TERM->KILL escalation, death verification, and identity re-confirmation
      primitives themselves (`terminate_and_verify`, `is_same_process`) are sound in isolation
      (see truth 6) — the defect is entirely in what gets handed to them.
    artifacts:
      - path: "crates/devflow-cli/src/commands.rs"
        issue: "build_stray_process_findings (:3023-3043) asserts unverified orphan-ness; collect_stray_process_findings (:3048-3050) and gate_sweep's stray pass (:1149-1232) feed the unfiltered census with no registry-reachability check; the stray pass ignores --root (:1143-1151)"
      - path: "crates/devflow-core/src/agent.rs"
        issue: "discover_stray_devflow_processes (:393-446) is structural-only by design (documented, not itself a bug) — but has no counterpart that filters against registry::load_roots()'s reachable pids before either surface acts on it"
      - path: "crates/devflow-cli/src/main.rs"
        issue: "--reap-strays help text (:389-397) describes clearing 'STATE-ORPHANED processes,' semantics the implementation does not have"
    missing:
      - "Filter the census against registry::load_roots()'s reachable pids (each root's state.monitor_pid and lock::holder) before either reporting a match as state-orphaned in doctor or signalling it in gate_sweep — a pid a live registry entry still reaches is by definition not a stray"
      - "Honour --root in the stray pass (scope the reachable-pid filter to the given root), or document loudly at the call site why it cannot be scoped"
      - "Reword doctor's detail string and main.rs's --reap-strays help text once the corrected scope is known"
      - "A regression test with a live, registry-reachable pid (a real state file naming it as monitor_pid) present alongside a genuine orphan in the same discovery pass, asserting only the orphan is reported/reaped"
deferred: []
---

# Phase 25: End-to-End Dogfood Blockers Verification Report

**Phase Goal:** Make an unattended `devflow start --phase N --agent claude --mode auto --yes-ship` run reach a completed Ship stage without a human touching it, by closing the four things that currently prevent it (a run starts on a current base — 25a; progresses through all stages — 25b; finishes with correct artifacts — 25c; a stalled run recovers without `kill(1)` — 25d), plus 25e (CI-throughput flake), 25f (docs drift), and 999.38 (PATH race).
**Verified:** 2026-07-28T16:19:31Z
**Status:** gaps_found
**Re-verification:** Yes — after gap closure (25-08/25-09 closed the prior report's two gaps; this run independently re-derived the whole phase against the post-gap-closure tree and found two NEW, unrelated Critical defects — see `re_verification.regressions` above)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 25a — a run starts on a current base ref, **safely** | ✗ FAILED | `ensure_base_ref_current`'s fast-forward (`preflight.rs:456-473`) uses `git update-ref` with no `<oldvalue>` guard (unconditional write) and a checked-out precondition that only probes `project_root`'s own HEAD. Confirmed by direct source read; matches REVIEW.md's CR-02 scratch-repo reproduction. See gaps. |
| 2 | 25b — `enforce_build_staleness` is adjudicated once per run (at `start`), never re-invoked mid-run | ✓ VERIFIED | Call present at `commands.rs:278`, after `state.worktree_path` is set (`:248`); absent from `pipeline_launch.rs`. `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` present and passing (part of the 688/0 full run). |
| 3 | 25c (derivation) — `compute_version` derives from (reachable semver tag, conventional-commit classification), refuses on unreachable baseline, floors correctly | ✓ VERIFIED | `version.rs`'s `reachable_semver_baseline`/`highest_semver_tag`/`classify_range_bump`/`VersionError::UnreachableBaseline` present and composed; unchanged from the previous verification's finding. |
| 4 | 25c (gate) — a major version bump opens a gate and never ships unattended, **in the default (worktree) execution path** — closes the previous verification's GAP 1 | ✓ VERIFIED | `preflight_major_bump_check` now evaluates `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` (`preflight.rs:629`), confirmed by direct source read; `generic_preflight_checks` (`:763-778`) aggregates all three checks (major-bump first) instead of `?`-short-circuiting, confirmed present. Spot-run both `preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head` and `preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first` -> both `1 passed`. |
| 5 | 25c (anchor) — `release_range_start`'s commit-range anchor excludes pre-release history across realistic release topologies — closes the previous verification's GAP 2 | ✓ VERIFIED | `release_range_start` (`version.rs:299-360`) now walks the FULL `--ancestry-path` list rather than only its first commit, confirmed by direct source read; matches the fix REVIEW.md's own CR-03 sketch proposed. Spot-run `version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge` -> `1 passed`. |
| 6 | 25d (primitives) — bounded TERM->KILL escalation with verified death; registry-independent discovery; identity re-confirmation before signalling | ✓ VERIFIED | `terminate_and_verify`, `discover_stray_devflow_processes`, `process_start_time`, `is_same_process` present in `agent.rs`; `process_age`/`STRAY_MIN_AGE` (25-12) correctly bound the fork/exec race and fail closed on unreadable/non-finite age data. Independently re-derived from source (not merely REVIEW.md's own analysis): guard ordering (identity -> age -> dry_run -> signal) is correct, no branch signals a too-young or unknown-age candidate. |
| 7 | 25d (surface) — an operator using `devflow doctor` / `gate sweep --reap-strays` never has a live, registered process misreported as an orphan or destroyed | ✗ FAILED | **Live-reproduced during this verification, not merely cited from REVIEW.md.** `devflow doctor --json` on this machine currently reports 40 `stray_processes` findings; pid 596367 among them is independently confirmed to be the recorded `monitor_pid` in a real, live `state-12.json` — `doctor`'s claim that it is reachable through "no registry entry, lock file, or state file" is false for this exact process. The printed repair (`devflow gate sweep --reap-strays`) would SIGKILL it, bypassing its wrapper's `trap cleanup`, orphaning its own child. See gaps. |
| 8 | 25e — the 999.47 CI flake (cmdline-inheritance race) — human-verified against 11 observations, residual explicitly stated, not claimed proven absent | ✓ VERIFIED (human-verified) | 25-11's measured site census (`25-SITE-CENSUS.md`: 4 VULNERABLE-POSITIVE + 2 VACUOUS-NEGATIVE sites, all barriered with `wait_for_exec_visibility`) and 25-12's production age floor (`agent::STRAY_MIN_AGE`) confirmed present and wired (see `## Key Link Verification`). `25-CI-TRIALS.md` records 6 local push-gate + 5 CI `Test`-job trials, all green, all at `82328b3` — independently corroborated by this verifier: `gh run view 30371091367` confirms `conclusion: success` at the stated head SHA, and `git merge-base --is-ancestor 82328b3 origin/feature/phase-25` exits 0 (origin is NOT at the pre-fix `a5a068f`). A human explicitly engaged with the evidence's own stated limitation (CI's `Test` job is the less-sensitive shape) before approving, recorded verbatim in `25-13-SUMMARY.md` Part A (2026-07-28). The record's own vocabulary (`no_reproduction`, never `closed`) and residuals (~6.1% / ~0.05%) are preserved here, not upgraded. |
| 9 | 999.38 — the test-suite PATH race is de-raced | ✓ VERIFIED | `ENV_MUTEX` guard + `test_support::git_command` hermetic reads confirmed present at `staleness.rs:1044-1046` and its three siblings. |
| 10 | 25f — CONTRIBUTING.md's release procedure and the ROADMAP/PROJECT.md versioning-policy prose no longer drift from what 25c implements | ✓ VERIFIED (human sign-off recorded) | `CONTRIBUTING.md` uses the `-c user.signingkey=...` indirection and no longer claims `tag.gpgsign=false`; `ROADMAP.md`'s Acceptance paragraph and June-2026 ban bullet amended per D-06/D-15/D-16. `25-VALIDATION.md`'s three human-judgment rows signed off verbatim ("B: approved," 2026-07-28) in `25-13-SUMMARY.md` Part B, with the orchestrator's own independent corroboration of each row recorded alongside (not substituting for it). |

**Score:** 8/10 truths verified (0 present-but-behavior-unverified, 2 failed)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/preflight.rs::ensure_base_ref_current`/`base_ref_currency` | 25a currency probe, safe fast-forward | ⚠️ WIRED but unsafe | Present, wired at `commands.rs:154` before `ensure_phase_reachable_on_base` — but see truth 1/gaps: the fast-forward itself is unsafe |
| `crates/devflow-cli/src/commands.rs::start()` staleness call site | 25b one-shot gate | ✓ VERIFIED | Present at `:278`, after `worktree_path` set; removed from `pipeline_launch.rs` |
| `crates/devflow-core/src/version.rs::compute_version` + helpers | 25c derivation | ✓ VERIFIED | Present, composed correctly |
| `crates/devflow-cli/src/preflight.rs::preflight_major_bump_check`/`major_bump_check_applies` | 25c D-09 gate, worktree-scoped | ✓ VERIFIED | `execution_root` threading + aggregation confirmed present; previous gap closed |
| `crates/devflow-core/src/version.rs::release_range_start` | 25c anchor, topology-robust | ✓ VERIFIED | Full ancestry-path walk confirmed present; previous gap closed |
| `crates/devflow-core/src/agent.rs::terminate_and_verify`/`discover_stray_devflow_processes`/`process_age` | 25d primitives | ✓ VERIFIED | Present, sound in isolation, tested with real spawned children |
| `crates/devflow-cli/src/commands.rs::build_stray_process_findings`, `gate_sweep --reap-strays` | 25d CLI surface — orphan-safe | ✗ UNSAFE | Present and wired, but not registry-reachability-filtered — see truth 7/gaps |
| `crates/devflow-core/src/test_support.rs::wait_for_exec_visibility` | 25e exec-visibility barrier | ✓ VERIFIED | Present, wired at all 4 vulnerable-positive sites (`agent.rs` x3, `commands.rs` x1, `reap_strays_e2e.rs` x1) |
| `.planning/phases/25-end-to-end-dogfood-blockers/25-SITE-CENSUS.md`, `25-CI-TRIALS.md` | 25e evidence artifacts | ✓ VERIFIED | Present, internally consistent, independently corroborated (CI run status, ancestry) |
| `CONTRIBUTING.md`, `.planning/ROADMAP.md`, `.planning/PROJECT.md` | 25f docs | ✓ VERIFIED | Confirmed via grep; human sign-off recorded |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `commands.rs::start()` | `preflight.rs::ensure_base_ref_current` | direct call, before `ensure_phase_reachable_on_base` | ⚠️ WIRED, unsafe implementation | Ordering confirmed at `:154`/`:164`; the called function's own internals are the defect (truth 1) |
| `commands.rs::start()` | `staleness.rs::enforce_build_staleness` | direct call, after `worktree_path` set | ✓ WIRED | Confirmed at `:278` |
| `preflight.rs::generic_preflight_checks` | `preflight.rs::preflight_major_bump_check` | aggregated (non-short-circuiting) call | ✓ WIRED, correctly | Confirmed at `:763-778`; major-bump evaluated first |
| `preflight.rs::preflight_major_bump_check` | `version.rs::{highest_semver_tag, reachable_semver_baseline, release_range_start, classify_range_bump}` | all five calls use `execution_root` | ✓ WIRED, correctly | Confirmed at `:629-669` |
| `commands.rs::collect_stray_process_findings`/`gate_sweep`'s stray pass | `agent.rs::discover_stray_devflow_processes` | direct call, unfiltered | ⚠️ WIRED, unsafe | Confirmed at `:3048-3050` and `:1149-1151` — no registry-reachability filter interposed (truth 7) |
| `commands.rs::gate_sweep`'s stray pass | `commands.rs::reap_stray_candidates` -> `agent::STRAY_MIN_AGE` | direct call | ✓ WIRED, correctly (for what it bounds) | Confirmed at `:1151`; bounds the fork/exec race, not registry-reachability — the two are different hazards and only the first is closed |
| `agent.rs`/`commands.rs` census tests | `test_support::wait_for_exec_visibility` | barrier before every vulnerable-positive census read | ✓ WIRED | Confirmed at all 4 sites named in `25-SITE-CENSUS.md`'s `## Vulnerable sites` |

### Data-Flow Trace (Level 4)

Not applicable — this phase produces CLI/library logic, not UI components rendering dynamic data.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace regression | `cargo test --workspace --no-fail-fast` | `688 passed; 0 failed` (summed across all 19 test binaries) | ✓ PASS (matches orchestrator's independently-reported 688/0) |
| 25c D-09 gate fires in the worktree fixture (25-08's regression test) | `cargo test --package devflow --bin devflow preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| 25c D-09 gate aggregates reasons instead of short-circuiting (25-08's regression test) | `cargo test --package devflow --bin devflow preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| 25c anchor fix under the trunk-commit topology (25-09's regression test) | `cargo test --package devflow-core --lib version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| **CR-01 live reproduction** — a registered process is misreported as orphaned | `devflow doctor --json \| jq '.stray_processes \| length'` = 40; cross-referenced pid 596367 against `/tmp/.tmp4T6jFk/.devflow/state-12.json` | `state-12.json`'s `monitor_pid` IS 596367 — doctor's "reachable through no registry entry, lock file, or state file" claim is false for this pid, live, right now | ✗ FAIL (confirms gap, not a phase deliverable) |
| **CR-02 corroboration** — `origin/feature/phase-25` ancestry (unrelated to the defect itself, corroborates 25e's evidence) | `git merge-base --is-ancestor 82328b3 origin/feature/phase-25` | exit 0 | ✓ PASS (supports truth 8, not truth 1) |
| Debt markers in phase-touched files | `rg -n 'TBD|FIXME|XXX'` across agent.rs, version.rs, test_support.rs, commands.rs, preflight.rs, pipeline_launch.rs, pipeline_gate.rs, staleness.rs, main.rs, reap_strays_e2e.rs, CONTRIBUTING.md | no matches | ✓ PASS |

### Requirements Coverage

*This project has no `.planning/REQUIREMENTS.md`; tracked by unit identifier per the phase's own convention. Not reported as a gap.*

| Unit | Backlog ID | Description | Status | Evidence |
|------|-----------|--------------|--------|----------|
| 25a | 999.51/DEN-76 | Base-ref currency | ✗ NOT SATISFIED | Truth 1 — the repair mechanism itself is unsafe |
| 25b | 999.48/DEN-73 | Staleness hoist | ✓ SATISFIED | Truth 2 |
| 25c | 999.49/DEN-74 | Version derivation + major-bump gate + anchor | ✓ SATISFIED | Truths 3/4/5 — both previously-open gaps closed |
| 25d | 999.44/DEN-68 | Orphan process reaping | ✗ PARTIALLY SATISFIED | Truth 6 (primitives) satisfied; truth 7 (operator-facing surface) FAILED |
| 25e | 999.47/DEN-72 | Flaky test dead predicate | ✓ SATISFIED (human-verified) | Truth 8 |
| 25f | (no backlog ID) | CONTRIBUTING drift | ✓ SATISFIED (human sign-off) | Truth 10 |
| 999.38 | folded in | PATH race | ✓ SATISFIED | Truth 9 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/src/commands.rs` | 3023-3049, 1149-1232 | CR-01: unverified orphan-ness claim feeds a SIGKILL repair, `--root` ignored | 🛑 Blocker | See truth 7 and gaps |
| `crates/devflow-cli/src/preflight.rs` | 445-479 | CR-02: `git update-ref` with no `<oldvalue>` guard, single-worktree checked-out probe | 🛑 Blocker | See truth 1 and gaps |
| `crates/devflow-cli/src/main.rs` | 389-397 | `--reap-strays` help text names semantics ("STATE-ORPHANED") the implementation does not have | ℹ️ Info (subsumed by CR-01) | Part of the same fix scope, not separately gating |
| `crates/devflow-core/src/version.rs` | 338-349 | WR-01: `merge-base --is-ancestor` spawn/exit-128 errors collapse into the same `false` as a genuine negative, biasing the anchor over-inclusive on a transient git failure | ℹ️ Info (recorded, not gating) | Distinct from CR-03 (already fixed); latent, not currently reachable per REVIEW.md's own severity call — not re-litigated here |
| `crates/devflow-core/src/test_support.rs` | 101, 120 | WR-02: `wait_for_exec_visibility`'s guard (ii) compares against the caller's cmdline, not the actual parent's — sound for every current call site (all are direct parents) but not generalizably "unambiguous" as documented | ℹ️ Info (recorded, not gating) | No current call site is a non-parent caller |
| `crates/devflow-cli/src/staleness.rs` | 689-786 | WR-03: `mid_run_stage_transition_does_not_readjudicate_staleness` spawns a real monitor wrapper via `launch_stage_inner` and never reaps it — leaks a live, later-orphaned process on every `cargo test --workspace` run | ⚠️ Warning | Independently corroborated during this verification: 22 live `trap cleanup TERM INT` processes found on this machine before this run even started, several rooted at deleted `/tmp/.tmp*` paths — consistent with repeated unreaped test runs. Does not affect the phase's shipped behavior (test-only), but actively degrades confidence in any future CR-01 fix's own test suite until closed. |
| `crates/devflow-cli/src/commands.rs` | 3727-3762 | WR-04: `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` is flaky by construction — its premise (fixture age < 2s) races the same container load that produces the CR-01/25e failure class | ℹ️ Info (recorded, not gating) | Test-only; the underlying floor logic is verified correct via other means |

## Human Verification Required

None outstanding. The two items this phase's own workflow explicitly routed to a human (25e's CI-observation closure judgment, and 25f's three doc-drift rows) were both already discharged with verbatim, dated human sign-off recorded in `25-13-SUMMARY.md` (Parts A and B, 2026-07-28) — independently corroborated by this verifier (CI run status via `gh run view`, ref ancestry via `git merge-base`, and direct grep of the three 25f rows against current source). Nothing further requires a human decision to *evaluate*; the two gaps below (CR-01, CR-02) require code fixes, not a human judgment call — they are objectively demonstrated defects, one of them live-reproduced on this machine during this verification.

## Gaps Summary

Two Critical, source-confirmed defects remain — both newly surfaced by `25-REVIEW.md`'s full re-review of the post-gap-closure tree, and both independently re-confirmed here by direct source reading and, for CR-01, live reproduction (not merely trusting the review's own analysis):

**CR-01 (unit 25d):** `devflow doctor` and `devflow gate sweep --reap-strays` share a census (`agent::discover_stray_devflow_processes`) that is purely structural — it has no orphan test. `doctor` nonetheless asserts each match "is running but reachable through no registry entry, lock file, or state file" and names `--reap-strays` as the repair; `gate_sweep --reap-strays` acts on that same unfiltered census, gated only by a 2-second age floor that bounds an unrelated race (999.47's fork/exec window), not registry-reachability. Live-reproduced on this machine: 40 current `doctor` findings, one of them (pid 596367) independently confirmed to be a real, live phase-12 monitor wrapper named by its own state file's `monitor_pid` field. Following `doctor`'s own printed repair would SIGKILL it — bypassing its `trap cleanup` and orphaning its child, i.e. manufacturing exactly the orphan class 999.44 (this unit's own backlog origin) exists to eliminate. This means **unit 25d does not safely deliver "a stalled run recovers without `kill(1)`"** — it delivers a mechanism that can convert a healthy run into the very failure mode it was built to fix.

**CR-02 (unit 25a):** `ensure_base_ref_current`'s fast-forward writes `refs/heads/<base>` with `git update-ref` and no `<oldvalue>` guard (an unconditional write vulnerable to a race with anything else that moves the local ref), and its "not checked out" precondition inspects only the single worktree `devflow start` runs from — not the repository-wide set of linked worktrees this project routinely uses (confirmed live during this run: a `.worktrees/phase-07` worktree was active). This means **unit 25a's repair path can itself silently corrupt the base ref it exists to keep current** — the same class of problem 25a was written to eliminate (a human having to repair the base ref by hand), reintroduced one layer down.

**Everything else in this phase is solid.** Both of the previous verification's gaps (the D-09 gate's worktree-scope bypass, and `release_range_start`'s anchor regression under an untested topology) are closed, confirmed by direct source read and passing targeted regression tests, not merely by the executor's own SUMMARY claims. 25d's core primitives (`terminate_and_verify`, identity re-confirmation, the age floor) are independently sound — the defect is entirely in what gets handed to them, unfiltered. 25e's flake-closure claim, which this project's own prior verification over-claimed once already (`structurally removed`, later falsified 2/2 by a real container run), is this time backed by a measured site census, a bounded exec-visibility barrier at every vulnerable site, a production age floor, an 11-observation CI-on-branch streak, and an explicit, engaged, verbatim human sign-off — recorded honestly, with its stated residual (~6.1%/~0.05%) intact, never upgraded to a proof of absence. 25f's documentation fixes are confirmed correct by direct grep and are also human-signed. 999.38's PATH race is de-raced and confirmed hermetic. The full 688-test workspace suite is green.

**This looks like real, closeable work, not a design failure of the phase's approach** — REVIEW.md's own sketch fixes for both CR-01 and CR-02 are small, scoped to the exact files 25-02/25-05/25-07 already touched, and neither requires reopening any of the phase's other units.

---

*Verified: 2026-07-28T16:19:31Z*
*Verifier: Claude (gsd-verifier)*
