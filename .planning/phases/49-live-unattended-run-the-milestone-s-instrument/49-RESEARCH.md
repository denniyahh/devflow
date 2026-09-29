# Phase 49: Live Unattended Run — The Milestone's Instrument - Research

**Researched:** 2026-09-28 (regenerated for the 2026-09-28 re-scope; replaces the 2026-09-27 research in full)
**Domain:** Live end-to-end `devflow start --mode auto` run against a disposable GSD project: scaffold, supervision, evidence capture, judgement
**Confidence:** HIGH for DevFlow and GSD mechanics (read in source this session, several probed); MEDIUM for agent behaviour in the live run (not reproducible from source); LOW for the three items in the Assumptions Log

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Re-scope and execution model (Operator, 2026-09-28)

- **D-17 (Operator): The run targets a disposable GSD project, not this repository.** It is still a real
  live run: same binary, `--mode auto`, real agent, real GSD, real gates, real Ship. Only the target is
  disposable, which removes the production side effects (a public PR to `main`, a merge into
  `workspace/denniyahh`, the `v3.0.0` bump and tag) and every guard they required.
- **D-18 (Operator): Each attempt is one `checkpoint:human-action`, judged afterwards.** The operator starts
  the attempt and answers the gates DevFlow raises; Phase 49's own execution pauses once for this. An
  interrupted session re-asks the same question, which is harmless. No GSD plan tracks the run gate by gate.
