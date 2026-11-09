---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 04
subsystem: infra
tags: [ci, github-actions, taskset, cpu-affinity, bash, regression-guards, yaml-parsing]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "46-01's `Sequential 2-CPU check` job, `scripts/lib/ci-cpus.sh` as the single CPU-list definition site, and `ci_parity_guards.rs`'s original whole-file guards"
provides:
  - "`cpu_pin_prefix()` — the single site deciding the `DEVFLOW_CI_CPUS=all` case, consumed by all three pin consumers"
  - "`scripts/assert-cpu-pin.sh` — a measurement-free decider for pinned-vs-unpinned core counts, unit-tested in all four branches"
  - "`split_jobs` / `recognise_pinned_sequential_job` — job-scoped workflow recognition driven by five fixtures"
  - "`crates/devflow-cli/tests/fixtures/ci-parity/` — one positive and four negative workflow fixtures"
affects: [46-05, 46-06, 46-07, phase-46 verification, any future ci.yml job or workflow guard]

actuals:
  tokens: 10605
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "A workflow guard is a function over `&str` driven by fixtures, not a `contains` search over the live file"
    - "A negative control asserts; the decider takes measurements as arguments so its failing branch is runnable off the runner"
    - "Conditional assertions print the reason they skipped"

key-files:
  created:
    - scripts/assert-cpu-pin.sh
    - crates/devflow-cli/tests/fixtures/ci-parity/valid.yml
    - crates/devflow-cli/tests/fixtures/ci-parity/commented-out.yml
    - crates/devflow-cli/tests/fixtures/ci-parity/decoy-job.yml
    - crates/devflow-cli/tests/fixtures/ci-parity/echoed-command.yml
    - crates/devflow-cli/tests/fixtures/ci-parity/no-pin.yml
  modified:
    - scripts/lib/ci-cpus.sh
    - scripts/check-in-container.sh
    - .github/workflows/ci.yml
    - crates/devflow-cli/tests/ci_parity_guards.rs
    - .planning/user/DEV-SETUP-CHECKLIST.md

key-decisions:
  - "The pin token the recogniser matches is `\"${CPU_PIN[@]}\"`, not `taskset -c` — the literal `taskset` line no longer exists in ci.yml, and pinning a guard to a spelling is what C-02 found."
  - "`echoed-command.yml` carries TWO echo lines: the plan-mandated pre-46-04 spelling and an echo of the CURRENT pin token. Only the second actually exercises the starts-with rule; the first would be rejected by any rule at all."
  - "`no-pin.yml` keeps the ci-cpus.sh source line so it isolates rule (b) rather than duplicating `commented-out.yml`'s failure path."
  - "`shellcheck` is not in this repo's gate, but the two scripts authored here are clean under it with one narrow SC2016 disable — verified against a deliberately-broken copy so the clean is not a blanket suppression."

