# Phase 46 — External adversarial review

**Date:** 2026-09-04
**Artifact reviewed:** `46-CONTEXT.md`
**Lanes run:** codex, agy (both external; neither is Claude)

## Lane roster and outcomes

| Lane | Model | Status | Findings | `file:line` citations |
|---|---|---|---|---|
| codex | default, `model_reasoning_effort=high` | completed, 148k tokens | 5 | 5 groups (~16 individual refs) |
| agy | `gemini-3.1-pro-high`, effort high, 30m print-timeout | completed | 2 | 2 groups |

Both lanes were probed live before the run (`codex exec` → `PROBE_OK`; `agy --version` → 1.1.24).
No lane dropped. Both wrote a complete `## CONFIRMED` / `## SUSPECTED` pair; both `SUSPECTED`
sections were `none`.

**Complementarity held again.** codex found four issues agy missed; agy's single critical finding
(R-02) was missed entirely by codex, despite codex's far higher citation count. Neither lane alone
would have been sufficient.

## Findings — all six independently verified against source before being recorded

Verification was performed by the orchestrator, not taken on the lanes' word. Each entry records
what was actually executed.

### R-01 — [critical] Validation and the fork consumer use different ref spellings
*Found by codex. Verified.*

`ensure_base_is_a_local_branch` validates the **fully-qualified** `refs/heads/{base}`, but the
consumer forwards the **raw** `{base}` to `git worktree add` as a start point. Any value that is
simultaneously a valid literal ref name and a revision expression passes validation and then forks
from something else.

- Demonstrated: `git branch '@'` succeeds; `git show-ref --verify refs/heads/@` exits 0; raw `@`
  resolves to `HEAD` (`develop` in the fixture), not to the branch named `@`.
- The asymmetry is documented in the repo's own comments — `commands.rs` states that
  `worktree::add` "forwards the raw value to `git worktree add` as a start point", and
  `parallel.rs` passes `base` through directly (`crates/devflow-core/src/worktree.rs:64-83`).
- **Pre-existing, not introduced by D-07** — today's `rev-parse` path prefixes `refs/heads/` too.
  The finding is that **D-07 does not close this class**, which the CONTEXT implies it does.
- Consequence: an accepted base can silently fork from the current checkout. Closing it requires
  validator and consumer to agree on one spelling, not a stricter validator alone.

### R-02 — [critical] "Advisory by omission" is not advisory to this repo's own agents
*Found by agy — missed by codex. Verified.*

D-05 makes the new CI job advisory by leaving it out of branch protection's required checks. That
governs GitHub's merge button only. `CLAUDE.md:64-65` separately requires asserting on a bare
`gh pr checks <PR>`, and `gh pr checks` shows **all** checks by default — the `--required` flag
exists precisely to narrow it.

- Verified from `gh pr checks --help`: `--required  Only show checks that are required`.
- Consequence: when the deliberately timing-sensitive job flakes, a bare `gh pr checks` reports a
  failing check, and an agent following `CLAUDE.md` treats the PR as not-green and stops. The job
  is advisory to GitHub and blocking to DevFlow's own agents — the exact opposite of D-05's intent.
- **Not executed:** that `gh pr checks` exits non-zero specifically on a failing non-required check
  was read from documentation, not reproduced against a live failing PR.

### R-03 — [major] D-08's two error messages have no discriminator under D-07
*Found by codex. Verified.*

D-07 substitutes a boolean `show-ref` check; D-08 requires two distinct refusal messages. Measured
in a scratch repo, `git show-ref --verify --quiet refs/heads/<x>` returns the **same** exit status
for every failure mode:

| spelling | exit |
|---|---|
| `develop` (real branch) | 0 |
| `develop~1` | 1 |
| `develop@{0}` | 1 |
| `nonexistent-xyz` | 1 |

- Consequence: a direct D-07 substitution can only produce one generic message, silently violating
  D-08.
- **The CONTEXT dismissed the tool that solves this.** D-07 rejects `check-ref-format` as "the
  wrong tool" — correct for the *existence* check, but it is exactly the right *classifier*: it
  rejects `develop~1`/`develop@{0}` on syntax while accepting `nonexistent-xyz`. The two decisions
  need both calls, which the CONTEXT does not say.

### R-04 — [major] Roadmap criterion 1 and D-02 disagree in text
*Found by both lanes independently — consensus.*

ROADMAP criterion 1 says the job runs "on a 2-core runner". D-02 delivers CPU **affinity** via
`taskset`, not a 2-core runner allocation.

