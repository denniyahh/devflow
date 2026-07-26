---
phase: 23-end-to-end-dogfood
plan: 14
subsystem: infra
tags: [devflow, dogfood, acceptance-test, preconditions, recovery-point, gap-closure, release-versioning]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood (23-13)
    provides: the reachability guard merged onto `origin/develop` via PR #32, plus the load-bearing finding that local `develop` was stale relative to `origin/develop`
provides:
  - Every one of plan 23-10's seven behavioural checks re-run against the post-merge tree and binary, each with verbatim output and exit code
  - Local `develop` fast-forwarded to `origin/develop` (pure fast-forward, 0 ahead / 120 behind, confirmed via non-checkout `git fetch origin develop:develop`) so the guard's own base-branch precondition is no longer false-refused
  - An eighth reachability check (ROADMAP heading + phase directory) verified read-only against `origin/develop`, alongside the record that the shipped guard now enforces the same condition structurally
  - A fresh, remote-only recovery ref `recovery/pre-23-15-acceptance-0dad20d` on `origin`, restore rehearsed in a throwaway clone, real pull-request restore path with ~2-minute CI-wait latency recorded
  - The operator's verbatim PROCEED authorization for 23-15's one-way acceptance launch, recorded against `origin/develop` SHA `0dad20d`
  - A pre-run `compute_version` finding (predicted `~1.11.338`, growing to `~1.11.339`+) re-verified independently against source and git history, with the orchestrator's stated root cause (a `--candidates=10` truncation) corrected to the actual mechanism (`git describe`'s nearest-tag-by-commit-distance heuristic colliding with this repo's main/develop sync-merge topology)
affects: [23-15]

# Tech tracking
tech-stack:
  added: []
  patterns: ["throwaway git clone for recovery-ref restore rehearsal, bounding the blast radius of a destructive reset test"]

key-files:
  created: []
  modified:
    - .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP-2.md

key-decisions:
  - "Fast-forwarded local `develop` to `origin/develop` inside this plan (Task 1) rather than deferring again, since it is a pure fast-forward (0 ahead / 120 behind, strict ancestor, git itself refused the non-checkout fetch refspec unless it was a fast-forward) and 23-15 has no step that re-checks or repairs the gap 23-13 flagged and declined to fix in place."
  - "Operator selected PROCEED with a predicted version of `1.8.2`, explicitly informed that `compute_version` will actually produce `~1.11.339` and that the resulting mismatch is itself the accepted finding, not an unforeseen surprise (D-07 is not re-litigated by this checkpoint; only launch-timing against this specific tree state was decided)."
  - "Corrected the orchestrator's stated root cause for `git describe` picking `v1.4.0` over `v1.8.1`: re-verification with `--candidates=20/50` and `--debug` disproved the candidate-limit explanation; the actual cause is `git describe`'s nearest-tag-by-commit-distance heuristic interacting with this repo's main/develop sync-merge topology, where `v1.4.0` sits on the develop-side lineage and `v1.5.0`-`v1.8.1` sit on the main-side lineage of squash-then-sync release commits (`merge-base --is-ancestor v1.4.0 v1.8.1` exits 1 — they are not on the same chain)."

patterns-established: []

requirements-completed: [23-acceptance]