patterns-established:
  - "Guard-as-function: workflow guards live as `&str` -> Result functions with fixture-driven negative cases, because a guard that can only run against the live file is only ever observed passing."
  - "Decider/measurer split: a CI assertion's decision logic goes in a script taking measurements as arguments, so `cargo test` exercises the failing branch."
  - "Loud skips: every conditional assertion prints why it skipped, because a silent skip and a pass are indistinguishable in a job log."
  - "Splitters read RAW lines: indentation is the only signal separating a top-level YAML key from a nested one, so a trim-everything helper must not precede scoping."

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "C-06 closed — `DEVFLOW_CI_CPUS=all` no longer breaks CI; the `all` case is decided once, in `cpu_pin_prefix()`, and all three consumers route through it."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "bash -c '. scripts/lib/ci-cpus.sh; CPUS=all; cpu_pin_prefix; test \"${#CPU_PIN[@]}\" -eq 0'"
        status: pass
      - kind: integration
        ref: "DEVFLOW_CI_CPUS=all + full simulated load-shape step -> step_exit=0 (baseline: taskset -c all nproc exits 1)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#cpu_pin_has_exactly_one_definition_site"
        status: pass
    human_judgment: false
  - id: D2
    description: "C-02 closed — the parity guard is scoped to the `sequential` job body and its negative cases run, driven by five fixtures."
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#recogniser_accepts_a_valid_pinned_sequential_job"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#recogniser_rejects_a_commented_out_pinned_suite_step"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#recogniser_rejects_a_decoy_job_carrying_the_pin"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#recogniser_rejects_an_echoed_pin_command"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#recogniser_rejects_an_unpinned_suite_step"
        status: pass
      - kind: integration
        ref: "live-file replay of agy's bypass: comment out the pinned suite line in .github/workflows/ci.yml -> `test result: FAILED. 0 passed; 1 failed` (was `1 passed`)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The job splitter stops counting `on:` block keys as jobs; the `>= 4` anti-vacuity assert is replaced by an exact key list, not deleted."
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#job_splitter_finds_exactly_the_live_ci_jobs"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#job_splitter_excludes_on_block_keys"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#container_jobs_using_bash_syntax_declare_a_bash_shell"
        status: pass
    human_judgment: false
  - id: D4
    description: "C-03 closed — the pinned/unpinned comparison is asserted by `scripts/assert-cpu-pin.sh`, conditional on `CPUS != all` and `unpinned > 2`, with both skip paths printing their reason."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "scripts/assert-cpu-pin.sh 0,1 4 4 -> exit 1 (the failing direction; no command in the repo could produce it before)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#assert_cpu_pin_fails_when_the_pin_did_not_narrow_the_core_count"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#assert_cpu_pin_passes_when_the_pin_narrowed_the_core_count"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#assert_cpu_pin_skips_and_says_why_for_the_all_override"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#assert_cpu_pin_skips_and_says_why_on_a_two_cpu_host"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#ci_workflow_calls_the_cpu_pin_decider_instead_of_echoing"
        status: pass
    human_judgment: false
  - id: D5
    description: "The `Sequential 2-CPU check` job's OWN behaviour on a real GitHub runner under the rewritten steps — that the array expansion, the re-source and the decider all work in the container's bash, and that the assertion neither flakes nor false-fails there."
    verification: []
    human_judgment: true
    rationale: "Everything above was measured on a 4-CPU Fedora host with the job's steps SIMULATED in a local bash. That establishes the logic, not the runner. Nothing here has executed inside `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` on `ubuntu-24.04`, and PR #208's precedent is that this specific job's shell environment differs from a local one in ways that killed it on line 1. Requires a real CI run to sign off."

duration: 15 min
completed: 2026-09-06
status: complete
---

# Phase 46 Plan 04: CI Guard Gap Closure Summary

**The CPU-pin `all` case now has one home instead of one-and-a-half; the pinned/unpinned core-count comparison is an assertion instead of three `echo`s; and the parity guard is a fixture-driven function over the `sequential` job's own body instead of a `contains` search over the whole file.**

## Performance

- **Duration:** 15 min
- **Started:** 2026-09-06T10:44:26Z
- **Completed:** 2026-09-06T10:59:35Z
- **Tasks:** 3
- **Files created/modified:** 11 (6 created, 5 modified)

## Accomplishments

- **C-06 closed.** `cpu_pin_prefix()` in `scripts/lib/ci-cpus.sh` is the only place the `all` case is decided, and all three consumers call it. The conditional was NOT duplicated into `ci.yml`.
- **C-03 closed.** `scripts/assert-cpu-pin.sh` decides; the CI step measures. Its failing branch runs under `cargo test`, which is the whole reason the split exists.
- **C-02 closed, and demonstrated closed on the live file** — not only against fixtures.
- **The job splitter stops counting `on:` keys as jobs**, and the anti-vacuity assert it broke was replaced rather than deleted.
- **Suite grew 11 -> 25 tests**, all green, with clippy, fmt and the full workspace suite green.

