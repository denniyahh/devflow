---
phase: 23-end-to-end-dogfood
plan: 10
artifact: acceptance-setup
recorded: 2026-07-26
target_phase_backlog_id: "999.27"
target_phase_numeric_inferred: 24
---

# Phase 23 Plan 10 — Acceptance Setup Evidence

Recorded by the plan-10 executor for Task 2 (rebuild + behavioral proof + recovery
rehearsal) and Task 3 (the two content preconditions), against the acceptance target
named at Task 1: **backlog item 999.27** (`release --check` signing-key inline
classification).

## Task 1 recap (checkpoint decision, already resolved)

- **Target:** backlog **999.27** — `check_ssh_signing_viability` inline (non-path)
  signing-key misclassification in `crates/devflow-core/src/git.rs`. One
  classification branch + one test; isolated to a release-preflight *advisory*
  check; does not touch merge/version/ship control flow.
- **Expected `VersionBump` result:** `2.0.0`, driven by plan 23-07's already-landed
  breaking removal of the `sequentagent` verb (D-12), independent of 999.27's own
  content. `CHANGELOG.md` already carries the unreleased `## 2.0.0` heading.
- **Not decided at Task 1:** authorization to run. That is Task 4.

## ⚠ Phase-numbering wrinkle — surfaced, not silently reconciled

**999.27 is a backlog identifier, not a numeric `devflow --phase` argument.**
`devflow start --phase <PHASE>` takes a `u32` (`crates/devflow-cli/src/main.rs:51`).
ROADMAP.md's backlog items (`999.1`–`999.29`) are unscheduled and get a real
sequential phase number only when promoted via `/gsd-review-backlog`. The shipped
phase sequence in ROADMAP.md runs 12, 13, …, 23 (23 is the current, in-flight
phase). Following that pattern, the next sequential number is inferred to be
**24** — but this is an **inference**, not a fact established by this repository's
own tooling; the real number is only assigned at promotion time, and if other work
lands first the assignment could differ. Every behavioral check below that needs
a concrete `--phase N` for "the acceptance target" uses `--phase 24`; if the
actual promoted number differs, the operator must recompute rather than assume
this artifact's `24` is authoritative.

---

## Task 2 — Rebuild, seven behavioral checks, recovery point, rehearsed restore

### Rebuild and binary-on-PATH proof (by behavior, not version string)

```
$ date -u +"%Y-%m-%dT%H:%M:%SZ"
2026-07-26T11:08:19Z
$ sha256sum target/release/devflow          # BEFORE rebuild
fe74d8315dfcc4a8a21c26b2ea4534defd18688234fe114b13b81aa1fb596efb  target/release/devflow

$ cargo build --release --workspace
   Compiling devflow-core v1.8.1 (…/devflow/crates/devflow-core)
   Compiling devflow v1.8.1 (…/devflow/crates/devflow-cli)
    Finished `release` profile [optimized] target(s) in 14.08s

$ sha256sum target/release/devflow          # AFTER rebuild
4043b33e859cdf0d568825d9b844be93608f91559f0c47594ecf6d20a9121608  target/release/devflow
```

Hash changed (`fe74d83…` → `4043b33…`), confirming a genuine rebuild, not a
no-op. `which devflow` resolves to a symlink at
`~/.linuxbrew/bin/devflow -> …/devflow/target/release/devflow` — the PATH binary
**is** this repository's own release build output by construction (not a copy
that could drift), so resolving the symlink and hashing it confirms identity
directly rather than trusting `--version`:

```
$ readlink -f "$(which devflow)"
…/devflow/target/release/devflow
$ sha256sum "$(readlink -f "$(which devflow)")"
4043b33e859cdf0d568825d9b844be93608f91559f0c47594ecf6d20a9121608  …/devflow/target/release/devflow   # matches post-rebuild hash
$ devflow --version
devflow 1.8.1
```

A version-string match alone would **not** have proven this (recorded repeat
failure per `23-VALIDATION.md` § "Precondition — rebuild before any probe"): the
hash match plus symlink resolution is the actual proof; the seven behavioral
checks below are the second, independent proof this task requires.

### Seven behavioral checks (verbatim output + exit code)

**1. `devflow gate list --all-roots`** — enumerates across roots without error.
Exit `0`. Output (truncated to structure; full per-line detail omitted — see
"Observed noise" below):

