# Phase 46 — deferred items

Discoveries made during execution that are OUT OF SCOPE for the plan that found them.
Per the executor scope boundary: only issues directly caused by the current task's changes are
auto-fixed. These are not.

---

## D-46-01-A — `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` fails under the 2-CPU pin

**Found during:** 46-01 plan-level verification step 3 (`scripts/check-in-container.sh all`).
**Status:** RESOLVED — but **NOT by the commit this entry originally credited**.

Corrected 2026-09-06. This said "RESOLVED 2026-09-05 by `d525f9a`". `d525f9a` was the 47-line
`env_lock` sweep, and **46-08 reverted it** — those lines are gone (holders went 135 -> 85), so a
reader following that pointer would look for a fix that no longer exists.

The real fix is the child-process migration in **46-06** (`8969d90`, `e4fcd0d`) and **46-07**
(`a84b731`, `a1a8748`, `35e0259`): the seven PATH-emptying windows now run in child processes with
`PATH` set on the spawned `Command`, so no sibling test can lose `git`, and `NoGitPath` is deleted
(0 references remain). Phase verification re-tested the original symptom: `wr01_...` was 2/2
deterministic failure under the pin and is now **4/4 green under a real `taskset -c 0,1`**.

The "Resolution" section at the end of this entry describes the `env_lock` approach and is
retained as the historical record of what was tried; it is not what shipped.

**Symptom**

```
thread 'staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks' (7415)
panicked at crates/devflow-cli/src/staleness.rs:1436:22:
called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }
test result: FAILED. 362 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.08s
```

`staleness.rs:1436` is the `.unwrap()` on `.output()` inside the test's `git` closure — i.e. a
process **spawn** failed with ENOENT, not an assertion about staleness.

**The measurement: the CPU pin is the single differing variable**

Same host, same image, same commit, same working tree. Only `DEVFLOW_CI_CPUS` changed.

| Run | `==> cpus:` | Result | Gate exit |
|---|---|---|---|
| `scripts/check-in-container.sh all` | `0,1` | `wr01_...` FAILED | `container_gate_exit=101` |
| `scripts/check-in-container.sh test` | `0,1` | `wr01_...` FAILED | `container_test_exit=101` |
| `DEVFLOW_CI_CPUS=all scripts/check-in-container.sh test` | `all` | `wr01_...` ok | `allcpu_exit=0` |
| `DEVFLOW_CI_CPUS=all scripts/check-in-container.sh test` | `all` | `wr01_...` ok | `allcpu2_exit=0` |

Host, unpinned, outside the container: `cargo test --workspace --no-fail-fast` → `workspace_exit=0`,
`363 passed; 0 failed` in that binary.

**What this establishes:** the failure is deterministic under the pin (2/2) and absent without it
(2/2), so the pin is the variable that flips it. It is not a flake.

**Root cause — CONFIRMED, and previously documented**

`crates/devflow-cli/src/test_support.rs:361` defines an RAII guard that "REPLACES `PATH` with a
deliberately EMPTY directory, so `git` — and every other binary — cannot be resolved at all",
whose own doc comment states: "`Command::output()` returns `Err(NotFound)` when the program cannot
be spawned". That is exactly the observed panic. `NeutralPath::install` (`:336`) does the same via
`std::env::set_var("PATH", dir.path())` — process-global, affecting every test thread in the
`--bin devflow` binary.

`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` spawns `git` WITHOUT holding
`ENV_MUTEX`, so it can run inside another test's empty-`PATH` window and find no `git`.

**This is a known, still-unfixed defect.** `.planning/milestones/v2.4.0-phases/34-.../deferred-items.md`
§2 diagnosed the identical mechanism on 2026-09-02 — "the outer half does **not** hold `ENV_MUTEX`
while it spawns... a `PATH` on which `git` is unresolvable" — with a "**Plausible fix, not
applied**". STATE.md's deferred table carries it as "pre-existing `PATH`/`ENV_MUTEX` race, fix
identified not applied".