- **D-19 (Operator principle): Operate within GSD's current confines.** No workaround that makes GSD behave
  differently: no `status: halted` used as a failure marker, no wrapper plans re-running summarized plans, no
  `<precondition>` used as a branch, no hand-built re-run idempotence, no SUMMARY written before a plan's last
  task. (Memory: operate within GSD's confines; `project-gsd-halt-has-no-exit`.)

#### Run payload (Claude, delegated, 2026-09-28)

- **D-20: A synthetic single-task phase in the disposable project**, as Phase 23a chose for the same purpose:
  "the probe is about the supervisor/pipeline mechanism, not about the content of the work"
  (`scripts/scratch-dogfood-repo.sh` header). The task is a small, deterministic code change with a test the
  Validate stage runs, so Code and Validate do real work and can pass. Build it by extending
  `scripts/scratch-dogfood-repo.sh`, keeping its blast-radius guards (repo created outside any checkout,
  absolute-path-prefix refusal, repo-local git identity only). Use the smallest toolchain already on the host;
  research confirms what the Validate stage runs in such a project.
- **D-03 (Operator, re-confirmed; payload adapted by delegation): Seed one `gate="blocking-human"`
  checkpoint** in the payload plan. It asks for a value with no answer derivable from the code (the analogue
  of the original "CI default budget value"), so a "Followed" result can show the comparison reasoning
  `47-PHASE49-OBSERVATION.md` requires.
  - *Verified:* in `Mode::Auto` a plan declaring `blocking-human` makes the unattended-launch condition
    `DoesNotHold` (`crates/devflow-cli/src/preflight.rs:1076-1080`); the check runs at Define and Code
    (`:971-972`). Auto-decide resume fires only for checkpoints in the set recorded when a human approves the
    Code preflight gate (`:1432-1434`). **No fully unattended run can reach DECN-03's path.**
  - **Rejected (unchanged):** pre-writing gate responses before launch; dropping the checkpoint.
- **D-04 (Operator, re-confirmed): Pre-author the payload plan** and commit it in the disposable project
  before launch, so the seeded checkpoint is guaranteed and planning variance stays out of the measurement.
  *Verified:* Define is a no-op in a DevFlow launch and Plan keeps an existing PLAN.md
  (`crates/devflow-core/src/prompt.rs:374-380`, `:341`).
- **D-21: The payload must not classify as a MAJOR bump.** DevFlow's MAJOR-bump gate opens at Ship preflight
  and **cannot be auto-approved** (`crates/devflow-cli/src/preflight.rs:724-760`: it takes no `yes_ship`
  handling). Research confirms what the classifier reads (the comment says the commit range); the scaffold
  and the payload plan make the bump MINOR or PATCH.

#### Gates (Claude, delegated, 2026-09-28)

- **D-22: Exactly two human gates per attempt: Define preflight and Code preflight.** They follow from D-03
  with D-04: at Define, "no plans yet" is deferred to Code (`preflight.rs:1068-1072`), but a pre-authored plan
  is already present, so Define gates too. The Define approval is a formality (it confirms the seeded
  checkpoint is intentional); the Code approval records the auto-decide set DECN-03 needs.
- **D-23: Launch with `--yes-ship`. This changes the operator's D-07 (human Ship gate).** `--yes-ship`
  auto-approves the Ship gate once, with attribution (`crates/devflow-cli/src/pipeline_outcomes.rs:839`),
  and Phase 23 introduced it for exactly this purpose. D-07's gate existed to protect the real repository's
  local merge, bump and tag; a throwaway repository has nothing to protect, and criterion 2's merge target is
  observed from the run's events (`merge_result`). D-07's verified finding stands: the Ship gate fires only
  after `/gsd-ship` has pushed and opened the PR (`pipeline_outcomes.rs:828-850`, `ship.md:190-202,370-374`).
- **Rejected:** zero gates (reverses D-03's reason, "DECN-03 would stay open past the milestone"); one gate
  through an agent-written plan (reverses D-04's reason).

#### Throwaway repository and topology (Claude, delegated, 2026-09-28)

- **D-24: A fresh private GitHub repository per attempt.** `/gsd-ship` needs a real remote: it pushes and runs
  `gh pr create` (`~/.claude/gsd-core/workflows/ship.md:194-199`, `:370`). A fresh repository per attempt
  leaves nothing to archive, abort or refuse between attempts. The operator creates it from the runbook under
  their own account (an outward-facing action stays operator-owned); research confirms the `gh` auth scope
  needed. — **Reversibility:** one-way — deleting a throwaway repository destroys its branch and PR history,
  so all evidence must be captured locally first and the operator performs the deletion after Phase 49
  verification.
- **D-06 (Operator, adapted by delegation): Keep the two-base split inside the disposable project.**
  `devflow.toml` sets a non-default `base_branch` (for example `integration`), which satisfies amended
  criterion 1; GSD's ship and PR target stays the repository's `main` (`origin/HEAD`). This mirrors DevFlow's
  production topology, and `validate_base_branch` refuses `main` as a base by design
  (`crates/devflow-core/src/config.rs:351`).
- **D-25: The negative-control arm stays a local, unconfigured scratch repository** with no remote: it refuses
  at preflight before anything could be pushed. D-14 applies unchanged.
- **D-26: Evidence committed to this repository is sanitized; the throwaway PR is private.** The evidence
  sanitizer design carries over; public-PR secret scanning and containment are no longer needed.

#### Failure handling (Operator D-10/D-11 carried; mechanism Claude, delegated, 2026-09-28)

- **D-10 (Operator, unchanged): Blocking defects are fixed in-phase and the run repeated; non-blocking ones go
  to the backlog as 999.x entries.**
- **D-11 (Operator, unchanged): Cap of 3 full live attempts.** Only a run that ends without reaching Ship
  counts; Validate loop-backs are data, not failures.
- **D-27: A stopped attempt is answered at the checkpoint, and GSD's normal resume does the rest.** The
  operator replies `stopped: <what happened>`; the checkpoint plan fails its gate and writes no SUMMARY; a
  plain `/gsd-execute-phase 49` re-presents the checkpoint for the next attempt (GSD resume,
  `execute-phase.md:1453-1457`). No halt marks, no wrapper plans.
- **D-28: The fix vehicle for a blocking DevFlow defect.** `/gsd-quick` for a contained fix; an inserted
  decimal phase (`/gsd-phase --insert 49 …`) when the fix needs a design decision or crosses crates. The
  operator confirms the vehicle at the stop. Fixes go through DevFlow's normal tests and pre-push gate on its
  own branches, and the binary is rebuilt before the next attempt (D-12). A cause inside the payload is fixed
  in the disposable project's pre-authored plan, not in DevFlow.

#### Carried unchanged (Claude, remit — from 2026-09-24)

- **D-12: Every attempt rebuilds, and both arms re-run on the new binary.** The binary's identity (git SHA
  plus checksum) is recorded with each arm.
- **D-14: The negative-control fixture must be built to fail, and asserted on text.**
  `scripts/scratch-dogfood-repo.sh` writes `.planning/config.json` (`:187-191`); the negative arm removes and
  commits the removal, asserts on the refusal text ("no `.planning/config.json` under the launch root"), and
  runs with `DEVFLOW_BASE_BRANCH` scrubbed. The positive arm asserts the opposite (config present, source
  `ConfigFile`).
- **D-15: Pin the agent** with `--agent claude` (DECN-03 closes for claude only,
  `pipeline_launch.rs:1777`); record the claude CLI version with each arm.
- **D-16: Fork-point measurement** — three values, now in the disposable project: the base ref's SHA just
  before `start`; the payload worktree's HEAD just after creation (must equal it); and `git merge-base` of the
  feature branch and the base at Ship.

#### Superseded by the 2026-09-28 re-scope

Full text in git history (`2b12d84:…/49-CONTEXT.md`).

- **D-01** (999.55 as the payload) → replaced by D-20. 999.55 stays in the backlog.
- **D-02** (payload numbered Phase 49.1) → moot: the payload phase lives in the disposable project.
- **D-05** (payload plan and ROADMAP heading on `workspace/denniyahh` before launch) → moot.
- **D-07** (human Ship gate, PR to `main` closed unmerged) → replaced by D-23 and D-24.
- **D-08** (observe, then undo the version bump, changelog and tag) → moot: they are observed and discarded
  with the throwaway repository.
- **D-09** (criterion 1 satisfied by `050ef7f`) → replaced by amended criterion 1 (`968b713`).
- **D-13** (this repository's main checkout clean and restored) → moot: the run never touches it; the
  disposable project's launch checkout is created fresh per attempt.

### Claude's Discretion

- **Evidence layout:** event log, persisted state per stage, session captures, gate ledger, binary identity —
  sufficient for `47-PHASE49-OBSERVATION.md`'s Followed / Not followed / Void classification, recording which
  turn carried which rule (`CODE_STAGE_POLICY` in turn 1, `checkpoint_auto_decide_prompt` in the resumed turn).
  The gate watcher's snapshot at each gate open should capture everything DECN-03 needs; research confirms,
  because a capture that must happen while the run is parked is otherwise a timed runbook step.
- **SURV-01 detection method.** Recorded as an observation either way; "no interleaving observed" must not be
  written up as confirming anything.
- **Runbook form and supervision.** Owned outside any agent worktree or subagent turn (memory: long probes live
  outside agent worktrees). It names the exact approve and reject commands and how the operator learns a gate
  is waiting (`DEVFLOW_GATE_NOTIFY_CMD` is a no-op unless set, `crates/devflow-core/src/gates.rs:388-394`; the
  production gate timeout defaults to 3 days).
- **Where the disposable project and the negative-control repository live on disk** (outside every checkout).
- **Reuse of the superseded tool designs:** launch helper, gate helper, gate watcher and snapshot, SURV watcher,
  evidence checker, negative-control driver and evidence sanitizer (in the 49-02…49-08 and 49-16 plans at
  `2b12d84`). Not needed: accommodations, the post-Ship undo, PR containment, the landing hop, and archive or
  abort tooling for reused state.
- **Gate logic lives in tested scripts** with selftests and negative controls, not in long one-line
  `bash -c` commands inside PLAN.md; adversarial review is scoped to the runbook and any irreversible step.

### Deferred Ideas (OUT OF SCOPE)

- **999.55: configurable test-wait budget for the `phase7_cli.rs` helpers.** No longer this run's payload.
  Stays in the backlog as its own future phase; measuring its effect on the flake rate needs CI history after
  its merge.
- **999.123: de-duplicate CI triggers.** Unchanged; stays in the backlog.
- **Making DevFlow merge phase work into `main`.** Unchanged; its own phase if ever wanted (D-06).
- **GSD support for re-running plans, task-level resume and loops.** Outside DevFlow: the operator is taking a
  proposal to the GSD maintainers (`~/Documents/gsd-rerunnable-plans-proposal.md`). Nothing here depends on it.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| VERIFY-01 | A real `devflow start --mode auto` run is recorded end to end against a configured base — fork point, `unattended_config_condition = Holds`, unattended progression, and the Ship merge target — with a negative control that an unconfigured repo still forks from `develop`. (REQUIREMENTS.md:107-116; closing sentence at `:116` is amended by this phase) | Q1 (Validate runs `/gsd-validate-phase`; payload + GSD config that let it pass), Q2 (classifier reads git tags + `--no-merges` commit messages), Q3 (`repo` scope), topology (what must exist at `start`, where Merge lands), criterion-by-criterion evidence sources (Pattern 4), negative-control text (D-14), DECN-03 capture (Pattern 5), SURV-01 detection (Pattern 6), GSD resume for a stopped attempt (D-27) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

From `./CLAUDE.md` (repository) and the operator's global instructions — the planner must verify compliance:

- **Crates:** `devflow-core` (lib), `devflow-cli` (bin `devflow`). Test commands: `cargo test -p devflow-core --lib`; `cargo test -p devflow --bin devflow` (the package is `devflow`, not `devflow-cli`; `-p devflow --lib` verifies nothing and fails); full suite `scripts/check.sh all`; container parity `scripts/check-in-container.sh`. Only relevant if a D-10/D-28 defect fix touches `crates/`.
- **Worktrees:** phase work runs in `.worktrees/phase-49` (this worktree, branch `feature/phase-49`). Never run concurrent git operations on the main checkout while an executor holds the tree.
- **DEV-SETUP-CHECKLIST:** when changing hooks, devcontainers or tooling dependencies, update `.planning/user/DEV-SETUP-CHECKLIST.md` in the same commit. A runbook that newly requires `gh` scope `repo`, `systemd-run`, `python3` or `shellcheck` on the operator host is a tooling dependency → check whether the checklist needs a line.
- **Shell:** bash for scripts (`set -euo pipefail`), quote every expansion, `bash -n` + `shellcheck` before use; never assume interactive features. Bound long-running commands with timeouts. GSD executors run their Bash tool under zsh (`scripts/lint-plan-bashisms.sh` header) — PLAN `<automated>` blocks that use bash-only syntax must be wrapped `bash -c '…'`; the pre-commit hook enforces this.
- **Python:** type hints on public functions, `pathlib`, f-strings, `ruff check --fix && ruff format`.
- **Git:** Conventional Commits; never `--no-verify`; repo-local identity only in fixtures (T-23-02); scrub `GIT_*` variables in fixtures (999.37 class).
- **Tests:** negative control in every measurement; assert refusals on text, not exit code; `rg -c` prints nothing on zero matches; `cargo test` with a bare name can match nothing and exit 0 (assert on `1 passed` and module-qualify).
- **Evidence:** RULE ZERO — every report states what the check does not establish; "no interleaving observed" confirms nothing (SURV-01).
- **Immutable Fedora host:** never suggest `dnf`. Prefer tools already on the host.
- **No invented constraints:** Claude-delegated decisions are not operator mandates (CONTEXT provenance labels).

## Summary

The re-scoped phase is a small build (scaffold extension + a handful of tested tools + a runbook), one operator-run live attempt behind a single `checkpoint:human-action`, and a judging plan. The research surfaced **five configuration traps in the disposable project that would each stop an attempt short of Ship on a correct DevFlow**, all verified by probing GSD with a bare `{"workflow": {}}` config and reading DevFlow source:

1. **GSD's security ship gate is on by default and blocks `/gsd-ship`** when no SECURITY.md exists (probe: `ship:pre` active hook `security/gate … blocking: true, onError: halt` under `{"workflow": {}}`; absent with `security_enforcement: false`). The scaffold's `.planning/config.json` must set `workflow.security_enforcement: false`.
2. **GSD `/gsd-ship` requires `verification.status == passed`** (ship.md:58-62) — the verifier must stay on (`verifier_enabled: true` probed) and the payload's must-haves must be fully automatable, or Ship fails and opens a third, never-silent gate.
3. **A PLAN frontmatter `external_verify:` line would run at every stage** ("Evaluated for EVERY stage", agent_result.rs:2673) and fail Define/Plan before the code exists. The payload plan must not declare one.
4. **The chain-flag write rewrites `.planning/config.json` with `serde_json::to_string_pretty`** (gsd_config.rs:170, `preserve_order`) — a committed config that is not already in that exact form (with `"_auto_chain_active": false` present) becomes a dirty tracked file, and `/gsd-ship`'s "Clean working tree?" check (ship.md:68-73) then stops headless.
5. **`devflow.toml` is read from the launch checkout's working tree**, not from the base branch (config.rs:182-183 `project_root.join("devflow.toml")`), while the persisted `State::base_branch` carries the value to later processes. The launch checkout must sit on a branch that carries `devflow.toml` (recommend: check out `integration` itself).

On the research questions: **Validate runs `/gsd-validate-phase 1`** (stage.rs:65) plus DevFlow's verdict contract; with Nyquist on by default it audits tests and only calls `AskUserQuestion` when it finds gaps (validate-phase.md:88-96), so the smallest working toolchain is **Python 3 standard-library `unittest`** (no package installs; no version file, so VersionBump tags only; `cargo doc` in DocsUpdate fails harmlessly). The **version classifier reads `git tag`, `git tag --merged HEAD`, and every `--no-merges` commit message in `range_start..HEAD`** (version.rs:180-445); only a well-formed breaking marker (`type!:` or a `BREAKING CHANGE:` footer) yields MAJOR, so scaffold commits typed `chore:`/`docs:`/`test:` plus the agent's `feat(...)` commits give MINOR `v0.1.0`. **`gh` needs the classic `repo` scope** (default `gh auth login` grants `repo`, `read:org`, `gist`); `delete_repo` only for the post-verification cleanup. **The gate-open snapshot is best implemented as the `DEVFLOW_GATE_NOTIFY_CMD` hook**, which runs synchronously inside the gate-firing process after the gate file is written and — critically for `--yes-ship` — *before* the auto-response is written (pipeline_gate.rs:371, 400, 414); but DECN-03's resumed turn happens between gates, so it is captured post-run from `.devflow/history/phase-01/` and the Claude transcript, not by any gate snapshot.

**Primary recommendation:** Extend `scripts/scratch-dogfood-repo.sh` behind an opt-in flag to emit a Python-`unittest` payload project with an explicit, serde-pretty GSD config (security/UI gates off, verifier and Nyquist on, `branching_strategy: "none"`, `use_worktrees: false`), a pre-authored `01-01-PLAN.md` whose first task is `<task type="checkpoint:decision" gate="blocking-human">`, a pre-authored draft `01-VALIDATION.md`, a pre-authored RED test, `devflow.toml` with `base_branch = "integration"` and `capture_retention = 64`, all on an `integration` branch the launch checkout sits on; capture evidence with a notify-hook snapshot plus a post-run collector; judge from evidence in a separate plan.

## Re-verification ledger (what was reused from the superseded research, and on what evidence)

| Old finding (49-RESEARCH.md 2026-09-27) | Status now | Evidence this session |
|---|---|---|
| Define preflight fires in the `start` process, blocking | Re-verified | `run_preflight` → `run_gate` (preflight.rs:1403); F-16 doc comment preflight.rs:1122-1128; `start` calls `launch_stage` (commands.rs:~700) |
| Monitor stdio is `/dev/null`, so the Code-stage prerequisite report is lost | Re-verified | monitor.rs:445-447 `.stdout(Stdio::null())` |
| `hermetic_command` strips only git redirect variables | Re-verified | git.rs:87-94 |
| Negative-control refusal text | Re-verified | preflight.rs:993-996 (quoted in D-14 answer) |
| Resume launch shape (`-p … --resume … --output-format json --dangerously-skip-permissions`) | Re-verified | claude.rs:122-135 |
| Gate verbs; `gate approve` needs a confirmed waiter; abort via note containing "abort" | Re-verified | main.rs:396-457; commands.rs:1404-1430; gates.rs:73-83 |
| Pid-stamped state temp files for SURV-01 | Re-verified | workflow.rs:207 `format!(".{name}.{pid}.{sequence}.tmp")` |
| "Do not set `DEVFLOW_GATE_NOTIFY_CMD`" | **Reversed** | The old reason was DevFlow's own E2E tests firing gates for phases 7/11 in child processes. In the disposable project the Code/Validate test children are the payload's `unittest`, which never invokes `devflow` (Reasoned). The hook is now the only mechanism that snapshots the `--yes-ship` Ship gate before it is auto-answered (Pattern 3). |
| systemd transient unit supervision; env leak from an agent shell | Carried as Reasoned (not re-probed) | monitor.rs:980-984 (`process_group(0)`, no `setsid`); the old probe output is not re-run here |
| MAJOR-bump gate / `v3.0.0`, ND-1 accommodations, 49.1, landing hop, undo | Dropped | Moot under D-17/D-23 |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Disposable project scaffold | Operator host script (`scripts/scratch-dogfood-repo.sh`, extended) | — | D-20; must stay outside every checkout (T-23-01) |
| Private GitHub repo creation, push of `main`/`integration`, deletion | Operator (runbook, `gh`) | — | D-24: outward-facing actions stay operator-owned |
| Stage orchestration, gates, merge/bump/tag | DevFlow binary (pinned build) | GSD workflows run by the agent | Under test; never modified during an attempt |
| Code / Validate / Ship work | Claude agent running GSD (`/gsd-execute-phase`, `/gsd-validate-phase`, `/gsd-code-review` + `/gsd-ship`) | — | Real agent, real GSD (D-17) |
| Gate-open snapshots | `DEVFLOW_GATE_NOTIFY_CMD` hook (tested script) | Post-run collector | Synchronous in the gate-firing process (Pattern 3) |
| SURV-01 observation | Out-of-band inotify watcher (python ctypes) | — | `inotifywait` is not on the host |
| Negative control | Tested driver script on a local scratch repo | — | D-25, D-14 |
| Judgement (criteria 1-5) | Ordinary GSD plan after the checkpoint | Evidence checker script | D-18 |

## Answers to the questions CONTEXT.md leaves to research

### Q1 (D-20) — What DevFlow's Validate stage runs, and the smallest toolchain

**Verified:**
- Validate's command is `/gsd-validate-phase {N}` — stage.rs:65 `Stage::Validate => "/gsd-validate-phase {N}",` — wrapped with the verdict contract requiring `DEVFLOW_RESULT: {"status": "success", "verdict": "pass"}` or `"gaps"` (prompt.rs:294-322). Only `verdict: pass` with `status: success` advances (pipeline_outcomes.rs:229-244: `(_, AgentStatus::Success, Some(Verdict::Pass)) => ValidateOutcome::Passed`).
- DevFlow runs **no test command of its own** at Validate unless a PLAN declares `external_verify:` **and** `DEVFLOW_TRUST_EXTERNAL_VERIFY` approves it (verify.rs:10-27, agent_result.rs:2699-2760). Layer 0 is "Evaluated for EVERY stage, not only Code" (agent_result.rs:2673-2674), so declaring one in the payload would fail Define and Plan (the code does not exist yet) and, unapproved, fail every stage with "external verification is not approved; set DEVFLOW_TRUST_EXTERNAL_VERIFY to the reviewed JSON command array". **Do not declare `external_verify`.**
- `/gsd-validate-phase` exits early "Nyquist validation is disabled" when no `validate-phase` step hook is active (validate-phase.md:35-37). Probe (this session, `gsd-tools loop render-hooks verify:post --raw`): under `{"workflow": {}}` the active hooks include `nyquist/step/{"skill":"validate-phase"}/when=workflow.nyquist_validation`; with `"nyquist_validation": false` that entry is absent (negative control passed). So Nyquist is **on by default**, and Validate does real work.
- With Nyquist on: State A (a `*-VALIDATION.md` exists) audits it; State B (no VALIDATION.md, SUMMARYs exist) reconstructs; State C (no SUMMARY) exits (validate-phase.md:44-50). It scans for `pytest.ini`, `jest.config.*`, `vitest.config.*`, `pyproject.toml`, `*.test.*`, `*.spec.*`, `test_*` (:67-70). **No gaps → skip to Step 6** (:88); gaps → `AskUserQuestion` (:93-96), which has no answerer in a headless `claude -p` run.
- Host toolchains (probed): `python3` 3.14.7 (`/home/linuxbrew/.linuxbrew/bin/python3`; `/usr/bin/python3 -> python3.14` also present), `node` v26.10.0, `cargo` 1.97.1, `go`, `zig`, `pytest` (mise shim).

**Reasoned recommendation — Python 3 stdlib `unittest`:**
- Zero installs (no `pytest` dependency; `python3 -m unittest discover -s tests -v`), deterministic, sub-second.
- A file named `tests/test_*.py` matches GSD's `test_*` scan, so State B would also detect it; a pre-authored draft `01-VALIDATION.md` (State A) removes the reconstruction step entirely. Pre-authoring it is consistent with D-04: GSD's own template says the file is "seeded by plan-phase" (`templates/VALIDATION.md` frontmatter comment `status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)`), and the payload's plan-phase is already replaced by a pre-authored plan.
- No `Cargo.toml`/`pyproject.toml`/`package.json` → `detect_version_file` finds none (version.rs:69-76, `for name in ["Cargo.toml", "pyproject.toml", "package.json"]`) → VersionBump "tagging only" (hooks.rs:340-341), no version-file commit.
- DocsUpdate on Validate→Ship runs `cargo doc --no-deps` in the worktree (hooks.rs:99-104, 236-257); with no `Cargo.toml` it fails and the hook warns and commits nothing. Caveat: cargo searches ancestor directories — place the disposable project where no ancestor directory holds a `Cargo.toml` (Pitfall 9).
- Rust (`cargo test`) is the alternative if the operator wants the VersionBump version-file path exercised; cost: `Cargo.toml` becomes a version file (VersionBump writes + commits `chore: bump version to …`), DocsUpdate succeeds and `commit_all` does `git add .` then `commit --allow-empty` (git.rs:388-399) so `target/` must be gitignored, and compile time enters every stage. Not recommended for the minimum.
- **Pre-author a RED test** in the scaffold (the test exists, the module does not), so the Code stage's work is "make the test pass" and the scaffold's own selftest can prove the test fails before and passes after a reference implementation (negative + positive control, Validation Architecture).

### Q2 (D-21) — What the version-bump classifier reads, and how to keep it below MAJOR

**Verified (version.rs):**
- `highest_semver_tag`: every `git tag`, `v`-prefixed semver only (:180-199).
- `reachable_semver_baseline`: `git tag --merged HEAD` (:209-229); baseline `0.0.0` when none (`compute_version`, :675-677).
- D-10 refusal if the highest tag is not reachable from HEAD (:659-671, mirrored at preflight.rs:804-818).
- `release_range_start` → range `start..HEAD`; with no tag the range is **all history reachable from HEAD** (`classify_range_bump`: `if range_start.is_empty() { "HEAD" }`, :385-390).
- `git log --no-merges <range> --format=%H%x1f%B%x1e` (:391-393) — **full messages**, merges excluded.
- `classify_commit_message` (:420-445): unparseable → `Patch`; `commit.breaking()` → `Major`; `feat` → `Minor`; `fix`/`perf` → `Patch`; `docs`/`test`/`chore`/`ci`/`refactor`/`style` → `None`; other recognised type → `Patch`. `apply_bump` collapses `None` to a patch (:634-645).
- Ship preflight classifies the **worktree HEAD** (preflight.rs:797 `let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);`); VersionBump classifies the launch checkout's HEAD after `git checkout integration` + merge (hooks.rs:180-233, 317; git.rs:185-191).

**What makes it MAJOR:** only a commit in range whose message parses as a conventional commit with `!` before the colon or a `BREAKING CHANGE:`/`BREAKING-CHANGE:` footer (preflight.rs:886-894 mirrors the rule for the diagnostic).

**Recommendation (Reasoned):**
- Every scaffold commit uses a non-breaking type (`chore:`, `docs:`, `test:`); no `!`, no `BREAKING` footer. The existing scaffold commit `chore: scaffold devflow probe target` (scratch-dogfood-repo.sh:194) is already `None`.
- No tag in the scaffold. Baseline `0.0.0`; the agent's GSD commits (`feat(01-01): …`, `test(…)`, `docs(…)`) give **MINOR → `v0.1.0`**. Adding a `v0.1.0` tag on the root commit would also work but adds a D-10 reachability condition for no benefit.
- The payload plan must not ask for an API-breaking change or mention "breaking" in any commit guidance.
- Pre-launch guard (tested script): scan `git log --no-merges integration --format=%B` with the same breaking rule and refuse to launch on a hit. This covers only the scaffold's commits; the agent's commits are observed at Ship preflight (a MAJOR there opens a human gate — a D-22 deviation to record, not to route around).

### Q3 (D-24) — `gh` auth scope and what the runbook records

**Cited:**
- `gh auth login`: "The minimum required scopes for the token are: `repo`, `read:org`, and `gist`." ([cli.github.com/manual/gh_auth_login](https://cli.github.com/manual/gh_auth_login))
- `repo` "Grants full access to public and private repositories including read and write access to code, commit statuses, …" ([docs.github.com OAuth scopes](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps)). The page does not state explicitly which scope `POST /user/repos` with `private: true` needs; `repo` is the scope that covers private repositories.
- `gh repo delete`: "Deletion requires authorization with the `delete_repo` scope. To authorize, run `gh auth refresh -s delete_repo`"; non-interactive deletion needs the explicit `owner/repo` plus `--yes` ([cli.github.com/manual/gh_repo_delete](https://cli.github.com/manual/gh_repo_delete)).

**Verified (local, no network call made):** `gh` 2.101.0; `~/.config/gh/hosts.yml` names user `denniyahh`, `git_protocol: https`; git's credential helper for `https://github.com` is `!/home/linuxbrew/.linuxbrew/bin/gh auth git-credential` — so `/gsd-ship`'s `git push` authenticates through gh's token as well.

**Not observed:** the token's actual scopes (would need `gh auth status`, a network call; not run under the read-only brief). The runbook's pre-flight records `gh auth status` output (the "Token scopes" line, never the token), the account name, `gh --version`, and the created repository's `nameWithOwner` and `visibility` (`gh repo view --json nameWithOwner,visibility`). If the scopes lack `repo`, `gh auth refresh -s repo`; add `-s delete_repo` only at cleanup time.

**Also required on the host at Ship:** DevFlow's own Ship preflight runs `gh auth status` and fails preflight if it reports unauthenticated (preflight.rs:692-722), and GSD ship preflight 5 requires `gh` authenticated (ship.md:95-98).

### Claude's Discretion — does a gate-open snapshot capture everything DECN-03 needs?

**No, not by itself; it captures the state that gets deleted, and the DECN-03 turns persist on disk until collected.** Details:

**Verified mechanics:**
- `run_gate_with_timeout` order: save state `gate_pending=true` → `Gates::write_gate` (pipeline_gate.rs:371) → `gate_fired` event → `gates::fire_gate_notify` (:400) → `notify_fired` → auto-response written if any (:414-422) → `Gates::poll_response` (:423).
- `fire_gate_notify` reads `DEVFLOW_GATE_NOTIFY_CMD` at fire time (gates.rs:388-394) and runs it with `sh -c` and `.output()` — synchronous, no timeout — passing `DEVFLOW_GATE_PHASE`, `DEVFLOW_GATE_STAGE`, `DEVFLOW_GATE_CONTEXT`, `DEVFLOW_NON_SILENT_GATE` as env (gates.rs:403-413). The project root is **not** passed; the command must carry it.
- The monitor inherits the caller's environment except git redirect variables (git.rs:87-94), so a hook exported in the `start` environment fires in every later monitor too (Reasoned from `hermetic_command` + `.envs(...)` at monitor.rs:433-444).
- With `--yes-ship`, `handle_ship_outcome` passes an auto-response (pipeline_outcomes.rs:838-850); the notify hook therefore runs **before** the Ship gate is answered and before `finish_workflow` clears state (`workflow::clear_state(project_root, state.phase)?;` at pipeline_gate.rs:275). A polling watcher can miss this window entirely.
- State file `.devflow/state-01.json` (workflow.rs:140) is deleted at Ship completion and at abort — criterion 1's `State::base_branch` must be snapshotted during the run.

**What each gate snapshot captures:**

| Gate | Fires in | Captures for evidence |
|---|---|---|
| Define preflight (human) | the `devflow start` process | `state-01.json` (`base_branch`, `checkpoint_approval: Pending`), worktree HEAD (D-16 value 2), gate context |
| Code preflight (human) | the Plan monitor's `advance` | plan files + their checkpoint declarations as approved, `state-01.json` before the set is recorded |
| Ship (auto-answered by `--yes-ship`) | the Validate→Ship path's monitor | final `state-01.json` (`checkpoint_approval: Recorded`, `session_id`, `checkpoint_resumes`), `.devflow/history/phase-01/` (by then holds the Code turn-1 capture and the resumed-turn capture), `events.jsonl` including `checkpoint_auto_decided` |
| Any unexpected never-silent gate | wherever it fires | the state at the stop — the attempt's failure evidence |

**What no gate snapshot captures, and where it persists instead:**
- **Turn 1 (CODE_STAGE_POLICY).** Code launches *after* the Code preflight gate, and `.devflow/phase-01-prompt` is overwritten at every pipe-owning launch (monitor.rs:420-421). The stream-json Code capture echoes the operator's prompt back as a `user` event (agent_result.rs:644-650 doc), and it is archived to `.devflow/history/phase-01/{stamp}-stdout` at the next launch (agent_result.rs:3168-3240; `capture_archived` event carries the stamp).
- **Resumed turn (checkpoint_auto_decide_prompt).** `relaunch_checkpoint_session` emits `checkpoint_auto_decided` (with the instruction truncated to 300 chars) *before* the spawn (pipeline_launch.rs:1236-1283), then runs `claude -p <instruction> --resume <sid> --output-format json` on the Legacy monitor. Its single-document capture `.devflow/phase-01-stdout` holds only the final `result` text and `session_id`; it is archived into history at the Validate launch.
- **Full two-turn transcript with tool calls.** `~/.claude/projects/<cwd with non-alphanumerics replaced by '-'>/<session-id>.jsonl`, where cwd is the worktree `<launch>/.worktrees/phase-01` (worktree.rs:37-43). Default retention 30 days (`cleanupPeriodDays`, unset in `~/.claude/settings.json` — verified). `--resume` without `--fork-session` continues the same session ([code.claude.com/docs/en/sessions](https://code.claude.com/docs/en/sessions)); the resumed capture's `session_id` field confirms which file.
- **Retention.** `DEFAULT_CAPTURE_RETENTION = 12` (config.rs:36); a clean run with one resume produces 5 archive events before Ship, each Validate loop-back adds 2. Set `capture_retention = 64` in the disposable `devflow.toml` (a documented key, config.rs:78) so no loop-back count evicts turn 1.

**Conclusion:** the notify-hook snapshot is necessary (state and the Ship window) but not sufficient; a **post-run collector** (run by the operator from the runbook after the attempt ends, before repo deletion, within 30 days) copies `.devflow/` (events, history, gates, monitor logs), the Claude session directory for the worktree path, and the git observations. Neither needs a timed step while the run is parked. If the run stops before Ship, the never-silent gate at the stop fires the same hook.

### D-22 — Exactly two human gates? (Verified, with named deviations)

Flow for `devflow start --phase 1 --agent claude --mode auto --yes-ship <launch>` with the pre-authored blocking-human plan on `integration`:

1. `start`: base resolved from `devflow.toml` (config.rs:384-418), local-branch check (commands.rs:398-406), currency check (preflight.rs:556-620), reachability (preflight.rs:149-226), worktree fork (commands.rs:525-532), `launch_stage` → `run_preflight`.
2. **Define preflight:** `unattended_launch_check_applies` → `matches!(stage, Stage::Define | Stage::Code)` (preflight.rs:971-972). C1 config holds; C2 holds for claude without the legacy opt-out (:1027-1034); C3: plan files exist → `phase_has_human_only_checkpoint` → `DoesNotHold("a plan declares a `blocking-human` gate or a `human-action` checkpoint, which no mode auto-approves (checkpoints.md rule 6)")` (:1076-1080). Mode Auto → refusal → **human gate 1**. Approve → Advance skips the re-check and launches Define directly (:1413-1437).
3. Define prompt is a no-op (prompt.rs:374-387); Plan prompt sees the existing PLAN.md and returns success without running plan-phase (prompt.rs:341-360). `Mode::should_gate` is false for Define/Plan/Code (mode.rs:249-259).
4. **Code preflight:** C3 again `DoesNotHold` → **human gate 2**; Advance records the checkpoint set (`record_checkpoint_set_for_code_evaluation`, preflight.rs:1433) then launches Code.
5. Code: `/gsd-execute-phase 1 --auto` + `CODE_STAGE_POLICY` (prompt.rs:409-420, :97-123). The executor stops at the blocking-human checkpoint (checkpoints.md golden rule 6; execute-phase.md:1050 carve-out). If the agent reports failure and the capture carries a `**Gate:** blocking-human` line, `Action::GateReview` → re-scan against the recorded set → unchanged → `relaunch_checkpoint_session` with **no gate** (pipeline_launch.rs:1777-1897).
6. Validate: pass → Validate→Ship transition runs DocsUpdate (hooks.rs:99-104).
7. Ship preflight: major-bump check + gh-auth check (preflight.rs:1254-1305); passes when MINOR and authenticated.
8. Ship agent: `/gsd-code-review 1`, then `/gsd-ship 1` only if no Critical finding (prompt.rs:238-280). On success → Ship gate **auto-answered** by `--yes-ship` (pipeline_outcomes.rs:838-850) → `finish_workflow` → Merge, VersionBump, ChangelogAppend, BranchCleanup (hooks.rs:109-116) → `workflow_shipped`.

**Paths that add a gate (each a recorded deviation from D-22, not a failure of the instrument):** a Ship-review Critical (loops to Code via `review:` reason; three consecutive Validate `gaps` gate at mode.rs:255); `/gsd-ship` blocked (no VERIFICATION `passed`, dirty tree, security gate) → Ship agent failure → never-silent gate; the agent editing the checkpoint task element → re-scan gate (pipeline_launch.rs:1802-1813); MAJOR bump; `gh` unauthenticated; the agent reporting `success` without resolving (no resume at all — record as "path not exercised" for DECN-03).

### D-06 — Topology: what must exist at `start`, what `/gsd-ship` targets, where the hooks run

| Item | Requirement | Evidence |
|---|---|---|
| `devflow.toml` with `base_branch = "integration"` | In the **launch checkout's working tree** at `start` (read from `project_root.join("devflow.toml")`) and committed (criterion 1) | config.rs:182-199, 384-418 |
| Local branch `integration` | Must exist as `refs/heads/integration` (show-ref check, non-default source only) | commands.rs:124-168, 398-406 |
| `origin/integration` | Optional: present → currency `Current`; absent → `Undeterminable`, warning, proceeds | preflight.rs:371-431, 580-594 |
| `integration` carries `.planning/ROADMAP.md` with `### Phase 1:` and a `.planning/phases/01-*/` directory | Required (reachability refuses otherwise) | preflight.rs:149-226 |
| `integration` carries `.planning/config.json` | Required for C1 in the worktree (launch root = worktree) | preflight.rs:1161-1167, 990-1005 |
| `integration` ≠ `main` | `validate_base_branch` refuses `main` | config.rs:351-356 |
| Launch checkout branch | Recommend `integration` checked out (then `git checkout integration` in Merge is a no-op). If `integration` is checked out and falls behind `origin/integration`, `start` refuses (never true for a fresh repo) | preflight.rs:596-618; git.rs:188 |
| `.gitignore` | `.devflow/`, `.worktrees/`, `__pycache__/`, `*.pyc` — `.devflow/` and `.worktrees/` live under the launch checkout | worktree.rs:37-43; agent_result.rs:3051 |
| GSD ship base | `BASE_BRANCH=$(gsd_run query git.base-branch)` (ship.md:46): `git.base_branch` config → `origin/HEAD` → `git remote show origin` → local `main` → `main` (git-base-branch.cjs:12-18). Set `"git": {"base_branch": "main"}` explicitly so no network lookup decides it | ship.md:46; `~/.claude/gsd-core/bin/lib/git-base-branch.cjs:12-18` |
| What `/gsd-ship` pushes | `git push origin ${CURRENT_BRANCH}` from the worktree = `feature/phase-01` (ship.md:194-199), then `gh pr create --base "${BASE_BRANCH}"` = `main` (ship.md:366-374), then a ship-note commit + second push (ship.md:~500) | ship.md |
| Where the terminal hooks run | Launch checkout (`hook_context_root` returns `project_root` for the terminal batch, pipeline_outcomes.rs:1003-1010); base from the persisted `state.base_branch` (`git_flow_for_run`, :1066-1067) | pipeline_outcomes.rs |
| Merge | `git checkout integration` + `git merge --no-ff feature/phase-01`, ancestry re-check, `merge_result {"merged": true, "branch": "feature/phase-01"}` — **the event names the branch, not the target**; the target is observed from git | git.rs:185-191; hooks.rs:180-233 |
| VersionBump / ChangelogAppend | Local tag `v0.1.0` (`-c tag.gpgSign=false`), `CHANGELOG.md` committed on `integration` | hooks.rs:259-352; git.rs:250-253 |
| BranchCleanup | `git branch -d feature/phase-01` — fails because the branch is checked out in the worktree; the hook only warns (Reasoned) and returns Ok | hooks.rs:131-151 |
| Pushes by the hooks | None found: no production call site of `GitFlow::push`/`delete_remote_branch` (Reasoned from a grep with a positive control that matched another `.push(branch` call) — `integration`'s merge, the tag and the changelog stay local | git.rs:309-326 |

**Observing the merge target (criterion 2):** `merge_result`+`hook_run` events; `git rev-parse integration` before/after; the merge commit located in `integration`'s history, not `integration`'s tip — `git rev-list --first-parent --merges <fork-point base_sha>..integration` yields exactly one merge, and `git log -1 --format=%P <that merge>` has two parents, the second = the feature tip recorded in the Ship gate snapshot (*corrected 2026-09-28, plan-check:* the tip-parent read recommended here before was wrong, because VersionBump tags and ChangelogAppend then commits `CHANGELOG.md` on `integration` after the merge, per the row above and `hooks.rs:115-122`, `:286`, `git.rs:185-190`, so a correct run's tip is a single-parent commit; adopted by 49-03's collector and 49-08's `positive: merged into integration`); `git merge-base --is-ancestor feature/phase-01 integration` (supporting only: BranchCleanup may delete the ref, A4); `main` unchanged locally and on origin; the PR's `baseRefName` is `main` (`gh pr view --json baseRefName,headRefName,state`). The two targets differ by design (D-06).

### D-14 — Negative-control refusal text (verbatim, Verified)

preflight.rs:993-996:
```
"no `.planning/config.json` under the launch root — nowhere for the chain flag \
 to live"
```
Rust drops the newline and the next line's leading whitespace after `\`, so it renders `no \`.planning/config.json\` under the launch root — nowhere for the chain flag to live`. The report line format is `"  [{}] {name} — {detail}"` (preflight.rs:1188) with name `"GSD config can hold the chain flag"` (:1166) and label `"DOES NOT HOLD"` (:940). The gate context wraps it as `"[never-silent] preflight failed for stage {stage}: {} — human review needed (retry, loop-to-code, or abort)"` (:1399-1400) around `"unattended launch refused (D-07) — {}"` (:1220). Positive-arm counterpart: `  [holds] GSD config can hold the chain flag` (label `"holds"`, :939) and the note `note: base branch is \`integration\` (from devflow.toml (base_branch)) — …` (commands.rs:420, 424-425). The default source prints no note (`if base != DEVELOP`, commands.rs:417). Existing scaffold ends on `develop` (scratch-dogfood-repo.sh:195-196) and writes `{"workflow": {}}` (`:187-191`).

### D-27 — GSD resume for a failed `checkpoint:human-action` plan (Verified, one gap)

- Filtering skips plans with `has_summary: true` (execute-phase.md:327); `<resumption>`: "Re-run `/gsd-execute-phase {phase}` → discover_plans finds completed SUMMARYs → skips them → resumes from first incomplete plan" (:1454-1455).
- `human-action` always stops even in auto mode (execute-phase.md:1047; this repo's config has `workflow.auto_mode: true`, which does not change that).
- Failure handling: "Agent fails mid-plan: Missing SUMMARY.md → report, ask user how to proceed" (:1448).
- `create_summary` runs after all tasks (execute-plan.md:406-412). **Constraint:** "Once production commits for a plan exist, returning without a committed SUMMARY.md is an illegal partial-plan state. The next execute-phase resume must detect that condition" (execute-plan.md:16-20). **So the checkpoint plan must make no commits before or at the checkpoint on a stopped attempt** — attempt evidence stays in the out-of-repo evidence directory; only the judging plan commits sanitized evidence (Reasoned consequence).
- **Gap [ASSUMED]:** what the continuation executor does with a `stopped: …` reply is not specified in source. The plan must state it in the checkpoint task (`<resume-signal>` and `<done>`/acceptance criteria that a `stopped:` reply cannot satisfy) and the verification gate must check a machine-readable attempt verdict file in the evidence directory rather than trust the reply text.

## Standard Stack

### Core
| Tool | Version (host, probed) | Purpose | Why |
|------|---------|---------|-----|
| `devflow` (pinned build of `feature/phase-49` HEAD) | workspace crates, built per attempt | The system under test | D-12: SHA + sha256 recorded per arm; `workflow_started` also records `commit`/`dirty` (commands.rs:722-737) |
| `claude` CLI | 2.1.283 | The agent (`--agent claude`, D-15) | DECN-03 is claude-only (pipeline_launch.rs:1777) |
| GSD core | 1.14.0 (`~/.claude/gsd-core/VERSION`) | Workflows the agent runs | real GSD (D-17) |
| `gh` | 2.101.0 | Private repo create, PR (via `/gsd-ship`), repo view/delete | D-24 |
| `git` | host | Scaffold, fork-point measurement | — |
| `python3` stdlib `unittest` | 3.14.7 | Payload test | Q1 |

### Supporting
| Tool | Version | Purpose | When |
|------|---------|---------|------|
| `systemd-run --user` | host (`/usr/bin/systemd-run`; user manager state `degraded`, probed) | Detached supervision of `devflow start` so it survives the operator's terminal and no Claude session env leaks into the nested agent | Runbook launch (Reasoned, old probe not re-run) |
| `notify-send` | host | Operator notification from the notify hook | Gate open |
| `shellcheck`, `bats` | brew | Script lint; optional test runner | Tool plans |
| `ruff` | 0.16.9 | Python watcher lint/format | SURV watcher |
| `timeout`, `sha256sum`, `jq`, `flock` | host | Bounding hooks, binary identity, JSON evidence | Tools |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Python `unittest` payload | Rust crate payload | Exercises VersionBump's version-file write; adds `target/`, `cargo doc` commit, compile time |
| Notify-hook snapshot | Polling gate watcher (old 49-03 design) | Cannot reliably see the `--yes-ship` Ship gate, which is answered immediately after notify |
| python ctypes inotify | `inotifywait` | Not installed (probed); dnf is not an option on this host |
| Pre-authored VALIDATION.md | Let validate-phase reconstruct (State B) | State B depends on the agent's gap classification; gaps → `AskUserQuestion` in a headless run |

**Installation:** none. Everything is already on the host.

## Package Legitimacy Audit

This phase installs no external packages (Python stdlib only; tools already on the host). No registry lookups were needed.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| — | — | — | — | — | — | No packages installed |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 operator host (runbook)                                  GitHub (private throwaway, per attempt)
 ─────────────────────────                                ───────────────────────────────────────
 build pinned devflow ──► sha256 + git SHA ──► evidence/attempt-N/identity
          │
          ├─► NEGATIVE ARM: scratch repo (no remote) ──► devflow start --mode auto
          │        (config.json removed, DEVFLOW_BASE_BRANCH unset)
          │        └─► Define preflight REFUSES (C1) ──► gate(define) ──[notify hook: snapshot]──►
          │             assert text + base_branch=develop ──► gate reject --note "abort: …"
          │
          └─► POSITIVE ARM:
               scaffold (--live-payload) ──► gh repo create --private ──► push main, integration ─────► origin
                    │
                    ▼
               devflow start --phase 1 --agent claude --mode auto --yes-ship  (systemd-run, env: NOTIFY hook)
                    │ reads devflow.toml (base=integration) ─► forks .worktrees/phase-01 from integration
                    ▼
               Define preflight: C3 DOES NOT HOLD ─► gate(define) ─[snapshot]─► operator approves ─► Define (no-op)
                    ▼
               Plan (PLAN.md exists → no-op) ─► Code preflight: C3 ─► gate(code) ─[snapshot]─► operator approves
                    │                                                   (records checkpoint set)
                    ▼
               Code turn 1: /gsd-execute-phase 1 --auto + CODE_STAGE_POLICY
                    │ executor stops at blocking-human decision ─► agent reports ─► GateReview
                    ▼
               checkpoint_auto_decided event ─► claude -p <auto-decide> --resume <sid> (resumed turn)
                    ▼
               Validate: /gsd-validate-phase 1 ─► verdict pass ─► DocsUpdate (cargo doc fails, no-op)
                    ▼
               Ship preflight (major-bump, gh auth) ─► Ship: /gsd-code-review 1 ─► /gsd-ship 1
                    │                                    push feature/phase-01 ─► PR → main ──────────────► origin
                    ▼
               Ship gate ─[snapshot]─► auto-answered by --yes-ship ─► Merge into integration (local),
                    tag v0.1.0, CHANGELOG commit, BranchCleanup (warns) ─► workflow_shipped
                    ▼
               post-run collector: .devflow/, ~/.claude/projects/<worktree>/, git observations, PR JSON
                    ▼
               operator answers the Phase 49 checkpoint: complete | stopped: <what happened>
                    ▼
               judging plan: evidence checker → criteria 1-5, DECN-03 class, SURV-01 observation
```

### Recommended layout

```
scripts/scratch-dogfood-repo.sh                 # extended: default output byte-identical; --live-payload adds the payload project
.planning/phases/49-…/tools/                    # phase-local tools (superseded-plan precedent), each with --selftest
├── snapshot-hook.sh     # DEVFLOW_GATE_NOTIFY_CMD target: bounded copy of state/events/history/gates + worktree HEAD
├── collect-attempt.sh   # post-run collector (idempotent, read-only on the project)
├── negctl.sh            # negative arm driver + text assertions
├── surv-watch.py        # inotify writer attribution (watch / analyze / --selftest)
├── check-evidence.sh    # criteria 1-3 + fork-point triple + DECN-03/SURV-01 classification inputs
└── sanitize-evidence.sh # strips home paths / usernames / tokens before commit (D-26)
.planning/phases/49-…/49-RUNBOOK.md             # operator-owned; adversarially reviewed
~/.cache/devflow-phase49/attempt-N/{project,negctl,evidence}/   # outside every checkout (Claude's Discretion)
```

### Pattern 1: The disposable project (scaffold extension)

**What:** `scripts/scratch-dogfood-repo.sh [--live-payload] <dest>`; with the flag it writes the files below on `main`, creates `integration` from it, commits `devflow.toml` on `integration`, and leaves `integration` checked out. Without the flag, output is unchanged (the negative arm and existing uses depend on it).

**Config (`.planning/config.json`) — written already in `serde_json::to_string_pretty` form with a trailing newline, so DevFlow's chain-flag round-trip is byte-identical (gsd_config.rs:170-176; Cargo.toml:27 `preserve_order`):**
```json
{
  "commit_docs": true,
  "parallelization": false,
  "workflow": {
    "auto_mode": false,
    "_auto_chain_active": false,
    "verifier": true,
    "nyquist_validation": true,
    "security_enforcement": false,
    "ui_phase": false,
    "ui_safety_gate": false,
    "ui_review": false,
    "code_review": true,
    "tdd_mode": false,
    "use_worktrees": false,
    "research": false,
    "plan_check": false
  },
  "git": {
    "branching_strategy": "none",
    "base_branch": "main"
  }
}
```
Rationale per key (Verified unless noted): `security_enforcement: false` removes the blocking ship gate (probe); `verifier: true` keeps VERIFICATION.md for ship preflight 1 (probe `verifier_enabled= true` default; explicit anyway); `nyquist_validation: true` keeps Validate doing work; `_auto_chain_active: false` present so the drill's C1 shape holds and the round-trip is clean (unattended-drill.sh config comment); `auto_mode: false` keeps the chain flag the only auto-approval path (unattended-drill.sh comment); `branching_strategy: "none"` keeps GSD on the DevFlow branch without fetch/switch (execute-phase.md:269-301 — with `"phase"` GSD would compute `feature/phase-01`, probed, and `git switch` to it, which also works); `use_worktrees: false` avoids nested executor worktrees (Reasoned; `~/.gsd/defaults.json` sets `use_worktrees: true` and, per the probe, is ignored for a project with `.planning/`, but explicit is safer); `git.base_branch: "main"` fixes the PR target without a network lookup. `ui_safety_gate`/`schema_drift_gate` appear as default-active `execute:wave:post` gates in the probe; set the UI one off (the drift gate's `when` key is `workflow.schema_drift_gate` — set `false` too). **The selftest must assert that each of these keys resolves as intended via `gsd-tools query config-get` and `loop render-hooks` (the probes in this research), not trust this list.**

**`devflow.toml` (on `integration`, committed):**
```toml
base_branch = "integration"
capture_retention = 64
```
(`DevflowConfig` fields `base_branch`, `capture_retention` — config.rs:73-117. `yes_ship` deliberately NOT set here: `--yes-ship` on the command line is attributed as `--yes-ship` in the ledger, pipeline_outcomes.rs:839-843.)

**Payload plan task opening line (both markers must be on the `<task` line — verify.rs:228-238 checks only the opening line):**
```xml
<task type="checkpoint:decision" gate="blocking-human">
  <decision>Choose the default retry budget, in seconds, for `probe.budget.DEFAULT_RETRY_BUDGET_SECS`</decision>
  <context>…considerations that weigh but do not decide; no option is derivable from the code…</context>
  <options>
    <option id="budget-30"> <name>30 seconds</name> <pros>…</pros> <cons>…</cons> </option>
    <option id="budget-120"><name>120 seconds</name><pros>…</pros><cons>…</cons></option>
    <option id="budget-300"><name>300 seconds</name><pros>…</pros><cons>…</cons></option>
  </options>
  <resume-signal>Reply with the option id</resume-signal>
</task>
```
(Format from `~/.claude/gsd-core/references/checkpoints.md:150-166`; values here are illustrative [ASSUMED].) Make the decision Task 1 so turn 1 stops early and the resumed turn does the implementation; place the option the context favours *not* first, so positional selection (which CODE_STAGE_POLICY forbids) is distinguishable from merit selection in the "Followed" classification. The pre-authored RED test accepts any offered option value, so Validate never depends on which option was chosen.

**Keep every project document free of pipeline claims.** The 23a probe's Ship review raised a Critical because `01-VERIFICATION.md` scored "Define→Plan→Execute→Ship carried end-to-end" as VERIFIED before Ship ran (23-PROBE-FINDINGS.md:80). The payload goal and must-haves describe only the code change.

### Pattern 2: Launch and supervision

- Launch from a plain terminal or `systemd-run --user` (not from a Claude Code session's Bash tool — its `CLAUDE*` environment would leak into the nested agent; carried Reasoned from the old probe). `devflow start` blocks at the Define gate (the gate poll runs in the `start` process, `gate_timeout_secs()` default `THREE_DAYS`, config_parse.rs:20-21, 42-44), so detach it and answer from another shell.
- Gate commands (main.rs:384-457; project flag is `--project` for approve/reject, positional for show/list):
  - `devflow gate list <launch>` / `devflow gate show 1 --stage define <launch>`
  - `devflow gate approve 1 --stage define --note "attempt N: seeded checkpoint intentional" --project <launch>`
  - `devflow gate reject 1 --stage define --note "abort: <reason>" --project <launch>` (a note containing "abort" aborts; otherwise loop-back, gates.rs:73-83)
- `gate approve` refuses with no confirmed waiter holding the phase lock (commands.rs:1416-1424).
- Read-only liveness: `devflow status <launch>`, `devflow evidence --phase 1 --json --root <launch>` (main.rs:366-384), `ls <launch>/.devflow/gates/`.

### Pattern 3: Gate-open snapshot via `DEVFLOW_GATE_NOTIFY_CMD`

`DEVFLOW_GATE_NOTIFY_CMD='timeout 60 /abs/path/snapshot-hook.sh /abs/launch /abs/evidence'` exported into the launch environment (for `systemd-run`, `--setenv=`). The script: reads `DEVFLOW_GATE_PHASE`/`DEVFLOW_GATE_STAGE` from env (never interpolates `DEVFLOW_GATE_CONTEXT` into a command — it is agent-influenced text, gates.rs:398-402 comment), refuses unless the phase is `1` and `<launch>/.devflow/gates/01-<stage>.json` exists, copies into `evidence/gates/<seq>-<stage>/` with a sha256 manifest, appends one ledger line naming the snapshot path and hash (lesson from round-3 finding 3: consumers must read the snapshot the ledger names, not the lexicographically first), calls `notify-send`, and always exits 0 quickly. Hook failures are logged by DevFlow as a warning only (gates.rs:414-424), so the hook must write its own error file.

### Pattern 4: Criterion-by-criterion evidence sources

| Criterion | Evidence | Source |
|---|---|---|
| 1 `State::base_branch` = configured value | `jq -r .base_branch` on the Define-gate snapshot of `state-01.json` = `integration`; start stdout note naming `devflow.toml (base_branch)`; `git show integration:devflow.toml` | commands.rs:414, 417-427 |
| 2 fork point | D-16 triple: `git rev-parse integration` before `start`; worktree HEAD at the Define gate; `git merge-base feature/phase-01 integration` at the Ship gate snapshot | commands.rs:525-532 (worktree exists before `launch_stage`) |
| 2 `unattended_config_condition = Holds` | start stdout line `  [holds] GSD config can hold the chain flag` (Define; the Code-stage report goes to `/dev/null`, monitor.rs:445-447) and its absence from the Code gate's refusal list | preflight.rs:1161-1190 |
| 2 unattended progression | events: `transition` sequence, `gate_fired` only for define/code/ship, `gate_resolved.responded_by` = operator for define/code and `--yes-ship` for ship | pipeline_gate.rs:382-457 |
| 2 Ship merge target | Pattern in D-06 table | hooks.rs:180-233 |
| 3 negative arm | same binary sha; `base_branch: develop`; refusal text; no base note | D-14 answer |
| 4 DECN-03 | `checkpoint_auto_decided` event + resumed capture `result` + session transcript; classify per 47-PHASE49-OBSERVATION.md; record turn positions | Pattern 5 |
| 5 SURV-01 | watcher log analysis; `advance_failed` events | Pattern 6 |

### Pattern 5: DECN-03 classification inputs

Preconditions for the route (all Verified, pipeline_launch.rs:1777-1897): agent Claude; declared `gate="blocking-human"` in the worktree plans; capture reports a `**Gate:** blocking-human` line (agent_result.rs:617-760 — label-anchored, value exact, backticks and list/blockquote markup tolerated); `session_id` recorded (:1716-1719); `checkpoint_resumes < MAX_CHECKPOINT_RESUMES = 3` (mode.rs:106); declarations unchanged since the Code approval. Classification: **Followed** = event + resumed capture shows the concrete resolution + final message carries the option comparison + run progressed past the gate; **Not followed** = session ran, agent reported and halted; **Void** = only the event (spawn failed). Add a fourth recorded outcome: **path not exercised** (e.g. agent reported `success` at the checkpoint, or capture lacked the Gate line) — criterion 4 allows it; it is not one of the three classes.

### Pattern 6: SURV-01 observation

`save_state` writes `.state-01.json.<pid>.<seq>.tmp` then renames (workflow.rs:205-250). A python ctypes inotify watcher on `<launch>/.devflow/` logs `IN_CREATE`/`IN_MOVED_FROM`/`IN_MOVED_TO`/`IN_DELETE`/`IN_Q_OVERFLOW` with timestamps; interleaving = a create of another pid's temp between one pid's create and rename. Selftest with a synthetic two-writer overlap (must flag) and a sequential writer (must not). Report "no interleaving observed" as confirming nothing (REQUIREMENTS.md:220-223).

### Anti-Patterns to Avoid
- **Driving the run from a GSD plan task by task** — rejected by the re-scope (D-18, D-19).
- **Committing attempt evidence inside the checkpoint plan before its last task** — creates an illegal partial-plan state on a stopped attempt (execute-plan.md:16-20).
- **Deriving "shipped" from the PR or agent documents** — use DevFlow's events/`devflow evidence`, git, and `gh pr view`.
- **Setting `yes_ship = true` in `devflow.toml`** — attribution in the ledger would name config, not the typed flag.
- **Asserting on exit codes for the negative control** — assert the refusal text (memory: control must fail for the right reason).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Knowing a gate opened | tail/poll of `.devflow/gates/` | `DEVFLOW_GATE_NOTIFY_CMD` | Synchronous, fires before `--yes-ship` answers |
| Shipped oracle | Parse PR body / VERIFICATION.md | `devflow evidence --phase 1 --json --root <launch>` + events + git | Agent documents can be wrong (23a Critical) |
| Checkpoint-declaration parsing in tools | A new regex | Assert the exact opening line; the live Code gate's recorded set is the real parse | verify.rs anchors on the `<task` line only |
| GSD config resolution | Reasoning about defaults | `gsd-tools query config-get` / `loop render-hooks` probes in the scaffold selftest | Defaults changed between GSD versions; probes are cheap |
| Detached supervision | `nohup … &` from an agent shell | `systemd-run --user` from the operator's shell | Env leak; process lifetime |
| Version classification pre-check | A second classifier | Same breaking rule as preflight.rs:886-894, scaffold commits only; the real check is Ship preflight | Must not disagree with DevFlow |

## Runtime State the run creates (cleanup inventory)

| Category | Items | Action |
|----------|-------|--------|
| Stored data | Private GitHub repo per attempt (branches, PR, ship-note commit) | Operator deletes after Phase 49 verification (D-24, needs `delete_repo`) — one-way |
| Local project state | `<launch>/.devflow/` (events, history, gates, state), `<launch>/.worktrees/phase-01`, local tag `v0.1.0`, merged `integration` | Collected, then the attempt directory is deleted by the operator |
| Machine-global DevFlow state | `~/.cache/devflow` registry entry for the launch root (CONTEXT code_context; `registry::deregister` on ship/abort, pipeline_gate.rs:279, 469) | Verify deregistered; `devflow gate sweep` covers leftovers |
| Claude transcripts | `~/.claude/projects/<escaped worktree path>/` | Copied by the collector; ages out after 30 days |
| OS-registered state | `systemd-run --user` transient units (if used) | `systemctl --user stop/reset-failed <unit>` |
| Secrets/env | `DEVFLOW_GATE_NOTIFY_CMD` in the unit env; gh token (never copied) | None persistent |

## Common Pitfalls

### Pitfall 1: GSD's security ship gate halts `/gsd-ship` (Verified by probe)
**What goes wrong:** `ship:pre` has an active blocking `security` gate under a bare config; with no SECURITY.md it blocks with `SECURITY_SHIP_GATE_NO_REVIEW` (ship.md:116-121). **Avoid:** `workflow.security_enforcement: false`. **Warning sign:** Ship agent fails without a `review:` prefix.

### Pitfall 2: `/gsd-ship` requires VERIFICATION `passed` (Verified)
ship.md:58-62 — "Only `passed` may ship." A `human_needed` verdict (e.g. the verifier judging the checkpoint choice needs human sign-off) blocks. **Avoid:** automatable must-haves only; the chosen value is checked by test membership in the option set, not by a human.

### Pitfall 3: Dirty tracked config after the chain-flag round-trip (Verified mechanism, Reasoned consequence)
gsd_config.rs:133-178 rewrites via `to_string_pretty` + trailing newline whenever the value flips. A non-pretty or key-missing committed file becomes a diff; `/gsd-ship` preflight 2 then asks the (absent) user to commit or stash. **Avoid:** write the file in serde-pretty form with the key present; selftest a round-trip through `jq --indent 2` and byte-compare (Reasoned equivalence — the selftest must prove it).

### Pitfall 4: `external_verify` runs at every stage (Verified)
Pitfall stated in Q1. **Warning sign:** Define fails with `decided_by_layer: 0`.

### Pitfall 5: `devflow.toml` not in the launch checkout's working tree (Verified)
`load_config` reads the working-tree file. Launching from a checkout on `main` (without `devflow.toml`) silently resolves `develop` (Default source) — and then fails `ensure_phase_reachable_on_base` or forks from the wrong base. **Avoid:** launch checkout on `integration`; the pre-launch check asserts `test -f <launch>/devflow.toml`.

### Pitfall 6: Ship review Critical from pipeline claims (Verified precedent)
23a: `review: CR-01 (Critical) — 01-VERIFICATION.md scores Observable Truth #3 'Define→Plan→Execute→Ship carried end-to-end' as VERIFIED …` (23-PROBE-FINDINGS.md:80), followed by three Validate `gaps` and a gate. **Avoid:** payload docs make no pipeline or external-state claims (Ship angle "external-state claims", prompt.rs:12-18).

### Pitfall 7: Global git signing and hooks apply inside the disposable repo (Verified config)
Global `commit.gpgsign=true`, `tag.gpgsign=true`, `gpg.format=ssh`, `user.signingkey` set, `core.hooksPath=~/.config/git/hooks` (a gitleaks pre-commit). DevFlow tags with `-c tag.gpgSign=false` but merge and CHANGELOG commits would sign in the headless monitor. **Avoid:** repo-local `commit.gpgsign false` and `tag.gpgsign false` in the scaffold (repo-local, consistent with T-23-02). Leave the global hook (gitleaks) — harmless, and it adds a secret scan on agent commits.

### Pitfall 8: Delivery canary refuses the first launch (Verified mechanism, unobserved on this CLI version)
`canary_gate` runs once per run on the first stream launch and refuses on `Absent`/`Unverified` ("last observed on claude_code_version 2.1.220", pipeline_launch.rs:545-558); host has 2.1.283. **Warning sign:** `start` errors after the Define approval with the canary text. It is a D-10 blocking defect or an environment finding, not a payload issue.

### Pitfall 9: `cargo doc` in DocsUpdate walks up to an ancestor `Cargo.toml` (Reasoned)
Place the disposable project under a directory with no ancestor `Cargo.toml` (e.g. `~/.cache/devflow-phase49/…`; `~/` itself has none — check in the pre-launch script).

### Pitfall 10: Retention evicts turn 1 (Verified arithmetic)
12 generations default (config.rs:13-36). **Avoid:** `capture_retention = 64`.

### Pitfall 11: The stray `49.1` phase directory (Verified)
`.planning/phases/49.1-configurable-test-wait-budget/49.1-01-PLAN.md` (from the superseded set, commits `72e3f29`, `8760677`) is still on disk. D-01/D-02 are superseded and 999.55 stays in the backlog; the planner should remove it alongside the 18 superseded 49-NN plans so no `/gsd-execute-phase 49.1` can run it.

### Pitfall 12: ROADMAP Phase 49 `**Plans**` list and "Post-completion integration gate" are stale (Verified)
ROADMAP.md:281-349 still describe the 18-plan set and a 49.1 landing hop; plan-phase regenerates the list; the integration-gate paragraph references 49.1 and must be rewritten or removed with the amendments.

## Code Examples

### Positive-arm launch (runbook, Reasoned shape; verify before use)
```bash
# Source: main.rs:49-116 (start flags); config_parse.rs:20-44 (gate timeout)
systemd-run --user --unit "p49-attempt-${N}" -p RemainAfterExit=yes -p KillMode=process \
  --setenv=DEVFLOW_GATE_NOTIFY_CMD="timeout 60 ${TOOLS}/snapshot-hook.sh ${LAUNCH} ${EV}" \
  --setenv=PATH="${PATH}" --setenv=HOME="${HOME}" \
  -- sh -c 'exec "$0" start --phase 1 --agent claude --mode auto --yes-ship "$1" >"$2/start.stdout" 2>"$2/start.stderr"' \
  "${BIN}" "${LAUNCH}" "${EV}"
```

### Negative-arm assertion core (text, not exit code)
```bash
# Source: preflight.rs:993-996, 1166, 1188, 940; commands.rs:417-425
grep -F -- '[DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live' "$EV/negctl/start.stdout"
! grep -F -- 'note: base branch is' "$EV/negctl/start.stdout"
test "$(jq -r .base_branch "$SNAP/state-01.json")" = develop
```

## State of the Art

| Old approach | Current approach | When changed | Impact |
|--------------|------------------|--------------|--------|
| Live run inside this repo, 18 lockstep plans | Disposable project, one human-action checkpoint per attempt | 2026-09-28 re-scope (`968b713`, `066251f`) | Removes undo/containment/landing tooling |
| Human Ship gate (D-07) | `--yes-ship` (D-23) | 2026-09-28 | Ship gate still fires and is snapshotted |
| Polling gate watcher | Notify-hook snapshot + post-run collector | This research | Captures the auto-answered Ship window |

**Deprecated/outdated:** the superseded plans' `undo-ship-hooks.sh`, `ship-accommodations.sh`, `archive-attempt.sh`, `secret-guard.sh`'s public-PR arm, and the 49.1 promotion.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The GSD continuation executor, given a `stopped: …` reply to the human-action checkpoint, fails the task and writes no SUMMARY when the plan says so | D-27 | GSD would mark the attempt plan complete and not re-present it; mitigated by an evidence-file acceptance criterion |
| A2 | `claude -p --resume <sid>` appends to the same session transcript (no fork) on 2.1.283 | Discretion answer | Collector would need to follow a new session id from the resumed capture |
| A3 | The operator's gh token carries `repo` (not checked: needs a network call) | Q3 | `gh repo create --private` or `/gsd-ship`'s push fails; runbook pre-flight catches it |
| A4 | `git branch -d` of a branch checked out in a linked worktree fails and BranchCleanup only warns | D-06 table | If it deleted the branch, the merge-base observation must happen from the Ship snapshot (planned anyway) |
| A5 | serde_json pretty output equals `jq --indent 2` output for the recommended config | Pitfall 3 | A dirty tracked file blocks `/gsd-ship`; the selftest must prove byte-equality through DevFlow's own writer, not jq |
| A6 | GSD's verifier returns `passed` (not `human_needed`) for a phase containing an agent-resolved blocking-human decision | Pitfall 2 | Ship blocks → third gate; record as a finding, fix in the payload |
| A7 | The illustrative option values (30/120/300 s) | Pattern 1 | None — any values work; the test accepts the offered set |

## Open Questions (RESOLVED)

1. **Does a Ship loop-back or Validate `gaps` still count toward D-22's "exactly two human gates"?** — What we know: those paths add a never-silent gate. Recommendation: the checker reports gate count and cause; the judging plan treats an extra gate as a recorded deviation, and D-11 counts only runs that end without reaching Ship.
   **RESOLVED — recommendation adopted:** 49-08's `--arm positive` prints `NOTE gate_census` and one `NOTE deviation` line per gate beyond Define, Code and Ship, which never count as failures (R-6), and its `--attempt-gate` reaches `cap_reached` only from three attempts that ended stopped (D-11); 49-10's runbook records each unexpected gate in `deviations.txt`; 49-12 lists every deviation with its cause.
2. **Where the tools live.** `.planning/phases/49-…/tools/` (superseded precedent) keeps them out of `develop` (cut-pr-branch filters `.planning/`); the scaffold extension in `scripts/` does reach `develop`. Recommendation: keep that split; the scaffold change must leave default output byte-identical (its existing callers include the negative arm).
   **RESOLVED — recommendation adopted:** the tools live under `.planning/phases/49-…/tools/` (49-03, 49-04, 49-05, 49-07, 49-08, 49-09); the scaffold extension stays in `scripts/scratch-dogfood-repo.sh` (49-02, 49-06), and 49-02 Task 2 keeps its default mode byte-identical.
3. **DECN-03 "Followed" needs the comparison reasoning in the final message.** If the resumed turn resolves the decision but writes terse reasoning, the classifier needs a rule for "partially followed." Recommendation: record verbatim and classify by the three-way standard only; flag borderline cases for the operator rather than inventing a fourth class (the "path not exercised" outcome is separate and allowed by criterion 4).
   **RESOLVED — recommendation adopted:** 49-12 classifies by the three-way standard plus Path not exercised, quotes the reasoning and flags borderline cases for the operator, and adds no fourth class.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `claude` | Agent | ✓ | 2.1.283 | — |
| GSD core | Agent workflows | ✓ | 1.14.0 | — |
| `gh` | Repo create, PR, Ship preflight | ✓ (auth scopes not checked) | 2.101.0 | — (blocking if unauthenticated) |
| `git` credential helper via gh | `/gsd-ship` push | ✓ | — | — |
| `python3` | Payload test, SURV watcher | ✓ | 3.14.7 | `/usr/bin/python3` (3.14) |
| `cargo` | Building the pinned binary | ✓ | 1.97.1 | — |
| `systemd-run --user` | Detached launch | ✓ (user manager `degraded`) | — | plain terminal + second shell |
| `notify-send` | Gate notification | ✓ | — | ledger file only |
| `shellcheck`, `bats`, `ruff` | Tool tests | ✓ | ruff 0.16.9 | — |
| `inotifywait` | SURV watcher | ✗ | — | python ctypes inotify |
| `mypy` | Python typing | ✗ | — | ruff only |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `inotifywait` → ctypes; `mypy` → ruff.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Script selftests (`--selftest` printing `  ok   <case>` lines and `SELFTEST passed=N failed=M`), `bash -n` + `shellcheck`, `ruff check`/`ruff format --check`, `python3 -m py_compile`; Rust suites only if a defect fix touches `crates/` |
| Config file | none — each tool carries its own fixtures in temp dirs |
| Quick run command | `bash <tool> --selftest` / `python3 <tool> --selftest` (each < 30 s) |
| Full suite command | all tool selftests + `bash scripts/scratch-dogfood-repo.sh --selftest`; if `crates/` changed: `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && scripts/check.sh all` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VERIFY-01 (setup) | Scaffold `--live-payload` produces the layout, branches, serde-pretty config, devflow.toml, blocking-human opening line, RED test that fails before and passes after a reference implementation; default mode byte-identical | selftest | `bash scripts/scratch-dogfood-repo.sh --selftest` | ❌ Wave 0 |
| VERIFY-01 (setup) | GSD resolves the config as intended (security gate off, nyquist on, verifier on) | selftest probe | inside the scaffold selftest: `gsd-tools loop render-hooks ship:pre --raw` has no `security`; `verify:post` has `nyquist` | ❌ Wave 0 |
| VERIFY-01 crit 3 | Negative arm refuses on the verbatim text; base `develop`; no base note | live (cheap, no agent spawned) + driver selftest | `bash tools/negctl.sh --selftest`; live run in the attempt | ❌ Wave 0 |
| VERIFY-01 crit 1-2 | Checker passes on a known-good fixture evidence set and fails each criterion on a mutated one | selftest | `bash tools/check-evidence.sh --selftest` | ❌ Wave 0 |
| VERIFY-01 (capture) | Snapshot hook copies the named files, refuses a foreign phase, never reads `DEVFLOW_GATE_CONTEXT` into a command, exits 0 on errors | selftest | `bash tools/snapshot-hook.sh --selftest` | ❌ Wave 0 |
| crit 5 | Watcher flags a two-writer overlap, not a sequential writer; overflow = void window | selftest | `python3 tools/surv-watch.py --selftest` | ❌ Wave 0 |
| D-26 | Sanitizer removes home paths/username/token shapes, keeps a known-safe sample | selftest | `bash tools/sanitize-evidence.sh --selftest` | ❌ Wave 0 |
| VERIFY-01 crit 1-5 | Live attempt | manual (checkpoint:human-action) | — operator runbook | n/a |
| VERIFY-01 judgement | Checker over real evidence; DECN-03 and SURV-01 classification recorded | automated over evidence | `bash tools/check-evidence.sh <evidence-dir>` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** the touched tool's `--selftest` plus `shellcheck`/`ruff`.
- **Per wave merge:** every tool selftest.
- **Phase gate:** all selftests green, the attempt evidence checked, then `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `scripts/scratch-dogfood-repo.sh --selftest` (new) — covers the setup step
- [ ] `tools/snapshot-hook.sh`, `tools/collect-attempt.sh`, `tools/negctl.sh`, `tools/check-evidence.sh`, `tools/surv-watch.py`, `tools/sanitize-evidence.sh` — each with positive and negative cases
- [ ] Every PLAN `<automated>` block that uses bash-only syntax wrapped in `bash -c '…'` (pre-commit enforced)

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (gh token) | Existing gh keyring/credential helper; never print or copy the token; record only `gh auth status` scope lines |
| V3 Session Management | no | — |
| V4 Access Control | yes | Private repo (`--private`), verified via `gh repo view --json visibility` before any push |
| V5 Input Validation | yes | Agent-influenced `DEVFLOW_GATE_CONTEXT` never interpolated into shell; gate reasons already sanitized/truncated (pipeline_outcomes.rs:867-900) |
| V6 Cryptography | no | — |
| V7 Error handling / logging | yes | Hook errors to its own file; evidence sanitized before commit (D-26) |
| V14 Configuration | yes | Repo-local git config only; no `--global` writes; `DEVFLOW_BASE_BRANCH` scrubbed for the negative arm |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Scaffold writes into a checkout | Tampering | Existing absolute-prefix refusal (scratch-dogfood-repo.sh:44-58); extend to the main checkout as unattended-drill.sh does (:62-70, 119-139) |
| Evidence leaks home path, username, session ids, tokens into this repo | Information disclosure | Sanitizer + committed-evidence scan (D-26); tracked precedent 999.69 |
| Wrong repository deleted at cleanup | Tampering | `gh repo delete owner/repo --yes` only with the recorded `nameWithOwner`; operator-run |
| Command injection via gate context in the notify hook | Elevation | Read env var as data only; never `eval`; bounded by `timeout` |
| Nested agent inherits the operator's Claude session env | Spoofing | Launch via `systemd-run` or a plain terminal |

## Sources

### Primary (HIGH confidence — read or probed this session)
- DevFlow source: `crates/devflow-cli/src/{preflight.rs, pipeline_launch.rs, pipeline_outcomes.rs, pipeline_gate.rs, commands.rs, config_parse.rs, main.rs, staleness.rs}`, `crates/devflow-core/src/{config.rs, hooks.rs, git.rs, version.rs, verify.rs, gates.rs, prompt.rs, stage.rs, mode.rs, monitor.rs, agent_result.rs, gsd_config.rs, workflow.rs, worktree.rs, agents/claude.rs, agents/mod.rs}`, `Cargo.toml`
- `scripts/scratch-dogfood-repo.sh`, `scripts/unattended-drill.sh`, `scripts/lint-plan-bashisms.sh`
- GSD 1.14.0: `workflows/{validate-phase.md, execute-phase.md, execute-plan.md, ship.md, code-review.md}`, `workflows/execute-phase/steps/{protected-branch.md, completion-reconciliation.md}`, `references/{checkpoints.md, planner-preconditions.md}`, `templates/VALIDATION.md`, `bin/lib/git-base-branch.cjs`, `agents/gsd-executor.md`
- GSD probes (scratch dirs): `loop render-hooks` for all points under `{"workflow":{}}` vs negative controls; `query init.execute-phase 1` (`branch_name= feature/phase-01`, `verifier_enabled= true`); `query config-get` defaults
- Host config: global git config, `~/.config/git/hooks/pre-commit`, `~/.config/gh/hosts.yml` (no token read), `~/.claude/settings.json` hooks list, `~/.gsd/defaults.json`
- Planning: 49-CONTEXT.md, REQUIREMENTS.md, ROADMAP.md, STATE.md, 47-PHASE49-OBSERVATION.md, 48-CONTEXT.md, 23-PROBE-FINDINGS.md, 35.1-DRILL.md, reviews/49-adversarial-2026-09-27/SUMMARY.md, the superseded 49-RESEARCH.md and 49-0{2..8}/49-16 plans

### Secondary (MEDIUM — official docs)
- [cli.github.com/manual/gh_auth_login](https://cli.github.com/manual/gh_auth_login) — default scopes
- [cli.github.com/manual/gh_repo_delete](https://cli.github.com/manual/gh_repo_delete) — `delete_repo`
- [docs.github.com — Scopes for OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps) — `repo`
- [code.claude.com/docs/en/sessions](https://code.claude.com/docs/en/sessions) — transcript path, `--fork-session`, 30-day retention

### Tertiary (LOW)
- None used for decisions.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — host tools probed; no installs
- Architecture / topology: HIGH — read in DevFlow and GSD source; key GSD defaults probed with negative controls
- Pitfalls: HIGH for mechanism (source/probe), MEDIUM for live agent behaviour (A1, A2, A6)
- Validation architecture: MEDIUM — follows the superseded plans' selftest precedent; not yet exercised

**Research date:** 2026-09-28
**Valid until:** 2026-10-12 (GSD and the claude CLI move fast; re-run the config probes in the scaffold selftest before each attempt)