## Task Commits

1. **Task 1: one home for the `all` case, both CI sites routed through it** — `47741a6` (fix)
2. **Task 2 RED: fixture cases against vacuous stubs** — `42ea8bd` (test)
3. **Task 2 GREEN: job-scoped splitter and recogniser** — `503e103` (fix)
4. **Task 3 RED: decision cases against an always-exit-0 stub** — `76dec4f` (test)
5. **Task 3 GREEN: the real decider plus the ci.yml rewrite** — `59de2f7` (fix)

Both TDD tasks carry a real RED gate followed by a GREEN gate, in that order.

## Measured values (required by the plan's `<output>`)

### `scripts/assert-cpu-pin.sh 0,1 4 4`

```
assert-cpu-pin: FAILED — the pin `0,1` did not narrow the visible core count:
unpinned_nproc=4, pinned_nproc=4. A partially-invalid CPU list narrows SILENTLY
and still exits 0, so the counts matching means the job is NOT running under the
load shape it claims.
exit=1
```

All four dispositioned outcomes, run this session:

| Invocation | Exit | Output |
|---|---|---|
| `0,1 4 2` | `0` | `PASSED — the pin \`0,1\` narrowed the visible core count from unpinned_nproc=4 to pinned_nproc=2.` |
| `all 4 4` | `0` | `SKIPPED — cpu_list is \`all\`, so no pin was applied and there is nothing to narrow (unpinned_nproc=4 pinned_nproc=4).` |
| `0,1 2 2` | `0` | `SKIPPED — this is a 2-CPU host (unpinned_nproc=2), and a 2-CPU pin cannot narrow a 2-CPU machine, so a failure here would be an artefact of the runner (pinned_nproc=2).` |
| `0,1 4 4` | **`1`** | `FAILED — the pin \`0,1\` did not narrow the visible core count…` |

The non-zero on the last row is the negative control. A decider that always exited 0 — which is precisely what the stub in `76dec4f` was, and what the three `echo`s it replaced amounted to — prints `0` there and the criterion fails.

### The recogniser's verdict on each of the five fixtures

Printed from the real functions via a temporary probe test (run with `--nocapture`, then removed; `grep -c probe_46_04_print_verdicts` over the file returns 0):

| Fixture | Verdict |
|---|---|
| `valid.yml` | `Ok("sequential")` |
| `commented-out.yml` | ``Err("job `sequential` has no line whose trimmed content STARTS WITH `\"${CPU_PIN[@]}\"` and runs `scripts/check.sh all` — the pinned invocation is missing, commented out, or merely echoed")`` |
| `decoy-job.yml` | ``Err("job `sequential` does not source `scripts/lib/ci-cpus.sh` in its own body — the CPU list and the `all` override both live there, and a mention elsewhere in the file (another job, or a comment) does not put the pin on this job")`` |
| `echoed-command.yml` | ``Err("job `sequential` has no line whose trimmed content STARTS WITH `\"${CPU_PIN[@]}\"` … or merely echoed")`` |
| `no-pin.yml` | ``Err("job `sequential` has no line whose trimmed content STARTS WITH `\"${CPU_PIN[@]}\"` … or merely echoed")`` |

One positive control, four negatives, three distinct failure paths (missing source line; missing pinned line; pinned line present but not at the start of the command).

### The splitter's exact job-key list for the live `ci.yml`

```
KEYS LIVE ci.yml => ["test", "clippy", "fmt", "sequential"]
```

Measured before the fix, by replaying the old splitter's logic over the same file: `["push", "pull_request", "test", "clippy", "fmt", "sequential"]` — six headers for four real jobs, which is what made `assert!(jobs.len() >= 4)` satisfiable by two non-jobs. Fixture key lists: `valid.yml` -> `["test", "sequential"]`; `decoy-job.yml` -> `["sequential", "decoy"]`.