Note the VICTIM test differs: phase 34 saw
`embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`; this is
`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`. Same file, same mechanism,
different casualty — consistent with a process-global race that hits whichever unguarded spawn
happens to overlap the window.

**What 46-01 adds that was not previously known**

Phase 34 classified this as a **flake**. Under the 2-CPU pin it is **not flaky** — it is
deterministic, 2/2. The pin narrows the scheduler enough that the overlap happens every time. That
is a direct, unplanned demonstration of the load shape's value: it converts an intermittent defect
that survived acknowledgement into one that fails reliably and can therefore be fixed and
regression-tested.

**What this still does NOT establish:**
- The mechanism is inferred from the guard's documented behaviour and an exactly-matching panic
  signature, not from instrumenting the interleaving. Nobody has captured the two threads
  overlapping.
- n = 4 runs (2 pinned, 2 unpinned). Enough to act on; far short of a reliability
  characterisation, and it says nothing about how a 2-vCPU GitHub runner will behave, which is a
  different machine under a different scheduler.

**Why 46-01 did not fix it**

- Not caused by 46-01. The `0,1` pin **pre-dates** this phase (it was `check-in-container.sh:89`
  before 46-01 relocated it), and both consumers still resolve to `0,1` — verified in the gate's own
  header line `==> cpus:   0,1` after the relocation. 46-01 moved where the value is defined, not
  what it is.
- 46-01's only Rust change is `crates/devflow-cli/tests/ci_parity_guards.rs`, which compiles into a
  **separate** integration-test binary. The failing test is in `--bin devflow`, as the log's own
  `to rerun pass '-p devflow --bin devflow'` confirms.
- Fixing it means either finding the spawn-time root cause or reworking the process-global env
  mutation in `pipeline_launch.rs` — a different file, not in 46-01's `files_modified`, and
  plausibly architectural (executor deviation Rule 4).

**Why it matters more than a normal deferred item**

Phase 46's stated purpose is to give CI the load shape that surfaces this class of defect. The very
first local exercise of that shape surfaced one. Whatever the root cause turns out to be, the new
`Sequential 2-CPU check` job would be **red on its first CI run** for this reason — which is an
argument for D-05's advisory posture, not against the job.

**Suggested next step:** a dedicated debug pass (`/gsd-debug`) to find the spawn-time root cause
before this phase's PR is opened, or an explicit operator decision to open the PR with the advisory
job knowingly red and the reason recorded.

**Resolution (2026-09-05, commit `d525f9a`)**

Fixed as a class, not as this instance. All 7 PATH-emptying windows in the `--bin devflow` target
already hold `ENV_MUTEX`; the 47 git-spawning tests that could overlap them did not. The contract
was one-sided, so `env_lock()` was added to all 47 rather than to `wr01_` alone. `git_command` was
deliberately left unwrapped — `no_git_path_makes_git_unresolvable_and_restores_it` and the five
`NoGitPath` tests in `pipeline_outcomes.rs` call it precisely because it is the same constructor
production uses, and pinning `PATH` inside it would turn those negative controls green and vacuous.

**Correction to this entry's own measurement.** The claim above — "deterministic under the pin
(2/2) ... It is not a flake" — did not replicate. Four independent runs on an *idle* machine were
all green and discriminated nothing:

| Run | pin | fix | Result |
|---|---|---|---|
| host `taskset -c 0,1 cargo test` | `0,1` | present | 363 passed |
| host `taskset -c 0,1 cargo test` | `0,1` | reverted | 363 passed |
| `scripts/check-in-container.sh all` | `0,1` | present | `check.sh: all OK`, exit 0 |
| `scripts/check-in-container.sh all` | `0,1` | `wr01_` guard removed | `check.sh: all OK`, exit 0 |

It reproduced only once four competing CPU hogs were pinned to the same two cores — i.e. under the
load a busy agent session creates, which is the condition 46-01 was measured under. Contended:

| | failures |
|---|---|
| without the guard | 1 / 8 (`wr01_`, `Indeterminate != Stale` at `staleness.rs:1502`) |
| with the guard | 0 / 12 (8 contended + 4 uncontended) |

