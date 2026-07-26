---
phase: 23-end-to-end-dogfood
plan: 14
artifact: acceptance-setup-2
recorded: 2026-07-26T21:28:11Z
supersedes: 23-ACCEPTANCE-SETUP.md (evidence there predates PR #32's merge; nothing from it is carried forward here without independent re-measurement)
---

# Phase 23 Plan 14 — Acceptance Retry Setup (Round 2)

Re-establishes every precondition for the acceptance retry against the
post-merge tree (guard 23-12/23-13 now on `develop`), cuts a fresh recovery
point, and records the operator's launch authorization. Nothing below is
copied from `23-ACCEPTANCE-SETUP.md` — every check is re-run against the
binary and tree state measured at the top of this task.

## Task 0 — Precondition check

23-13's checkpoint returned a merge confirmation (`0dad20d`), and
`23-GUARD-SHIP-RECORD.md` Task 3 records the freshly built binary's hash
(`b5db079a…6dc98`) matching the PATH-resolved `devflow`. Re-confirmed directly
in this task (see "Tree state" below) before any check ran. Precondition met.

## Carry-forward finding, adjudicated (not deferred again)

`23-GUARD-SHIP-RECORD.md` Finding 1 recorded, and left unfixed by design, that
local `develop` was 0 ahead / 120 behind `origin/develop`, and that
`commands.rs:146` calls `ensure_phase_reachable_on_base(project_root, phase,
DEVELOP)` where `DEVELOP` is the literal string `"develop"` — the **local**
branch, not `origin/develop`. That finding stated explicitly: *"Re-measuring
launch preconditions immediately before the acceptance attempt is 23-14's
job."* This is that re-measurement, and this is the adjudication.

**Re-measured, before touching anything:**

```
$ git fetch origin
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse develop
e0f87c2c2230257f7aa8092a836225626941d09a
$ git rev-list --left-right --count develop...origin/develop
0	120
$ git merge-base --is-ancestor develop origin/develop; echo "exit: $?"
exit: 0
```

Confirmed unchanged from 23-13's record: local `develop` is a strict ancestor
of `origin/develop`, 0 ahead / 120 behind — a pure fast-forward gap, not a
divergence.

**Consequence, re-confirmed on the ROADMAP heading before any fix:**

```
$ git show develop:.planning/ROADMAP.md | rg -c '^### Phase 24:'
(no match — 0)
$ git show origin/develop:.planning/ROADMAP.md | rg -c '^### Phase 24:'
1
```

Local `develop` genuinely lacks phase 24's heading; `origin/develop` has it.
Since `commands.rs:146` reads the **local** ref at launch, `devflow start
--phase 24` run right now, unmodified, would consult the ref that lacks the
heading and refuse — exactly the false-green risk 23-13 flagged and declined
to fix in place, deferring the decision here.

**Decision: fast-forward local `develop` to `origin/develop`, applied in this
task, not deferred further.** Rationale: this is a pure fast-forward (0
ahead / 120 behind, strict ancestor, confirmed twice above) — no divergent
local commits exist to lose, no merge, no rebase, no force-push. `23-15` has
no step that re-checks or repairs this condition; deferring again would
reproduce the exact gap that sank the first acceptance attempt (`23-FINDINGS.md`
§B1), just moved one plan later. This is a mechanical correctness fix
(Rule 1/2 class — the guard's own base-branch precondition would otherwise be
false-refused), not an architectural change: it makes the local ref agree
with the remote ref it is already a strict ancestor of.

**Applied, without switching the working branch:**

```
$ git rev-parse --abbrev-ref HEAD
feature/phase-23
$ git fetch origin develop:develop
From https://github.com/denniyahh/devflow
   e0f87c2..0dad20d  develop    -> develop
$ git rev-parse develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse --abbrev-ref HEAD
feature/phase-23
$ git status --porcelain
(empty)
```

`develop` was updated via a non-checkout fetch refspec (`git fetch origin
develop:develop`), which git only permits when the update is itself a
fast-forward — an independent, structural confirmation of the "pure
fast-forward" claim above, not just this task's own arithmetic. `HEAD`
remained on `feature/phase-23` throughout; the working tree was never
touched, and `git status --porcelain` is empty before and after.

**Re-confirmed after the fast-forward — both refs now agree:**

```
$ git show develop:.planning/ROADMAP.md | rg '^### Phase 24:'
### Phase 24: `release --check` Signing-Key Inline Classification
$ git ls-tree -r --name-only develop -- .planning/phases/ | rg '^\.planning/phases/24-'
.planning/phases/24-release-check-signing-key-inline-classification/.gitkeep
$ [ "$(git rev-parse develop)" = "$(git rev-parse origin/develop)" ] && echo EQUAL
EQUAL
```

**This tree-state change is named again in Task 3's checkpoint** as part of
what the operator is authorizing — it is not a silent side effect of this
plan.

---

## Task 1 — Tree state at measurement time

```
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse HEAD
3b1c3ede3afc4a179c358a736aedc2c09b6fffcc
$ git rev-parse --abbrev-ref HEAD
feature/phase-23
$ git status --porcelain
(empty)
```

Every reachability check below is resolved against `origin/develop` (now
identical to local `develop` after the fast-forward above) — not the working
branch `feature/phase-23`.

**Binary freshness, re-proven independently of 23-13's own proof:**

```
$ sha256sum ./target/release/devflow
b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  ./target/release/devflow
$ command -v devflow
<homebrew-prefix>/bin/devflow
$ sha256sum "$(command -v devflow)"
b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  <homebrew-prefix>/bin/devflow
$ devflow --version
devflow 1.8.1
$ git diff origin/develop HEAD -- crates/ Cargo.toml Cargo.lock | wc -l
0
```

The PATH-resolved binary is byte-identical to the freshly built one, and this
repository's working tree is diff-empty against `origin/develop` across every
path that affects the compiled binary. This is the same binary `23-GUARD-SHIP-RECORD.md`
Task 3 measured (`b5db079a…6dc98` both times) — no rebuild was needed because
no code changed between 23-13 and this task, only the local `develop` ref
moved.

## The seven behavioural checks (verbatim output + exit code)

**1. `devflow gate list --all-roots`** — Exit `0`:

```
PHASE  STAGE     AGE       ROOT / CONTEXT
12     code      1h!       /tmp/.tmp8YPpPz
           [never-silent] stage code failed: Project root /tmp/.tmp8YPpP… [truncated; full output in .devflow/]
12     plan      6h!       /tmp/.tmpNuFaCh
           [never-silent] stage plan failed: Cannot plan phase 12: proje… [truncated; full output in .devflow/]
12     code      6h!       /tmp/.tmpSAHPzj
           [never-silent] stage code failed: Phase 12 cannot execute: pr… [truncated; full output in .devflow/]
12     code      4h!       /tmp/.tmpqZmoON
           [never-silent] stage code failed: Phase 12 has no plans to ex… [truncated; full output in .devflow/]
EXIT=0
```

Enumerated without error across roots. **Known-finding class:** these four
rows are `23-FINDINGS.md` §A3 noise — leftover registry entries from this
project's own e2e test suites (`gate_sweep_e2e.rs`/`stop_e2e.rs`, which spawn
real `devflow advance` children and do not reap them), pointing at
now-nonexistent `/tmp/.tmpXXXXXX` scratch roots. None of the four rows are
duplicates of each other (three distinct roots register under `code`, one
under `plan`) — this run does **not** reproduce §A2's exact duplicate-entry
shape, only the general "stale registry noise from test suites" class of §A1/A3.
No home directory or OS username appears in this output.