### What the C-03 assertion does NOT establish

Stated plainly, because the plan requires it and because the assertion is easy to over-read:

- It does **not** establish that `rustc` or the test-harness threads were actually confined. It compares two `nproc` readings; `nproc` reports the calling process's own affinity mask. The mechanism actually being relied on is that CPU affinity is **inherited across `fork`/`exec`**, and `nproc` does not measure inheritance.
- It does **not** establish that the runner was allocated 2 cores, or any particular number. It establishes only that the pinned count is *lower* than the unpinned one.
- It does **not** run at all when `CPUS=all` or when the host reports <= 2 CPUs. Both skips are printed, but a reader scanning for green must still notice which of the three outcomes actually occurred.
- Everything measured this session ran on a **4-CPU Fedora host with the job's steps simulated in a local bash**, not on `ubuntu-24.04` inside the pinned container. See coverage entry D5.

`nproc` here is diagnostic, not the sole oracle — which is the operator's C-03 disposition, restated as a limit rather than as a caveat.

## Guards demonstrated failing in their catching direction

Every guard touched or added was shown to fail in the direction it exists to catch. Verbatim, as required:

**1. The pre-46-04 guard was vacuous — replicated this session, not taken on report.** Commenting out the live suite step (`ci.yml:138`, `      # - run: taskset -c "$CPUS" scripts/check.sh all`) and running the old guard:

```
test ci_workflow_runs_the_sequential_check_under_a_cpu_pin ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out
cargo_exit=0
```

**2. The same bypass against the NEW guard.** Same edit, same file, after Task 2:

```
line now:           # "${CPU_PIN[@]}" scripts/check.sh all
test ci_workflow_runs_the_sequential_check_under_a_cpu_pin ... FAILED
/…/ci.yml: job `sequential` has no line whose trimmed content STARTS WITH
`"${CPU_PIN[@]}"` and runs `scripts/check.sh all` — the pinned invocation is
missing, commented out, or merely echoed
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 17 filtered out
```

`ci.yml` was restored after both probes (`git diff --stat -- .github/workflows/ci.yml` empty).

**3. Task 2's RED gate — every new test failing against deliberately vacuous stubs** (`42ea8bd`, `test result: FAILED. 11 passed; 7 failed`):

```
job_splitter_excludes_on_block_keys
    Got: ["push", "pull_request", "test", "sequential"]
job_splitter_finds_exactly_the_live_ci_jobs
    Got: ["push", "pull_request", "test", "clippy", "fmt", "sequential"]
recogniser_rejects_a_commented_out_pinned_suite_step   FAILED
recogniser_rejects_a_decoy_job_carrying_the_pin        FAILED
recogniser_rejects_an_echoed_pin_command               FAILED
recogniser_rejects_an_unpinned_suite_step              FAILED
container_jobs_using_bash_syntax_declare_a_bash_shell  FAILED
```

**4. Task 3's RED gate — the always-exit-0 stub, which IS the C-03 defect** (`76dec4f`, `test result: FAILED. 18 passed; 7 failed`): all six `assert_cpu_pin_*` tests plus `ci_workflow_calls_the_cpu_pin_decider_instead_of_echoing`.

**5. Task 1's expected RED, predicted by the plan.** After routing the pin through the helper, the old spelling-pinned guard went red exactly as forecast — `test result: FAILED. 10 passed; 1 failed`, `Found: []`. That a behaviour-preserving change breaks the guard is the C-02 finding restated.

**6. Negative control for Task 1's shell gates.** On a temp copy of `ci.yml` with a second `all` conditional and a re-typed `0,1` appended:

```
neg_retyped_literal_count=1              neg_retyped_gate_exit=1(GATE FIRED)
neg_duplicated_all_conditional_count=1   neg_duplicate_gate_exit=1(GATE FIRED)
```

On the real file both counts are `0` and both gates exit `0`. The gates discriminate.