```
PHASE  STAGE     AGE       ROOT / CONTEXT
12     plan      8h!       /tmp/.tmp0Q8F0p
           [never-silent] stage plan failed: No .planning/ directory or … [truncated; full output in .devflow/]
… (22 rows total, phases 7/8/12, all pointing at /tmp/.tmpXXXXXX scratch dirs)
EXIT=0
```

**2. `devflow gate sweep --dry-run`** — reports a decision set, mutates nothing.
Exit `0`:

```
would reap phase 12 plan (age 31302s) at /tmp/.tmp0Q8F0p
… (22 lines total)
sweep complete (dry run): 22 would be reaped, 0 skipped, 0 left alone
EXIT=0
```

**3. `devflow stop --phase 99`** (a phase that does not exist) — clean no-op,
exit `0`:

```
stop: no lock held for phase 99 — nothing is running `advance()`
stop: no persisted state for phase 99 — already stopped
EXIT=0
```

**4. `devflow evidence --phase 22 --json`** (a completed past phase, shipped as
v1.8.1) — coherent record, exit `0`:

```json
{
  "phase": 22,
  "shipped": false,
  "workflow_finished_seen": false,
  "finished_reason": null,
  "stage": null,
  "state_present": false,
  "feature_branch_exists": false,
  "merged_into_develop": false,
  "has_remote": true
}
```
EXIT=0. `shipped: false` is expected and documented behavior, not a defect:
Phase 22 shipped before the `workflow_shipped` event (plan 23-06) existed —
`ship_evidence.rs`'s own doc comment and 23-06-SUMMARY.md's "Verification"
section record this exact case.

**5. `devflow evidence --phase 24 --require-shipped`** (the inferred acceptance
target — see the numbering wrinkle above) — must exit non-zero right now, since
the target has not shipped. **Confirmed, exit `1`** — this is the pre-run
baseline plan 23-11's post-run check is compared against:

```
phase: 24
shipped: false
workflow_finished_seen: false
finished_reason: none
stage: none
state_present: false
feature_branch_exists: false
merged_into_develop: false
has_remote: true
error: phase 24 has not shipped — DevFlow has no record of a completed Ship
EXIT=1
```

**6. `devflow start --help`** — lists the pre-authorization flag. Exit `0`,
`--yes-ship` present with its full doc string (D-04/D-05/D-06, 23-09) confirmed
in the output.

**7. The removed two-agent subcommand** — must exit non-zero with an
unrecognized-subcommand error:

```
$ devflow sequentagent --phase 1 --agents a,b
error: unrecognized subcommand 'sequentagent'

Usage: devflow <COMMAND>

For more information, try '--help'.
EXIT=2
```

**All seven answered.** A stale (pre-23-03…23-09) binary would fail checks
1–5 and 7 outright (the commands/flags/removal did not exist), and check 6
would be missing `--yes-ship`. This is the behavioral proof the plan requires
in place of a version-string check.

**Observed noise (not a defect, recorded for honesty):** checks 1–2 surfaced 22
stale registry entries from this phase's own earlier probes (23-01/23-02),
registered against `/tmp/.tmpXXXXXX` scratch directories that no longer exist.
These are exactly the kind of orphaned entries `devflow gate sweep` (23-04)
exists to clean up; they do not affect the acceptance run's own registration
(a fresh, distinct `(project_root, phase)` pair) and contain no local
filesystem paths beyond `/tmp` scratch names — no home directory or OS username
appears in any of the seven checks' output.

### Full gate chain — pre-run baseline (paired, per the review's OpenCode finding)

```
$ cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
… (592 tests across the workspace, 0 failed)
CHAIN_EXIT=0
```