- Verified by running the pinned image directly:
  `docker run --rm mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm sh -c '...'` →
  `which taskset` = `/usr/bin/taskset`, `nproc` = **4**, `taskset -c 0,1 nproc` = **2**.
- That measurement is its own negative control: 4 vs 2 from the same container proves the pin
  discriminates rather than the command merely succeeding.
- **Partly disconfirms codex's framing:** the decision is *not* unimplementable. `taskset` is
  present in the image and works. What is wrong is the roadmap's wording, which an auditor reading
  literally would fail.
- Consequence: amend criterion 1 to say what will be delivered (2-CPU affinity over a sequential
  run), or the phase closes against a criterion it does not literally meet.

### R-05 — [minor] D-01 misstates the existing CI pattern
*Found by codex. Verified.*

D-01 says the new job carries "the same `scripts/assert-image-parity.sh` step and `safe.directory`
step the three existing jobs carry". Only the `Test` job runs `assert-image-parity.sh`
(`.github/workflows/ci.yml:51-52`); `Clippy` and `Format` do not. All three do carry
`safe.directory`.

- Consequence: a planner told to "mirror the three existing jobs" would be following a description
  that does not match the file.

### R-06 — [minor] D-12's migration count is wrong
*Found by codex. Verified.*

D-12 records "11 in-repo call sites (`stop_e2e.rs` ×10, `reap_strays_e2e.rs:163`)". The actual
count is **8** in `stop_e2e.rs` (`grep -c -- '--root'` = 8) plus 1 in `reap_strays_e2e.rs` = **9**.

- Consequence: the decision itself is unaffected (`--root` is kept, so nothing migrates), but the
  document records a verification count that was never actually counted.

## Verification summary

Six findings raised, six confirmed against source. None was rejected. One (R-04) had its severity
framing corrected during verification — the approach is sound, only the roadmap's wording is wrong.

---

# Phase 46 — External adversarial CODE review

**Date:** 2026-09-05
**Artifact reviewed:** the `feature/phase-46-pr` diff vs `origin/develop` — 10 commits, head
`11d7dcb`, 15 files, 962 insertions. This is the code-stage review; the section above reviewed
`46-CONTEXT.md` at the plan stage.
**Lanes run:** codex, agy (both external; neither is Claude)

## Lane roster and outcomes

| Lane | Model | Status | Findings | `file:line` citations |
|---|---|---|---|---|
| codex | default, `model_reasoning_effort=high` | **dropped once**, then completed on retry, 192.8k tokens | 5 | 2261 |
| agy | `gemini-3.1-pro-high`, effort high, 30m print-timeout | completed, 7.0KB | 6 | 11 |

Both probed live first (`codex exec` → `PROBE_OK`; `agy --version` → 1.1.26).

**codex's drop, recorded as a typed reason rather than as a pass.** Its first run produced 39
bytes — `Reading additional input from stdin...` — and was then killed for host memory pressure.
The memory kill was incidental: `codex exec --help` documents that "if stdin is piped and a prompt
is also provided, stdin is appended as a `<stdin>` block", and a backgrounded invocation inherits
an open pipe that never closes, so it blocks forever before dispatching. **Re-run with
`< /dev/null`.** This is a third codex failure signature, distinct from the two the
`external-review` skill already documents (review-on-stderr, and empty output from a short
`--print-timeout`).

**Complementarity held for the third time.** Three findings were found by both lanes
independently (C-01, C-02, C-04). agy alone found C-05 and C-06; codex alone found C-03 and C-07.
Neither lane alone would have been sufficient — and C-03 in particular invalidates a verification
claim the orchestrator had already made in conversation.

## Findings — all verified against source before being recorded

Verification was performed by the orchestrator, not taken on the lanes' word. Each entry records
what was actually executed.

### C-01 — [critical] The `env_lock` class fix closes 47 of 78
*Found by agy and codex independently. Verified, and larger than either reported.*

`d525f9a` inserted `let _guard = env_lock();` into 47 tests found by scanning for **direct** git
spawns (`git_command(`, `run_git`, `Command::new("git")`). Tests that reach git only **through a
helper** were never matched.

- Measured transitively: **31 further tests** call one of 31 git-spawning non-test helpers
  (`init_repo`, `init_tagged_repo`, `currency_fixture`, `major_bump_fixture`,
  `reachability_fixture`, …) and hold no guard.
- codex traced one end-to-end: `preflight.rs:2557` → `reachability_fixture` → git spawn at
  `preflight.rs:2525`, sharing the unit-test process with `NoGitPath` at
  `pipeline_outcomes.rs:3224`.