**7. Negative control for the `--exact` evidence.** `-- --exact recogniser_rejects_a_commented_out_pinned_suite_step` gives `test result: ok. 1 passed; … 17 filtered out`, `cargo_exit=0`. The same command with a name matching nothing gives `test result: ok. 0 passed; … 18 filtered out`, `cargo_exit_for_nonexistent_name=0` — reproducing this repo's documented trap and confirming the `1 passed` + non-zero `filtered out` pair is what makes the evidence real.

**8. Negative control for the shellcheck clean.** `shellcheck scripts/assert-cpu-pin.sh scripts/lib/ci-cpus.sh` exits `0` with no findings. The same invocation on a copy with a deliberately unquoted glob exits `1` with 7 findings across `SC2086`, `SC2125`, `SC2317` — so the clean is a real clean, and the one `SC2016` disable (prose backticks inside `printf` format strings) is not suppressing anything else.

## Plan-level verification

| Check | Result |
|---|---|
| `cargo test -p devflow --test ci_parity_guards` | `test result: ok. 25 passed; 0 failed; 0 filtered out`, `cargo_exit=0` (11 tests before this plan) |
| `bash scripts/assert-cpu-pin.sh 0,1 4 4` | `exit=1` |
| `DEVFLOW_CI_CPUS=all` through `cpu_pin_prefix` | `all_prefix=<empty>`, `all_len=0`; baseline `taskset -c all nproc` -> exit 1, `failed to parse CPU list: all` |
| `cargo clippy --workspace --all-targets -- -D warnings` | `clippy_exit=0` |
| `cargo fmt --check` | `fmt_exit=0` |
| `cargo test --workspace --no-fail-fast` | `cargo_exit=0`, every `test result:` line `0 failed` (largest binary: 764 passed) |
| `.planning/user/DEV-SETUP-CHECKLIST.md` updated in the same commit as each `.github/workflows/` change | yes — `47741a6` and `59de2f7` |
| No `crates/devflow-cli/src/**`, `scripts/hooks/pre-commit`, or `CLAUDE.md` touched | `prohibited_path_count=0`, `scope_gate=PASS` |
| No `continue-on-error`; job name unchanged | `continue-on-error` count 0; `ci.yml:95  name: Sequential 2-CPU check` |

## Files Created/Modified

- `scripts/assert-cpu-pin.sh` **(new)** — the decider. Takes `<cpu_list> <unpinned_nproc> <pinned_nproc>`, measures nothing, four outcomes, both skips loud, usage + non-numeric guards.
- `scripts/lib/ci-cpus.sh` — adds `cpu_pin_prefix()`, the single home of the `all` case. Sets an argv-prefix array rather than executing, because the local gate needs it inside `docker run`. Still no `set -euo pipefail` (the file is sourced).
- `scripts/check-in-container.sh` — its private copy of the `all` conditional replaced by a `cpu_pin_prefix` call; invocation expands `"${CPU_PIN[@]}"`.
- `.github/workflows/ci.yml` — both pin sites re-source the fragment and call the helper; the load-shape step now calls the decider; comments rewritten to state the two skip conditions and the assertion's limits.
- `crates/devflow-cli/tests/ci_parity_guards.rs` — `split_jobs`, `recognise_pinned_sequential_job`, `PIN_TOKEN`, `LIVE_CI_JOB_KEYS`, 14 new tests; pin guard re-expressed through the recogniser; `jobs.len() >= 4` replaced by an exact key list.
- `crates/devflow-cli/tests/fixtures/ci-parity/{valid,commented-out,decoy-job,echoed-command,no-pin}.yml` **(new)**.
- `.planning/user/DEV-SETUP-CHECKLIST.md` — three new `[PATTERN]` bullets: the `cpu_pin_prefix` single-site rule, guard-as-fixture-driven-function, and assert-don't-echo.

## Decisions Made