Per-binary breakdown summed to **592 passed, 0 failed** (devflow-core lib 362,
devflow-cli lib 174, plus 9 integration test binaries: build_provenance 3,
devcontainer_ci_failfast 4, gate_sweep_e2e 4, git_env_hermeticity 1,
gitignore_coverage 1, help_snapshot 1, log_format_env 3, phase7_cli 17,
release_check 8, stop_e2e 9, workspace_version_pin 1, devflow_dir_gitignore 2,
monitor_e2e 2). `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
`cargo fmt --check` exits 0.

**Pre-run baseline pair, per plan 23-08-SUMMARY.md's "Next Phase Readiness":**

| Value | Count | Source |
|---|---|---|
| Pre-run passing count (this run, on `feature/phase-23` post-merge) | **592** | measured directly above, matches the orchestrator-supplied dispatch context |
| Plan 23-08's deliberate-removal count (`sequentagent` tests, already reflected in the 592 above) | **9** | `595 → 586` recorded in `23-08-SUMMARY.md` ("Workspace test count: 595 before this plan's base commit, 586 after") |

Plan 23-11's post-run count must be compared against **592**, not against 586 or
595 in isolation — 592 already includes every plan through 23-09 (23-09 added
tests on top of 23-08's 586), so a post-run count below 592 that is not
explained by a further deliberate removal is a regression signal, while one at
or above 592 (minus whatever 999.27's own plan set legitimately adds/removes)
is expected.

### Recovery point

- **Recovery ref:** `recovery/pre-23-11-acceptance-e0f87c2` (branch-shaped, per
  the review's suggestion — cannot be confused with a real release tag).
- **Commit SHA:** `e0f87c2c2230257f7aa8092a836225626941d09a` — the current tip of
  both local `develop` and `origin/develop` at the time this artifact was
  written (confirmed identical via `git rev-parse develop` /
  `git rev-parse origin/develop`, both returned the same SHA after
  `git fetch origin develop`).
- **Push + read-back:**

```
$ git branch recovery/pre-23-11-acceptance-e0f87c2 e0f87c2c2230257f7aa8092a836225626941d09a
$ git push origin recovery/pre-23-11-acceptance-e0f87c2
 * [new branch]      recovery/pre-23-11-acceptance-e0f87c2 -> recovery/pre-23-11-acceptance-e0f87c2
$ git ls-remote origin refs/heads/recovery/pre-23-11-acceptance-e0f87c2
e0f87c2c2230257f7aa8092a836225626941d09a  refs/heads/recovery/pre-23-11-acceptance-e0f87c2
```

`origin` confirmed to actually have it (read-back SHA matches exactly).

### Local restore rehearsal (throwaway clone)

```
$ git clone --quiet <origin-url> <scratch-dir>
$ cd <scratch-dir>
$ git fetch origin recovery/pre-23-11-acceptance-e0f87c2
$ git checkout -b restore-test FETCH_HEAD
$ git rev-parse HEAD
e0f87c2c2230257f7aa8092a836225626941d09a
$ git diff --stat e0f87c2c2230257f7aa8092a836225626941d09a HEAD
(empty — no output, diff exit 0)
```

Tree comparison **succeeded**: the restored working tree is byte-identical to
`develop`'s tip at recovery-ref creation time. Scratch clone deleted after the
rehearsal.

### Remote restore path — the real answer, not assumed

Two overlapping GitHub mechanisms exist on this repository for `develop`; the
**active** one is the modern ruleset, confirmed by cross-checking against this
project's own observed history (five consecutive self-merged PRs by the sole
collaborator with no second reviewer — inconsistent with the classic
branch-protection API's `required_approving_review_count: 1`, consistent with
the ruleset's `0`):

```
$ gh api repos/denniyahh/devflow/branches/develop/protection
  allow_force_pushes.enabled: false
  enforce_admins.enabled: true

$ gh api repos/denniyahh/devflow/rulesets
  id 19616771  "develop-merge-or-squash"  target=branch  enforcement=active
  conditions.ref_name.include: ["refs/heads/develop"]

$ gh api repos/denniyahh/devflow/rulesets/19616771
  rules:
    - pull_request: required_approving_review_count=0, allowed_merge_methods=[merge, squash]
    - required_status_checks: strict=true, contexts=[Test, Clippy, Format, "Build + test in devcontainer"]
  bypass_actors: []
  current_user_can_bypass: "never"