coverage:
  - id: D1
    description: "All seven of plan 23-10's behavioural checks re-run against the post-merge binary and tree, each with verbatim output and exit code, none carried forward"
    requirement: "23-acceptance"
    verification:
      - kind: other
        ref: "23-ACCEPTANCE-SETUP-2.md 'The seven behavioural checks' section — commands 1-7, each with EXIT= recorded"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow evidence --phase 24 --require-shipped exits non-zero pre-run, recorded as the baseline 23-15's post-run check is compared against"
    requirement: "23-acceptance"
    verification:
      - kind: other
        ref: "23-ACCEPTANCE-SETUP-2.md check 5: EXIT=1, labelled 'PRE-RUN BASELINE FOR 23-15'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Fresh recovery ref cut on origin at the origin/develop tip, distinct from the spent recovery/pre-23-11-acceptance-e0f87c2, no local copy created"
    requirement: "23-acceptance"
    verification:
      - kind: other
        ref: "git ls-remote origin refs/heads/recovery/pre-23-15-acceptance-0dad20d matches origin/develop SHA; git branch --list 'recovery/pre-23-15-acceptance-*' empty"
        status: pass
    human_judgment: false
  - id: D4
    description: "Restore path from the recovery ref rehearsed in a throwaway clone (byte-identical restore proven) and the real protected-branch pull-request restore path recorded with its latency"
    requirement: "23-acceptance"
    verification:
      - kind: other
        ref: "23-ACCEPTANCE-SETUP-2.md 'Throwaway-clone restore rehearsal' (git diff --stat empty) and 'The real restore path' (gh run list shows both required checks under 2 minutes)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Phase 24 reachability from origin/develop verified both read-only (human check) and structurally (23f guard at launch)"
    requirement: "23-acceptance"
    verification:
      - kind: other
        ref: "23-ACCEPTANCE-SETUP-2.md eighth check: quoted '### Phase 24:' heading line and '.planning/phases/24-...' tree path"
        status: pass
    human_judgment: false
  - id: D6
    description: "Operator authorized the one-way acceptance launch against a named origin/develop SHA with a recorded predicted version"
    requirement: "23-acceptance"
    verification: []
    human_judgment: true
    rationale: "A checkpoint:decision authorization is an operator judgment call by design (reversibility gate for 23-15's one-way merge/version/changelog write) — not something an automated check can substitute for. Recorded verbatim in 23-ACCEPTANCE-SETUP-2.md Task 3 for human audit."

# Metrics
duration: N/A (continuation agent — resumed at Task 3 only; Tasks 1-2 duration recorded in prior agent's session)
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 14: Acceptance Retry Setup (Round 2) Summary

**Re-measured every acceptance precondition against the post-merge tree, fast-forwarded a stale local `develop`, cut a fresh remote-only recovery ref with a rehearsed restore path, and recorded the operator's verbatim PROCEED authorization alongside an independently re-verified `compute_version` defect finding (predicted `~1.11.339`, not the operator's guessed `1.8.2`).**

## Performance

- **Tasks:** 3 (Tasks 1-2 executed by a prior agent; this continuation agent recorded Task 3 only)
- **Files modified:** 1 (`23-ACCEPTANCE-SETUP-2.md`)

## Accomplishments