**2. `devflow gate sweep --dry-run`** — Exit `0`, mutates nothing:

```
would reap phase 12 plan (age 22534s) at /tmp/.tmpNuFaCh
would reap phase 12 code (age 22818s) at /tmp/.tmpSAHPzj
sweep complete (dry run): 2 would be reaped, 0 skipped, 2 left alone
EXIT=0
```

2 of the 4 registered entries exceed the default reap-age threshold and would
be reaped; the other 2 ("left alone") are younger. This is the expected shape
of §A1/A3 noise (registry entries a test suite leaked, not evidence of a new
defect) — reported as a decision set, no mutation performed (dry run).

**3. `devflow stop --phase 99`** (nonexistent phase) — clean no-op, Exit `0`:

```
stop: no lock held for phase 99 — nothing is running `advance()`
stop: no persisted state for phase 99 — already stopped
EXIT=0
```

**4. `devflow evidence --phase 22 --json`** — coherent record, Exit `0`:

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

`shipped: false` is expected and documented, not a defect: phase 22 shipped
before the `workflow_shipped` event (plan 23-06) existed.

**5. `devflow evidence --phase 24 --require-shipped`** — **must exit
non-zero. Confirmed, Exit `1`.** This is the pre-run baseline 23-15's
post-run check will be compared against — labelled here explicitly, before
launch:

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

