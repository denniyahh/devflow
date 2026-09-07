# Phase 46: CI Load Shape and Operator Input Validation - Context

**Gathered:** 2026-09-03
**Revised:** 2026-09-04 — six findings from `46-REVIEWS.md` (external adversarial review, codex +
agy lanes) folded in. Every revision below is traceable to an R-nn in that document.
**Status:** Ready for planning. Both decisions the review left open were ruled by the operator on
2026-09-04 and are recorded as **DECIDED** at D-02 and D-05. No open decisions remain.

<domain>
## Phase Boundary

One CI workflow addition plus two CLI input-surface corrections. No pipeline behaviour change,
no source change to the run loop.

1. **INFRA-01** — CI gains a job that runs the local pre-push gate's exact command shape:
   `scripts/check.sh all` (sequential `fmt` → `clippy` → `test`) inside the pinned devcontainer
   image, CPU-constrained to two cores. The three existing parallel jobs stay and still pass —
   this is an addition, not a re-shape.
2. **VALID-01** — a configured `base_branch` naming a computed revision rather than a branch is
   refused, with the existing negative controls preserved.
3. **VALID-02** — `devflow stop` accepts a positional project root the way `start`, `resume` and
   `status` do.

**Not in scope:** claiming the new CI job catches the 999.47 `/proc` fork-inheritance race;
converging `evidence`/`sweep` onto the positional form; any change to branch protection beyond
not adding this job to it; **closing the validator/consumer ref-spelling asymmetry** (R-01) — the
validator qualifies `refs/heads/{base}` while the fork consumer forwards the raw `{base}`, which
is a devflow-core change, pre-existing, and explicitly not claimed here (see D-07 and Deferred).
</domain>

<decisions>
## Implementation Decisions

### CI load shape (INFRA-01)