- All seven of plan 23-10's behavioural checks re-run against the post-merge binary (`b5db079a…6dc98`) and tree (`origin/develop` `0dad20d`), each with verbatim output and exit code — none copied forward from `23-ACCEPTANCE-SETUP.md`.
- Local `develop` was found stale (0 ahead / 120 behind `origin/develop`, per 23-13's carried-forward finding) and fast-forwarded in place via a non-checkout `git fetch origin develop:develop`, since it was a pure fast-forward with no divergent commits to lose.
- A pre-run baseline confirmed: `devflow evidence --phase 24 --require-shipped` exits `1` against `origin/develop` `0dad20d` — the number 23-15's post-run check is compared against.
- An eighth reachability check confirmed Phase 24's ROADMAP heading and phase directory exist on `origin/develop`, and recorded that the shipped 23f guard now also enforces this structurally.
- A fresh recovery ref, `recovery/pre-23-15-acceptance-0dad20d`, pushed to `origin` only (no local copy — `devflow cleanup` deleted the previous ref twice by ancestry per 23-FINDINGS §B2). Its restore was rehearsed in a throwaway clone (byte-identical restore proven via empty `git diff --stat`), and the real restore path — a revert landed through a protected-branch pull request, ~2 minutes of CI wait, no force-push or admin bypass available — was written out step by step.
- The two content preconditions (security artifact, no self-attested Ship claim) were re-decided against current source rather than copied forward: both remain "accepted unmitigated," but the mechanism behind Precondition A's disposition was corrected (the capability registry resolves `security` gate activation independently of the raw `.planning/config.json` key, which no longer exists as a literal top-level field).
- The full pre-run gate chain (`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`) ran clean: 608 passed / 0 failed across 17 binaries, clippy and fmt both exit 0.
- **Task 3 (this continuation agent):** recorded the operator's verbatim three-exchange authorization — PROCEED, predicted version `1.8.2`, explicit acknowledgment that the actual result is expected to differ and that the mismatch is itself the intended finding — against `origin/develop` SHA `0dad20d` (re-confirmed unchanged since Task 2).
- Independently re-verified the `compute_version` pre-run finding rather than accepting it on trust: confirmed major=`1` (`Cargo.toml`), minor=`11` (raw `git tag` count, no semver filter), patch=`338` (`git describe --tags --abbrev=0` resolves to `v1.4.0`, not `v1.8.1`) — computed version `≈1.11.338`, growing with the run's own commits. **Corrected the orchestrator's stated root cause**: `--candidates=20`/`--candidates=50` and `git describe --debug` disprove the "10-candidate limit" explanation; the real cause is `git describe`'s nearest-tag-by-commit-distance heuristic interacting with this repository's main/develop sync-merge topology — `v1.4.0` sits on the develop-side lineage while `v1.5.0`-`v1.8.1` sit on the main-side (squash-then-sync) lineage, and `merge-base --is-ancestor v1.4.0 v1.8.1` confirms they are not on the same chain, so `v1.4.0`'s raw commit-distance to `HEAD` (338) is genuinely smaller than `v1.8.1`'s (656).

## Task Commits

Tasks 1-2 were executed and committed by a prior agent before this continuation agent was spawned:

1. **Task 1: Re-run every precondition against the post-merge tree** - `e8ff9a9` (docs)
2. **Task 2: Cut fresh recovery ref on origin, rehearse restore** - `8ecc173` (docs)
3. **Task 3: Authorize the acceptance launch (this agent)** - `5e865f9` (docs)

**Plan metadata:** (this commit, following this SUMMARY)

## Files Created/Modified
- `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP-2.md` - tree-state re-measurement, seven behavioural checks, eighth reachability check, fresh recovery ref + restore rehearsal, pre-run gate-chain baseline, and the operator's verbatim authorization with the pre-run version finding

## Decisions Made
- Fast-forwarded local `develop` to `origin/develop` in Task 1 (pure fast-forward, git-verified via the non-checkout fetch refspec's own semantics) rather than deferring the fix a third time.
- Operator selected PROCEED, predicted `1.8.2`, with full knowledge that `compute_version` will actually produce `~1.11.339` — the mismatch is the accepted finding, not a re-litigation of D-07.
- Corrected the pre-run finding's stated root cause from a candidate-limit truncation to the actual git-describe/merge-topology interaction, per this task's explicit instruction to re-verify rather than copy measurements on trust.

## Deviations from Plan

None - plan executed exactly as written. Task 3's checkpoint content (the version-prediction root-cause explanation) required independent re-verification per the continuation agent's explicit instructions, which surfaced a correction to the *mechanism* behind an already-correct empirical prediction — this is the re-verification behavior the resume instructions explicitly asked for, not a deviation from the plan's tasks.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

23-15 may proceed: the operator has authorized the one-way acceptance launch against `origin/develop` SHA `0dad20d`, a fresh recovery ref (`recovery/pre-23-15-acceptance-0dad20d`) protects the pre-run state with a rehearsed and written-down restore path, and the pre-run `devflow evidence --phase 24 --require-shipped` baseline (exit 1) is recorded for comparison against 23-15's post-run check. 23-15 should compare its actual resulting version against both the operator's stated prediction (`1.8.2`) and the independently pre-computed estimate here (`~1.11.339`) — a match against the latter, not the former, is the expected and accepted outcome per the operator's own authorization.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*
