---
phase: 27-scrub-redirecting-git-environment-from-production-calls
plan: 06
subsystem: infra
tags: [rust, subprocess, security, git, env-hygiene, validation, census]

# Dependency graph
requires:
  - phase: 27-scrub-redirecting-git-environment-from-production-calls (plans 01-05)
    provides: "The complete 41-site migration (git.rs, worktree.rs, version.rs, agent_result.rs, staleness.rs, commands.rs, preflight.rs) onto devflow_core::git::{hermetic_command, git_command}"
provides:
  - "27-SPAWN-CENSUS.md — a full workspace spawn-edge census (not just literal `Command::new(\"git\")`) proving Sweep A (0 unscrubbed direct git constructions) is real, and explicitly stating RESEARCH Assumption A2 is OPEN with 5 named, evidenced, unmitigated reaches-git/direct-git sites outside this phase's 7-file scope"
  - "27-VALIDATION.md filled in: Per-Task Verification Map (no TBD remaining), both hostile-GIT_DIR acceptance commands proven green with verbatim output, the stale 37-failure baseline superseded in writing by the re-measured 98, nyquist_compliant: true"
  - "A measured finding that the RESEARCH Open Question #2 residual (pipeline_gate/pipeline_outcomes non-termination under a hostile GIT_DIR) is resolved as a bonus effect of the migration — no longer needs its own backlog follow-up"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Workspace spawn-edge census: enumerate every Command::new(...) in production code (above each file's #[cfg(test)] mod tests boundary, and excluding files that are themselves entirely feature/cfg-gated out of production builds), classify direct-git / reaches-git / cannot-reach-git, and confirm each classification by tracing the production call graph rather than assuming from the site's location alone."

key-files:
  created:
    - .planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-SPAWN-CENSUS.md
  modified:
    - .planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-VALIDATION.md

key-decisions:
  - "D-01/D-02/D-03 measured, not restated: Sweep A (comment-filtered literal `Command::new(\"git\")` grep) returned 0 workspace-wide; both hostile-GIT_DIR acceptance commands reached a literal `test result:` line with 0 failed; build.rs confirmed byte-identical across the full phase diff via `git diff --stat $(git merge-base HEAD develop)..HEAD`."
  - "RESEARCH Assumption A2 (the exhaustiveness gap — whether any spawn edge beyond the 41 counted `Command::new(\"git\")` sites can still reach git) is explicitly stated OPEN, not silently closed. 5 unmitigated sites found and evidenced: monitor.rs:148 (spawns the AI agent that performs the phase's actual git work — highest consequence), hooks.rs:222 (sh -c \"cargo doc\", the same indirect build.rs chain 27-04 closed for test_cmd, left open at this second call site), gates.rs:323 and verify.rs:106 (both sh -c an operator-supplied/approved arbitrary command), and commands.rs's cmd_check(\"git\",\"git\",...) — a literal, unscrubbed `git --version` construction invisible to Sweep A's grep because the program name is threaded through a variable, not spelled Command::new(\"git\") in source (functionally inert since --version performs no ref/object resolution, but a real D-01 gap by the letter of its unconditional wording)."
  - "Re-measuring the pre-migration baseline against the phase base commit was judged impractical inside this dedicated, isolated worktree (no git checkout to a foreign ref). Cited 27-RESEARCH.md's live-measured 54/44/98 baseline instead, dated and attributed, per this plan's own explicit fallback instruction."
  - "The pipeline_gate/pipeline_outcomes residual (RESEARCH Open Question #2) is recorded as a FINDING, not a gate: it now completes clean under a hostile GIT_DIR (47 passed, 0 failed, ~16s wall, was previously non-terminating within 180-480s bounds) — a bonus effect of the migration, not something this plan diagnosed or fixed directly."

requirements-completed: [D-01, D-02, D-03]

coverage:
  - id: D1
    description: "Workspace-wide comment-filtered sweep of crates/*/src/ finds zero unscrubbed direct git constructions"
    requirement: "D-01"
    verification:
      - kind: other
        ref: "rg --no-heading -n 'Command::new(\"git\")' crates/devflow-core/src crates/devflow-cli/src | rg -v ':\\s*(//|///|//!)' | wc -l == 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both hostile-GIT_DIR acceptance commands reach a literal test result: line with 0 failed"
    requirement: "D-03"
    verification:
      - kind: integration
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support (411 passed, 0 failed)"
        status: pass
      - kind: integration
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes (188 passed, 0 failed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "crates/devflow-cli/build.rs is byte-identical across the whole phase diff"
    requirement: "D-02"
    verification:
      - kind: other
        ref: "git diff --stat $(git merge-base HEAD develop)..HEAD -- crates/devflow-cli/build.rs (empty)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every production spawn edge is enumerated with file/line/disposition; RESEARCH Assumption A2 is stated CLOSED or OPEN with evidence, never left implicit"
    requirement: "T-27-15"
    verification:
      - kind: other
        ref: "27-SPAWN-CENSUS.md § Full classification table and § Assumption A2 verdict"
        status: pass
    human_judgment: true
    rationale: "A2 is stated OPEN (not CLOSED) with 5 named unmitigated sites and a proposed backlog entry — this is a genuine finding requiring an operator decision on whether/when to migrate them, not a closed loop."

# Metrics
duration: 33min
completed: 2026-07-30
status: complete
---

# Phase 27 Plan 06: Workspace Census and Acceptance Evidence Summary

**Closed the phase by measurement: a workspace-wide spawn-edge census (not just the literal 41-site grep) that explicitly finds and escalates 5 unmitigated `reaches-git`/`direct-git` sites outside the phase's original scope, plus both hostile-`GIT_DIR` acceptance commands run to a real `test result:` line with 0 failed and `27-VALIDATION.md` filled in with this run's evidence.**

## Performance

- **Duration:** ~33 min (commit range 15:57 → 16:05 UTC, plus context-loading and census investigation time)
- **Tasks:** 2 (both `auto`) — 2 commits
- **Files modified/created:** 2 (1 new: `27-SPAWN-CENSUS.md`; 1 modified: `27-VALIDATION.md`)

## Accomplishments

- **Sweep A** (comment-filtered literal `Command::new("git")` grep, workspace-wide): **0**. The 41-site migration is confirmed complete by direct measurement, not by summing five per-plan claims.
- **Sweep B** (every `Command::new(...)` in production code, any program): enumerated all 43 hits across both crates, classified `direct-git` / `reaches-git` / `cannot-reach-git`, with production-vs-test-only status confirmed by direct inspection of each file's `#[cfg(test)] mod tests` boundary (not assumed).
- Found **5 genuinely unmitigated spawn edges** the 41-site literal count could never see: `monitor.rs:148` (spawns the AI agent binary itself — the single highest-consequence site, since the agent performs the phase's actual git commits/pushes), `hooks.rs:222` (`sh -c "cargo doc"`, the same indirect `build.rs` chain `27-04` closed for `test_cmd`, left open at this second call site), `gates.rs:323` and `verify.rs:106` (both `sh -c` an operator-supplied/approved arbitrary command), and — the most structurally interesting finding — `commands.rs`'s `cmd_check("git", "git", ...)` inside `devflow doctor`'s environment check, a **literal, unscrubbed `git --version` construction that Sweep A's own grep cannot see**, because the program name is threaded through a generic helper's `cmd: &str` parameter rather than spelled `Command::new("git")` in source.
- Confirmed `commands.rs::test_cmd`'s `sh -c "cargo …"` spawn is already mitigated by `27-04` (`hermetic_command("sh", project_root)`, verified live at line 1955).
- Confirmed `agent.rs`'s three `sh` sites (707, 739, 778) are genuinely test-only — each sits inside a `#[test]` fn below the file's `#[cfg(test)] mod tests` boundary at line 532, verified by direct inspection, not assumed.
- Ran both hostile-`GIT_DIR` acceptance commands to a literal `test result:` line: devflow-core **411 passed, 0 failed**; devflow-cli (skip `pipeline_gate`/`pipeline_outcomes`) **188 passed, 0 failed**. Neither timed out.
- Re-verified two `<verify>` checks `27-01` and `27-05` had documented as unsatisfiable at their own plan's scope boundary — both now pass now that every file in the phase is merged: `git::` suite 38/0, `preflight::` suite 41/0.
- Measured the `pipeline_gate`/`pipeline_outcomes` residual (RESEARCH Open Question #2) under a hostile `GIT_DIR`, bounded at 300s: **completed clean, 47 passed, 0 failed, ~16s wall** — previously non-terminating within 180–480s bounds pre-migration. Recorded as a finding, not a gate, per the plan's own instruction.
- Normal-environment gates all clean: `cargo test --workspace` (0 failed everywhere), `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings), `cargo fmt --check` (clean).
- `crates/devflow-cli/build.rs` confirmed byte-identical across the whole phase diff (`git diff --stat $(git merge-base HEAD develop)..HEAD` — empty).
- `27-VALIDATION.md`: Per-Task Verification Map filled in (real task IDs, plans, waves, commands — zero `TBD` remaining), Wave 0 requirements checked off, the spec-less probe fallback skip line recorded verbatim, `nyquist_compliant: true`, `status` left `draft` (advancing it is `/gsd-validate-phase`'s job).

## Task Commits

1. **Task 1: Workspace spawn-edge census** — `2929a21` (docs) — `27-SPAWN-CENSUS.md` created; states `A2 is OPEN`.
2. **Task 2: Phase acceptance run and validation evidence** — `94dcc26` (test) — `27-VALIDATION.md` filled in with this run's evidence.

**Plan metadata:** (this commit)

## Acceptance commands — verbatim final `test result:` lines

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support
test result: ok. 411 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.94s

$ GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes
test result: ok. 188 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 9.61s
```

**Baseline being compared against:** `27-RESEARCH.md` § Summary, measured live at HEAD `6350798` on 2026-07-30 — **54 failed / 352 passed** (devflow-core) and **44 failed / 139 passed** (devflow-cli), combined **98 failures**. Cited rather than re-measured in this run: checking out the phase base commit inside this dedicated, isolated worktree was judged impractical (would require detached-HEAD or foreign-ref checkout, which this agent does not own the lifecycle to perform safely). ROADMAP.md § "Phase 27" and `27-CONTEXT.md` D-03 both still carry the older, stale **`37`** figure — this SUMMARY, `27-VALIDATION.md`, and `27-RESEARCH.md` all supersede it with the re-measured 98.

## `pipeline_gate`/`pipeline_outcomes` residual (RESEARCH Open Question #2)

```
$ GIT_DIR=<hostile>/.git timeout 300 cargo test -p devflow --bin devflow -- pipeline_gate pipeline_outcomes
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 188 filtered out; finished in 4.60s
```

**Outcome: completed clean**, elapsed ~16s wall-clock (two `date +%s` timestamps 16 seconds apart bracketing the run), well under the 300s bound. This was the single reproducible non-termination in the whole phase during research — plausibly resolved because these test modules call through `GitFlow` methods in `git.rs` (one of the migrated files), and the pathological slowdown against a foreign/unrelated repository no longer occurs once the constructor resolves the caller's own repo instead. Recorded as a finding per the plan's instruction — the phase's acceptance does not depend on this outcome.

## Files Created/Modified

- `.planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-SPAWN-CENSUS.md` (new) — Sweep A/B commands and verbatim output, full classification table (43 production/test spawn sites), explicit "A2 is OPEN" verdict, proposed backlog entry.
- `.planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-VALIDATION.md` (modified) — Per-Task Verification Map filled in, baseline correction published, acceptance run evidence recorded, sign-off checklist checked, `nyquist_compliant: true`.

No source file under `crates/` was modified by this plan (`git diff --stat HEAD -- crates/` — empty, confirmed after both tasks).

## Decisions Made

- The census methodology (Sweep B) enumerated **every** `Command::new(...)`, not just ones matching an "obviously git-related" heuristic — this is what surfaced the `cmd_check("git", "git", ...)` finding a narrower search would have missed.
- Both `devflow-core/src/test_support.rs` and `devflow-cli/src/test_support.rs` were excluded from the production census wholesale (not just by their internal `#[cfg(test)]` boundary), because both files are gated at the `mod` declaration itself (`lib.rs:78` / `main.rs:7`) and are never compiled into a shipped `devflow` binary regardless of position within the file.
- Did not migrate any of the 5 found `reaches-git`/`direct-git` sites — per the plan's explicit scope boundary (missing information for `hooks.rs`'s legitimate-`GIT_DIR`-in-a-hook-context caveat; context cost for a second per-site migration inside a wave-3 acceptance plan). Documented as an escalated finding with a proposed backlog entry instead.
- Chose to state `A2 is OPEN` even though 4 of the 5 sites are low-to-medium severity and the 5th (`monitor.rs`) is a pre-existing, long-standing pattern — per the plan's own instruction not to let the phase's green acceptance numbers imply the class is closed when the census says it is not.

## Deviations from Plan

### Auto-fixed / Adjusted Issues

**1. [Rule 1 — environment flake, not a defect] One collateral test failure on the first hostile-`GIT_DIR` devflow-core run**
- **Found during:** Task 2, Step 2's first acceptance-command run.
- **Issue:** `agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape` failed with "exec visibility timed out before the fixture became discoverable" — a process-liveness timing check with no git dependency whatsoever.
- **Fix:** Re-ran the single test in isolation (passed in 0.01s) and re-ran the full suite (passed cleanly, 411/0). Classified as the same resource-contention flake class `27-05-SUMMARY.md` already documented for `cargo test --workspace`'s default parallelism in this sandboxed environment — not a regression tied to the hostile `GIT_DIR`.
- **Files modified:** none (measurement-only finding).
- **Committed in:** documented in `27-VALIDATION.md` under § Post-Migration Acceptance Run (`94dcc26`).

**2. [Rule 4-adjacent, but resolved as a documented fallback, not an architectural change] Could not re-measure the pre-migration baseline against the phase base commit inside this worktree**
- **Found during:** Task 2, Step 1.
- **Issue:** The plan's Step 1 asks to re-measure the baseline from "a detached checkout or `git stash`-equivalent of the phase base commit." Doing so inside this dedicated, isolated worktree would require checking out a foreign ref or a detached HEAD, which risks violating the worktree-isolation invariant this execution runs under (this agent does not own recovery for this worktree — see `<worktree_branch_check>`).
- **Fix:** Followed the plan's own explicit fallback instruction: cited `27-RESEARCH.md`'s live-measured 54/44/98 baseline, dated (2026-07-30) and attributed (HEAD `6350798`), rather than silently substituting the stale `37` or fabricating a fresh measurement this worktree cannot safely produce.
- **Files modified:** none beyond the citation in `27-VALIDATION.md`.
- **Committed in:** `94dcc26`.

---

**Total deviations:** 2 (1 environment-flake investigation with no code impact, 1 documented use of the plan's own stated fallback for an impractical-in-this-worktree measurement step). Neither affects the substantive D-01/D-02/D-03 acceptance, which is independently proven by the passing, correctly-scoped acceptance commands and the empty `build.rs` diff.

## Issues Encountered

None beyond the two deviations above.

## User Setup Required

None — no external service configuration required.

## Finding requiring an operator decision

**`27-SPAWN-CENSUS.md` states `RESEARCH Assumption A2 is OPEN`.** Five
production spawn edges outside this phase's original 7-file/41-site scope can
still reach `git` with an inherited, unscrubbed environment:

1. `crates/devflow-core/src/monitor.rs:148` — spawns the AI agent binary
   (`claude`/`codex`/etc.) that performs the phase's actual git commits and
   pushes. **Highest consequence** — the same threat class as this phase's own
   motivating incident (999.39/CR-01), one process deeper: instead of
   `devflow` itself misresolving a repository, the AI agent it spawns could.
2. `crates/devflow-core/src/hooks.rs:222` — `sh -c "cargo doc --no-deps"`,
   the identical indirect `sh → cargo → build.rs::run_git` chain `27-04`
   closed for `commands.rs::test_cmd`, left open at this second call site.
3. `crates/devflow-core/src/gates.rs:323` — `sh -c <operator-supplied
   DEVFLOW_GATE_NOTIFY_CMD>`.
4. `crates/devflow-core/src/verify.rs:106` — `sh -c <operator-approved
   external verification command>`.
5. `crates/devflow-cli/src/commands.rs:2086` (via `cmd_check`, `devflow
   doctor`'s environment check) — a **literal**, unscrubbed `git --version`
   construction invisible to a literal `Command::new("git")` grep because the
   program name is threaded through a variable. Functionally low-risk
   (`--version` performs no ref/object resolution), but a real, previously-
   uncounted D-01 gap by the letter of its unconditional policy.

None of these were migrated by this plan — doing so would require per-site
judgment (missing information: `hooks.rs` in particular is a real
counter-example where git itself can legitimately set `GIT_DIR` when invoking
a hook) and would expand a wave-3 acceptance plan into a second migration.
`27-SPAWN-CENSUS.md` § Proposed backlog entry names the title, evidence, and
severity breakdown for a follow-up. **This is stated plainly, not softened —
the phase's green acceptance numbers (D-01/D-02/D-03, both hostile-GIT_DIR
commands at 0 failed) do not imply this broader class is closed.**

## Next Phase Readiness

- The phase's own acceptance contract (D-01/D-02/D-03) is fully evidenced:
  Sweep A at 0, both hostile-`GIT_DIR` commands green, `build.rs` untouched.
- `27-VALIDATION.md` carries this run's evidence, `nyquist_compliant: true`,
  `status: draft` (awaiting `/gsd-validate-phase`).
- A genuine, evidenced follow-up exists (`27-SPAWN-CENSUS.md`'s 5 unmitigated
  sites) for the operator to route to a backlog entry — not blocking this
  phase's own closure, but should not be lost.
- No blockers for phase closure on this plan's own scope.

---
*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Completed: 2026-07-30*

## Self-Check: PASSED
- FOUND: `.planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-SPAWN-CENSUS.md`
- FOUND: `.planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-VALIDATION.md`
- FOUND commit: `2929a21` (docs: Task 1, spawn census)
- FOUND commit: `94dcc26` (test: Task 2, acceptance run)
