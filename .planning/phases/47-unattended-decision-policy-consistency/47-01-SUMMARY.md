---
phase: 47-unattended-decision-policy-consistency
plan: 01
subsystem: testing
tags: [insta, snapshot, ci, check.sh, gitignore, cargo-deny, cargo-machete, tdd]

requires: []
provides:
  - "`insta` 1.48.0 wired as a workspace dev-dependency in devflow-core and devflow-cli"
  - "`run_test` in scripts/check.sh runs cargo test under `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`"
  - "First committed snapshot baseline: the PRE-FIX Claude/OpenCode FullExecute fix prompt (no decision-policy section)"
  - "Two durable Rust guards over the wiring: invocation pinning and the .snap/.snap.new ignore split"
  - "Recorded demonstration that the guard fails in the INSTA_FORCE_UPDATE direction on this repository"
affects: [47-02, 47-03, 47-04, "D-15 snapshot suite", "scripts/check.sh", "CI Test job"]

actuals:
  tokens: 4159        # chars/4 over the realized diff c4f4ca1..e3f4fe2 (16639 chars incl. Cargo.lock; 14769 excl.)
  tasks: 3
  commits: 3          # MEASURED: git rev-list --count c4f4ca1..HEAD at SUMMARY write (excludes this docs commit)
plan_head_before: c4f4ca1e419a99428e0d7443670a0f13f0d011ea

tech-stack:
  added: ["insta 1.48.0 (dev)", "similar 2.7.0 (transitive, dev)", "console 0.16.6 (transitive, dev)", "encode_unicode 1.0.0 (transitive, dev)"]
  patterns:
    - "Snapshot baselines are created only via `INSTA_UPDATE=always cargo test -p <crate> <testname>`, run once, read before committing"
    - "Source-asserting guards over scripts count INVOCATION lines (comments stripped, first word env/cargo), never `.find()`"
    - "`git check-ignore` negative controls use `--no-index` and exact exit codes (0/1), never `!success()`"

key-files:
  created:
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/devflow-core/Cargo.toml
    - crates/devflow-cli/Cargo.toml
    - crates/devflow-core/src/prompt.rs
    - scripts/check.sh
    - .gitignore
    - .planning/user/DEV-SETUP-CHECKLIST.md
    - crates/devflow-cli/tests/ci_parity_guards.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs

key-decisions:
  - "Task 1 TDD split: every piece of wiring (dependency, run_test, .gitignore, checklist, failing test) in the RED commit so the dependency, CI change and checklist share one commit; the reviewed baseline alone is GREEN"
  - "gitignore guard uses `git check-ignore --no-index` with exact exit codes — without it a TRACKED baseline reads as not-ignored even under a `*.snap` rule (measured: exit 1 without, 0 with)"
  - "run_test guard asserts the COUNT of env/cargo-first-word invocation lines equals 1 — a stale-banner mutation proved a `.find()`-style check would be insufficient"
  - "RED evidence for a cargo test run is classified via a mechanical TAP transcription of the real log, because gsd-core's tdd-red-evidence classifier parses node TAP only"

patterns-established:
  - "Snapshot drift guard: env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no in run_test, *.snap tracked, *.snap.new ignored"
  - "Revert-and-fail proof for source guards: each mutation must print `test result: FAILED`, then restore and re-pass"

requirements-completed: [DECN-02, DECN-03]  # copied verbatim from PLAN frontmatter; NOT marked in REQUIREMENTS.md — requirements.ready-ids reported 0/2 ready (sibling plans 47-02..05 declare both and have no SUMMARY yet)

coverage:
  - id: D1
    description: "insta wired at workspace level and in both crates; run_test hardened with env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no; *.snap.new ignored; checklist § 8 corrected with the baseline recipe"
    requirement: DECN-02
    verification:
      - kind: other
        ref: "47-01-PLAN.md Task 1 <automated> gate (gate_rc=0, all counters in range)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Committed reviewed pre-fix baseline for claude_style_full_execute_fix_prompt_snapshot (contains /gsd-execute-phase, no decision-policy heading)"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::claude_style_full_execute_fix_prompt_snapshot"
        status: pass
      - kind: other
        ref: "scripts/check.sh test (check_test_exit=0, 0 failed binaries)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Guard observed failing in the INSTA_FORCE_UPDATE direction: un-hardened force-update 0 (defect), hardened drift 101, hardened drift+force-update 101, hardened clean 0"
    requirement: DECN-02
    verification:
      - kind: other
        ref: "scripts/check.sh test under the four recorded conditions (see ## Task 2 demonstration)"
        status: pass
    human_judgment: false
  - id: D4
    description: "check_script_pins_snapshots_against_env_override and gitignore_ignores_snapshot_scratch_but_not_the_baseline, each shown to fail under reverting mutations"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#check_script_pins_snapshots_against_env_override"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/gitignore_coverage.rs#gitignore_ignores_snapshot_scratch_but_not_the_baseline"
        status: pass
    human_judgment: false
  - id: D5
    description: "cargo deny check and cargo machete green on the real workspace; cargo deny list control discriminates dev vs regular dependency"
    requirement: DECN-02
    verification:
      - kind: other
        ref: "47-01-PLAN.md Task 3 <automated> gate (gate_rc=0) + cargo deny list three-way control"
        status: pass
    human_judgment: false
  - id: D6
    description: "DEV-SETUP-CHECKLIST § 8 prose is accurate and usable by a future contributor on another machine"
    verification: []
    human_judgment: true
    rationale: "Prose accuracy and replicability are judgment calls; only the presence of the recipe string is machine-checked"

