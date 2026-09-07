---
phase: 46-ci-load-shape-and-operator-input-validation
verified: 2026-09-06T21:40:00Z
status: passed
score: 3/3 success criteria verified
behavior_unverified: 0
overrides_applied: 0
gaps_closed_after_verification:
  - truth: "SC-1 — the sequential 2-CPU job reports against a PR's current HEAD_SHA in `gh pr checks`"
    status: CLOSED 2026-09-06 — see the amendment appended to this report
    reason: >-
      The job exists, is correctly shaped, and is fully guarded — but the only CI green that
      exists is on PR #208 head `11d7dcb`, which is NOT an ancestor of the branch head `04c2748`
      (the two have diverged; 151 commits separate them). At `11d7dcb` the job was its PRE-46-04
      form: `scripts/assert-cpu-pin.sh` did not exist in the tree at all, and the pin step was
      the three bare `echo`s that review finding C-03 identified as a false-green, plus the
      literal `taskset -c "$CPUS"` that C-06 says dies on the `all` override. The CURRENT job's
      three `. scripts/lib/ci-cpus.sh` steps, its `cpu_pin_prefix` array expansion and its
      `assert-cpu-pin.sh` call have never executed on a GitHub runner.
    artifacts:
      - path: ".github/workflows/ci.yml"
        issue: "Current `sequential` job body is unproven in CI; the observed green covers a superseded body."
    missing:
      - "Push the current tree to the PR branch and observe `Sequential 2-CPU check` reporting against the new HEAD_SHA."
      - "Confirm the three parallel jobs (Test, Clippy, Format) are still green on that same HEAD_SHA."
deferred:
  - truth: "Validator/consumer ref-spelling mismatch — a real local branch named `HEAD` passes validation while the raw value reaches `git worktree add`"
    addressed_in: "GitHub #207 (OPEN)"
    evidence: "Operator disposition C-05/C-07; confirmed untouched — `ensure_base_is_a_local_branch` last modified by 46-02 (1272584), not by any gap plan."
  - truth: "`cargo nextest` adoption"
    addressed_in: "ROADMAP backlog 999.122"
    evidence: "Present in ROADMAP.md; measured as a wash (76s vs 78s) and structurally incompatible with `suite_reap_audit`."
---

# Phase 46: CI Load Shape and Operator Input Validation — Verification Report

**Phase Goal:** CI reproduces the sequential `fmt → clippy → test` load shape the local pre-push
gate runs, and the two operator inputs that fail confusingly today behave the way the rest of the
CLI does.

**Verified:** 2026-09-06 · **Status:** gaps_found · **Re-verification:** No — initial.

Verified in worktree `/var/home/denniyahh/Github/devflow/.worktrees/phase-46`, branch
`feature/phase-46`, HEAD `04c2748`.

---

## Verdict per success criterion

| # | Criterion | Verdict |
|---|---|---|
| 1 | Sequential 2-CPU CI job, pinned, reporting in `gh pr checks`, parallel jobs unaffected | **PARTIAL** |
| 2 | `base_branch` refuses a computed revision, negative control intact | **PASS** |
| 3 | `devflow stop` takes a positional project root, wrong root names the argument | **PASS** |

---

## Criterion 1 — Sequential 2-CPU CI job — PARTIAL

### What is verified

**The job exists and has the right shape.** `.github/workflows/ci.yml:94-180` defines job
`sequential` / name `Sequential 2-CPU check`, in the same pinned container as the other three,
with `defaults: run: shell: bash`. Its final step runs `"${CPU_PIN[@]}" scripts/check.sh all`.
`scripts/check.sh:57-61` shows `all` = `run_fmt` → `run_clippy` → `run_test`, in that order, in
one job. One job, sequential, correct order.

**The pin is real and narrows.** Run this session:

```
. scripts/lib/ci-cpus.sh; cpu_pin_prefix
CPUS=0,1 · unpinned=4 · pinned=2
```

The `all` override degrades correctly rather than dying (C-06): with `DEVFLOW_CI_CPUS=all`,
`prefix_len=0` and the bare `nproc` returns 4, exit 0.