**PRE-RUN BASELINE FOR 23-15: `devflow evidence --phase 24 --require-shipped` exits `1` (non-zero) as of `2026-07-26T21:28:11Z`, against `origin/develop` SHA `0dad20d`.**

**6. `devflow start --help`** — Exit `0`, `--yes-ship` present with its full
doc string:

```
Options:
      --phase <PHASE>  Phase number to work on
      --agent <AGENT>  Agent to launch [default: claude]
      --mode <MODE>    Pipeline mode: `auto` runs to Ship unattended; `supervise` gates at Validate
      --force          Overwrite the feature branch if it already exists
      --no-worktree    Run the agent directly in the primary checkout instead of an isolated worktree (not recommended for unattended runs)
      --dry-run        Print the pipeline that would run without launching anything
      --until <UNTIL>  Run the pipeline through `<stage>` and halt cleanly before advancing further (e.g. `--until plan` runs Define+Plan then stops before Code). `ship` is rejected — the pipeline already stops there
      --yes-ship       Pre-authorize the Ship gate so this run can reach a completed Ship stage unattended (D-04/D-05/D-06, 23-09). The Ship gate still fires and is still answered through the normal gate protocol — this only supplies the approval automatically, attributed to `--yes-ship` in the gate ledger. Must be typed on every invocation: it cannot be set in `devflow.toml` or any environment variable (D-05), so an unattended auto-merge can never become a standing, silent default
  -h, --help           Print help
EXIT=0
```

**7. The removed two-agent subcommand** — Exit `2` (non-zero), unrecognized
subcommand:

```
$ devflow sequentagent --phase 1 --agents a,b
error: unrecognized subcommand 'sequentagent'

Usage: devflow <COMMAND>

For more information, try '--help'.
EXIT=2
```

**All seven answered against the post-merge binary and tree**, none carried
forward from `23-ACCEPTANCE-SETUP.md`.

## The eighth check — the guard's own subject, read-only

```
$ git show origin/develop:.planning/ROADMAP.md | rg '^### Phase 24:'
### Phase 24: `release --check` Signing-Key Inline Classification
$ git ls-tree -r --name-only origin/develop -- .planning/phases/ | rg '^\.planning/phases/24-'
.planning/phases/24-release-check-signing-key-inline-classification/.gitkeep
```

Both halves confirmed on `origin/develop` — the ROADMAP heading line and the
phase-directory path are quoted verbatim above. **This same condition is now
also enforced structurally by the shipped 23f guard at launch time**
(`ensure_phase_reachable_on_base`, merged via PR #32, ancestor-confirmed of
`origin/develop` in `23-GUARD-SHIP-RECORD.md` Task 2/3). Two independent
checks now exist — this human read-only check, and the guard's own runtime
check — and the guard's is the one that will actually hold on some future
attempt when nobody remembers to run the human one by hand.

## The two content preconditions, re-decided

### Precondition A — the security artifact

**Re-read, not copied forward.** `.planning/config.json` (read directly,
quoted below) no longer contains a top-level `workflow.security_enforcement`
key at all — this is a genuine change in mechanism description from
`23-ACCEPTANCE-SETUP.md`, which quoted that key as present and `true`. `git
log --oneline -- .planning/config.json` shows the file's last two changes are
unrelated reviewer-list edits (`b7ed0f3`, `817b8ff`), both predating this
phase; the file's `workflow` object has held its current shape
(`granularity`, `auto_mode`, `commit_docs`, `subagent_timeout`,
`_auto_chain_active`, `nyquist_validation`, `ai_integration_phase`) since
before Phase 23 began:

```json
{
  "commit_docs": true,
  "workflow": {
    "granularity": "medium",
    "auto_mode": true,
    "commit_docs": true,
    "subagent_timeout": 300000,
    "_auto_chain_active": false,
    "nyquist_validation": true,
    "ai_integration_phase": true
  },
  ...
}
```

**The actual enforcement mechanism does not read this key directly** —
`~/.claude/gsd-core/workflows/ship.md` states explicitly: *"Resolve active
`ship:pre` gate hooks from the capability registry — the registry evaluates
each hook's `when` condition, so do **not** read `workflow.security_enforcement`
directly."* Re-run against the actual resolution path instead of the raw
config file:

```
$ gsd_run loop render-hooks ship:pre --raw
{
  "point": "ship:pre",
  "activeHooks": [
    {
      "capId": "security",
      "kind": "gate",
      "when": "workflow.security_enforcement",
      "check": {
        "predicate": {
          "kind": "artifact-frontmatter-equals",
          "artifact": "SECURITY.md",
          "field": "threats_open",
          "equals": 0
        }
      },
      "blocking": true,
      "onError": "halt"
    }
  ],
  ...
}
EXIT=0
```

**Finding: the security `ship:pre` gate is active and blocking**
(`capId: "security"`, `blocking: true`) regardless of the raw config key's
absence — the capability registry resolves its own default for
`workflow.security_enforcement` independently of `.planning/config.json`'s
literal contents. **Practical conclusion is unchanged from the previous
attempt** (preflight will demand a `*-SECURITY.md` with `threats_open == 0`
before shipping), but the *mechanism* by which that conclusion is reached is
corrected here — this is exactly the "fresh disposition with its own reason"
this precondition requires, not an inherited "as before."

**Phase 24's own plan set is still empty** (`git show origin/develop:.planning/ROADMAP.md`,
quoted above: "**Plans:** 0 plans", "`[ ] TBD`"), so whether Phase 24's Plan
stage will actually produce a `<threat_model>` block substantial enough to
trigger `*-SECURITY.md` generation cannot be checked today, for the identical
reason as the previous attempt.

**Disposition, dated 2026-07-26: escalated — accepted unmitigated.** Same two
remedies as before are available and NOT directed here: (1) accept the risk,
relying on the never-silent gate to surface a Ship-stage block rather than a
silent failure if the artifact is missing when needed; or (2) require Phase
24's own Plan stage to produce a `<threat_model>` block as an explicit
condition of proceeding. Recorded here for Task 3 to weigh, not resolved by
this task.

### Precondition B — no self-attested Ship-completion claim

**Re-read:** Phase 24's ROADMAP entry (`origin/develop`) still shows **0
plans** and no `must_haves`/acceptance-criteria block — no `PLAN.md` exists
to inspect, identical in kind to the previous attempt's finding, re-confirmed
today rather than assumed to still hold.