duration: 26min
completed: 2026-09-11
status: complete
---

# Phase 47 Plan 01: Snapshot Drift Guard Summary

**`insta` snapshot guard over the Claude/OpenCode loop-back prompt, with `run_test` pinned by `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`, observed on this repository to turn the force-update re-bless from exit 0 into exit 101**

## Performance

- **Duration:** ~26 min
- **Started:** 2026-09-11T17:08:32Z
- **Completed:** 2026-09-11T17:34Z
- **Tasks:** 3 (Task 1 tracer, Task 2, Task 3 assert-only)
- **Files modified:** 11 (1 created, 10 modified)

## Accomplishments

- A wording change to the FullExecute fix prompt now fails `scripts/check.sh test`, including under
  an exported `INSTA_FORCE_UPDATE=1`, which previously re-blessed the baseline and exited 0.
- The committed baseline records the DECN-02 defect as a reviewed artifact: the loop-back prompt
  carries `/gsd-execute-phase 47 --auto` and the completion protocol, and no decision-policy section.
- Two Rust guards fail if the `env -u` wiring or the `.snap`/`.snap.new` split is removed, each
  shown failing under its reverting mutations.
- `cargo deny` / `cargo machete` confirmed on the real workspace, with a control proving the deny
  measurement discriminates.

## Task Commits

1. **Task 1 (tracer, TDD RED): wire insta + failing snapshot test** - `4dbba5c` (test)
2. **Task 1 (TDD GREEN): reviewed pre-fix baseline** - `1a812eb` (feat)
3. **Task 2: two source guards over the wiring** - `e3f4fe2` (test)
4. **Task 3:** no commit — asserts only; manifests byte-identical to Task 1 state

## Automated gate counters (as printed)

| Gate | Printed counters | Exit |
|---|---|---|
| Task 1 | `check_test_exit=0` `cargo_test_invocation_lines=1` `hardened_invocation_lines=1` `tracked_snapshots=1` `policy_heading_in_baseline=0` `fix_command_in_baseline=1` `gitignore_snapnew_lines=2` `checklist_recipe_lines=1` `gate_rc=0` | 0 |
| Task 2 | `cargo_exit_a=0` `guard_a_one_passed=1` `guard_a_filtered=25 filtered out` `cargo_exit_b=0` `guard_b_one_passed=1` `guard_b_filtered=1 filtered out` `residual_dirty_paths=0` `gate_rc=0` | 0 |
| Task 3 | `deny_check_exit=0` `machete_exit=0` `manifest_diff_paths=0` `machete_ignore_entries=0` `gate_rc=0` | 0 |

`gitignore_snapnew_lines=2` is the rationale comment line plus the pattern line; the gate needs >= 1.
The Task 1 gate run after the GREEN commit doubled as the tracer feedback gate re-verify (interactive,
`end-of-phase`, `<automated>`-only verify): passed, expansion proceeded.

## Task 2 demonstration — four exit codes and baseline bytes

Baseline sha256 when clean (496 bytes): `2dd81e06a939f00556431d30722002e60f6d229f41daf5f22a944ce6a39a1bd7`.
Each case appended one drift line to the committed `.snap`, ran the full `scripts/check.sh test`, then
restored with `git checkout --`.

| Run | `run_test` | Env | Exit | Baseline sha256 before → after | Observation |
|---|---|---|---|---|---|
| Defect | plain `cargo test --workspace --no-fail-fast` | `INSTA_FORCE_UPDATE=1` | **0** | `7542ecfd…` → `2dd81e06…` | insta printed `updated snapshot`; drift line gone; target test `ok` — green over an unread diff |
| (1) drift | hardened | none | **101** | `8d3f44e2…` → `8d3f44e2…` | target test FAILED; no `.snap.new` written |
| (2) drift + force-update | hardened | `INSTA_FORCE_UPDATE=1` | **101** | `842f9a48…` → `842f9a48…` | target test FAILED; `updated snapshot` 0 times; drift line still present |
| (3) clean | hardened | none | **0** | `2dd81e06…` → `2dd81e06…` | 0 failed binaries |