**The pin decider discriminates — negative control established.** `scripts/assert-cpu-pin.sh`
takes its measurements as arguments, so its failing direction is reachable. Exercised all five
branches directly:

| Input | Result | Exit |
|---|---|---|
| `0,1 8 2` | PASSED | 0 |
| `0,1 8 8` | **FAILED** | **1** |
| `all 8 8` | SKIPPED (names reason) | 0 |
| `0,1 2 2` | SKIPPED (names reason) | 0 |
| `0,1 eight 2` | rejects non-numeric | 2 |

This is the criterion's most important property: the guard would go red if the pin stopped
narrowing. It is not a false-green.

**The parity guards would catch a regression in the job itself.** `cargo test -p devflow --test
ci_parity_guards` → `25 passed; 0 failed`. Both externally-demonstrated bypasses are regression
fixtures that assert `Err`: `commented-out.yml` (agy's bypass) and `decoy-job.yml` (codex's
bypass), plus `echoed-command.yml` and `no-pin.yml`. The job splitter is pinned to the exact live
job key list rather than to a count — the old `>= 4` assertion was satisfiable by the two `on:`
keys.

**The exec-bit defect that would have broken the job is fixed.** `deferred-items.md` §46-05 #1
records `scripts/assert-cpu-pin.sh` tracked as mode `100644`, which would fail in CI with exit 126
(the item carries its own measured negative control). It has since been corrected — `git ls-files
-s` now reports `100755`, applied in `2deadf6`. `scripts/lib/ci-cpus.sh` remains `100644`, which is
correct: it is sourced, never executed.

**The suite is green under a real pin.** `deferred-items.md` D-46-01-A recorded
`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` failing 2/2 deterministically
under the pin, resolved by `d525f9a` — which operator disposition C-01 then ordered **reverted** in
favour of child-processing. So I re-ran the failing direction after the revert:

```
taskset -c 0,1 cargo test -p devflow --bin devflow    ×4
run1..run4: test result: ok. 363 passed; 0 failed
wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks ... ok
```

4/4 green against a previously 2/2-deterministic failure. Supporting artifacts: `NoGitPath`
references = **0**, `env_lock()` holders = **85**, and
`an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` passes (1 passed, 362 filtered
out) — a both-directions test, not a presence check.

**Advisory status is correct.** `gh pr checks 208 --required` lists only `Test`, `Clippy`,
`Format`, `Build + test in devcontainer`; the ruleset for `develop` confirms exactly those four
contexts. `Sequential 2-CPU check` is deliberately absent, matching D-05.

### What is NOT verified — the gap

The criterion requires the job to **report against a PR's current `HEAD_SHA` in `gh pr checks`**.

`gh pr checks 208` does show `Sequential 2-CPU check  pass  3m35s`, and the check-runs API confirms
all seven contexts are bound to head_sha `11d7dcb3`. So the reporting *mechanism* is demonstrated.

But that head is stale in a way that matters:

```
branch head              04c2748
PR #208 head             11d7dcb   (branch feature/phase-46-pr)
git merge-base --is-ancestor 11d7dcb 04c2748  →  NO   (diverged)
git rev-list --count 11d7dcb..04c2748          →  151
```

And the divergence covers precisely the job under test:

```
git cat-file -e 11d7dcb:scripts/assert-cpu-pin.sh   →  ABSENT at 11d7dcb
git cat-file -e 11d7dcb:scripts/lint-plan-bashisms.sh → ABSENT at 11d7dcb
git diff --quiet 11d7dcb 04c2748 -- .github/workflows/ci.yml → DIFFERS (56 lines)
```

The job body that went green at `11d7dcb` was:

```yaml
- name: Record the load shape this job actually got
  run: |
    echo "unpinned_nproc=$(nproc)"
    echo "cpu_list=$CPUS"
    echo "pinned_nproc=$(taskset -c "$CPUS" nproc)"
- run: taskset -c "$CPUS" scripts/check.sh all
```