- **The commit message for `d525f9a` states "Fixed as a class, not as this instance." That claim
  is false** and must be corrected. The flake is reduced, not eliminated.

### C-02 — [critical] The CI parity guards never check anything belongs to the `sequential` job
*Found by agy and codex independently. Verified by demonstration.*

`ci_parity_guards.rs:331`, `:341` and `:357` each search the whole workflow text independently.

- agy: `ci_workflow_runs_the_sequential_check_under_a_cpu_pin` uses `workflow.lines()` rather than
  the `code_lines` helper — whose own doc comment says guards that count occurrences must use it.
  **Demonstrated:** commenting out `- run: taskset -c "$CPUS" scripts/check.sh all` leaves CI
  running fully unpinned, and the guard still reports `test result: ok. 1 passed`.
- codex: a decoy job (or comments) can supply the job name, the `ci-cpus.sh` source path and the
  `taskset … check.sh all` text while `sequential` itself has none of them.
- The newer `container_jobs_using_bash_syntax_declare_a_bash_shell` splitter is job-scoped, but its
  own anti-vacuity assert is weak: it treats any 2-space key ending in `:` as a job, so `push:` and
  `pull_request:` from the `on:` block count — 6 "jobs" parsed for 4 real ones, and
  `assert!(jobs.len() >= 4)` can be satisfied by two non-jobs.

### C-03 — [major] D-04's negative control is echoed, never asserted
*Found by codex. Verified.*

`.github/workflows/ci.yml:128-130` states "The unpinned count is the negative control for the
pinned one -- they must differ." The step below it (`:131-135`) is three bare `echo`s with no
comparison, and always exits 0. The job reports green whether or not the pin narrowed anything.

This is the repo's own documented anti-pattern — the `rg -c` dead gate in CLAUDE.md — recurring:
documenting a gate is not implementing one. **The orchestrator cited this control in conversation
as evidence the pin took effect.** The observed values were real (`unpinned_nproc=4`,
`pinned_nproc=2`, read from the job log), so the pin did work on that run, but the job enforces
nothing and a future equal-count run would still be green.

### C-04 — [major] The pre-commit bashism guard is trivially bypassable
*Found by agy and codex independently. codex committed through the live hook to prove it.*

Both of these were committed successfully through `scripts/hooks/pre-commit` in a temp repo:

```md
<automated>
echo "${PIPESTATUS[0]}"
</automated>
```
```md
<automated>echo "${PIPESTATUS[0]}" # bash -c</automated>
```

- `pre-commit:113`'s `grep -v 'bash -c'` drops any line **containing** that string anywhere, so a
  trailing comment defeats it.