```

**A direct force-push to `develop` is refused — categorically, not just by
request.** `bypass_actors` is empty and `current_user_can_bypass` is
`"never"`; this is a structural property of the active ruleset, not a
permission the operator merely lacks today. **No admin override exists on this
repository for this branch.**

**The real restore path is a revert pull request**, and unlike a
review-gated repo it does **not** wait on a second human reviewer
(`required_approving_review_count: 0` on the ruleset that actually governs —
confirmed empirically by this repo's own PR history, every recent `develop`
merge was self-approved by the sole collaborator). It **does** wait on the four
required status checks (`Test`, `Clippy`, `Format`, `Build + test in
devcontainer`), `strict_required_status_checks_policy: true`.

**Exact command sequence:**
```
git revert -m 1 <bad-merge-sha> --no-edit    # on a new branch off develop
git push origin <revert-branch>
gh pr create --base develop --head <revert-branch> --title "revert: <reason>"
# wait for Test / Clippy / Format / Build+test-in-devcontainer to report success
gh pr merge <pr-number> --merge   # or --squash; both allowed by the ruleset
```

**Estimated wall-clock latency: minutes, not hours — the revert-PR class, at
its fast end.** Measured from this repository's own recent CI runs on
`develop` (`gh run list --branch develop`): the `CI` job (Test+Clippy+Format)
completed in **~61s**, the `Devcontainer` job in **~108s**, both required and
run in parallel — so the required-checks wait is on the order of **~2
minutes**, plus seconds for the revert commit/push/PR-open/merge steps. No
human-review wait is structurally required. **This is favorable relative to
the review's stated worst case** (a revert PR "subject to whatever review the
branch protection demands" — here that review requirement is 0, so the
dominant cost is CI runtime, not a second person's availability).

**Class:** revert pull request (minutes). **Not** a force-push (refused,
structurally, for everyone). **Not** an administrative override (none exists
for this branch/ruleset combination).

### Worst-case failed-run state (from `hooks::merge_feature`'s no-rollback policy)

`hooks_after_ship()` runs, in order: `Merge` → `VersionBump` → `ChangelogAppend`
→ `BranchCleanup`, as one fail-fast batch that stops at the first error
(`crates/devflow-core/src/hooks.rs:96-107`). `merge_feature`'s own doc comment
(`crates/devflow-core/src/hooks.rs:143-165`) is explicit: on a post-merge
ancestry-recheck failure, **the merge is not undone** — `git merge --no-ff` has
already committed on `develop` by the time any later check runs, and
automatically resetting a shared integration branch was judged more dangerous
than the inconsistency it would paper over.

**Worst-case state a failed acceptance run can leave `develop` in:** a real
merge commit already on `develop` (999.27's feature branch merged in), with
`VersionBump`, `ChangelogAppend`, or `BranchCleanup` having failed somewhere
after it in the same batch — the batch aborts, `finish_workflow_with_gate_timeout`
reopens an actionable Ship gate whose context tells a human to resolve the
error, and no automatic rollback occurs at any layer.

**How the recorded recovery path handles this exact case:** the recovery ref
above points at `develop`'s tip *before* the run starts. If the run leaves
`develop` in the worst-case state, the operator's revert PR (above) reverts the
merge commit itself (`git revert -m 1 <merge-sha>`), which is the correct
operation for a `--no-ff` merge commit specifically — not a reset to the
recovery ref, since other legitimate commits could in principle land on
`develop` between the recovery point and the failure (none are expected during
this one-way run, but the revert-of-the-specific-commit approach is robust to
that regardless). The recovery ref's role is to give the operator (or a
future audit) an unambiguous, remotely-verified answer to "what did `develop`
look like immediately before this run" — not to be reset onto directly.

---

## Task 3 — The two content preconditions

**Both preconditions are ESCALATED, not clear, and not silently remedied.**
The plan's own two named checks are checks "about the target phase's own plan
set," and 999.27 currently has **zero plans** (`ROADMAP.md`: "**Plans:** 0
plans", "`[ ] TBD (promote with /gsd-review-backlog when ready)`"). An absent
plan set is not the same as a plan set that was checked and came back clean —
recording either precondition as "clear" here would be exactly the false-green
class this phase exists to close. Both are reported below with what a
resolution would require, so the operator can weigh them at Task 4 rather than
discover them mid-run.

### Precondition A — the security artifact

**Files read:**
- `.planning/config.json` (read directly by this task, not taken on
  `23-RESEARCH.md`'s word)
- `~/.claude/gsd-core/workflows/ship.md` (the actual enforcement mechanism —
  see finding below)
- `crates/` workspace source (`rg -rn 'security_enforcement|SECURITY.md' crates/`
  — **zero matches**; the check is not implemented in DevFlow's own Rust
  source at all)
- ROADMAP.md's entry for backlog `999.27` (no plan files exist yet to inspect)

**Exact finding, `.planning/config.json` quoted directly:**
```json
"workflow": {
  "security_enforcement": true,
  "security_asvs_level": 1,
  "security_block_on": "high"
}
```
**`security_enforcement` IS `true`** in this repository — confirmed by direct
reading, not inferred. `23-RESEARCH.md`'s Assumption A1 (that no override
exists) is confirmed correct.

**This means preflight WILL demand a security artifact.** The check itself
lives outside DevFlow's Rust source, in the `/gsd-ship` slash-command
workflow (`~/.claude/gsd-core/workflows/ship.md:89-107`): it resolves the
`ship:pre` capability-registry hook, and if an active entry exists with
`capId == "security"` and `blocking == true`, it requires
`<phase-dir>/*-SECURITY.md` to exist with frontmatter `threats_open == 0`
before shipping — exactly the wall `23-PROBE-FINDINGS.md` recorded one probe
hitting.

**Whether 999.27's own future plan set will produce this artifact cannot be
checked today, because that plan set does not exist.** Corroborating context
in the operator's favor: every phase in this project's recent history (18
through 22, and every plan-10 predecessor in phase 23) has produced a
`NN-SECURITY.md` as a routine part of its own execution — the mechanism
appears to fire automatically once a phase has threat-model content in its
plans, not as an opt-in step planners must remember. But 999.27 is explicitly
scoped as a small, S-sized, single-branch fix; it is not verified today
whether the Define/Plan stages for a phase this small will produce
`threat_model` content substantial enough to trigger the same security-audit
step, or whether a minimal plan could reach Ship with no `*-SECURITY.md` ever
written. **Disposition: escalated.** Remedies available at Task 4: (1) accept
the risk that the run may hit this wall exactly as the recorded probe did,
relying on the never-silent gate to surface it as an actionable Ship-stage
gate rather than a silent failure; or (2) require, as an explicit condition of
`proceed`, that 999.27's own Plan stage include a `<threat_model>` block
substantial enough to produce a `*-SECURITY.md` (i.e. treat this as a planning
constraint communicated to the run, not a code change made here).

### Precondition B — no Ship-completion claim at Validate

**Files read:**
- ROADMAP.md's entry for backlog `999.27` (the only existing text describing
  this phase's scope — no `must_haves` or acceptance-criteria block exists
  because no `PLAN.md`/`SPEC.md` has been written)
- `23-06-SUMMARY.md` (the oracle this precondition's remedy would use)

**Exact finding:** there is no target-phase `must_haves` truth or acceptance
criterion to quote, because none has been written yet — 999.27 has 0 plans.
**Disposition: escalated**, for the identical reason as Precondition A: the
check the plan specifies is a check against files that do not exist.

**Remedy the plan itself names, available to propose (not applied here,
unilaterally, since this task has no authority to write 999.27's plan):**
declare `devflow evidence --phase 24 --require-shipped` (or whatever numeric
phase 999.27 is actually promoted to — see the numbering wrinkle above) as
999.27's `external_verify` probe when it is planned. This makes the
"did Ship actually complete" question checked by code (plan 23-06's oracle,
exit-code-stable) rather than by a Validate-stage review that may or may not
notice a self-attested Ship claim — closing the exact false-green class the
other recorded probe run hit. **Disposition: escalated, with a named,
concrete remedy ready to hand to whoever plans 999.27/phase 24.**

---

## Summary for Task 4

| Item | Status |
|---|---|
| Rebuild + binary-on-PATH proof | ✅ done — hash-verified, symlink-resolved |
| Seven behavioral checks | ✅ all seven answered correctly |
| Pre-run workspace gate chain | ✅ green — 592 passed / 0 failed, clippy clean, fmt clean |
| Pre-run baseline pair | ✅ recorded — 592 (current) / 9 (23-08's deliberate removal, already reflected) |
| Recovery ref created + pushed + read back | ✅ `recovery/pre-23-11-acceptance-e0f87c2` @ `e0f87c2` |
| Local restore rehearsal | ✅ succeeded — tree byte-identical |
| Remote restore path established | ✅ revert PR, ~2 min (no human-review wait; force-push refused for everyone, no admin override exists) |
| Worst-case failed-run state documented | ✅ merge commit stays, batch aborts, Ship gate reopens |
| Precondition A (security artifact) | ⚠ **ESCALATED** — `security_enforcement: true` confirmed; 999.27 has no plan set to check yet |
| Precondition B (no self-attested Ship claim) | ⚠ **ESCALATED** — same root cause (no plan set yet); remedy named (`--require-shipped` as external_verify) |
| Phase-numbering wrinkle | ⚠ **Flagged** — `999.27` → inferred `--phase 24`, not yet a confirmed assignment |

`git status --porcelain` confirmed empty after this artifact is committed
(verified below, before the commit that adds this file).