That is the C-03 false-green (three bare `echo`s, nothing compared) and the C-06 literal `taskset`.
**The green therefore certifies the defective form of the job, not the fixed one.** Every fix from
46-04 onward — the sourced `cpu_pin_prefix`, the `assert-cpu-pin.sh` call, the `${CPU_PIN[@]}`
array expansion that requires `shell: bash` — is unexercised on a runner.

This is a state gap, not a code defect: the code is present, correct, guarded, and locally green
under a real pin. It closes by pushing the current tree and observing one CI run.

---

## Criterion 2 — `base_branch` refuses a computed revision — PASS

**Implementation.** `commands.rs:168-217` qualifies to `refs/heads/{base}` and gates on `show-ref
--verify --quiet` (not `rev-parse`, which is what admitted the bypass). Refusals split into two
arms — missing-branch (keeps the `git branch …` advice) and malformed/revision (no advice) — and
both interpolate `{base}`, so both name the offending value.

**Independently reproduced in a disposable repo**, replicating the two git calls exactly, on a
`develop` that genuinely has a parent commit — including the literal `refs/heads/develop~1` the
in-repo test deliberately substitutes away from:

| Value | Outcome |
|---|---|
| `refs/heads/develop~1` | REFUSED → malformed/revision arm |
| `develop~1` | REFUSED → malformed/revision arm |
| `develop@{0}` | REFUSED → malformed/revision arm |
| `develop^{}` | REFUSED → malformed/revision arm |
| `develop` | **ACCEPTED** (negative control) |
| `workspace/example` | **ACCEPTED** (negative control) |
| `refs/heads/nonexistent-xyz` | REFUSED → missing-branch arm, pre-existing reason |
| `nonexistent-xyz` | REFUSED → missing-branch arm, pre-existing reason |

**The bypass is proven to have existed**, so the refusals are not vacuous — under `rev-parse
--verify` all three qualified forms resolve:

```
resolves: refs/heads/develop~1
resolves: refs/heads/develop@{0}
resolves: refs/heads/develop^{}
```

**Test is real, not a name-typo false-green:**

```
cargo test -p devflow --bin devflow ensure_base_is_a_local_branch
test result: ok. 1 passed; 0 failed; 362 filtered out
```

The test carries its own accept-arm negative control, prove-the-bypass probes on the *qualified*
form, and a paired `contains(spelling)` / `!contains("git branch")` discriminator that prevents the
two messages silently collapsing into one.

**Documented deviation, judged acceptable.** The criterion names `refs/heads/develop~1`; the test
substitutes `workspace/example~1`, because the fixture creates `develop` at the root commit so
`develop~1` cannot resolve and would pass identically before and after the fix. The reasoning is
recorded in the test's doc comment, and I verified the literal criterion value independently above.
The substitution strengthens the test rather than weakening it.

---

## Criterion 3 — `devflow stop` positional project root — PASS