**Case (1) alone certifies `insta`, not the wiring** (RESEARCH A-1, Pitfall 2): a drifted baseline
fails with or without `INSTA_UPDATE=no` on a clean shell. **Case (2) is the one that measures what
was built** — it is the defect row's condition, and it flipped from 0 to 101. Case (3) is the
control proving the guard is not a constant-fail. Final residual over `scripts/check.sh` and the
snapshot directory: 0.

In the defect run only the invocation line was un-hardened; the `echo "==> …"` banner still printed
the `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` form while the run went green. That is precisely the
stale-banner case the Task 2 guard's count assertion exists for.

**Guard discrimination (revert-and-fail, each printed `test result: FAILED. 0 passed; 1 failed`):**

| Mutation | Guard | Exit |
|---|---|---|
| invocation `env INSTA_UPDATE=no cargo test …` (lost `-u`), banner left stale | A | 101 |
| invocation `env -u INSTA_FORCE_UPDATE cargo test …` (lost `INSTA_UPDATE=no`) | A | 101 |
| second invocation line `cargo test --doc` added | A | 101 |
| `.gitignore` gains `*.snap` | B | 101 |
| `.gitignore` loses `*.snap.new` | B | 101 |
| clean tree (before and after) | A, B | 0, 0 |

Under the `*.snap` mutation, `git check-ignore -q` on the tracked baseline exited **1** (not ignored)
without `--no-index` and **0** with it — the measurement behind the guard's `--no-index`.

## Task 3 — cargo deny / machete on the real workspace

- `cargo deny check`: exit 0 — `advisories ok, bans ok, licenses ok, sources ok`. No `deny.toml` change.
- `cargo machete`: exit 0 — "didn't find any unused dependencies". **This does not establish anything
  about `insta`:** machete 0.9.2 does not analyze `[dev-dependencies]` (measured 2026-09-10 on a
  scratch crate with a control that fired). It confirms only that no unused REGULAR dependency was
  introduced. No `cargo-machete` ignore entry added.
- `cargo deny list` control (occurrences of `insta` / `similar` / `console` / `encode_unicode`):
  - `insta` as dev-dependency: **0 / 0 / 0 / 0**
  - `insta` moved to `[dependencies]` of devflow-core: **1 / 1 / 1 / 2** (`encode_unicode` is listed
    under both of its licences)
  - reverted: **0 / 0 / 0 / 0**; manifests + `Cargo.lock` residual 0

The control changed the output, so RESEARCH A-4's dev-dependency-exclusion finding is confirmed on
this workspace rather than transferred from the scratch crate.

## TDD Gate Compliance

- Task 1 — `task.is-behavior-adding` = true. RED `4dbba5c` precedes GREEN `1a812eb`; no `feat(47-01)`
  before RED (checked immediately before creating the baseline). No REFACTOR needed.
- RED run: `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow-core --lib
  claude_style_full_execute_fix_prompt_snapshot` → exit 101, `prompt::tests::claude_style_full_execute_fix_prompt_snapshot ... FAILED`,
  `snapshot assertion for 'claude_style_full_execute_fix_prompt_snapshot' failed`, `0 passed; 1 failed; 764 filtered out`, nothing written.
- `gsd_run check tdd-red-evidence` verdicts: raw cargo log → `INVALID_RED (zero_tests_discovered)`,
  because the classifier parses only node TAP (`# tests N`, `not ok N - name`); a TAP transcription
  generated mechanically from the same log → `RED_EVIDENCE_OK (target_test_failed)`; the same
  transcription with a wrong target name → `INVALID_RED (no_target_test_failure)` (control).
  The verifier exited 0 for all three — its verdict lives in the JSON, not the exit code.
- Task 2 — test-only (exempt from the behavior-adding gate). It pins wiring Task 1 already built, so
  a first run is green by construction; discrimination is established by the mutation table above.
  Commit type `test(47-01)`.

## Files Created/Modified

- `Cargo.toml` — `insta = "1"` in `[workspace.dependencies]` with the D-14/D-15 + `env -u` obligation comment
- `Cargo.lock` — 4 net-new crates: insta 1.48.0, similar 2.7.0, console 0.16.6, encode_unicode 1.0.0; 0 lines removed
- `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml` — `insta = { workspace = true }` under `[dev-dependencies]`
- `crates/devflow-core/src/prompt.rs` — `claude_style_full_execute_fix_prompt_snapshot` (fully-qualified `insta::assert_snapshot!`)
- `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap` — the pre-fix baseline
- `scripts/check.sh` — `run_test` hardened; `--no-fail-fast` rationale kept verbatim; banner matches the invocation
- `.gitignore` — commented `*.snap.new` block; `*.snap` stays tracked
- `.planning/user/DEV-SETUP-CHECKLIST.md` — § 8 zero-network claim scoped to runtime deps; baseline recipe added
- `crates/devflow-cli/tests/ci_parity_guards.rs` — `check_script_pins_snapshots_against_env_override`
- `crates/devflow-cli/tests/gitignore_coverage.rs` — `gitignore_ignores_snapshot_scratch_but_not_the_baseline`