- **D-01:** The new job runs **inside the pinned devcontainer image**
  (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`), carrying the `safe.directory` step
  that all three existing jobs carry and the `scripts/assert-image-parity.sh` step that **only the
  `Test` job** carries (`.github/workflows/ci.yml:51-52`).

  **Corrected (R-05):** an earlier draft of this decision said all three existing jobs run
  image-parity. They do not — `Clippy` and `Format` carry only `safe.directory`. A planner told to
  "mirror the three existing jobs" would have been following a description that does not match the
  file. The new job should mirror `Test`, which is the job whose step set it actually wants.

  999.47 reproduces reliably only inside the pinned container; a bare runner would differ from
  both the existing jobs and the local gate this job exists to mirror.

  **CONFIRMED 2026-09-04 against a fact D-01 was originally decided without.**
  `.github/workflows/devcontainer.yml:52` **already runs `scripts/check.sh all` sequentially**, and
  its job `Build + test in devcontainer` is **already required on both `develop` and `main`** —
  verified against the repository rulesets (`develop-merge-or-squash`, `main-squash-only`), each
  requiring 4 contexts: `Test`, `Clippy`, `Format`, `Build + test in devcontainer`. So the
  sequential shape is not the gap; the **2-CPU pin** is (`devcontainer.yml` has no `taskset`, no
  `DEVFLOW_CI_CPUS` — both counted 0).

  This was put to the operator as a genuine fork, because it reopens D-01 and D-05:

  - **Chosen:** add the **new advisory job** to `ci.yml` as D-01/D-05 already specify. The cost is
    a duplicated full-suite run (`devcontainer.yml` currently takes ~3m10s wall-clock, stable
    across 6/6 recent successful runs, unpinned on 4 vCPU; a 2-CPU pin will be slower — **not
    measured**). D-05 already frames the new job as a trial to be "promoted if it earns it", and
    promotion now has a concrete meaning: fold the pin into the required `devcontainer.yml` job and
    delete the advisory one. The duplication is the price of the trial period, deliberately paid.
  - **Rejected:** pinning the existing `devcontainer.yml` job directly — no duplication, but it is
    required on both branches, so the most flake-prone configuration would become a merge blocker
    on day one, reversing D-05's entire rationale.
  - **Rejected:** pinning it *and* un-requiring it — that silently drops a sequential-suite merge
    guarantee that has gated both branches since July, and this phase's scope fence excludes
    "any change to branch protection beyond not adding this job to it".

  **Planner: do not re-litigate this.** `devcontainer.yml` is not the target file. Do not modify it.

- **D-02:** The load shape is pinned by **CPU constraint, not by runner size**. The intended
  shape is the local gate's: `taskset -c 0,1` over a sequential `check.sh all`. This makes the
  runner's actual core count irrelevant to correctness.

  **Context that motivated this:** `denniyahh/devflow` is a public repository, and GitHub's free
  standard runner for public repos is larger than 2 vCPU. The roadmap's success criterion says
  "on a 2-core runner" — that premise is **not established**; no job has ever printed `nproc`.
  Pinning with `taskset` makes the criterion true by construction instead of by assumption.

  **DECIDED (2026-09-04) — the roadmap criterion was amended to match this decision (R-04, raised
  independently by both review lanes).** Criterion 1 said "on a 2-core runner"; D-02 delivers 2-CPU **affinity** on
  a runner of unspecified size. Measured directly in the pinned image: `which taskset` =
  `/usr/bin/taskset`, `nproc` = **4**, `taskset -c 0,1 nproc` = **2** — the same container
  answering two different ways is the measurement's own negative control, and it establishes that
  the mechanism works. What is wrong is only the criterion's text: an auditor reading it literally
  would have failed the phase for not allocating a 2-core runner.

  `ROADMAP.md` criterion 1 now reads "pinned to two CPUs — `taskset -c 0,1`, the same constraint
  the local pre-push gate applies", and carries an explicit **Not claimed:** clause for runner
  allocation, with the 4-vs-2 measurement inline. `roadmap.analyze` still resolves phase 46 after
  the edit, so the milestone window is intact. **Bypass declared:** `gsd-tools phase` exposes only
  `add`/`add-batch`/`insert`/`remove`/`complete`/`list-plans` — there is no verb for editing an
  existing phase's success criteria, so this was a hand-edit under the repo's third legitimate
  bypass reason (no GSD command covers it).

- **D-03:** The `0,1` default is **sourced from `scripts/check-in-container.sh:89`**
  (`CPUS="${DEVFLOW_CI_CPUS:-0,1}"`) rather than re-typed into `ci.yml`, so the local gate and CI
  cannot drift apart. — **Reversibility:** costly — the whole point is a single definition site;
  reintroducing a literal in `ci.yml` re-opens the drift this decision closes.

  **Constraint the planner must solve, not assume:** `check-in-container.sh` drives `docker run`
  and therefore **cannot be invoked from inside a GitHub container job**. The decision is the
  intent (one definition site); the mechanism is open — e.g. parse the default out of the script
  without executing it, or have the job read it via a small shell extraction step. Do not
  interpret D-03 as "call `check-in-container.sh` from CI"; that path does not work.

- **D-04:** The job **records the shape it actually got** — print the CPU pin in effect and
  `nproc` as an early step — so the load shape is evidence in the run log rather than an
  assumption in the YAML.

- **D-05:** The job is **advisory, expressed by omission from branch protection's required
  checks**. Nothing in-tree marks it advisory. Explicitly rejected: `continue-on-error: true`,
  which would make the job report SUCCESS on a real suite failure and poison the `gh pr checks`
  reading this repository's own rules require.

  Rationale for advisory-first: it is a 2-CPU sequential run of the entire suite — the slowest
  job by a wide margin, and its timing sensitivity is the entire point, so it is also the job
  most likely to flake. It is watched across this milestone, then promoted if it earns it.

  **DECIDED (2026-09-04) — omission from required checks makes the job advisory to GitHub's merge
  button only (R-02).** `CLAUDE.md:64-65` separately requires asserting on a bare `gh pr checks <PR>`, and that
  command lists **all** checks by default; `--required` exists precisely to narrow it
  (`gh pr checks --help`: "Only show checks that are required"). So the moment this deliberately
  timing-sensitive job flakes, an agent following this repository's own rule reads the PR as
  not-green and stops — the job is advisory to GitHub and blocking to DevFlow's agents, the exact
  inversion of what D-05 intends.

  **Resolution — option 1.** Routine green-ness assertions on this repo use
  `gh pr checks <PR> --required`, and `CLAUDE.md`'s rule is amended to say so. This preserves the
  rule's actual target: it exists to stop `gh run list` branch-history proxying, and `--required`
  still asserts against the PR's current `HEAD_SHA`, so nothing the rule protects is lost. The
  rejected alternative was making the job required, which would have reversed advisory-first.

  **The `CLAUDE.md` edit is NOT a task for any plan in this phase.** `CLAUDE.md` is untracked on
  `develop` and the pre-commit hook refuses it on a `feature/*` branch ("refusing to commit
  personal/agent artifacts"), so it cannot be committed from this phase's worktree at all. It was
  applied out-of-band on `workspace/denniyahh` on 2026-09-04. A planner that writes a task to edit
  `CLAUDE.md` will produce a task that cannot commit.

  **Not executed:** that `gh pr checks` exits non-zero specifically on a failing *non-required*
  check was read from `gh pr checks --help`, not reproduced against a live failing PR.

- **D-06:** **Acceptance evidence is this phase's own PR.** The job must appear green in
  `gh pr checks <PR>` against that PR's current `HEAD_SHA` before the phase closes. No throwaway
  PR. This is the repository's standing rule (a branch `gh run list` does not establish PR check
  status) applied to the phase that adds the job.

  **Acceptance reads the unfiltered listing, never `--required`.** This is the inverse of D-05's
  open question and does not depend on how it resolves: D-05 keeps the job out of required checks,
  so `gh pr checks --required` would not list this job at all and would report a green PR without
  ever having looked at the thing being accepted. Assert on this job's own row in the full
  listing, against the PR's current `HEAD_SHA`.

### base_branch validation (VALID-01)

- **D-07:** Validation switches from `git rev-parse --verify --quiet refs/heads/{base}` to
  **`git show-ref --verify refs/heads/{base}`**.

  **Measured, with negative controls in both directions** (scratch repo, 2026-09-03):

  | spelling | `rev-parse --verify` (today) | `show-ref --verify` |
  |---|---|---|
  | `develop` (real local branch) | accept | accept |
  | `develop~1` | **accept — the live bug** | reject |
  | `develop@{0}` | **accept — the live bug** | reject |
  | `develop^{}` | **accept — the live bug** | reject |
  | `refs/heads/develop~1` | reject | reject |
  | `nonexistent-xyz` | reject | reject |

  **The bug is narrower than the filing suggests.** `ensure_base_is_a_local_branch`
  (`commands.rs:147`) already prefixes `refs/heads/`, so `refs/heads/develop~1` is *already*
  refused — it becomes `refs/heads/refs/heads/develop~1`. The live hole is revision **suffix**
  syntax that survives the prefix. Any fix that only targets the `refs/heads/` spelling closes
  nothing that is currently open.

  **`git check-ref-format` was considered and rejected as the *existence* check**: it is
  syntax-only and accepts `nonexistent-xyz`, so it cannot replace `show-ref`. That rejection stands
  for D-07 and **does not extend to D-08** — `check-ref-format` is exactly the right *classifier*
  for the two messages, and D-08 now requires it. A character denylist was also rejected —
  `show-ref` needs no such list and cannot mis-reject a legitimate branch name.

  **What D-07 does not close (R-01).** Validation qualifies `refs/heads/{base}`, but the consumer
  forwards the **raw** `{base}` to `git worktree add` as a start point — the repo's own comments
  say so (`crates/devflow-core/src/worktree.rs:64-83`; `parallel.rs` passes `base` through
  directly). Any value that is simultaneously a valid literal ref name *and* a revision expression
  passes validation and then forks from something else. Demonstrated: `git branch '@'` succeeds,
  `git show-ref --verify refs/heads/@` exits 0, and raw `@` resolves to `HEAD` rather than to the
  branch named `@`. This is **pre-existing** — today's `rev-parse` path prefixes `refs/heads/` too
  — and switching to `show-ref` does not narrow it. Closing it requires the validator and the
  consumer to agree on one spelling, not a stricter validator. That is a devflow-core change
  outside this phase's boundary; see Deferred. **Do not let a plan or a SUMMARY claim VALID-01
  closes this class.**

- **D-08:** The refusal emits **two distinct messages**. A syntactically valid but missing branch
  keeps today's message and its actionable advice (`git branch {base} origin/{base}`). A value
  carrying revision syntax gets its own message identifying the suffix as the problem — today's
  advice is actively misleading there, since it instructs the operator to create a branch
  literally named `develop@{0}`.

  **The discriminator is `git check-ref-format`, not `show-ref` (R-03).** Measured in a scratch
  repo, `git show-ref --verify --quiet refs/heads/<x>` exits **1 identically** for `develop~1`,
  `develop@{0}` and `nonexistent-xyz`, and 0 only for a real branch — a boolean with no way to
  tell malformed from merely missing. A direct D-07 substitution therefore cannot produce two
  messages; it silently degrades to one generic message and violates D-08 without failing. The
  implementation needs **both** calls: `show-ref` decides accept/reject, and on rejection
  `check-ref-format` decides which of the two messages to emit. D-10's test must exercise both
  refusal arms, not just "it was refused".

- **D-09:** The caller-side scoping stays as it is — the check remains applied only to a
  non-`Default` base, per the existing doc comment at `commands.rs:140-146`. Applying it
  unconditionally would convert `phase_reachability_on_base`'s `Undeterminable` fall-open into a
  hard refusal and regress every existing project.

- **D-10:** The test extends the existing
  `ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch`
  (`commands.rs:5459`), which already carries its own negative control, rather than adding a
  parallel test. The existing accept-arms (`workspace/example`, `develop`) must still pass —
  they are what stop the new assertions from passing against a helper that returns `Err`
  unconditionally.

### `devflow stop` project root (VALID-02)

- **D-11:** `stop` gains a **positional `[PROJECT]` with `default_value = "."`**, matching
  `resume`/`status` (`main.rs:182-183`, `257-258`) exactly.

- **D-12:** **`--root` is kept**, and **takes precedence over the positional when supplied**.
  No error on the both-supplied case; the flag simply wins.

  This reverses an earlier direction in the same discussion (remove `--root`, hard break). Kept
  because: `evidence` (`main.rs:370`) and `sweep` (`main.rs:467`) also take `--root`, so removing
  it from `stop` alone trades one inconsistency for another; and **9** in-repo `stop --root` call
  sites (`stop_e2e.rs` ×8; `reap_strays_e2e.rs:163` ×1), plus one doc-comment mention
  (`reap_strays_e2e.rs:4`) and `OPERATIONS.md:43`, would need migrating for
  no user-visible gain. **Result: this phase carries no breaking change and needs no
  `BREAKING CHANGE` CHANGELOG entry.** — **Reversibility:** reversible — additive only.

  **Corrected (R-06), then re-counted.** An earlier draft said 11 call sites (`×10` + 1); it had
  been asserted without being counted. The count is **9**, and getting there needs care a bare
  `grep -c` does not give: `grep -c` counts *lines*, not occurrences; `reap_strays_e2e.rs` returns
  2 lines of which one is a `//!` doc comment, not a call; and `crates/devflow-cli/tests/` holds 12
  `--root` lines in total, the extra 2 being `gate sweep --root` in `gate_sweep_e2e.rs:177,217` —
  a different subcommand that no `stop` migration would touch. The decision is unaffected either
  way (`--root` is kept, so nothing migrates); the number is recorded correctly so a later reader
  does not re-derive it from the same ambiguous grep.

- **D-13:** A genuinely wrong root must still fail, and must **name the offending argument**
  rather than emit a bare usage error. That failure mode is the actual content of VALID-02:
  during the #200 reproduction, `devflow stop --phase 7 .` read as "stop did not work" when it
  meant "stop rejected my input" — the wrong signal from a recovery verb reached for when
  something is already going wrong. `project_root` (`main.rs:718`) already produces a
  path-naming error; the requirement is that the positional reaches it instead of dying in clap.

### Claude's Discretion

- Exact `ci.yml` job name, `timeout-minutes`, and step ordering, consistent with the existing
  three jobs.
- Exact wording of the two `CliError` messages in D-08, consistent with `commands.rs` conventions.
- The extraction mechanism for D-03's shared `0,1` default, subject to D-03's stated constraint.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements and milestone framing
- `.planning/REQUIREMENTS.md` — INFRA-01, VALID-01, VALID-02 entries, each with its
  re-verification note and implementation note
- `.planning/ROADMAP.md` § "Phase 46: CI Load Shape and Operator Input Validation" — the three
  success criteria, including their explicit **Not claimed** clauses
- `CLAUDE.md` — the `gh run list` vs `gh pr checks` rule that D-06 applies, and the `:64-65`
  wording D-05's open question may require amending
- `.planning/phases/46-ci-load-shape-and-operator-input-validation/46-REVIEWS.md` — the external
  review this context was revised against; read it before disputing any decision above

### CI load shape (INFRA-01)
- `.github/workflows/ci.yml` — the three existing parallel jobs (`test:31`, `clippy:55`,
  `fmt:67`), the pinned container block, the `safe.directory` and image-parity steps to mirror
- `scripts/check.sh` — the `all` target (`case` at :56, `all` at :57) that already does
  `fmt` → `clippy` → `test` sequentially
- `scripts/check-in-container.sh` — `CPUS="${DEVFLOW_CI_CPUS:-0,1}"` at :89 (D-03's single
  definition site), the `taskset` construction at :93, the `docker run` at :133 that makes it
  uncallable from a container job
- `scripts/hooks/pre-push` — :220, the invocation the new CI job mirrors
- `scripts/assert-image-parity.sh` — the parity assertion the new job reuses
- `.devcontainer/devcontainer.json` — the single source of truth for the image tag

### base_branch validation (VALID-01)
- `crates/devflow-cli/src/commands.rs:140-171` — `ensure_base_is_a_local_branch`, including the
  doc comment explaining the caller-side scoping D-09 preserves
- `crates/devflow-cli/src/commands.rs:5459-5495` — the test D-10 extends, and its negative control
- `crates/devflow-cli/src/commands.rs:332` — the single call site
- `crates/devflow-core/src/worktree.rs:64-83` — the fork consumer that forwards the **raw** base,
  the asymmetry R-01 identifies and D-07 explicitly does not close

### `devflow stop` root (VALID-02)
- `crates/devflow-cli/src/main.rs:345-353` — the `Stop` variant as it stands
- `crates/devflow-cli/src/main.rs:181-183`, `:257-258` — `Resume`/`Status` positional shape to match
- `crates/devflow-cli/src/main.rs:700-703` — the dispatch arm
- `crates/devflow-cli/src/main.rs:718-738` — `project_root`, which walks **up** to the nearest
  `.devflow` ancestor
- `crates/devflow-cli/tests/stop_e2e.rs`, `crates/devflow-cli/tests/reap_strays_e2e.rs:163` — the
  9 `--root` call sites that D-12 leaves working
- `OPERATIONS.md:43` — the documented `stop` row; update if the positional is added to the table
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/check.sh all` — the sequential shape already exists as a target. The new CI job's
  body is effectively one line; no new script is needed.
- `scripts/assert-image-parity.sh` — reused verbatim from the `test` job.
- The `ensure_base_is_a_local_branch` test's negative control block — already written, already
  correct; D-10 extends rather than duplicates it.

### Established Patterns
- Every CI job in this repo runs inside the pinned image and asserts image parity before doing
  work. The new job follows that shape.
- Project-root arguments are positional with `default_value = "."` on `start`, `resume`,
  `status`, `ship` and `approve`; `--root` is the minority form used by `stop`, `evidence` and
  `sweep`.
- `project_root` resolves by walking **up** to the nearest `.devflow` directory, which is why the
  default `.` already works from inside a phase worktree — the argument is rarely needed in
  practice, and VALID-02 is about consistency and error legibility, not new capability.

### Integration Points
- `ci.yml` — new job alongside `test`/`clippy`/`fmt`; the `concurrency` group at :27 already
  covers it.
- `commands.rs:147` — single function change; one call site at :332.
- `main.rs:345` + `:700` — CLI variant and its dispatch arm.
</code_context>

<specifics>
## Specific Ideas

- **The explanation that drove D-01/D-02.** CI and the local gate already share the pinned image;
  what differs is the *load shape*. The local gate runs `taskset -c 0,1 scripts/check.sh all` —
  sequential, one process tree, squeezed onto two cores. CI runs three separate jobs, each with a
  whole runner and no ordering between them. 999.47 is a timing-sensitive `/proc`
  fork-inheritance race, and three roomy parallel jobs is the one configuration guaranteed not to
  reproduce it. `check-in-container.sh:87-88` says as much in its own comment: the wide setting
  "hides races that CI sees".

- **What this phase does NOT claim.** That the new job catches 999.47. CI has rejected 0 of the
  pushes the local gate rejected 2 of 2. The claim is that the load shape now exists in CI —
  capability, not a catch.

- **Two decisions were reversed mid-discussion** and the later answer stands: `--root` is kept
  with precedence (D-12, reversing an earlier "remove it"), and no follow-up backlog entry is
  filed for `evidence`/`sweep` (reversing an earlier "file the rest").

- **This context has been through one external adversarial review** (`46-REVIEWS.md`, 2026-09-04):
  two independent non-Claude lanes, six findings, all six verified against source by the
  orchestrator before being recorded here. Two were plain factual errors in this document (R-05,
  R-06) and are corrected in place. Two changed what the implementation must do (R-01, R-03). Two
  needed an operator ruling and got one on 2026-09-04 (R-02 → `gh pr checks --required` plus a
  `CLAUDE.md` amendment; R-04 → ROADMAP criterion 1 amended), both marked **DECIDED** above. The
  review's own complementarity note is worth carrying forward: the lane with far fewer citations
  found the critical issue the other missed.
</specifics>

<deferred>
## Deferred Ideas

- **Converging `evidence` and `sweep` onto a positional project root.** Raised, considered, and
  explicitly dropped by operator decision — not filed as a backlog entry, deliberately. Recorded
  here only so a future reader knows it was weighed rather than missed.
- **Promoting the new CI job to a required check.** Out of scope for this phase by D-05; revisit
  after the milestone has watched it across real pushes.
- **Measuring whether the new job actually catches 999.47.** Not answerable by adding the job;
  needs field evidence over time.
- **Closing the validator/consumer ref-spelling asymmetry (R-01).** `ensure_base_is_a_local_branch`
  validates `refs/heads/{base}` while `worktree::add` forks from the raw `{base}`, so a base that
  is both a literal ref name and a revision expression (`@` is the demonstrated case) can be
  accepted and then fork from the current checkout. Pre-existing, confirmed by a live
  reproduction, and untouched by D-07. **DECIDED 2026-09-05: filed as GitHub issue
  denniyahh/devflow#207, not widened into VALID-01.** The operator ruled it non-urgent — `base_branch`
  is operator-configured rather than attacker-supplied, the only known trigger is a branch literally
  named `@` with `base_branch` set to it, and the failure is visible and recoverable. Phase 46's
  boundary is unchanged: 46-02's prohibition against claiming VALID-01 closes this class still
  stands, and the phase-close surfacing requirement in 46-02's `<output>` can now cite #207 rather
  than re-raising an undecided question.
</deferred>

---

*Phase: 46-CI Load Shape and Operator Input Validation*
*Context gathered: 2026-09-03*