**Disposition, dated 2026-07-26: escalated — accepted unmitigated**, for the
same underlying reason: the check the plan specifies (a check against the
target phase's own must-haves/acceptance criteria) is a check against files
that do not exist yet. **Remedy offered and NOT directed:** declare `devflow
evidence --phase 24 --require-shipped` as Phase 24's own `external_verify`
probe once it is planned, making "did Ship actually complete" a code-checked
question (exit-code-stable, plan 23-06's oracle) rather than a Validate-stage
self-attestation. Recorded as available-but-declined for Task 3, matching the
previous attempt's disposition.

## Full gate chain — pre-run baseline

Run as a direct `&&` status chain (not a piped/grep shape — the rejected
`cargo test … | rg 'FAILED' && exit 1 || …` form falls through its `||`
branch on a compile/link failure that prints no `test result: FAILED` line,
per `23-REVIEWS.md` finding 3):

```
$ cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
```

**Per-binary test result lines (verbatim, in the order they ran):**

```
test result: ok. 184 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (devflow-cli unittests, src/main.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/build_provenance.rs)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/ci_parity_guards.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/gate_sweep_e2e.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/git_env_hermeticity.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/gitignore_coverage.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/help_snapshot.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/log_format_env.rs)
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out    (tests/phase7_cli.rs)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/release_check.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/start_reachability_e2e.rs)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/stop_e2e.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/workspace_version_pin.rs)
test result: ok. 363 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (devflow-core unittests, src/lib.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/devflow_dir_gitignore.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (tests/monitor_e2e.rs)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out     (Doc-tests devflow_core)
```

**Total: 608 passed, 0 failed, across 17 binaries** — matches the orchestrator-
supplied dispatch context measured independently before this plan began.

```
$ cargo clippy --workspace --all-targets -- -D warnings
   Compiling devflow v1.8.1 (…/crates/devflow-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.61s
CLIPPY_EXIT=0

$ cargo fmt --check
FMT_EXIT=0
```

**CHAIN_EXIT=0.** Clippy re-checked only `devflow-cli` (devflow-core's clippy
pass was already cached clean from an earlier build in this same session with
no source changes in between — confirmed by `git diff origin/develop HEAD --
crates/` above returning 0 lines); it still exits 0 with `-D warnings`
promoting every lint to an error, so a clean exit is a genuine zero-warning
result, not a skipped check. This 608/0/clean pair is the pre-run baseline
23-15's own post-run chain is compared against; a post-run "still green"
claim with no pre-run pair on record would not be a delta.

## Worst-case failed-run state

Restated directly from `crates/devflow-core/src/hooks.rs` source (not from
memory or from the previous artifact's paragraph):

`hooks_after_ship()` (`hooks.rs:105-112`) runs, in order, as one fail-fast
batch: `Merge` → `VersionBump` → `ChangelogAppend` → `BranchCleanup`.
`merge_feature`'s own doc comment (`hooks.rs:156-165`) states the no-rollback
policy explicitly: on the post-merge ancestry re-check's failure path,
`merge_feature` does **not** undo the merge — `git merge --no-ff` has already
committed onto `develop` by the time the re-check runs, and "automatically
resetting a shared integration branch is a far more dangerous operation than
the inconsistency it would be papering over." Instead it returns `Err`; the
containing batch fails; `finish_workflow_with_gate_timeout` reopens an
actionable Ship gate whose context tells a human to resolve the git error;
the operator decides.

**Worst-case state a failed run can leave `develop` in:** a real `--no-ff`
merge commit already on `develop` (phase 24's feature branch merged in), with
`VersionBump`, `ChangelogAppend`, or `BranchCleanup` having failed somewhere
after it in the same batch — no automatic rollback at any layer, an
actionable gate open instead of a silent failure.

**How the recovery path (Task 2, below) handles this exact case:** the
recovery ref points at `develop`'s tip immediately before this run starts. If
`develop` ends up in the worst-case state above, the correct remedy is `git
revert -m 1 <merge-sha>` against the specific bad merge commit — not a reset
onto the recovery ref directly, since other legitimate commits could in
principle land on `develop` between the recovery point and the failure. The
recovery ref's role is to give an unambiguous, remotely-verified answer to
"what did `develop` look like immediately before this run," which the revert
path consumes as its base of comparison.

## Redaction check, Task 1 section

Redaction checklist: OS username, home-directory basename, absolute home
paths, temporary-directory paths, remote URLs. `<homebrew-prefix>` is used
above in place of this operator's actual PATH-resolved binary location
(`/home/linuxbrew/.linuxbrew`, a system-wide Linuxbrew prefix, not this
operator's own home directory, but redacted anyway for consistency with
`23-GUARD-SHIP-RECORD.md`'s established convention). The `/tmp/.tmpXXXXXX`
scratch-root names surfaced by checks 1 and 2 carry no home path and are left
as-is per the plan's explicit instruction.

```
$ rg -n '/home/denniyahh|/var/home/denniyahh' .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP-2.md
(no match)
```

Grep run against the file as committed, Task 1 section only, at the point
this check was run. `denniyahh` itself does appear once in this section
(the fast-forward record's `From https://github.com/denniyahh/devflow` line),
but the pattern checked against the actual 999.10 leak class — an operator's
*local filesystem path* (home directory, absolute path, tempdir) — returns no
match. Read narrowly against that leak class: this operator's OS username and
their public GitHub account name are the identical string
(`23-GUARD-SHIP-RECORD.md` confirmed this via `whoami`/`getent passwd` vs
`gh auth status`), this repository is public, and every prior committed
artifact in this phase (`23-GUARD-SHIP-RECORD.md`) already retains this exact
string in GitHub URLs unredacted. No `/home/denniyahh` or
`/var/home/denniyahh` absolute path appears anywhere in this section — only
the public repository URL, retained for the same reason
`23-GUARD-SHIP-RECORD.md` retained it.

---

## Task 2 — Fresh recovery point on `origin`, real restore path

### Cut at the right commit

Recovery ref must point at the `origin/develop` tip Task 1 recorded:
`0dad20d3e85d82d60235b8f91cb944e4cbed433c`. Named per the established
convention, scoped to this plan and carrying the short SHA it protects:
`recovery/pre-23-15-acceptance-0dad20d`.

```
$ git ls-remote origin 'refs/heads/recovery/pre-23-15-acceptance-*'
(no match — none exists yet)
```

### Push and read back — no local branch created

```
$ git push origin 0dad20d3e85d82d60235b8f91cb944e4cbed433c:refs/heads/recovery/pre-23-15-acceptance-0dad20d
remote: Create a pull request for 'recovery/pre-23-15-acceptance-0dad20d' on GitHub by visiting:
remote:      https://github.com/denniyahh/devflow/pull/new/recovery/pre-23-15-acceptance-0dad20d
To https://github.com/denniyahh/devflow.git
 * [new branch]      0dad20d3e85d82d60235b8f91cb944e4cbed433c -> recovery/pre-23-15-acceptance-0dad20d

$ git ls-remote origin refs/heads/recovery/pre-23-15-acceptance-0dad20d
0dad20d3e85d82d60235b8f91cb944e4cbed433c	refs/heads/recovery/pre-23-15-acceptance-0dad20d
```

**Read-back SHA (`0dad20d3e85d82d60235b8f91cb944e4cbed433c`) equals the
`git rev-parse origin/develop` value Task 1 recorded exactly.**

The push itself ran through this repository's tracked pre-push hook
(`scripts/check-in-container.sh all`, in the pinned devcontainer) — `cargo
fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace --no-fail-fast` all ran again inside the container
and reported `check.sh: all OK` before the push left the machine, an
independent (containerized) re-confirmation of the same 608/0/clean result
Task 1 measured natively.

```
$ git branch --list 'recovery/pre-23-15-acceptance-*'
(empty)
```

**No local copy exists — deliberate.** `devflow cleanup` judges branches
merged by ancestry, and a recovery ref is by definition an ancestor of the
branch it protects, so it is *always* eligible for deletion. `23-FINDINGS.md`
§B2 records this happening twice in one day to the previous recovery ref
(`recovery/pre-23-11-acceptance-e0f87c2`). This ref is deliberately
remote-only for that reason. The new ref name is distinct from
`recovery/pre-23-11-acceptance-e0f87c2` — that ref points at the pre-Phase-23
commit (`e0f87c2`), now 120 commits stale relative to `origin/develop`; it
protects nothing for this attempt and is left untouched per its own recorded
disposition (`23-FINDINGS.md` §B2a — retained until Phase 23's merge into
`develop` is settled, which it now is via PR #32, but pruning that ref is not
this task's job).

### Throwaway-clone restore rehearsal

Cloned this repository's local working copy into a scratch directory
(`git clone --no-hardlinks`, local filesystem source, no network, no remote
side effect):

```
$ git clone --no-hardlinks --quiet <project-root> <tmpdir>
$ cd <tmpdir>
$ git checkout develop
Switched to a new branch 'develop'
$ git rev-parse develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c

$ echo "bad-run-marker" > BAD_RUN_MARKER.txt
$ git add BAD_RUN_MARKER.txt
$ git commit -q -m "simulate bad run" --no-verify
$ git rev-parse develop
a6feb0c9583db008e7e45923f314f1133ac33315

$ git reset --hard 0dad20d3e85d82d60235b8f91cb944e4cbed433c
HEAD is now at 0dad20d Merge pull request #32 from denniyahh/feature/phase-23
$ git rev-parse develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c

$ git diff --stat 0dad20d3e85d82d60235b8f91cb944e4cbed433c develop
(empty — no output)
```

**Tree comparison succeeded: byte-identical.** `git rev-parse develop` after
the restore equals the recovery SHA exactly, and `git diff --stat` between
the recovery SHA and the restored `develop` produced no output. The clone
already contained this exact commit as `develop`'s own tip (the recovery ref
and `develop`'s current tip are the same SHA today), so the rehearsal proves
the mechanical reset operation reproduces the pre-run tree — the load-bearing
property the real restore path depends on. Clone removed at the end of the
task:

```
$ rm -rf <tmpdir>
$ test -d <tmpdir> && echo "STILL EXISTS" || echo "removed"
removed
```

### The real restore path — protected branch, PR required, CI wait

`develop` is governed by an active ruleset (re-confirmed today, unchanged
from `23-ACCEPTANCE-SETUP.md`'s prior measurement):

```
$ gh api repos/denniyahh/devflow/branches/develop/protection
allow_force_pushes.enabled: false
enforce_admins.enabled: true

$ gh api repos/denniyahh/devflow/rulesets/19616771
rules:
  - pull_request: required_approving_review_count=0, allowed_merge_methods=[merge, squash]
  - required_status_checks: strict=true, contexts=[Test, Clippy, Format, "Build + test in devcontainer"]
bypass_actors: []
current_user_can_bypass: "never"
```

**Force-push refused categorically** — `bypass_actors` empty,
`current_user_can_bypass: "never"`, no admin override exists for this branch.
The real restore is a **revert pull request**, not a force-push and not an
administrative bypass:

```
git revert -m 1 <bad-merge-sha> --no-edit    # on a new branch off develop
git push origin <revert-branch>
gh pr create --base develop --head <revert-branch> --title "revert: <reason>"
# wait for Test / Clippy / Format / "Build + test in devcontainer" to report success
gh pr merge <pr-number> --merge   # or --squash; both allowed by the ruleset
```

**Latency, measured from this repository's own recent CI runs on `develop`:**

```
$ gh run list --branch develop --limit 5
completed  success  Merge pull request #32 ...  CI            develop  push  1m40s  2026-07-26T21:02:27Z
completed  success  Merge pull request #32 ...  Devcontainer  develop  push  1m51s  2026-07-26T21:02:27Z
completed  success  Merge pull request #31 ...  CI            develop  push  1m37s  2026-07-26T19:30:17Z
completed  success  Merge pull request #31 ...  Devcontainer  develop  push  2m2s   2026-07-26T19:30:17Z
```

Both required jobs run in parallel and complete in **under 2 minutes**; no
human-review wait is structurally required (`required_approving_review_count:
0`). **Budget ~2 minutes of CI wait plus the seconds it takes to author and
push the revert commit and open/merge the PR** — this is the real cost of
undoing a bad run, stated here so it is budgeted rather than discovered at
02:00.

## Redaction check, Task 2 section

```
$ rg -n '/home/denniyahh|/var/home/denniyahh' .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP-2.md
(no match)
```

Grep run against the file as committed, after this task's additions. No
operator home-directory path appears anywhere in this section — the `gh api
repos/denniyahh/devflow/...` command lines and the `git push`/`git fetch`
transcripts' `github.com/denniyahh/devflow` URLs are the public repository's
own account name, not a local filesystem leak; see the Task 1 redaction
section above for the full interpretation and its citation of
`23-GUARD-SHIP-RECORD.md`'s identical precedent. The scratch clone's tempdir
path is redacted to `<tmpdir>` and the project root to `<project-root>`
throughout this section, per the 999.10 leak class this checklist actually
targets (local filesystem paths, not the public repo account name).

---

## Task 3 — Authorization

*(recorded by the continuation agent once the operator responds to the
checkpoint)*