## Decisions Made

- TDD commit split for Task 1 as in `key-decisions` — the plan's same-commit rule (dependency + CI
  change + checklist) forced the wiring into RED; the baseline is the only GREEN content.
- Guard B uses `--no-index` and exact exit codes; guard A asserts a count of 1, not presence.
- Kept `insta` default features (`colors`), per RESEARCH A-6.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Vacuous negative control in the described gitignore guard**
- **Found during:** Task 2
- **Issue:** The plan's behavior ("`git check-ignore` reports a `.snap` path as NOT ignored") would pass
  vacuously for the real, tracked baseline: `check-ignore` never reports tracked paths as ignored, and
  `!success()` also reads a git error (exit 128) as "not ignored".
- **Fix:** `--no-index` plus `assert_eq!` on exit codes `Some(0)` / `Some(1)`.
- **Files modified:** `crates/devflow-cli/tests/gitignore_coverage.rs`
- **Verification:** under a `*.snap` rule, exit 1 without `--no-index` vs 0 with it; guard fails under that mutation.
- **Committed in:** `e3f4fe2`

### Plan-fact discrepancies (no code change)

**2. Lockfile adds 4 crates, not 3.** Plan and RESEARCH A-6 say `insta`, `console`, `similar`;
`encode_unicode` 1.0.0 is also net-new (A-6 claimed every other transitive was already locked).
Licence-clean per `cargo deny check`.

**3. RESEARCH § A-5 carries no inline correction.** The plan's read_first says A-5 "now carries the
correction inline"; the file at `c4f4ca1` still states only the original (false) causal claim. The
plan's own correction was followed, and the Task 3 SUMMARY text states the machete limit rather than A-5.

**4. GREEN commit amended for message length.** First GREEN subject was 73 chars (commit-msg hook
warning, user rule <= 72); amended unpushed from `06fd318` to `1a812eb`, message only, same tree.

---

**Total deviations:** 1 auto-fixed (Rule 1), 3 recorded discrepancies.
**Impact on plan:** The Rule 1 fix is what makes the `.snap` negative control real. No scope creep.

## Issues Encountered

- gsd-core `check tdd-red-evidence` cannot read cargo test output (node TAP only) and exits 0 even on
  INVALID_RED. Worked around with a generated transcription plus a wrong-target control; not filed.
- zsh `=word` expansion broke an `echo ====` separator in an exploratory command (no effect on gates).

## What this does NOT establish

- **Not run in the pinned CI container.** Every run here was on the host toolchain;
  `scripts/check-in-container.sh test` and the CI Test job have not executed this wiring. `env -u`
  is coreutils-standard, but container parity is unverified.
- **One observation per case.** The four-row demonstration is one run each; it shows the mechanism,
  not a flake rate.
- **Both Rust guards assert on source**, not behaviour: they catch removal of the tokens/patterns, not
  an insta release that changes `INSTA_FORCE_UPDATE` semantics. The behavioural proof is the
  demonstration table, which is not re-run by the suite.
- **machete green says nothing about `insta`** (see Task 3).
- **RED evidence verdict rests on a transcription** of the cargo log, not on the raw log.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 47-02: it must drift this baseline deliberately (adding the decision-policy section to
  `fix_prompt`) and re-bless it with `INSTA_UPDATE=always`, asserting the content change.
- `graphify-out/graph.json` and `manifest.json` were refreshed locally in the worktree and are
  deliberately uncommitted (CLAUDE.md graphify rule).

## Self-Check: PASSED

- FOUND: all 11 key files, including the `.snap` baseline (git-tracked)
- FOUND: commits `4dbba5c`, `1a812eb`, `e3f4fe2`; pre-amend `06fd318` not reachable from HEAD
- FOUND: symbols `claude_style_full_execute_fix_prompt_snapshot`, `check_script_pins_snapshots_against_env_override`, `gitignore_ignores_snapshot_scratch_but_not_the_baseline` (1 each)
- Stub scan: no stubs (only pre-existing "placeholder" wording about `{N}` substitution)
- `cargo fmt --check` exit 0; `cargo clippy --workspace --all-targets -- -D warnings` exit 0
