# Phase 46: CI Load Shape and Operator Input Validation — Research

**Researched:** 2026-09-04
**Domain:** GitHub Actions workflow shape + CPU affinity; git ref plumbing (`show-ref` /
`check-ref-format`); clap 4 derive argument shape
**Confidence:** HIGH — every load-bearing claim below was measured in this session, most with a
negative control in both directions. The three exceptions are tagged `[ASSUMED]` and listed in the
Assumptions Log.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

Copied from `46-CONTEXT.md` `<decisions>`. Every one is binding. Where research below bears on a
decision it *refines the mechanism*; nothing below reopens a decision.

**CI load shape (INFRA-01)**

- **D-01:** The new job runs **inside the pinned devcontainer image**
  (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`), carrying the `safe.directory` step
  that all three existing jobs carry and the `scripts/assert-image-parity.sh` step that **only the
  `Test` job** carries (`.github/workflows/ci.yml:51-52`). The new job should mirror `Test`, which
  is the job whose step set it actually wants.
- **D-02:** The load shape is pinned by **CPU constraint, not by runner size** — `taskset -c 0,1`
  over a sequential `check.sh all`. This makes the runner's actual core count irrelevant to
  correctness. ROADMAP criterion 1 was amended on 2026-09-04 to match (R-04).
- **D-03:** The `0,1` default is **sourced from `scripts/check-in-container.sh:89`**
  (`CPUS="${DEVFLOW_CI_CPUS:-0,1}"`) rather than re-typed into `ci.yml`, so the local gate and CI
  cannot drift apart. *Reversibility: costly.* **Constraint the planner must solve, not assume:**
  `check-in-container.sh` drives `docker run` and therefore **cannot be invoked from inside a
  GitHub container job**. The decision is the intent (one definition site); the mechanism is open.
  Do not interpret D-03 as "call `check-in-container.sh` from CI"; that path does not work.
- **D-04:** The job **records the shape it actually got** — print the CPU pin in effect and
  `nproc` as an early step — so the load shape is evidence in the run log rather than an
  assumption in the YAML.
- **D-05:** The job is **advisory, expressed by omission from branch protection's required
  checks**. Nothing in-tree marks it advisory. Explicitly rejected: `continue-on-error: true`.
  **Resolution (option 1):** routine green-ness assertions on this repo use
  `gh pr checks <PR> --required`; `CLAUDE.md`'s rule was amended out-of-band on 2026-09-04.
  **The `CLAUDE.md` edit is NOT a task for any plan in this phase** — it cannot be committed from
  this worktree.
- **D-06:** **Acceptance evidence is this phase's own PR.** The job must appear green in
  `gh pr checks <PR>` against that PR's current `HEAD_SHA` before the phase closes. No throwaway
  PR. **Acceptance reads the unfiltered listing, never `--required`.**

**base_branch validation (VALID-01)**

- **D-07:** Validation switches from `git rev-parse --verify --quiet refs/heads/{base}` to
  **`git show-ref --verify refs/heads/{base}`**. **What D-07 does not close (R-01):** the
  validator/consumer ref-spelling asymmetry. **Do not let a plan or a SUMMARY claim VALID-01
  closes this class.**
- **D-08:** The refusal emits **two distinct messages** — missing-branch (today's message and its
  `git branch {base} origin/{base}` advice) vs. revision-syntax (its own message identifying the
  suffix). **The discriminator is `git check-ref-format`, not `show-ref` (R-03).** The
  implementation needs **both** calls. D-10's test must exercise both refusal arms.
- **D-09:** The caller-side scoping stays as it is — the check remains applied only to a
  non-`Default` base, per the existing doc comment at `commands.rs:140-146`.
- **D-10:** The test extends the existing
  `ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch`
  (`commands.rs:5459`) rather than adding a parallel test. The existing accept-arms
  (`workspace/example`, `develop`) must still pass.

**`devflow stop` project root (VALID-02)**

- **D-11:** `stop` gains a **positional `[PROJECT]` with `default_value = "."`**, matching
  `resume`/`status` (`main.rs:182-183`, `257-258`) exactly.
- **D-12:** **`--root` is kept**, and **takes precedence over the positional when supplied**. No
  error on the both-supplied case; the flag simply wins. **This phase carries no breaking change
  and needs no `BREAKING CHANGE` CHANGELOG entry.** *Reversibility: reversible — additive only.*
- **D-13:** A genuinely wrong root must still fail, and must **name the offending argument**
  rather than emit a bare usage error. `project_root` (`main.rs:718`) already produces a
  path-naming error; the requirement is that the positional reaches it instead of dying in clap.

### Claude's Discretion

- Exact `ci.yml` job name, `timeout-minutes`, and step ordering, consistent with the existing
  three jobs.
- Exact wording of the two `CliError` messages in D-08, consistent with `commands.rs` conventions.
- The extraction mechanism for D-03's shared `0,1` default, subject to D-03's stated constraint.

### Deferred Ideas (OUT OF SCOPE)

- **Converging `evidence` and `sweep` onto a positional project root.** Raised, considered, and
  explicitly dropped by operator decision — not filed as a backlog entry, deliberately.
- **Promoting the new CI job to a required check.** Out of scope by D-05; revisit after the
  milestone has watched it across real pushes.
- **Measuring whether the new job actually catches 999.47.** Not answerable by adding the job.
- **Closing the validator/consumer ref-spelling asymmetry (R-01).** Pre-existing, untouched by
  D-07. **Not filed as a 999.x backlog entry** — whether to file it, or to widen VALID-01 to cover
  it, is the operator's call and has not been made.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| INFRA-01 | CI runs a job that reproduces the sequential `fmt → clippy → test` load shape on a 2-core runner | §"The sequential shape already exists in CI — the *pin* does not" (reframes the gap), §"D-03: extraction mechanisms measured", §"taskset failure modes", §"ci_parity_guards.rs is the local validation home" |
| VALID-01 | A configured `base_branch` naming a computed revision rather than a branch is refused | §"D-07 + D-08: the two-call implementation", the full classification table, and the `base_branch_fixture` trap that makes `develop~1` a false-positive test arm |
| VALID-02 | `devflow stop` accepts the project root the same way `start`, `resume` and `status` do | §"D-11 + D-12: the clap shape", `main.rs` line references, D-13 error-path confirmation, re-verified `--root` call-site count |
</phase_requirements>

---

## Summary

CONTEXT.md is unusually complete and I did not re-derive what it already measured. Research went
to the three gaps it left open plus the validation architecture, and it turned up **one finding
that reframes INFRA-01** and **one that would have produced a green-but-worthless test for
VALID-01**.

**The reframing.** `.github/workflows/devcontainer.yml` **already runs `scripts/check.sh all`
sequentially** and its job — `Build + test in devcontainer` — is a **required status check on both
`main` and `develop`**. INFRA-01's own re-verification note ("no `all` job exists") is wrong
because it read only `ci.yml`. So the sequential `fmt → clippy → test` shape is *not* absent from
CI. What is genuinely absent is the **2-CPU pin**, which is the substance of criterion 1 as
amended by D-02 ("pinned to two CPUs — the same constraint the local pre-push gate applies"). The
phase is still correct and still worth doing; the framing in the requirement, and criterion 1's
"the three existing parallel jobs", need to be read as **four existing checks, one of which is
already sequential**. A planner that writes "no sequential job exists in CI today" into a SUMMARY
would be shipping a false claim.

**The false-positive test arm.** `base_branch_fixture` (`commands.rs:5414`) branches `develop`
from the repository's **root commit**, so `develop~1` does not resolve *at all* in that fixture.
Today's `rev-parse --verify refs/heads/develop~1` already exits 1 there. Adding `develop~1` to
D-10's loop yields a test that passes identically before and after the fix — it proves nothing.
`workspace/example~1` is the `~N` spelling that actually reproduces the live bug under this
fixture, and `develop@{0}` / `develop^{}` reproduce it as-is. Measured table below.

**The mechanisms.** D-03's extraction has a clear winner on the "fails loudly" axis (a `grep`
whose exit status is captured, or a sourceable fragment; a `sed` extraction fails *silently*).
D-08's discriminator is `git check-ref-format` invoked on the **already-qualified**
`refs/heads/{base}` string with **no flags** — that form classifies all three revision spellings as
malformed and `nonexistent-xyz` as well-formed-but-missing, needs no repository, and sidesteps
option-injection. D-11/D-12's clap shape needs **no** conflict/override attribute:
`root.unwrap_or(project)` expresses the precedence exactly, and distinguishing a typed `.` from a
defaulted `.` turns out to be irrelevant to D-13.

**Primary recommendation:** three plans, one per requirement, in that dependency-free order —
VALID-01 and VALID-02 are pure Rust and land first because they are provable by `cargo test`;
INFRA-01 lands last in the *phase* but its job must be **merged and running** before later phases
push, so it is the plan whose acceptance (D-06) is deliberately deferred to this phase's own PR.

---

## Project Constraints (from CLAUDE.md)

The planner MUST honour these; they are the repo's own paid-for lessons and several bear directly
on this phase's `<automated>` blocks.

| # | Directive | Bearing on this phase |
|---|-----------|----------------------|
| C-1 | **`cargo test -p devflow --lib` verifies nothing** — `devflow` is binary-only; cargo exits non-zero before running a test. Use `-p devflow --bin devflow`. `-p devflow-core --lib` *is* valid. | Every VALID-01/VALID-02 acceptance command. `ensure_base_is_a_local_branch`'s test lives inside the **binary** crate's `#[cfg(test)]` module. |
| C-2 | **`cargo test --exact <name>` exits 0 when the name matches nothing.** Assert on a real `1 passed` with a **non-zero `filtered out`** count. | D-10's test-extension acceptance. |
| C-3 | **`rg -c <pat> \| rg '^0$'` is a constant-fail, not a zero-check** — `rg -c` prints nothing and exits 1 on zero matches. Print the count and the command's own exit code on separate lines and assert on those. | Any "ci.yml contains no literal `0,1`" check. Prefer a Rust guard in `ci_parity_guards.rs` over a shell grep. |
| C-4 | **A grep over source counts comment prose.** Strip comments before counting. | `scripts/check-in-container.sh:85-88` is a comment block that contains the string `2-core` and the word `CPU`; `ci.yml` is comment-heavy. A naive `grep '0,1' ci.yml` would match a comment. |
| C-5 | **`gh run list` (branch history) does not establish PR check status.** Assert on `gh pr checks <PR>` against the current `HEAD_SHA`. **Amendment (2026-09-04):** routine green-ness uses `gh pr checks --required`; **accepting the new advisory job requires the UNFILTERED listing**, because an advisory job is by definition absent from `--required`. | D-06. Both halves verified live — see §"gh pr checks: `--required` verified to filter". |
| C-6 | **A pipeline's exit code is the last command's.** Capture the exit code of the command you care about. | Any `... \| tail`/`\| head` in an acceptance block. |
| C-7 | **A revert that hangs is not a revert that fails.** Require the failing direction to *print a failure*, not merely to not-pass. | Every new guard in this phase needs a demonstrated failing direction (mutate → observe `test result: FAILED` → restore). |
| C-8 | **`git commit` runs against whatever branch is checked out.** Run `git rev-parse --abbrev-ref HEAD` before any commit not immediately preceded by a checkout. | Confirmed: this worktree is on `feature/phase-46` and tracks `.planning/` (1005 files). [VERIFIED: `git rev-parse --abbrev-ref HEAD` → `feature/phase-46`; `git ls-files .planning \| wc -l` → `1005`, this session] |
| C-9 | **Keep `DEV-SETUP-CHECKLIST.md` in sync in the same commit.** | Load-bearing here — see §"Pitfall 8". |
| C-10 | **Prefer GSD commands; declare bypasses out loud.** | Applies to plan/execute lifecycle, not to source edits. |
| C-11 | **Do not edit `CLAUDE.md` from this worktree** — the pre-commit hook refuses it on `feature/*`. | [VERIFIED: `scripts/hooks/pre-commit:69-73` — the refusal regex includes `CLAUDE\.md` and fires on any branch that is not `workspace/*` or `personal/*`.] **`.planning/` is explicitly exempt** (`pre-commit:60-65`), so `DEV-SETUP-CHECKLIST.md` *is* committable from here. |

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Sequential 2-CPU load shape | CI / GitHub Actions workflow (`.github/workflows/ci.yml`) | — | It is a *scheduling* property of the runner, expressible only where the job is defined. |
| The `0,1` default value | Shell script (`scripts/check-in-container.sh`, or a fragment it sources) | CI workflow (reader, never author) | D-03: one definition site. CI must **read**, never re-declare. |
| Asserting the CI job's *shape* | Rust integration test (`crates/devflow-cli/tests/ci_parity_guards.rs`) | — | The repo already owns this pattern: locally-runnable assertions over CI YAML. It is the only part of INFRA-01 a `cargo test` can prove. |
| Asserting the CI job *ran and was green* | GitHub API via `gh pr checks` (D-06) | — | Not expressible in-tree at all. Acceptance is the PR. |
| `base_branch` existence check | `devflow-cli` (`commands.rs::ensure_base_is_a_local_branch`) | git plumbing (`show-ref`) | Single call site at `commands.rs:332`; D-09 keeps caller-side scoping. |
| `base_branch` refusal-message classification | `devflow-cli` (same function) | git plumbing (`check-ref-format`) | Purely syntactic; no repository state involved. |
| Forking from the validated base | `devflow-core` (`worktree::add`) | — | **Out of scope (R-01).** The spelling asymmetry lives here, not in the validator. |
| CLI argument shape and precedence | `devflow-cli` (`main.rs` clap derive + dispatch arm) | — | D-11/D-12 are entirely `main.rs:345-352` and `:700-703`. |
| Path resolution + wrong-root error text | `devflow-cli` (`main.rs::project_root:718`) | — | D-13 needs no new error; it needs the positional to *reach* the existing one. |

---

## Standard Stack

**No new dependencies.** This phase adds zero packages in any ecosystem. Everything it needs is
already in the workspace or is a git/coreutils binary present in the pinned image.

### Core (already present)

| Component | Version | Purpose | Why standard here |
|-----------|---------|---------|-------------------|
| `clap` | `4` (workspace dep, `features = ["derive"]`) | CLI parsing | [VERIFIED: `Cargo.toml:29` — `clap = { version = "4", features = ["derive"] }`; `crates/devflow-cli/Cargo.toml:18` — `clap.workspace = true`] Already the shape `resume`/`status` use. |
| `git` | 2.55.0 on this host | `show-ref`, `check-ref-format` | [VERIFIED: `git --version` → `git version 2.55.0`, this session] Both subcommands are ancient plumbing; no version floor is at risk. |
| `taskset` (util-linux) | present at `/usr/bin/taskset` in the pinned image | CPU affinity | [VERIFIED: `docker run --rm mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm sh -c 'command -v taskset'` → `/usr/bin/taskset`, this session] |
| Rust built-in test harness (`cargo test`) | workspace toolchain | All automated validation | No third-party test framework in this repo. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `taskset -c 0,1` | `--cpuset-cpus` on a `docker run` | Not available: a GitHub `container:` job is started by the runner, not by us. `check-in-container.sh` uses `docker run` and is therefore uncallable from CI (D-03's stated constraint). |
| `taskset -c 0,1` | `cargo test -- --test-threads=2` | Constrains only the test harness's thread pool, not `rustc` codegen units, not `cargo`'s own job server, and not the process-level scheduling 999.47 is sensitive to. Not the local gate's shape. Reject. |
| `git check-ref-format` as the *existence* check | — | Already rejected by D-07 and correctly: it accepts `nonexistent-xyz`. It is the **classifier**, not the check. |
| A character denylist for revision syntax | — | Already rejected by D-07. `check-ref-format` is git's own rule set and cannot drift from it. |
| clap `conflicts_with` / `overrides_with` between `--root` and `[PROJECT]` | plain `root.unwrap_or(project)` | The attribute forms change `--help` and would make the both-supplied case an error, which D-12 forbids. Reject. |

**Installation:** none.

---

## Package Legitimacy Audit

**Not applicable — this phase installs zero external packages.** No `npm install`, no `cargo add`,
no `pip install`, no new GitHub Action beyond `actions/checkout@v7`, which is already pinned and
in use by all four existing jobs. [VERIFIED: `.github/workflows/ci.yml` and
`.github/workflows/devcontainer.yml` read in full this session — the only third-party actions
present are `actions/checkout@v7` (both files) and `devcontainers/ci@v0.3` (devcontainer.yml
only), neither of which this phase adds or changes.]

If a plan later proposes adding a caching action (e.g. `Swatinem/rust-cache`), that would
reintroduce this section and require the legitimacy gate. **Recommendation: do not.** See
Pitfall 9.

---

## Architecture Patterns

### The sequential shape already exists in CI — the *pin* does not

This is the single most important finding for INFRA-01 and it contradicts the requirement's own
re-verification note.

[VERIFIED: `.github/workflows/devcontainer.yml:34-52`, read in full this session]

```yaml
jobs:
  devcontainer:
    name: Build + test in devcontainer
    runs-on: ubuntu-24.04
    timeout-minutes: 40
    steps:
      - uses: actions/checkout@v7
      - name: Build devcontainer and run CI-parity checks
        uses: devcontainers/ci@v0.3
        with:
          push: never
          runCmd: |
            set -e
            git config --global --add safe.directory "$PWD"
            scripts/check.sh all
```

And it is **required** on both trunks:

[VERIFIED: `gh api repos/denniyahh/devflow/rulesets/19616771 --jq '.rules[] | select(.type=="required_status_checks") | .parameters'` and the same for ruleset `19616766`, this session — both return verbatim:]

```json
{"do_not_enforce_on_create":false,
 "required_status_checks":[{"context":"Test"},{"context":"Clippy"},{"context":"Format"},
                           {"context":"Build + test in devcontainer"}],
 "strict_required_status_checks_policy":true}
```

Consequences the planner must carry:

1. **There are four required checks, not three.** Criterion 1's "the three existing parallel jobs
   (`test`, `clippy`, `fmt`) still run and still pass" is about `ci.yml` only. The `Build + test in
   devcontainer` check must **also** still pass, and it is the one most likely to be perturbed if
   anyone touches `scripts/check.sh` — which this phase must not.
2. **The gap INFRA-01 closes is the CPU pin, not the sequential ordering.** Write SUMMARY prose
   accordingly. `.planning/REQUIREMENTS.md:116-124`'s "no `all` job exists" is scoped to `ci.yml`
   and is misleading read alone.
3. **Do not modify `Build + test in devcontainer` to add the pin.** It is required on both trunks,
   so pinning it would make a deliberately timing-sensitive constraint *blocking* — the exact
   inversion D-05 exists to prevent. It also runs via `devcontainers/ci@v0.3`, a different
   mechanism (it builds the devcontainer rather than running the pinned image directly).
4. **Never rename it.** `ci_parity_guards.rs:216-226` already guards this
   (`devcontainer_job_name_matches_the_required_status_check`), and the workflow's own header
   records the 2026-07-26 incident where deleting it wedged every merge to develop.
5. **Required checks live in RULESETS, and `ci.yml`'s header comment tells you to check the wrong
   place.** `ci.yml:10-12` recommends
   `gh api repos/denniyahh/devflow/branches/main/protection --jq .required_status_checks.contexts`,
   which returns only `["Test","Clippy","Format"]` and misses the fourth. The correct command is
   the one `devcontainer.yml:12-14` gives:
   `gh api repos/denniyahh/devflow/rules/branches/<branch>`. [VERIFIED both:
   `gh api repos/denniyahh/devflow/branches/main/protection --jq '.required_status_checks.contexts'`
   → `["Test","Clippy","Format"]`; `gh api repos/denniyahh/devflow/branches/develop/protection --jq '.required_status_checks'`
   → *empty*, and `gh api repos/denniyahh/devflow/rules/branches/develop` → the two `19616771`
   rules `pull_request` and `required_status_checks`.] Fixing `ci.yml`'s header comment is
   **discretionary** and cheap; it sits in a file this phase edits anyway.

### D-03: extraction mechanisms, measured

D-03 fixes the intent (one definition site) and leaves the mechanism to the planner. Three
candidates were built and each was run against three fixtures: the real script, a **value-changed**
copy (`0,1` → `0,1,2`), and an **anchor-moved** copy (`CPUS=` renamed to `CPU_LIST=`). The
anchor-moved fixture is the negative control — a mechanism that reports success against it is
silently broken.

[VERIFIED: all rows below executed this session against copies of `scripts/check-in-container.sh`
in a scratch directory.]

| Mechanism | real script | value changed to `0,1,2` | **anchor moved (negative control)** | Honours `DEVFLOW_CI_CPUS`? |
|---|---|---|---|---|
| **A. `sed` literal extraction**<br>`sed -n 's/^CPUS="\${DEVFLOW_CI_CPUS:-\(.*\)}"$/\1/p'` | `0,1`, exit 0 | `0,1,2`, exit 0 | **empty value, exit 0 — SILENT FAILURE** | no |
| **B. `eval` of the grepped assignment**<br>`eval "$(grep -E '^CPUS=' …)"` | `0,1` | `0,1,2` | **`CPUS` unset, `grep` exit 1 — LOUD** | **yes** — `DEVFLOW_CI_CPUS=all` → `CPUS=all`; unset → `CPUS=0,1` |
| **C. sourceable fragment** (`scripts/lib/ci-cpus.sh` containing the one line, sourced by both `check-in-container.sh` and the CI step) | `0,1` | `0,1,2` | **there is no anchor to move**; a missing/renamed file makes `.` fail under `set -e` — LOUD | yes |

**Recommendation: C, with B as the fallback.**

- **C is strongest** because it removes textual coupling entirely: CI and the local gate read the
  *same file*, so "the anchor moved" is not a reachable state. Cost: one new file plus a one-line
  change to `check-in-container.sh`. It **relocates** D-03's named definition site away from
  `check-in-container.sh:89`, which is a documentation change, not a violation — D-03's stated
  intent is "one definition site", and C keeps exactly one. The planner should say so explicitly in
  the plan rather than leave a reader to reconcile it with the CONTEXT's line reference.
- **B is the zero-touch fallback** if the planner would rather not modify the local gate's script
  at all. It preserves the `DEVFLOW_CI_CPUS` override semantics for free, which A does not.
  **B is only loud if the grep's exit status is actually captured** — `eval "$(grep …)"` as a
  single command does **not** trip `set -e` in a GitHub `run:` block. Capture it in an assignment
  first, and add a non-empty guard:

  ```yaml
  - name: Read the CPU pin from the local gate's single definition site
    run: |
      line="$(grep -E '^CPUS=' scripts/check-in-container.sh)"   # fails the step under -e if absent
      eval "$line"
      [ -n "${CPUS:-}" ] || { echo "could not read CPUS from scripts/check-in-container.sh" >&2; exit 1; }
      echo "CPUS=$CPUS" >> "$GITHUB_ENV"
  ```

- **A is rejected.** It fails silently on the one input that matters. `sed -n …p` exits 0 whether
  or not it printed anything, so the CI step would succeed with an empty pin and the job would run
  **unpinned** while its own log claimed nothing was wrong — a false-green generator of exactly the
  kind this repo has paid for twice.

**What would make the recommendation break:** C breaks if someone inlines the value back into
`check-in-container.sh` and deletes the fragment — loudly, at `.`-time. B breaks if the assignment
is restructured into a function or given a `readonly`/`declare` prefix — loudly, at `grep`-time.
Neither breaks on a value change, which is the intended-change case. **A `ci_parity_guards.rs`
assertion should pin whichever is chosen** (see Validation Architecture) so the mechanism cannot be
quietly replaced by a re-typed literal.

### `taskset` failure modes — the pin can narrow silently

[VERIFIED: all rows executed this session in the pinned image via
`docker run --rm mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm sh -c '…'`]

| Command | stdout | exit |
|---|---|---|
| `taskset -c 0,1 nproc` | `2` | 0 |
| `taskset -c 0,99 nproc` | **`1`** | **0 — partially-invalid list is SILENTLY NARROWED** |
| `taskset -c 98,99 nproc` | `taskset: failed to set pid 8's affinity: Invalid argument` | 1 |
| `nproc` (unpinned, this host's docker) | `4` | 0 |
| `taskset -c 0,1 sh -c 'sh -c nproc'` | `2` | 0 — **affinity is inherited by grandchildren** |
| `taskset -c 0,1 sh -c 'taskset -p $$'` | `pid 10's current affinity mask: 3` | 0 |

Two things follow:

1. **Inheritance is what makes the pin real.** Affinity crosses `fork`/`exec`, so `cargo`'s
   `rustc` invocations and the test harness's threads all inherit the 2-CPU mask. This is why the
   local gate's shape reproduces at all, and it is why `--test-threads=2` is not a substitute.
2. **D-04's print step must print the *effective* count, not echo the string.** A partially-valid
   list narrows to whatever CPUs exist and exits 0. `echo "cpus: $CPUS"` cannot distinguish a
   working `0,1` from a silently-narrowed one. Print both:

   ```yaml
   - name: Record the load shape this job actually got
     run: |
       echo "nproc (unpinned):        $(nproc)"
       echo "CPUS (from check-in-container.sh): $CPUS"
       echo "nproc under the pin:     $(taskset -c "$CPUS" nproc)"
   ```

   The third line is the measurement; the first is its negative control (they must differ). This
   is exactly the 4-vs-2 pairing `46-REVIEWS.md` R-04 used, moved into the job itself.

**Is `0,1` safe on the actual runner?** Yes. `denniyahh/devflow` is public, and GitHub's standard
`ubuntu-24.04` runner for **public** repositories is 4 vCPU / 16 GB (private repos get 2 vCPU /
8 GB). [CITED: https://docs.github.com/en/actions/reference/runners/github-hosted-runners] So CPUs
0 and 1 both exist and the mask is a genuine 2-CPU pin, not a narrowing. Note this makes
`scripts/check-in-container.sh:85-88`'s comment ("GitHub's standard hosted runners are 2-core")
**stale for this repository** — the value `0,1` is still right, the stated reason is not.
Correcting that comment is discretionary and belongs in whichever plan touches the file.

### D-07 + D-08: the two-call implementation

**The classifier is `git check-ref-format` invoked on the already-qualified `refs/heads/{base}`
string, with no flags.**

Why the qualified form and no flags, measured [VERIFIED: scratch repo, this session]:

| Argument form | `nonexistent-xyz` | `develop~1` | `-x` |
|---|---|---|---|
| `check-ref-format "refs/heads/$v"` | **0 (well-formed)** | **1 (malformed)** | 0 — the arg is `refs/heads/-x`, never option-shaped |
| `check-ref-format "$v"` (raw) | **1 — wrong**: a one-level name needs `--allow-onelevel` | 1 | **129 — usage error**: the raw value is parsed as a flag |
| `check-ref-format --allow-onelevel "$v"` | 0 | 1 | 129 — still option-shaped |

The raw form is wrong twice over: it misclassifies every plain branch name as malformed unless
`--allow-onelevel` is added, and it exposes an option-injection surface for a base beginning with
`-`. The code already builds `format!("refs/heads/{base}")` at `commands.rs:156`; reuse that exact
string for both calls.

**`check-ref-format` needs no repository.** [VERIFIED: run from `/`,
`git check-ref-format 'refs/heads/develop~1'` → exit 1 and
`git check-ref-format 'refs/heads/nonexistent-xyz'` → exit 0, this session.] It is a pure text
check, so it is hermetic and cannot be perturbed by the fixture's git state.

**Full behaviour table, evaluated inside a faithful replica of `base_branch_fixture`**
(`git init -b main`, one commit, `git branch develop`, `checkout -b workspace/example`, second
commit, `checkout develop`, `update-ref refs/remotes/origin/main`) [VERIFIED: this session]:

| `base` value | `rev-parse --verify -q refs/heads/$v` (**today's check**) | `show-ref --verify -q refs/heads/$v` (**D-07**) | `check-ref-format refs/heads/$v` (**D-08**) | Resulting arm |
|---|---|---|---|---|
| `workspace/example` | 0 | 0 | 0 | **ACCEPT** (existing negative control) |
| `develop` | 0 | 0 | 0 | **ACCEPT** (existing negative control) |
| `nonexistent-xyz` | 1 | 1 | 0 | reject → **MISSING** (today's message, unchanged) |
| `origin/main` | 1 | 1 | 0 | reject → **MISSING** (existing arm, unchanged) |
| `refs/heads/main` | 1 | 1 | 0 | reject → **MISSING** (existing arm, unchanged) |
| `HEAD` | 1 | 1 | 0 | reject → **MISSING** (existing arm, unchanged) |
| `workspace/example~1` | **0 — LIVE BUG** | 1 | 1 | reject → **MALFORMED** (new message) |
| `develop@{0}` | **0 — LIVE BUG** | 1 | 1 | reject → **MALFORMED** (new message) |
| `develop^{}` | **0 — LIVE BUG** | 1 | 1 | reject → **MALFORMED** (new message) |
| `develop~1` | 1 — *already rejected in this fixture* | 1 | 1 | reject → **MALFORMED** (new message) |

Read the `rev-parse` column as "what happens on `develop` today". Three spellings flip from accept
to reject; six were already rejected and must stay rejected; two must stay accepted.

**Implementation skeleton** (wording is Claude's discretion per CONTEXT; structure is not):

```rust
pub(crate) fn ensure_base_is_a_local_branch(
    project_root: &Path,
    base: &str,
) -> Result<(), CliError> {
    let qualified = format!("refs/heads/{base}");

    // D-07: existence. `show-ref --verify` refuses revision SUFFIX syntax that
    // survives the `refs/heads/` prefix; `rev-parse --verify` accepts it.
    let exists = git_command(project_root)
        .args(["show-ref", "--verify", "--quiet", &qualified])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if exists {
        return Ok(());
    }

    // D-08: classify the refusal. `check-ref-format` is syntax-only — it
    // cannot decide existence (it accepts `nonexistent-xyz`), which is exactly
    // why it is the right discriminator between "missing" and "malformed".
    // Fail-closed to the MISSING arm on a spawn error, matching today's
    // behaviour for an unavailable git.
    let well_formed = git_command(project_root)
        .args(["check-ref-format", &qualified])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(true);

    if well_formed {
        Err(CliError::Message(format!(
            // today's message, verbatim — including `git branch {base} origin/{base}`
        )))
    } else {
        Err(CliError::Message(format!(
            // new message: name the revision syntax as the problem, and do NOT
            // advise creating a branch literally named `{base}`.
        )))
    }
}
```

Two design notes the planner should carry into the plan text:

- **`.unwrap_or(false)` for existence, `.unwrap_or(true)` for the classifier.** A git that cannot
  be spawned must still *refuse* (fail-closed, as today) and must fall back to the *existing*
  message rather than a new message it has no evidence for.
- **`git_command` is the hermetic wrapper**, not `Command::new("git")`. [VERIFIED:
  `crates/devflow-core/src/git.rs:72-74` — `pub fn git_command(repo: &Path) -> Command { hermetic_command("git", repo) }`,
  which strips every `GIT_*` redirecting variable at `git.rs:87-94`.] It is already imported in
  `commands.rs:30`.

### D-11 + D-12: the clap shape

The shape being matched, verbatim [VERIFIED: `crates/devflow-cli/src/main.rs`, read this session]:

```rust
// main.rs:181-183 (Resume)         // main.rs:256-258 (Status)
/// Project root.                   /// Project root.
#[arg(default_value = ".")]         #[arg(default_value = ".")]
project: PathBuf,                   project: PathBuf,
```

What `Stop` is today [VERIFIED: `main.rs:345-352`]:

```rust
Stop {
    /// Phase to stop.
    #[arg(long)]
    phase: PhaseId,
    /// Project root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
},
```

and its dispatch arm [VERIFIED: `main.rs:700-703`]:

```rust
Command::Stop { phase, root } => stop(
    &project_root(root.unwrap_or_else(|| PathBuf::from(".")))?,
    phase,
),
```

**The change is two lines of variant and one of dispatch:**

```rust
Stop {
    #[arg(long)]
    phase: PhaseId,
    /// Project root. Overrides the positional argument when supplied.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Project root.
    #[arg(default_value = ".")]
    project: PathBuf,
},
```

```rust
Command::Stop { phase, root, project } => stop(&project_root(root.unwrap_or(project))?, phase),
```

`root.unwrap_or(project)` **is** D-12's precedence: `--root` wins when `Some`, the positional (whose
clap default is `.`) is used otherwise, and the both-supplied case is not an error. Note the
existing `unwrap_or_else(|| PathBuf::from("."))` becomes redundant and should be removed — the
positional's `default_value = "."` now supplies it, which is precisely what makes the shape
identical to `Resume`/`Status`.

**No conflict/override attribute is needed or wanted.** `conflicts_with` would make the
both-supplied case an error (forbidden by D-12); `overrides_with` is for repeated *flags* and does
not relate a flag to a positional. Both would alter `--help`. Plain `unwrap_or` changes `--help`
only by adding an `[PROJECT]` row, matching `resume`/`status`.

**Does "user typed `.`" vs "clap defaulted to `.`" matter for D-13?** **No.** Both produce the
identical `PathBuf(".")`, both flow into `project_root`, and `project_root` treats them
identically. D-13's requirement is about the *wrong-root* case, where the value is never `.`:

```rust
// main.rs:718-724 — already names the offending argument
fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if !project.exists() {
        return Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )));
    }
```

So D-13 needs **no new error path** — only for the positional to reach this function instead of
dying in clap. (For completeness: clap *can* distinguish the two via
`ArgMatches::value_source(...) == Some(ValueSource::DefaultValue)`, but that requires dropping to
the builder API or `#[command(flatten)]` gymnastics and buys nothing here. Do not.)

**Blast radius, re-verified.** [VERIFIED, this session:
`for f in $(grep -rl -- '--root' crates/devflow-cli/tests/); do grep -o -- '--root' "$f" | wc -l; done`
→ `stop_e2e.rs: 8`, `reap_strays_e2e.rs: 2`, `gate_sweep_e2e.rs: 2`. Of `reap_strays_e2e.rs`'s two,
one is the `//!` doc comment at `:4`; the call is `:163`. `gate_sweep_e2e.rs:177,217` are
`gate sweep --root`, a different subcommand.] **9 `stop --root` call sites** — confirming D-12's
corrected count exactly. All 9 keep working; nothing migrates.

**One existing test touches `stop --help`** and will not break:
[VERIFIED: `crates/devflow-cli/tests/stop_e2e.rs:314-326` — `stop_help_documents_phase_flag`
asserts only `stdout.contains("--phase")`.] It is the natural place to add a `[PROJECT]`
assertion.

---

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---|---|---|---|
| Deciding whether a string is a legal git ref name | A character denylist / regex for `~ ^ : ? * [ \` and `@{` | `git check-ref-format` | git's rules are more subtle than the obvious denylist (`..`, trailing `.lock`, leading/trailing `/`, `@` alone, control characters). A denylist drifts from git; the plumbing cannot. Already ruled by D-07/D-08. |
| Constraining the load shape | `--test-threads=N`, `-j N`, or a `CARGO_BUILD_JOBS` env | `taskset -c "$CPUS"` | Only affinity applies to the whole process tree (measured above: inherited by grandchildren). The other knobs constrain one layer each and are not what the local gate does. |
| Keeping CI's CPU value in sync with the local gate | Copying `0,1` into `ci.yml` with a "keep in sync" comment | Read it from the one definition site (§D-03) | This repo has already been bitten by exactly this class for the image tag, which is why `assert-image-parity.sh` exists. A comment is not a mechanism. |
| Asserting a workflow's shape | A shell `grep` in a plan's acceptance block | A `#[test]` in `crates/devflow-cli/tests/ci_parity_guards.rs` | The repo already owns this pattern (7 such tests). It runs under `cargo test`, fails with a readable message, and avoids CLAUDE.md's `rg -c` and comment-counting traps (C-3, C-4). |
| Detecting which checks are required | `gh api .../branches/<b>/protection` | `gh api repos/denniyahh/devflow/rules/branches/<b>` | Classic protection under-reports: it misses `Build + test in devcontainer`. Measured above. |
| A wrong-root error message for `stop` | A new `CliError` in the `Stop` arm | `project_root` (`main.rs:718`) | It already names the path. D-13 is a routing fix, not a message fix. |

**Key insight:** every "custom solution" available in this phase is a *re-declaration of a value or
rule that already has exactly one home in this repo*. The phase's whole theme — one definition of
green, one definition of the pin, one definition of a legal ref — is anti-duplication. A plan that
re-types a value has failed the phase's intent even if its tests pass.

---

## Common Pitfalls

### Pitfall 1: adding `develop~1` to D-10's loop proves nothing

**What goes wrong:** the new test arm passes identically before and after the fix.
**Why:** `base_branch_fixture` (`commands.rs:5426-5448`) creates `main` at a single root commit,
then `git branch develop` from it. `develop` therefore has **no parent**, and
`rev-parse --verify refs/heads/develop~1` already exits 1 today. [VERIFIED: fixture replicated
this session; `git log --oneline --all` shows `develop` and `main` both at the root commit `init`,
with only `workspace/example` carrying the second commit.]
**How to avoid:** use **`workspace/example~1`** for the `~N` arm — it resolves today (exit 0) and
must stop resolving after the fix. `develop@{0}` and `develop^{}` reproduce as-is and need no
fixture change.
**Warning sign:** an arm whose "before" behaviour was never asserted. The existing test's own
idiom is the guard — it asserts the bare `rev-parse` succeeds *first*, so the test "proves the
bypass exists rather than merely that the helper is strict" (`commands.rs:5479-5481`). **Keep that
idiom for the new arms**, but note it must assert the **qualified** `refs/heads/{spelling}` form
for the suffix cases, not the bare form the existing loop uses.

### Pitfall 2: mutating `base_branch_fixture` has a second consumer

**What goes wrong:** adding a commit to `develop` in the fixture to make `develop~1` resolve.
**Why it's risky:** `base_branch_fixture` is used by **two** tests. [VERIFIED: `grep -n
"base_branch_fixture" crates/devflow-cli/src/commands.rs` → definition at `:5414`, call sites at
`:5462` and `:5568`.] The second is
`no_worktree_start_forks_the_feature_branch_from_the_configured_base` (`:5565`), which asserts a
fork point against `workspace/example`.
**How to avoid:** prefer `workspace/example~1` (no fixture change at all). If the fixture must
change, run **both** tests and say so in the plan's acceptance block.

### Pitfall 3: `check-ref-format` on the raw value

**What goes wrong:** every plain branch name is classified as malformed, so *all* refusals get the
new message and the "missing branch" arm becomes unreachable — a silent D-08 violation with green
tests, because both arms still refuse.
**Why:** `check-ref-format` requires at least two slash-separated components unless
`--allow-onelevel`. Measured: raw `nonexistent-xyz` → exit 1; qualified → exit 0.
**How to avoid:** pass the **qualified** string. The negative control that catches this is the
`nonexistent-xyz` arm asserting the **missing** message specifically, not merely `is_err()`.

### Pitfall 4: asserting `is_err()` instead of asserting *which* message

**What goes wrong:** D-08 degrades to one generic message and the test cannot tell. This is R-03's
failure mode reappearing one level up.
**How to avoid:** D-10's assertions must match on message content — one arm asserting the
`git branch` advice is **present**, the other asserting it is **absent** and that the message names
the revision syntax. That pairing is the negative control.

### Pitfall 5: `taskset` narrows silently

**What goes wrong:** the job runs on a mask smaller than intended and reports success.
**Why:** `taskset -c 0,99 nproc` → `1`, exit **0** (measured). Only a *fully* invalid list errors.
**How to avoid:** D-04's print step must run `taskset -c "$CPUS" nproc`, not echo `$CPUS`.
On this repo's public-repo 4-vCPU runners `0,1` is safe [CITED: GitHub docs], but the print is
what makes that observable rather than assumed.

### Pitfall 6: `gh pr checks --required` for acceptance

**What goes wrong:** the acceptance command reports a green PR without ever looking at the job
being accepted, because D-05 deliberately keeps it out of required checks.
**How to avoid:** D-06 already forbids it. Acceptance greps the new job's own row out of the
**unfiltered** `gh pr checks <PR>` listing, against the PR's current `HEAD_SHA`. Routine
green-ness elsewhere uses `--required` (C-5).
**Both directions verified live** — see §"gh pr checks" below.

### Pitfall 7: the new job's name collides with, or gets added to, a required check

**What goes wrong:** naming the job `Test`, `Format`, `Clippy` or `Build + test in devcontainer`
either duplicates a required context or hijacks it.
**How to avoid:** pick a name outside that set (e.g. `Sequential 2-CPU check`) and **do not touch
the rulesets**. Verify with `gh api repos/denniyahh/devflow/rules/branches/develop` and the
`.../rulesets/<id>` parameters, not with classic branch protection.

### Pitfall 8: forgetting `DEV-SETUP-CHECKLIST.md` in the same commit

**What goes wrong:** a post-commit warning, and a checklist that is stale until someone notices.
**Why it applies here:** the hook's trigger regex covers **both** files this phase touches.
[VERIFIED: `scripts/hooks/post-commit:81` — the pattern includes
`^\.github/(workflows/|PULL_REQUEST_TEMPLATE\.md|ISSUE_TEMPLATE/)` **and**
`^scripts/(check\.sh|check-in-container\.sh|assert-image-parity\.sh)$`.] So the `ci.yml` change
triggers it, and so does D-03 mechanism C's edit to `check-in-container.sh`.
**What needs updating:** `.planning/user/DEV-SETUP-CHECKLIST.md:126` currently reads
"`.github/workflows/ci.yml` — three required jobs (`Test`, `Clippy`, `Format`)". After this phase
that line is wrong twice: there is a fourth job in `ci.yml`, and there is a fourth *required* check
that lives in `devcontainer.yml`. [VERIFIED: `.planning/user/DEV-SETUP-CHECKLIST.md:124-136`, read
this session.]
**And it is committable from here.** `.planning/` is explicitly exempt from the pre-commit
personal-artifact refusal (`scripts/hooks/pre-commit:60-65`), unlike `CLAUDE.md`.

### Pitfall 9: adding a cargo cache action to make the slow job faster

**What goes wrong:** the job stops measuring what it exists to measure. A warm cache changes which
crates compile, which changes the process/thread interleaving — the exact variable 999.47 is
sensitive to. It also introduces the first third-party action this phase would need to vet.
**How to avoid:** don't. Accept the runtime; the job is advisory (D-05) precisely so its slowness
is not on the merge path. See the timing budget below.

### Pitfall 10: an `<automated>` block that cannot fail

Restating C-2/C-3 concretely for this phase's most likely commands:

- ❌ `cargo test -p devflow --lib …` — exits non-zero before running anything.
- ❌ `cargo test … --exact some_name` alone — exits 0 on a typo'd name.
- ✅ `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch 2>&1 | tee /tmp/t.log; echo "cargo_exit=${PIPESTATUS[0]}"` then assert the log contains `1 passed` **and** a non-zero `filtered out`.
- ❌ `rg -c 'taskset' .github/workflows/ci.yml | rg '^0$'` — constant-fail.
- ✅ `n=$(rg -c 'taskset' .github/workflows/ci.yml || true); echo "count=${n:-0}"` then assert on `count=`.
- ✅ Better still: put the assertion in `ci_parity_guards.rs` and let `cargo test` own it.

---

## Code Examples

### The new `ci.yml` job (shape, not final wording)

Mirrors `Test` (D-01): pinned image, `safe.directory`, image parity. `timeout-minutes` is
discretionary — see the budget below.

```yaml
  sequential:
    name: Sequential 2-CPU check          # NOT a required context — D-05
    runs-on: ubuntu-24.04
    container:
      image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm   # literal, per ci_parity_guards
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v7
      - name: Trust the workspace
        run: git config --global --add safe.directory "$GITHUB_WORKSPACE"
      - name: Assert CI image matches the devcontainer definition
        run: scripts/assert-image-parity.sh "mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm"
      - name: Read the CPU pin from its single definition site      # D-03
        run: |
          . scripts/lib/ci-cpus.sh                                  # mechanism C
          echo "CPUS=$CPUS" >> "$GITHUB_ENV"
      - name: Record the load shape this job actually got           # D-04
        run: |
          echo "nproc (unpinned):    $(nproc)"
          echo "CPUS:                $CPUS"
          echo "nproc under the pin: $(taskset -c "$CPUS" nproc)"
      - run: taskset -c "$CPUS" scripts/check.sh all
```

**The image tag must be a literal on its own `image:` line.** [VERIFIED:
`crates/devflow-cli/tests/ci_parity_guards.rs:250-274` —
`ci_workflow_runs_the_pinned_devcontainer_image` iterates **every** line in `ci.yml` starting with
`image:` and `assert_eq!`s it against the tag parsed out of `.devcontainer/devcontainer.json`. A
YAML anchor, an `env` reference or a differing tag fails that test.]

**The run line must not start with `cargo`.** [VERIFIED: `ci_parity_guards.rs:148-158` —
`ci_workflow_delegates_to_the_shared_check_script` asserts no trimmed line starts with
`- run: cargo ` or `run: cargo `. A `taskset … scripts/check.sh all` line passes; it also does not
disturb that test's three `workflow.contains("scripts/check.sh <target>")` assertions.]

### Timing budget for `timeout-minutes`

Measured from a real run [VERIFIED:
`gh api repos/denniyahh/devflow/actions/jobs/<id> --jq '.steps[]'` for run `33686614066` on
`develop`, and `gh pr checks 201`, this session]:

| Job | Wall time | Notes |
|---|---|---|
| `Format` | 59 s | `Initialize containers` alone is ~28 s of that |
| `Clippy` | 1 m 03 s – 1 m 05 s | |
| `Test` | 2 m 02 s – 2 m 17 s | `scripts/check.sh test` step = **86 s**; cargo reports `Finished \`test\` profile … in 23.79s` |
| `Build + test in devcontainer` | 3 m 03 s – 3 m 51 s | already sequential `check.sh all`, unpinned, plus a devcontainer build |

`timeout-minutes: 45` is generous. The existing `Test` job uses 30 at full runner parallelism and
`Build + test in devcontainer` uses 40 for the same sequential work unpinned; halving the CPUs
roughly doubles the compile-bound portion. [ASSUMED — the 2-CPU sequential wall time has not been
measured; the first real run is the measurement, which is one more reason the job is advisory.]

### The D-10 test extension (structure)

```rust
// existing accept-arms stay exactly as they are — they are the negative control
assert!(ensure_base_is_a_local_branch(root, "workspace/example").is_ok());
assert!(ensure_base_is_a_local_branch(root, "develop").is_ok());

// existing reject-arms (origin/main, refs/heads/main, HEAD, <sha>) stay as they are

// NEW: revision-suffix spellings that survive the `refs/heads/` prefix.
// `workspace/example~1` — NOT `develop~1`: `develop` is the fixture's ROOT
// commit, so `develop~1` is already refused today and would pass unchanged.
for spelling in ["workspace/example~1", "develop@{0}", "develop^{}"] {
    // Prove the bypass exists first, on the QUALIFIED form the helper builds.
    let bare = git_command(root)
        .args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{spelling}")])
        .output().expect("rev-parse");
    assert!(bare.status.success(),
        "fixture is wrong: `refs/heads/{spelling}` must resolve today");

    let err = ensure_base_is_a_local_branch(root, spelling).unwrap_err();
    let msg = err.to_string();
    // D-08 arm A: names the revision syntax, and does NOT advise creating a
    // branch literally named `{spelling}`.
    assert!(!msg.contains("git branch"),
        "revision-syntax refusal must not repeat the create-a-branch advice: {msg}");
}

// D-08 arm B, the discriminator's negative control: a well-formed but missing
// branch keeps today's message and its advice.
let err = ensure_base_is_a_local_branch(root, "nonexistent-xyz").unwrap_err();
assert!(err.to_string().contains("git branch nonexistent-xyz origin/nonexistent-xyz"));
```

The `!msg.contains("git branch")` / `msg.contains("git branch …")` pair **is** the D-08
discriminator test. Without both halves the test cannot distinguish two messages from one.

### `gh pr checks`: `--required` verified to filter

[VERIFIED live against merged PR #201 (`feature/phase-45-pr` → `develop`), this session:]

```
$ gh pr checks 201            # 10 rows, exit 0
Graphify                        pass ...
Graphify Formal Verification    skipping ...
Build + test in devcontainer    pass  3m3s  ...
Build + test in devcontainer    pass  3m51s ...
Clippy / Format / Test          pass  (×2 each)

$ gh pr checks 201 --required  # 8 rows, exit 0
Build + test in devcontainer    pass  ×2
Clippy / Format / Test          pass  (×2 each)
```

The two `Graphify*` rows are dropped by `--required` and nothing else changes — that difference is
the negative control proving the flag actually filters rather than merely succeeding. R-02's
resolution is mechanically sound. **Not established:** that `gh pr checks` exits non-zero
specifically on a *failing non-required* check. Both runs above were green, so this measurement
says nothing about the failing case; `46-REVIEWS.md` R-02 already records that as read from
`--help`, not reproduced. It does not block this phase.

---

## Runtime State Inventory

This is not a rename phase, but it **does** change live service configuration that is not in git,
so the categories are answered rather than omitted.

| Category | Items found | Action required |
|---|---|---|
| Stored data | **None.** No datastore keys, collection names or IDs are affected. | none |
| Live service config | **Two repository rulesets hold the required-check list, and neither is in git**: `develop-merge-or-squash` (id `19616771`) and `main-squash-only` (id `19616766`), each listing `Test`, `Clippy`, `Format`, `Build + test in devcontainer`. Classic branch protection on `main` separately lists three; `develop`'s classic protection lists none. | **No change** — D-05 requires the new job be *omitted*. The action is to **verify** after merge that the list is still exactly these four, i.e. that nobody added the new job. |
| OS-registered state | **None.** | none |
| Secrets / env vars | `DEVFLOW_CI_CPUS` is an *optional override* read by `check-in-container.sh:89` and (under D-03 mechanism B or C) by the new CI job. It is not a secret and is unset everywhere by default. | none — but the plan should state whether CI sets it (recommendation: no, so CI gets the same `0,1` default the local gate gets) |
| Build artifacts / installed packages | **None.** No package rename, no egg-info, no published artifact. The new CI job creates no cache. | none |

---

## Environment Availability

| Dependency | Required by | Available | Version | Fallback |
|---|---|---|---|---|
| `git` | VALID-01 (`show-ref`, `check-ref-format`) | ✓ | 2.55.0 | — |
| `docker` + pinned image | verifying the CI job's image behaviour locally | ✓ | image `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` present in the local image cache | `DEVFLOW_SKIP_CONTAINER_CHECK=1` (host checks; explicitly **not** equivalent — `scripts/hooks/pre-push:211-217`) |
| `taskset` | INFRA-01 | ✓ host `/home/linuxbrew/.linuxbrew/bin/taskset`; ✓ image `/usr/bin/taskset` | util-linux | none needed |
| `gh` | D-06 acceptance | ✓ | 2.100.0 (2026-09-03) | none — D-06 is not satisfiable without it |
| `cargo` / workspace toolchain | all three requirements | ✓ (repo builds in CI today) | pinned via `rust-toolchain.toml` | — |
| `rg` | acceptance-block greps | ✓ | — | prefer `ci_parity_guards.rs` assertions instead (C-3) |

**Missing dependencies with no fallback:** none.

---

## Validation Architecture

### Test Framework

| Property | Value |
|---|---|
| Framework | Rust built-in `libtest` via `cargo test`. No third-party test framework in this workspace. |
| Config file | none — `Cargo.toml` workspace members only. Unit tests live in `#[cfg(test)] mod tests` inside `commands.rs` / `main.rs`; integration tests in `crates/devflow-cli/tests/*.rs`. |
| Quick run command (per task) | `cargo test -p devflow --bin devflow <name>` for unit tests; `cargo test -p devflow --test <file_stem>` for integration tests. **Never** `-p devflow --lib` (C-1). |
| Full suite command | `scripts/check.sh all` locally, or `scripts/check-in-container.sh all` for CI parity (what the pre-push hook runs, `scripts/hooks/pre-push:220`). |

### Phase Requirements → Test Map

| Req | Behaviour | Test type | Automated command | File exists? |
|---|---|---|---|---|
| VALID-01 | `develop@{0}`, `develop^{}`, `workspace/example~1` are refused; `workspace/example` and `develop` still accepted; `origin/main`/`refs/heads/main`/`HEAD`/`<sha>` still refused | unit | `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch` | ✅ `commands.rs:5459` — **extend**, per D-10 |
| VALID-01 | The two refusal messages are distinguishable (D-08) | unit | same test — the `contains("git branch …")` / `!contains("git branch")` pair | ✅ same test |
| VALID-02 | `devflow stop --phase N .` succeeds; `--root` still works; `--root` wins when both supplied | integration (e2e, spawns the built binary) | `cargo test -p devflow --test stop_e2e` | ✅ `crates/devflow-cli/tests/stop_e2e.rs` — **add** cases |
| VALID-02 | A wrong root fails with a message naming the path, not a bare usage error (D-13) | integration | same file: assert exit != 0 **and** stderr contains `project path does not exist:` and the offending path | ✅ same file |
| VALID-02 | `--help` documents the positional | integration | `cargo test -p devflow --test stop_e2e stop_help_documents_phase_flag` | ✅ `stop_e2e.rs:315` — **extend** to also assert `PROJECT` |
| INFRA-01 | `ci.yml` defines a job that runs `scripts/check.sh all` under a `taskset` pin | integration (YAML assertion) | `cargo test -p devflow --test ci_parity_guards` | ✅ `crates/devflow-cli/tests/ci_parity_guards.rs` — **add** a guard |
| INFRA-01 | `ci.yml` does **not** re-type the `0,1` literal (D-03's single definition site) | integration (YAML assertion, comments stripped — C-4) | same file | ✅ same file — **add** a guard |
| INFRA-01 | The new job's name is not one of the four required contexts (D-05) | integration (static list, with the `gh api …/rules/branches/<b>` command in the failure message, matching the existing `devcontainer_job_name_matches_the_required_status_check` idiom) | same file | ✅ same file — **add** a guard |
| INFRA-01 | **The job actually ran and was green on the PR's current `HEAD_SHA`** (D-06) | **manual / external — not automatable in-tree** | `gh pr checks <PR>` **unfiltered**, then grep the new job's row; separately confirm `gh pr view <PR> --json headRefOid` matches the run's commit | ❌ — acceptance evidence, recorded in the phase's SUMMARY/VERIFICATION, not a test file |

**Why INFRA-01 splits in two.** Nothing in-tree can prove a GitHub job ran. What `cargo test`
*can* prove is that the workflow file has the required shape — which is precisely the pattern
`ci_parity_guards.rs` already exists for. Every guard added there must have a **demonstrated
failing direction** (C-7): mutate the YAML, run the test, capture a real `test result: FAILED`,
restore. A guard whose failing direction was never observed is a guard that has not been shown to
discriminate.

### Sampling Rate

- **Per task commit:** the narrow command for the file touched — e.g.
  `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch` (assert `1 passed` **and**
  non-zero `filtered out`, per C-2), or `cargo test -p devflow --test ci_parity_guards`.
- **Per wave merge:** `cargo test --workspace --no-fail-fast` (i.e. `scripts/check.sh test`).
- **Phase gate:** `scripts/check-in-container.sh all` green (this is what `git push` runs anyway
  via `scripts/hooks/pre-push:220`), then `gh pr checks <PR>` on the PR — `--required` for
  green-ness (C-5), **unfiltered** for accepting the new job (D-06).

### Wave 0 Gaps

**None — existing test infrastructure covers all three requirements.** All three target files
already exist and are already wired into `cargo test`:

- `crates/devflow-cli/src/commands.rs` `#[cfg(test)] mod tests` (VALID-01)
- `crates/devflow-cli/tests/stop_e2e.rs` (VALID-02)
- `crates/devflow-cli/tests/ci_parity_guards.rs` (INFRA-01, the automatable half)

No framework install, no `conftest`-equivalent, no new test file is required. Every change is an
extension of an existing test or an addition to an existing integration-test file.

---

## Security Domain

`security_enforcement` is not set in `.planning/config.json`; absent = enabled, so the section is
included. This phase's attack surface is small but non-empty.

### Applicable ASVS categories

| ASVS category | Applies | Standard control |
|---|---|---|
| V2 Authentication | no | no auth surface |
| V3 Session Management | no | — |
| V4 Access Control | **yes, indirectly** | Branch-protection rulesets are the access control being *relied on* (D-05). The control is "do not add the new job to the required list", verified via `gh api …/rules/branches/<b>`. |
| V5 Input Validation | **yes — this is VALID-01's whole content** | `git check-ref-format` (git's own rule set) rather than a hand-rolled denylist; the value is always passed in its `refs/heads/`-qualified form, which is never option-shaped. |
| V6 Cryptography | no | — |
| V12 Files & Resources | **yes, mildly** | `project_root` canonicalises and walks up (`main.rs:726-738`); a non-existent path is rejected before canonicalisation. Adding a positional does not widen this. |

### Known threat patterns for this stack

| Pattern | STRIDE | Standard mitigation | Status here |
|---|---|---|---|
| Argument/option injection via a `-`-prefixed value reaching `git` | Tampering | Pass the qualified `refs/heads/{base}` string, never the raw value | Measured: raw `-x` → `check-ref-format` exit **129** (parsed as a flag); qualified `refs/heads/-x` → exit 0. Recommendation already reflects this. |
| Ref-spelling confusion: validator and consumer disagree | Tampering / EoP (fork from unintended commit) | Validator and consumer must agree on one spelling | **Present and OUT OF SCOPE (R-01).** `worktree::add` forwards the raw `{base}` (`crates/devflow-core/src/worktree.rs:64-83`). Not closed by D-07. Must not be claimed closed. |
| `eval` of file content in CI | Code execution | Prefer sourcing a dedicated fragment over `eval`-ing a grepped line | D-03 mechanism C avoids `eval` entirely; mechanism B `eval`s a line from a file already fully trusted (it is executed verbatim by the pre-push gate). Low risk either way; C is cleaner. |
| A CI job that reports SUCCESS on a real failure | Repudiation | Never `continue-on-error: true` | Already rejected by D-05, explicitly. |
| Supply chain: a new third-party GitHub Action | Tampering | Pin by SHA / vet | **Not applicable — this phase adds no action.** Do not add a cache action (Pitfall 9). |

---

## State of the Art

| Old approach | Current approach | When changed | Impact here |
|---|---|---|---|
| Required checks in classic branch protection | **Repository rulesets** | GitHub rulesets GA, 2023 | `gh api …/branches/<b>/protection` under-reports. Use `gh api …/rules/branches/<b>`. `ci.yml:10-12`'s header still recommends the old command. |
| `rev-parse --verify <ref>` as an existence check | `show-ref --verify <fully-qualified-ref>` | long-standing git guidance | Exactly D-07. `rev-parse` is a *revision parser*; `show-ref` is a *ref lookup*. Using a revision parser as a ref existence check is the root cause of VALID-01. |
| GitHub public-repo standard runners at 2 vCPU | **4 vCPU / 16 GB for public repos** (private stayed 2/8) | Jan 2024 | Makes `taskset -c 0,1` a genuine pin rather than a no-op, and makes `check-in-container.sh:85-88`'s "runners are 2-core" comment stale for this repo. |

---

## Assumptions Log

| # | Claim | Section | Risk if wrong |
|---|---|---|---|
| A1 | The 2-CPU sequential job will complete in well under 45 minutes (estimated ~5–10 min from the measured 86 s `check.sh test` step, 63 s `Clippy`, 59 s `Format`, and 3–4 min unpinned `check.sh all`). | Timing budget | Low. If it overruns, the job times out and — being advisory — blocks nothing. Raise `timeout-minutes` on the first real run. This is measured by the phase's own PR. |
| A2 | `gh pr checks` exits non-zero when a *non-required* check fails. | `gh pr checks` §, D-05 | Low for this phase. Both live runs observed were green, so this was **not** measured. `46-REVIEWS.md` R-02 already records it as read from `--help`. If wrong, D-05's resolution is *stronger* than needed, not weaker. |
| A3 | The GitHub runner's cpuset always includes CPUs 0 and 1. | taskset § | Low — public-repo `ubuntu-24.04` is 4 vCPU [CITED: GitHub docs], and D-04's print step converts this from an assumption into per-run evidence. If a runner ever presents fewer, `taskset` narrows *silently* (measured) and only the print step reveals it. |

---

## Open Questions (RESOLVED)

All three were closed during planning. Each answer below names where it landed. Question 3 is
resolved as *deliberately deferred to phase close* — that is its answer, not an absence of one.

1. **Which D-03 mechanism does the operator prefer — the sourceable fragment (C) or the guarded
   `eval`-of-grep (B)?**
   - What we know: C is structurally stronger (no textual anchor to move) but relocates D-03's
     named definition site off `check-in-container.sh:89`. B keeps that line as the definition site
     and touches nothing local, at the cost of coupling `ci.yml` to a textual pattern.
   - What's unclear: whether the operator reads D-03's `:89` reference as binding on the *file* or
     only on the *principle*. CONTEXT lists the mechanism under Claude's Discretion, which suggests
     the principle.
   - Recommendation: **the planner picks C and states the relocation explicitly in the plan**; it
     falls inside the stated discretion and is the stronger mechanism. No checkpoint needed. Flag
     it in the plan text so a reviewer reconciling `:89` against the diff is not surprised.
   - **ANSWER: option C, the sourceable fragment `scripts/lib/ci-cpus.sh`.** Landed in `46-01-PLAN.md`
     Task 1, which creates the fragment and makes both `scripts/check-in-container.sh` and
     `.github/workflows/ci.yml` source it. The relocation off `check-in-container.sh:89` is stated
     explicitly in that task's `<reversibility rating="costly">` block, so a reviewer reconciling
     D-03's `:89` reference against the diff finds the reasoning in the plan rather than having to
     reconstruct it. `46-01-PLAN.md` Task 2 then guards the "exactly one definition site" invariant
     in-tree.

2. **Should `ci.yml`'s stale header comment (the classic-protection verification command, and the
   "three required checks" framing) be corrected in this phase?**
   - What we know: it is wrong today — it misses `Build + test in devcontainer` — and the phase
     edits that file anyway. `devcontainer.yml:12-14` already carries the correct dual-check
     instruction, so there is a template to copy.
   - What's unclear: nothing substantive; it is a scope-discipline question.
   - Recommendation: **yes, as a one-line comment fix inside the INFRA-01 plan.** It is in a file
     the phase already opens, it costs nothing, and leaving a known-wrong verification command in
     place in the very phase that discovered it is the "documenting a broken gate is not fixing it"
     failure CLAUDE.md calls out for Phase 44.
   - **ANSWER: yes.** Landed in `46-01-PLAN.md` Task 1, STEP 5, as a one-line comment correction
     inside a file the phase already opens. The corrected text follows `devcontainer.yml:12-14`'s
     dual-check instruction and the `rules/branches` `gh api` form, not the classic
     `branches/<branch>/protection` form.

3. **Does the operator want R-01 (the validator/consumer spelling asymmetry) filed as a 999.x
   backlog entry now that this phase has confirmed it live?**
   - What we know: CONTEXT's Deferred section says explicitly that whether to file it is the
     operator's call and **has not been made**.
   - Recommendation: **this phase does not decide it and does not file it.** Surface it once, at
     phase close, so the decision is made deliberately rather than by silence. Not a blocker.
   - **ANSWER: DEFERRED TO PHASE CLOSE — deliberately NOT decided here.** This is the resolution, not
     a gap: CONTEXT.md records the filing decision as the operator's and states it has not been made,
     so neither the researcher nor the planner may make it. The mechanism that keeps it from being
     decided by silence lives entirely in `46-02-PLAN.md` — its `<output>` SUMMARY requirement, its
     acceptance criterion, and `T-46-07`'s `accept` disposition, nine references in all. It also
     prohibits any claim that VALID-01 closes R-01. **Corrected:** an earlier draft of this answer
     pointed at `46-01-PLAN.md` §`<verification>`, which contains zero references to R-01 — that
     block is entirely INFRA-01. No 999.x entry is created by this phase.

---

## Sources

### Primary (HIGH confidence — measured in this session)

- `git version 2.55.0` scratch repos: `rev-parse --verify` / `show-ref --verify` /
  `check-ref-format` exit-code matrices, both in a generic repo and in a faithful replica of
  `base_branch_fixture`. Negative controls in both directions.
- `docker run --rm mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`: `taskset` presence,
  `nproc` 4 vs 2, partial-invalid narrowing (exit 0), fully-invalid failure (exit 1), grandchild
  inheritance.
- D-03 mechanism harness: three extraction mechanisms × three fixtures (real / value-changed /
  anchor-moved).
- `gh api repos/denniyahh/devflow/rulesets/{19616771,19616766}`,
  `gh api repos/denniyahh/devflow/branches/{main,develop}/protection`,
  `gh api repos/denniyahh/devflow/rules/branches/develop`.
- `gh pr checks 201` vs `gh pr checks 201 --required` (10 rows vs 8, both exit 0).
- `gh api repos/denniyahh/devflow/actions/jobs/<id>` step timings for run `33686614066`.
- Source files read in full or by exact line range this session:
  `.github/workflows/ci.yml`, `.github/workflows/devcontainer.yml`, `scripts/check.sh`,
  `scripts/check-in-container.sh:1-45,70-142`, `scripts/assert-image-parity.sh`,
  `scripts/hooks/pre-push:205-221`, `scripts/hooks/pre-commit:56-73`, `scripts/hooks/post-commit:69-102`,
  `crates/devflow-cli/src/commands.rs:130-180,318-345,5414-5495,5555-5575`,
  `crates/devflow-cli/src/main.rs:170-200,248-272,336-362,690-760`,
  `crates/devflow-cli/tests/ci_parity_guards.rs` (all 297 lines),
  `crates/devflow-cli/tests/stop_e2e.rs:308-345`,
  `crates/devflow-core/src/git.rs:40-110`,
  `.planning/user/DEV-SETUP-CHECKLIST.md:122-140`, `OPERATIONS.md:35-50`, `Cargo.toml:29`.

### Secondary (MEDIUM confidence — official documentation)

- GitHub Docs, GitHub-hosted runners — public-repo `ubuntu-24.04` = 4 vCPU / 16 GB / 14 GB;
  private = 2 vCPU / 8 GB.
  https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- `gh pr checks --help` — `--required  Only show checks that are required` (also cited by
  `46-REVIEWS.md` R-02).

### Upstream phase artifacts (authoritative for decisions, not re-derived)

- `.planning/phases/46-ci-load-shape-and-operator-input-validation/46-CONTEXT.md`
- `.planning/phases/46-ci-load-shape-and-operator-input-validation/46-REVIEWS.md`
- `.planning/REQUIREMENTS.md` (INFRA-01 `:116-124`, VALID-01 `:39-49`, VALID-02 `:50-56`)
- `.planning/STATE.md`, `CLAUDE.md`

### Tertiary (LOW confidence)

None. Every claim above is either measured this session, cited to official documentation, or
tagged `[ASSUMED]` in the Assumptions Log.

---

## Metadata

**Confidence breakdown:**

| Area | Level | Reason |
|---|---|---|
| Standard stack | HIGH | No new dependencies; every tool's presence and version measured directly. |
| Architecture / mechanisms (D-03, D-07/D-08, D-11/D-12) | HIGH | Each mechanism measured with a negative control that had to produce the opposite result, and each did. |
| INFRA-01 reframing (devcontainer.yml already sequential; four required checks) | HIGH | Read from the workflow file and confirmed against the live ruleset API. |
| VALID-01 fixture trap (`develop~1`) | HIGH | Fixture replicated from `commands.rs:5426-5448` and the spelling measured directly. |
| Pitfalls | HIGH for the measured ones; the CLAUDE.md-derived ones (C-1..C-9) are the repo's own recorded findings. |
| Timing budget | MEDIUM | Real per-job times measured; the 2-CPU sequential figure is an estimate (A1). |
| `gh pr checks` failing-case exit code | LOW | Not reproduced (A2); does not block. |

**Research date:** 2026-09-04
**Valid until:** 2026-10-04 for the git and clap findings (stable plumbing / stable major
version). **Re-verify the ruleset contents before relying on them** — they are live service config
outside git and can change without a commit (`gh api repos/denniyahh/devflow/rules/branches/develop`).