- `pre-commit:111` requires the bashism on the same line as `<automated>`; multi-line blocks escape
  (disclosed in the hook's own comment, but disclosure is not mitigation).
- False negatives: `${arr[0]}` (zsh indexes from 1) and `${BASH_REMATCH[1]}` (zsh populates
  `$match`) both expand silently empty under zsh and are not in the pattern.
- `git diff --name-only` quotes paths containing non-ASCII or tabs; `[ -f "$plan" ]` then fails and
  the file is skipped silently.

### C-05 — [major] The MISSING-arm advice is fatally broken for a whole input class
*Found by agy. Verified empirically against real git.*

`check-ref-format` accepts `refs/heads/{base}` for several values that are not usable branch names,
so they land in the MISSING arm and receive `git branch {base} origin/{base}` advice that cannot work:

| base | `check-ref-format refs/heads/{base}` | arm | what the advice actually does |
|---|---|---|---|
| `HEAD` | 0 | MISSING | `fatal: 'HEAD' is not a valid branch name` |
| `@` | 0 | MISSING | `fatal: not a valid object name: 'origin/@'` |
| `-x` | 0 | MISSING | ``error: unknown switch `x'`` |
| `refs/heads/main` | 0 | MISSING | directs the user to create `refs/heads/refs/heads/main` |

Negative controls hold: `develop~1`, `develop@{0}` and `""` correctly reach the MALFORMED arm;
`develop` correctly reaches MISSING. The values are all correctly **rejected** — the defect is that
D-08 existed specifically to make the refusal message useful.

### C-06 — [minor] `DEVFLOW_CI_CPUS=all` breaks CI while the local gate absorbs it
*Found by agy. Verified.*

`scripts/lib/ci-cpus.sh:18` advertises `DEVFLOW_CI_CPUS=all` as the override.
`scripts/check-in-container.sh:79-83` special-cases it to `PIN=()`; `.github/workflows/ci.yml:134`
and `:138` do an unconditional `taskset -c "$CPUS"`, which fails with
`taskset: failed to parse CPU list: all`. A parity break inside the single file whose only purpose
is parity.

### C-07 — [critical, pre-existing and deliberately out of scope] R-01 recurs, and is worse than recorded
*Found by codex. Reproduced by codex.*

R-01 above (tracked as GitHub #207) was recorded at the plan stage as "the validator qualifies
`refs/heads/{base}` while the consumer forwards the raw value". codex reproduced the consequence: a
**real local branch literally named `HEAD`** passes both validator commands, while
`crates/devflow-core/src/worktree.rs:81` forwards raw `HEAD` to `git worktree add`, so the worktree
is created at the current HEAD commit rather than at the branch. Its own output:
`base=HEAD show=0 format=0 configured=6c951b2… consumer=0 head=e24e216…` — configured and actual
differ.

This is not introduced by phase 46 and is explicitly deferred, but "still validates" understated
it: the failure mode is a silent wrong fork base, not a validation gap.

## Clean

`devflow stop` positional-vs-flag precedence and `--help`. Both lanes independently found no
defect, and the orchestrator verified it separately: `project: PathBuf` carries
`#[arg(default_value = ".")]`, `root: Option<PathBuf>`, and `main.rs:707` is
`root.unwrap_or(project)` — flag wins when supplied, positional otherwise, and the nine existing
`--root` call sites pass `Some` and are unchanged.

## Verification summary

| Finding | How verified |
|---|---|
| C-01 | Transitive call-graph scan over `crates/devflow-cli/src`: 31 git-spawning helpers, 31 unguarded tests reaching them |
| C-02 | Commented out the `taskset` step; guard still reported `1 passed`. Splitter re-run: 6 headers parsed for 4 jobs |
| C-03 | Read `ci.yml:126-135` — three `echo`s, no comparison, no `exit` |
| C-04 | Read `pre-commit:111,113`; codex committed both bypass forms through the live hook |
| C-05 | Ran `git check-ref-format` and `git branch` for all eight inputs in a scratch repo |
| C-06 | Read both consumers; ran `taskset -c all nproc` → `failed to parse CPU list: all` |
| C-07 | codex's own reproduction output, plus read of `worktree.rs:74-83` |

**Not checked:** live GitHub rulesets and hosted-runner CPU affinity; a full stress run of the
`ENV_MUTEX` race after a complete fix; whether any of the 31 C-01 tests has ever actually flaked in
CI history.

## Operator dispositions (2026-09-05)

Decided by Dennis Kim after the fix plan was itself adversarially reviewed by both lanes. The
plan as first drafted was substantially wrong; these supersede it.

| Finding | Disposition |
|---|---|
| C-01 | **Child-process the PATH-emptying tests.** Not the `env_lock` sweep. Revert the 47 `env_lock` lines added by `d525f9a`; leave the 88 pre-existing holders (they guard genuine env mutation, not this hazard). **Seven sites, not six** — the six `NoGitPath::install()` uses plus the ad-hoc `empty_path_dir` at `pipeline_launch.rs:2468`, which empties `PATH` by the same mechanism; missing it would leave the hazard live and make the reverts unsafe. |
| C-02 | Fix. Job-scope the parity guards, and unit-test the recogniser against fixtures (valid job passes; commented command fails; decoy job fails; `on.push` creates no job; `echo 'taskset …'` fails) — the current guards only inspect the live file, so they never exercise their own negative cases. |
| C-03 | Fix, **conditionally**. Assert pinned != unpinned only when `CPUS != all` AND `unpinned_nproc > 2`. An unconditional assertion contradicts C-06 and fires falsely on any 2-core host. `nproc` stays diagnostic, not the sole oracle. |
| C-04 | Fix. Block-parse `<automated>`; keep `--cached … --diff-filter=ACM` with `-z` and `read -r -d ''`; extend the pattern to any `${name[N]}` and `${BASH_REMATCH[N]}`. Move the scanner into a tested script CI also runs — a hook is early feedback, not enforcement. **AMENDED 2026-09-06 (operator-approved):** "CI also runs" is satisfied by the scanner's FIXTURE SUITE (`plan_bashism_scanner.rs` via `cargo test --workspace` -> `check.sh` -> all four CI jobs), NOT by a corpus scan. `origin/develop` tracks **zero** `PLAN.md` files, so a CI corpus scan would inspect nothing and report green on every PR — the exact false-green class this finding exists to close. A corpus-scan CI step is explicitly prohibited. Recorded here because phase verification scored this disposition unmet by reading the un-amended wording. Both proven bypasses become regression cases. |
| C-05 | **DEFERRED with #207.** codex established there is no safe middle: `commands.rs:185-187` returns `Ok` the moment the ref exists, so a new refusal arm never runs for the dangerous case (a real local branch named `HEAD`), while the raw value still reaches `git worktree add`. Advice-only polish would leave the silent wrong-fork intact. Fix it as the canonical-start-point change or not at all. |
| C-06 | Fix. Mirror the local gate's `all` special-case into **both** CI `taskset` call sites (`ci.yml:134` and `:138`), not just the suite step. |
| C-07 | Remains deferred as #207. Attach codex's wrong-fork reproduction to that issue so the severity is on record. |

**`cargo nextest` filed as backlog, not adopted now.** Per-test process isolation would delete the
C-01 hazard class outright, and nextest 0.9.143 is already installed. Measured on this workspace
before deciding: test sets are **identical** (name-diffed both directions — 0 missing; the single
apparent extra is a `#[should_panic]` formatting difference), wall clock is **76s vs 78s — a wash,
not a speedup**, and the repo has **0 doctests** so nextest's doctest gap costs nothing. It was
deferred for two concrete reasons, both measured: `devflow::phase7_cli suite_reap_audit` is
*structurally* incompatible — it is a cross-test invariant auditor that must observe siblings in
one process, and under isolation it sees `registered=0` and reports a false leak — and 6 further
tests are flagged "leaky" and have not been traced. Adopting it also means changing the gate
(`scripts/check.sh:19,48` and `ci.yml`) in the same phase that added the gate's newest job.

---

# Phase 46 — Final gap-closure review

**Date:** 2026-09-06
**Artifact reviewed:** source changes in `8fe168d..HEAD` (plans 46-07 and 46-08) plus the
new staged-plan scanner from 46-05.
**Lanes requested:** agy (Gemini 3.8 Flash, high) and Hermes / DeepSeek v4 Pro (high).

## Lane roster and outcome

| Lane | Outcome | Weight |
|---|---|---|
| agy | **dropped** — model began reviewing but its stream was interrupted by a server restart before a final response | no finding or clean bill counted |
| Hermes / DeepSeek v4 Pro | completed from its persistent session; read source and executed targeted checks | findings independently reproduced below |

The review is therefore single-lane evidence, not cross-model consensus. Its two confirmed
scanner failures are independently reproduced in disposable Git repositories, so they are not
accepted on the model's word.

## Findings

### C-08 — [critical] `--staged` scans the working tree instead of the staged blob

*Found by Hermes. Independently reproduced.*

`scripts/lint-plan-bashisms.sh:105` discovers paths from the index using
`git diff --cached --name-only -z`, but `:138` reads `awk … "$path"` from the working tree. It
never reads `git show :"$path"`.

Reproduction in a disposable repo: stage the known bad `comment-bypass-PLAN.md` (contains
`${PIPESTATUS[0]}` outside a real bash command), overwrite the working-tree file with the known
safe `legit-PLAN.md`, then run `bash scripts/lint-plan-bashisms.sh --staged`. It prints
`scanned 1 file(s)` and exits 0. The staged blob remains bad; `git commit` records the bad blob.
Any partial-staging workflow (`git add -p`, or stage then re-edit) bypasses the hook in the
dangerous direction.

### C-09 — [major] `--staged` silently skips renamed plans

*Found by Hermes. Independently reproduced.*

`scripts/lint-plan-bashisms.sh:105` uses `--diff-filter=ACM`, excluding `R`. In a disposable repo,
rename a staged bad `old-PLAN.md` to `new-PLAN.md`; Git reports `R100`, the ACM query yields zero
paths, and the scanner prints `scanned 0 file(s)` / exits 0. Renames are rare but this is exactly
the false-green shape the script was introduced to eliminate.

### C-10 — [major] The only mode CI does not exercise is `--staged`

*Found by Hermes. Verified by source inspection.*

`crates/devflow-cli/tests/plan_bashism_scanner.rs` invokes the scanner only with explicit fixture
paths; it never invokes `--staged`. Thus its `-z` parsing, diff filter, index-vs-working-tree
semantics, and the two failures above receive no automated coverage. A fix must include disposable
Git-repository fixtures that test real staged blobs, not only path arguments.

## Clean / residual

Hermes found no defect in the child-process PATH migration, CPU `all` handling, stop precedence,
or the container bash declaration. This is a single-lane clean bill, so it is not consensus and
does not replace verification. Its CI pin observation remains residual: the runtime affinity
assertion proves `taskset` can narrow a child, while the static job recogniser proves the suite
line is pinned; a sufficiently dynamic future rewrite could evade both.