**What this does not establish.** 0/8 contended against a ~12.5% baseline has roughly a 34% chance
of occurring with no fix at all, so the counts are weak evidence on their own. The load-bearing
argument is mechanical: `ENV_MUTEX` makes the overlap impossible by construction, and the 1/8 run
confirms the overlap is the failure mode. The pin alone is *not* sufficient to reproduce this —
concurrent CPU load is also required — so a green pinned CI run on an idle runner should not be
read as evidence the race is gone.

The reproduced failure surfaced at `staleness.rs:1502` (`Indeterminate != Stale`) rather than
`1436:22` (`Os { code: 2, NotFound }`) as recorded above: the same unresolvable-`git` cause landing
after the code caught the spawn error instead of inside the fixture-setup closure.

## 46-05 #1 — `scripts/assert-cpu-pin.sh` is tracked mode `100644` and will fail in CI

**Status:** RESOLVED 2026-09-06 by `2deadf6` — `git update-index --chmod=+x`; the index now
reports `100755`. Negative control at the tracked mode: 644 -> exit 126 Permission denied,
755 -> exit 0. Applied by the orchestrator rather than deferred, because the alternative was
discovering a metadata bit as a red PR.

`git ls-files -s scripts/assert-cpu-pin.sh` reports `100644`. The script is invoked
**directly** in two places, neither through `bash <path>`:

- `.github/workflows/ci.yml:165` — `scripts/assert-cpu-pin.sh "$CPUS" "$unpinned" "$pinned"`
- `crates/devflow-cli/tests/ci_parity_guards.rs:874` — `Command::new(&path)`, whose own doc
  comment says it is invoked directly *"so a lost exec bit fails here instead of in CI"*

It passes locally only because the working-tree copy carries the bit and `core.fileMode` is
`false` in this checkout, so git never noticed. A fresh clone or a CI checkout materialises
mode `644`.

**Measured, with a negative control** (2026-09-06):

```
mode644_exit=126   mode644_err=bash: .../x.sh: Permission denied
mode755_exit=0
```

**Fix:** `git update-index --chmod=+x scripts/assert-cpu-pin.sh`, then verify with
`git ls-files -s`. Not applied here: 46-05's plan scopes it out of 46-04's files, and the
defect is not caused by anything 46-05 changed. `scripts/lint-plan-bashisms.sh` — added by
46-05 — hit the identical trap and was corrected in-plan, which is how this was noticed.

## 46-05 #2 — SEVEN tracked plans fail the scanner, not three

**Status:** open — informational; grandfathered by the scanner's staged-only scope.

**Count corrected 2026-09-06.** This entry said "three archived plans" with a bare `<automated>`
tag in prose. Measured against the finished scanner over `git ls-files '*PLAN.md'`, **14 files
fail — but 7 of those are the scanner's own must-fail fixtures** under
`crates/devflow-cli/tests/fixtures/plan-bashisms/`, which are supposed to fail. The real tracked
plans are **seven**: `46-01`, `46-02`, `46-03` (this phase's own, 12 occurrences between them),
`34-06` (2), and three never previously listed — `41-01`, `27-06`, and v1.0's `01-ci-tests/PLAN.md`
(1 each). Several carry real `${PIPESTATUS[0]}` bashisms in `<automated>` blocks, i.e. the actual
defect class, not only the prose-tag case described below. Grandfathering remains correct; the
number in the record was wrong.

The new scanner treats an opening tag with no closing tag as a malformed block (deliberately:
otherwise "open a tag and never close it" is a bypass by construction). Three already-committed
plans mention the tag as prose rather than opening a block, and would be refused if re-staged:

```
.planning/milestones/v1.0-phases/01-ci-tests/PLAN.md:27
.planning/milestones/v2.0.0-phases/27-scrub-.../27-06-PLAN.md:292
.planning/milestones/v2.8.0-phases/41-antigravity-driver/41-01-PLAN.md:414
```

This is the conservative direction the plan asked for — a false positive costs one edit, a false
negative is the whole finding — and the scanner's guidance now names the case and tells the author
to write the tag indirectly. Not fixed: retro-editing committed plans rewrites the executed record.