**Definition.** `main.rs:345-355` gives `Stop` both `--root <ROOT>` (optional, documented as
overriding) and `project: PathBuf` with `default_value = "."` — matching `status`'s `[PROJECT]
Project root [default: .]`.

**Live behaviour, exit codes captured without a pipeline:**

| Command | rc | Output |
|---|---|---|
| `stop --phase 7 .` | 0 | `no lock held for phase 7` / `no persisted state` |
| `stop --phase 7 /nonexistent-xyz-root` | **1** | `error: project path does not exist: /nonexistent-xyz-root` |
| `stop --phase 7 --root /nonexistent-xyz-root` | 1 | same message |
| `stop --phase 7 --root /nonexistent-xyz-root /tmp` | 1 | `--root` wins, as documented |

The wrong-root failure **names the offending argument** and is not a bare clap usage error — the
exact #200 failure mode the criterion targets.

**Parity is byte-identical, not merely similar:**

```
stop   --phase 7 /nonexistent-xyz-root   rc=1  error: project path does not exist: /nonexistent-xyz-root
resume --phase 7 /nonexistent-xyz-root   rc=1  error: project path does not exist: /nonexistent-xyz-root
status           /nonexistent-xyz-root   rc=1  error: project path does not exist: /nonexistent-xyz-root
```

`cargo test -p devflow --test stop_e2e` → `14 passed; 0 failed`. The suite asserts both directions,
including the absence of the message on the success paths (`stop_e2e.rs:737,761`) — so it would
fail if the validator started rejecting valid roots.

**Scope note, not a gap.** Validation is existence-only: `stop --phase 7 /tmp` exits 0
("already stopped") rather than complaining that `/tmp` is not a DevFlow project. That matches
`resume`/`status` exactly and matches `stop`'s documented idempotence contract, so it is
parity-correct — which is what the criterion asks for.

---

## Requirements coverage

| Requirement | Status | Evidence |
|---|---|---|
| INFRA-01 — CI runs a sequential `fmt → clippy → test` job pinned to two CPUs | **PARTIAL** | Job present, correctly pinned, guarded by 25 passing parity tests and a five-way-discriminating decider; not yet observed running in CI in its current form (see Criterion 1 gap). |
| VALID-01 — `base_branch` naming a computed revision is refused | **SATISFIED** | Independently reproduced in a disposable repo with both negative controls; 1 passed / 362 filtered out. |
| VALID-02 — `stop` accepts the project root like `start`/`resume`/`status` | **SATISFIED** | Byte-identical error and rc across the three verbs; 14 `stop_e2e` tests pass. |

No orphaned requirements: REQUIREMENTS.md maps exactly INFRA-01, VALID-01, VALID-02 to Phase 46,
and all three are claimed by the plans.

---

## Third review round — C-08..C-10 (scanner) — all closed

A third adversarial round (commit `d8ca56e`, 2026-09-06) reviewed the staged-plan scanner built in
46-05 and raised three findings beyond C-01..C-07. **This round is single-lane evidence, not
cross-model consensus:** it was run by codex using Hermes / DeepSeek v4 Pro, and the agy lane
**dropped** — its stream was interrupted by a server restart before a final response, so it is
recorded as neither a finding nor a clean bill. Plan 46-09 was written and executed to close all
three.

I verified each independently in disposable Git repositories this session rather than accepting the
plan's or the orchestrator's word (full result table in W-1 below):

| Finding | Severity | Defect | Verified closed |
|---|---|---|---|
| C-08 | critical | `--staged` read the WORKING TREE, not the staged blob — stage a bad plan, overwrite the working file with a safe one, and it exited 0 while git committed the bad blob. Any `git add -p` / stage-then-edit workflow bypassed the hook in the dangerous direction. | **Yes.** Reproduced the exact scenario: now exits **1**, and fails for the RIGHT reason — it reports `46-x-PLAN.md:7: ${PIPESTATUS[0]}`, which is the *staged blob's* content, not the working tree's. `scripts/lint-plan-bashisms.sh:135-146` gates the filesystem-read branch on `staged -eq 0`. |
| C-09 | major | `--diff-filter=ACM` excluded renames (`R`), so a renamed bad plan yielded `scanned 0 file(s)` and exit 0. | **Yes.** `scripts/lint-plan-bashisms.sh:117` now uses `--diff-filter=ACMR`. Reproduced: a renamed plan whose destination is bad now exits 1. Deletions remain deliberately excluded — verified they produce `scanned 0 file(s)` / exit 0 rather than a read failure. |
| C-10 | major | The test suite never invoked `--staged` at all, so none of that mode's semantics had coverage. | **Yes, and slightly better than reported.** `plan_bashism_scanner.rs` now carries **four** `--staged` test functions, not three: `staged_bad_working_tree_clean_is_blocked`, `staged_clean_working_tree_bad_is_accepted`, `staged_renamed_bad_plan_is_blocked`, `staged_deleted_plan_is_skipped`. The second is the inverse-direction control — staged clean while the working tree is bad must be *accepted* — which is what makes the pair a real discriminator rather than a one-way assertion. Suite: `15 passed; 0 failed`. |

**The round's clean bill does not substitute for verification.** Hermes found no defect in the
child-process PATH migration, CPU `all` handling, stop precedence, or the container bash
declaration. Being single-lane, that is not consensus — and it is not what this report rests on.
Each of those four areas was verified here independently: the PATH migration by running
`an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` plus 4/4 pinned suite runs and
the `NoGitPath`=0 / `env_lock`=85 counts; `all` handling by executing the override and observing
`prefix_len=0`; stop precedence by running `--root` against a positional live; and the bash
declaration by reading `defaults: run: shell: bash` at `ci.yml:108-110` alongside the
`container_jobs_using_bash_syntax_declare_a_bash_shell` guard.

**Residual the round itself flagged, and I concur.** The runtime affinity assertion proves `taskset`
can narrow a child; the static job recogniser proves the suite line is pinned. Neither proves the
*suite* ran confined, and a sufficiently dynamic future rewrite of the job could evade both. This
is the same limit recorded under "What this does NOT establish".

---

## Warnings (not criterion-blocking)

**W-1 — C-04's "CI also runs it" half is unmet.** The operator disposition reads: *"Move the
scanner into a tested script CI also runs — a hook is early feedback, not enforcement."* The
scanner is a genuinely tested script — C-08..C-10 above are closed — and I confirmed its
behaviour in disposable repos rather than taking the tests' word:

| Case | Result |
|---|---|
| C-08 — bad blob staged, safe file in worktree | **rc=1, refused** (the false-green is closed) |
| clean staged plan | rc=0 (negative control) |
| no staged plans | rc=0, `scanned 0 file(s)` (negative control) |
| C-09 — renamed plan whose destination is bad | rc=1, refused |
| deleted plan | rc=0, `scanned 0 file(s)` — not a read error |

But `grep -rl "bashism\|lint-plan" .github/workflows/` returns **0 files**. The only invoker is
`scripts/hooks/pre-commit`. CI runs the scanner's *tests* (`plan_bashism_scanner`, 15 passed), never
the *scanner* against the repository's plans. Enforcement therefore depends on each developer
having `core.hooksPath=scripts/hooks` set and not using `--no-verify` — which is precisely the
"early feedback, not enforcement" distinction the disposition drew.

**W-2 — the grandfathered-violation count in the record is understated.** Scanning all 205
non-superseded plans, **7** fail — not the 3 named in `deferred-items.md` §46-05 #2:

```
v1.0-phases/01-ci-tests/PLAN.md
v2.0.0-phases/27-scrub-.../27-06-PLAN.md
v2.4.0-phases/34-.../34-06-PLAN.md          ← not listed in deferred-items.md
v2.8.0-phases/41-antigravity-driver/41-01-PLAN.md
46-01-PLAN.md  46-02-PLAN.md  46-03-PLAN.md ← this phase's own plans
```

`34-06` and this phase's first three plans carry real `${PIPESTATUS[0]}` bashisms inside
`<automated>` blocks — the actual defect class, not the prose-tag case the deferred item describes.
So some `<automated>` acceptance assertions in 46-01..46-03 may have been false-greens under zsh.
Mitigating: those three plans' outputs were re-reviewed and gap-closed by 46-04..46-09, and I
verified the resulting behaviours directly rather than through those blocks. Grandfathering is
still the right call (retro-editing rewrites the executed record) — the record just needs the
honest number.

---

## Deferred items — confirmed scoped out and actually untouched

| Item | Where | Confirmed |
|---|---|---|
| C-05 / C-07 — validator/consumer ref-spelling mismatch | GitHub **#207, OPEN** | `ensure_base_is_a_local_branch` last touched by `1272584` (46-02); no gap plan modified it. `worktree.rs` still forwards the raw branch to `git worktree add`. The function's doc comment records the open asymmetry as R-01. |
| `cargo nextest` adoption | ROADMAP `999.122` | Entry present; not adopted. |
| 14 pre-existing bashism-violating plans | scanner's staged-only scope | Left alone — though the real count is 7, see W-2. |

---

## What this does NOT establish

- **That the current CI job passes on a GitHub runner.** This is the gap above, stated plainly:
  every green that exists is for a superseded job body on a diverged commit.
- **That the pin confines rustc or the harness threads.** `nproc` measures what one child process
  can see, not affinity inheritance. `assert-cpu-pin.sh` says this itself; I am not claiming more.
- **That 4/4 pinned runs is a reliability characterisation.** It is a weak bound. It is meaningful
  only relative to a previously *deterministic* 2/2 failure — a flipped sign, not a guarantee. And
  it was measured on a 4-CPU host outside the container, not on a 2-vCPU GitHub runner inside the
  pinned image, which is a different scheduler on a different machine.
- **That the load shape catches the 999.47 `/proc` race.** Explicitly not claimed by the criterion,
  and nothing here tested it.
- **That the bashism scanner protects the repository.** It protects a commit that goes through the
  hook. `--no-verify`, an unset `core.hooksPath`, or any non-commit path is unguarded (W-1).
- **That the phase's own `<automated>` acceptance blocks were sound.** Three of them contain the
  bashism class the phase later built a scanner to catch (W-2). My verdicts rest on commands I ran
  this session, not on those blocks.
- **Working-tree cleanliness.** `graphify-out/graph.json` and `manifest.json` are modified and
  several files are untracked. No source file is dirty; this does not affect any verdict.

---

## Recommended next action

One CI observation closes the only gap. Push the current tree to the PR branch and assert:

```bash
gh pr checks <PR> --required          # four required contexts green on the new HEAD_SHA
gh pr checks <PR> | grep 'Sequential' # the advisory job's own row — --required hides it
```

Per CLAUDE.md, the advisory job is by definition absent from `--required`, so its row must be read
from the unfiltered listing or the check reports green without having looked at the thing being
accepted.

---

_Verified: 2026-09-06 · Verifier: Claude (gsd-verifier)_

---

## Amendment — SC-1 closed, 2026-09-06 (orchestrator)

The gap this report recorded was a STATE gap, not a code defect, and it has been closed by doing
the thing the report recommended (option (a): push and observe one CI run).

**What was done.** The PR branch was re-cut from the current phase head with
`scripts/cut-pr-branch.sh` (29 code commits included, 35 planning-only excluded, forbidden-path
audit clean), force-pushed, and PR #208 moved `11d7dcb` -> **`538ca1e`**. The local pre-push
container gate passed first (`check.sh: all OK` under `cpus: 0,1`).

**Evidence, read UNFILTERED per D-06** (`--required` omits this advisory job by definition), with
`gh pr view 208 --json headRefOid` matching the local branch head `538ca1e`:

    Sequential 2-CPU check   pass   3m28s
    Sequential 2-CPU check   pass   3m33s
    Test / Clippy / Format / Build + test in devcontainer   pass

Both `Sequential 2-CPU check` runs pass on code that carries every 46-04+ fix — the condition this
report correctly said had never been exercised on a runner.

**One required check failed first, and was not blindly re-run.** `Build + test in devcontainer`
failed once (2m50s) while its duplicate passed (3m9s) on the identical commit. Diagnosed before
acting: `reference_and_cleanup_worktree_cli_flow` timed out in `wait_for` (200 x 25ms = a 5-second
budget) waiting for a gate file. It is the pre-existing tracked flake **ROADMAP 999.23** (formerly
DEN-48, "`phase7_cli.rs` git-fixture reliability under CI concurrency") — the ROADMAP entry names
this exact test. Phase 46 never touched `phase7_cli.rs` (its only commit there is a `sync develop`
merge) and the test passes 3/3 locally. Only then was the single failed job re-run; it passed
(2m58s).

Contributing factor worth recording: `ci.yml` and `devcontainer.yml` each fire on BOTH `push` and
`pull_request`, so four workflow runs execute per commit and contend for runner capacity — which is
the load under which this 5-second budget lost. Unfiled.

**What this amendment does NOT establish.** That the sequential job catches the 999.47 `/proc`
race — still explicitly not claimed. That the job is stable: two passing runs is a weak bound, and
999.23 demonstrates this repo's CI is capacity-sensitive. That the bashism scanner runs on hosted
CI as a corpus scan — it deliberately does not; see the amended C-04 disposition.