1. **`PIN_TOKEN` is `"${CPU_PIN[@]}"`, not `taskset -c`.** After Task 1 the literal `taskset` line does not exist in `ci.yml`. Matching a spelling is what C-02 found wrong, but a recogniser must match *something* — the token chosen is the one the helper actually produces, and the guard's doc comment now says so explicitly so a future reader does not hunt for `taskset`.
2. **`echoed-command.yml` carries two echo lines.** The plan names `- run: echo 'taskset -c "$CPUS" scripts/check.sh all'` as a required case, but under the current token that string would be rejected by *any* rule, so it does not test the starts-with rule at all. A second line echoing the current token (`- run: echo '"${CPU_PIN[@]}" scripts/check.sh all'`) is the one that genuinely exercises it. Both are present.
3. **`no-pin.yml` keeps the `ci-cpus.sh` source line.** Otherwise it would fail on rule (a) and duplicate nothing useful; with the source line present it isolates rule (b) against a bare unpinned invocation.
4. **Narrow `SC2016` disable rather than rewording the prose.** `shellcheck` is not part of this repo's gate, but leaving six informational findings on a brand-new script means a future run's signal is noise. The disable is file-scoped to one code and verified not to be hiding anything.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The plan's Task 2 verify command puts `--exact` before `--`**

- **Found during:** Task 2 (verify step 2)
- **Issue:** `cargo test -p devflow --test ci_parity_guards --exact <name>` fails with `error: unexpected argument '--exact' found` and `cargo_exit=1`. `--exact` is a libtest argument and must follow `--`.
- **Fix:** Ran `cargo test -p devflow --test ci_parity_guards -- --exact <name>`. Result: `test result: ok. 1 passed; 0 failed; … 17 filtered out`, `cargo_exit=0`.
- **Files modified:** none (the defect is in the plan text, not in shipped code)
- **Verification:** corrected command run; negative control with a nonexistent name confirms `0 passed` / exit 0, so the `1 passed` + `17 filtered out` pair is real evidence.
- **Committed in:** n/a — no code change. Recorded here so the plan's verify block is not copied forward as-is.

**2. [Rule 3 - Blocking] The plan's Task 3 verify command reads the wrong exit code for `cargo fmt`**

- **Found during:** Task 3 (verify step 3)
- **Issue:** `cargo fmt --check 2>&1 | tail -3; echo "fmt_exit=$?"` reports `tail`'s exit code, not `cargo fmt`'s — the exact pipeline hazard this repo's CLAUDE.md documents. It would print `fmt_exit=0` for a formatting failure.
- **Fix:** Used `echo "fmt_exit=${PIPESTATUS[0]}"` (inside the `bash -c` the plan already wraps these in, since the executor's own shell is zsh where `${PIPESTATUS[0]}` expands to nothing).
- **Files modified:** none
- **Verification:** `clippy_exit=0`, `fmt_exit=0` with the corrected form.
- **Committed in:** n/a — no code change.

**3. [Rule 2 - Missing Critical] `shellcheck` hygiene on the two scripts authored here**

- **Found during:** Task 3
- **Issue:** `scripts/assert-cpu-pin.sh` produced six `SC2016` findings (prose backticks inside single-quoted `printf` formats) and `scripts/lib/ci-cpus.sh` one `SC2034` (`CPU_PIN` "unused" — it is consumed by callers). Not gate-blocking, but a new script that ships noisy makes a future shellcheck run useless as a signal.
- **Fix:** One file-scoped `# shellcheck disable=SC2016` above the first command in `assert-cpu-pin.sh` (placement matters — the first attempt landed after `set -euo pipefail` and only scoped to `usage()`), and a one-line `SC2034` disable on `cpu_pin_prefix`.
- **Files modified:** `scripts/assert-cpu-pin.sh`, `scripts/lib/ci-cpus.sh`
- **Verification:** `shellcheck` exit 0 on both, with the deliberately-broken-copy negative control above proving it still discriminates.
- **Committed in:** `59de2f7`

**Pre-existing and deliberately NOT fixed:** `scripts/check-in-container.sh:23` carries an `SC2155` warning that predates this plan and sits outside the lines it touches. Out of scope.

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing-critical). Two of the three are defects in the plan's own verification commands rather than in shipped code — both would have produced a false green if run as written, which is the same class this plan exists to close.
**Impact on plan:** No scope creep. No code behaviour changed by the deviations beyond two shellcheck directive comments.

## Issues Encountered

- **The commit subject on Task 1 exceeded 72 characters** (83) and the `commit-msg` hook warned without blocking. Amended to a 57-character subject (`47741a6`; the pre-amend hash `1b248a0` is orphaned). Task 2's RED subject is 76 characters and was left as-is — noted rather than silently ignored.
- **The temporary probe test used to print the recogniser verdicts** had to be added, run with `--nocapture`, and removed. It is not in the tree (`grep -c probe_46_04_print_verdicts` -> 0) and not in any commit. The verdict table above is its real output, not a reconstruction.

- **Three `gsd-tools` (v1.12.0) defects hit during the STATE update**, all corrected by hand and
  recorded here rather than absorbed silently:
  1. `state.advance-plan` read `total_plans: 3` from stale frontmatter, declared `reason:
     last_plan` / `status: ready_for_verification`, and set `current_plan` wrong — the phase has 8
     plans, 4 done. Matches this project's known quirk for that verb.
  2. **Every state verb that writes the frontmatter rewrites `milestone_name`** from
     `Unattended Run Survivability` to `milestone (ACTIVE — Unattended Run Survivability)`,
     evidently re-derived from a ROADMAP heading. Observed twice (`state.advance-plan`,
     `state.record-session`), so it must be corrected AFTER the last verb runs, not before.
  3. `state.add-decision --summary-file` rejects any absolute path outside the repo root
     (`Path escapes allowed directory`) and returns `added: false`. It succeeds with a
     repo-relative path. The rejection is loud, so it is a usability defect rather than a
     false-green one.
  `state.validate` returns `valid: true, warnings: []` after the hand corrections.

## Known Stubs

None. The two stubs written during this plan (`scripts/assert-cpu-pin.sh`'s `exit 0` body in `76dec4f`, and the vacuous `split_jobs`/`recognise_pinned_sequential_job` in `42ea8bd`) were TDD RED gates, both replaced in the immediately following GREEN commit. Neither survives at HEAD.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern, or schema change. `scripts/assert-cpu-pin.sh` reads only its three positional arguments, validates the two numeric ones against `*[!0-9]*`, and writes to stdout/stderr.

## User Setup Required

None — no external service configuration.

## Next Phase Readiness

- C-02, C-03 and C-06 are closed and evidenced. C-01, C-04, C-05 and C-07 are other plans' or deferred (`#207`).
- **Blocking nothing, but unproven on a runner:** coverage entry D5. Nothing in this plan has executed inside the pinned container on `ubuntu-24.04`. The two rewritten steps use bash array expansion, which depends on the job's existing `defaults: run: shell: bash` — present and guarded, but PR #208's precedent is that this job's shell environment has surprised the repo before. Phase verification should read `gh pr checks <PR>` against the current `HEAD_SHA`, and for the advisory `Sequential 2-CPU check` row specifically must read the **unfiltered** listing, since `--required` by definition excludes it.
- Not pushed. The pre-push container gate is the orchestrator's call.

## Self-Check: PASSED

- All 6 created files present on disk (`[ -f ]` each).
- All 5 commit hashes present in `git log --oneline --all`.
- Every `<acceptance_criteria>` / `<done>` condition re-run this session; every plan-level `<verification>` command re-run this session with its real output recorded above.

---
*Phase: 46-ci-load-shape-and-operator-input-validation*
*Completed: 2026-09-06*
