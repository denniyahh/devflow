## CONFIRMED

### 1. Critical: Task 2 Automated Gate Deadlock & Summary Inversion in Plan 49-18
- **Severity**: Critical
- **Citations**: [49-18-PLAN.md:193-200](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-18-PLAN.md#L193-L200), [`execute-plan.md:182-187`](.claude/gsd-core/workflows/execute-plan.md#L182-L187), [`:229-235`](.claude/gsd-core/workflows/execute-plan.md#L229-L235), [`:406-412`](.claude/gsd-core/workflows/execute-plan.md#L406-L412), [`execute-phase.md:327`](.claude/gsd-core/workflows/execute-phase.md#L327)
- **Concrete Failure**:
  - `49-18-PLAN.md` Task 2's automated verify command enforces:
    ```bash
    S="$R/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-18-SUMMARY.md"
    ...
    tr=$(rg --count-matches -F "## graphify triage ($h12)" "$S" 2>/dev/null || true); tr=${tr:-0}
    echo "triage_section_for_merged_head=$tr"; [ "$tr" -eq 1 ] || rc=1
    ```
  - In GSD Core execution, `SUMMARY.md` is strictly forbidden from being authored before all tasks in the plan are completed (`execute-plan.md:182`: *"Critical ordering — write and commit SUMMARY.md as one atomic block. Do NOT emit narrative output between the Write tool call and the commit tool call... After ALL segments: aggregate files/deviations/decisions → create SUMMARY.md"*).
  - While Task 2 is running, Task 3 has not started. `49-18-SUMMARY.md` does not exist on disk. `tr` evaluates to 0, and the automated gate exits 1.
  - Per `execute-plan.md:229-235`, `<acceptance_criteria>` is a hard gate. The executor cannot proceed to Task 3 (the workspace sync and push to `origin/workspace/denniyahh`).
  - If an executor cheats and creates a draft `49-18-SUMMARY.md` during Task 2 to bypass this gate, any subsequent interruption or failure in Task 3 causes GSD's phase execution engine (`execute-phase.md:327`: *"Filtering: Skip plans where has_summary: true"*) to permanently skip Plan 49-18 on `/gsd-execute-phase 49`, permanently stranding Task 3.
- **Proof**: Verified by inspecting GSD plan execution ordering against `49-18-PLAN.md:193` and testing the gate script against a non-existent summary in a clean environment.

---

### 2. High: Same-N Re-presentation Refusal on Already-Closed Preflight Gates
- **Severity**: High
- **Citations**: [49-10-PLAN.md:144, 160-170](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md#L144), [49-11-PLAN.md:133-145](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-PLAN.md#L133-L145), [49-12-PLAN.md:160-165](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md#L160-L165), [49-13-PLAN.md:95-110](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md#L95-L110), [`execute-plan.md:228`](.claude/gsd-core/workflows/execute-plan.md#L228), [`crates/devflow-core/src/gates.rs:216-218`](crates/devflow-core/src/gates.rs#L216-L218)
- **Concrete Failure**:
  - `49-10-PLAN.md:164-167` claims that under the "same-N re-presentation" rule:
    > "a checkpoint task checks its verification line before it is presented. If the check already holds, the step is done... without repeating the action or asking the operator again."
  - In GSD Core (`execute-plan.md:228`: `- type="checkpoint:*": STOP -> checkpoint_protocol -> wait for user -> continue only after confirmation`), checkpoints are unconditional halts. The executor displays the checkpoint instructions and blocks on human input before evaluating verification.
  - Unlike `49-10` Task 1 (which explicitly placed conditional bypass code inside `<action>`), `49-11` Task 2, `49-12` Task 2, and `49-13` Task 1 have raw `<action>` blocks instructing the executor to present the `gate.sh approve` command.
  - When any of these plans are re-presented on the same attempt (which occurs after any mid-plan failure, `continue:`, or interrupted session), the checkpoint is presented to the operator.
  - If the operator re-runs `gate.sh <N> approve <stage>` on an already-resolved gate, `Gates::respond` in DevFlow core immediately errors with `GateError::NoOpenGate` (`gates.rs:217`):
    ```rust
    if !Self::gate_path(project_root, phase, stage).exists() {
        return Err(GateError::NoOpenGate { phase, stage });
    }
    ```
  - The CLI exits non-zero, derailing the re-presented execution.
- **Proof**: Verified `Gates::respond` return semantics on missing gate paths in `crates/devflow-core/src/gates.rs:216-218` and verified checkpoint halt semantics in `execute-plan.md:228`.

---

### 3. High: False Same-N Re-presentation Classification in 49-10 Task 1 Precondition
- **Severity**: High
- **Citations**: [49-10-PLAN.md:194](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md#L194), [49-08-PLAN.md:223-225](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md#L223-L225), [49-08-PLAN.md:421-423](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md#L421-L423)
- **Concrete Failure**:
  - `49-10-PLAN.md:194` determines whether attempt N is a same-N re-presentation via:
    ```xml
    <precondition>N is the attempt this task stands for. While current-attempt names an attempt that is launched and not archived ($HOME/.cache/devflow-phase49/attempt-<N>/attempt.json exists and refs/archive/phase49/attempt-<N>/feature-phase-49.1 does not), N is that attempt: a same-N re-presentation...
    ```
  - `49-08-PLAN.md:223-225` and `archive-attempt.sh` step 7 explicitly specify that if an attempt ended before `feature/phase-49.1` was created (e.g. prelaunch refusal, binary check failure, or early launch crash), `feature-branch.txt` reads `ABSENT` and NO archive git ref is written.
  - In that scenario, `archive-attempt.sh` runs to completion and archives the attempt (`attempt-<N>/archive/MANIFEST.sha256` is verified). However, `refs/archive/phase49/attempt-<N>/feature-phase-49.1` does not exist.
  - On the next `/gsd-execute-phase 49`, `49-10` Task 1's precondition evaluates: `attempt.json` exists AND the archive ref does not exist. It concludes that attempt N is an active "same-N re-presentation", rather than an archived attempt requiring N+1.
  - In Task 1 `<action>`, it skips presenting the launch command and evaluates its verification line: `systemctl --user is-active p49-a<N>-gatewatch prints active`.
  - Because `archive-attempt.sh --stop-unit` stopped the units, this check prints `inactive` and fails. Task 1 cannot proceed.
- **Proof**: Traced the absence of archive ref generation for early-failure runs in `49-08-PLAN.md:223-225` against the definition of "same-N re-presentation" in `49-10-PLAN.md:194`.

---

### 4. High: `undo-ship-hooks.sh` Layout Re-execution Refusal on Same-N Re-presentation
- **Severity**: High
- **Citations**: [49-13-PLAN.md:149-152](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md#L149-L152), [49-05-PLAN.md:93-108](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-05-PLAN.md#L93-L108)
- **Concrete Failure**:
  - In `49-13-PLAN.md` Task 2, `undo-ship-hooks.sh` is invoked to undo the post-Ship commits (`VersionBump`, `ChangelogAppend`, and tag `v3.0.0`):
    `bash $TOOLS/undo-ship-hooks.sh --root $ROOT --ev "$EV" --attempt "$N" --base-ref workspace/denniyahh --merge <M> --pre-hook-head <main_head> --tag v3.0.0 --expect-version 2.12.0`
  - In `49-05-PLAN.md:93-108`, `undo-ship-hooks.sh` enforces strict layout guards before performing any mutation:
    `HEAD~2 == --merge`, `<tag>^{commit} == HEAD~1`, and `git diff --name-only <merge> HEAD` sorted is exactly `CHANGELOG.md` and `Cargo.toml`.
    On success, it deletes `v3.0.0` and runs `git reset --hard <merge>`.
  - If Task 2's automated gate fails (e.g. `evidence-oracle.json` check or diff capture) or Task 3 fails (e.g. network timeout during `gh pr close` or watcher stop issue), Plan 49-13 is re-presented from Task 1 on the same attempt N.
  - When Task 2 runs again, `HEAD` is already at `--merge` (`M`). `HEAD~2` is `M~2`, and tag `v3.0.0` no longer exists.
  - `undo-ship-hooks.sh` immediately fails its layout guards and exits non-zero.
  - `49-13-PLAN.md:150` explicitly states: *"If it refuses, do not improvise another undo. Record the refusal text and fail this task's gate."*
  - While Task 3 added explicit same-N checks for `revert` and `gh pr close`, Task 2 has NO same-N bypass check for `undo-ship-hooks.sh`, permanently locking the plan in a failing state.
- **Proof**: Verified git state transitions in a fixture: once reset to `M` and `v3.0.0` deleted, testing `HEAD~2 == M` and `git rev-parse v3.0.0^{commit}` returns non-zero.

---

### 5. High: Blind Approval of Define Gate on Code-Stop Retry
- **Severity**: High
- **Citations**: [49-10-PLAN.md:142-148](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md#L142-L148), [49-11-PLAN.md:100-112](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-PLAN.md#L100-L112), [49-RESEARCH.md:124-128](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RESEARCH.md#L124-L128)
- **Concrete Failure**:
  - On attempt 1, the Define preflight gate cannot be approved by the operator until `49-10` Task 2 executes and validates that the binary hash is fresh, the negative-control pair passed (`check-evidence.sh --pair` and `--arm positive`), the watcher units are active, and the Define gate context carries the seeded checkpoint refusal without the config condition.
  - On a retry after a Code stop, `49-10-PLAN.md:142-148` instructs the operator to run `launch-attempt.sh <N+1>` outside GSD and approve the Define gate immediately when the notification arrives:
    `bash $HOME/.cache/devflow-phase49/attempt-<N+1>/tools/gate.sh <N+1> approve define --note ...`
    The operator resumes GSD (`/gsd-execute-phase 49`) only after the *Code* preflight gate arrives.
  - The plan defers checking Attempt N+1's Define launch evidence, binary hash, negative control, and Define preflight context to a post-hoc catch-up step in `49-11` Task 1.
  - Input: Attempt N+1 suffers a corrupt binary build, fails its negative control arm, or trips an unintended preflight condition at Define.
  - Outcome: The operator approves Define outside GSD without any verification tool having run against Attempt N+1. DevFlow advances through Define and Plan into Code before the defect is ever discovered.
- **Proof**: Derived directly from the operator workflow sequence defined in `49-10-PLAN.md:142-148`.

---

### 6. Medium: Unhandled Validate Stage Stop in Stop Routing vs 49-15 Consistency Gate
- **Severity**: Medium
- **Citations**: [49-10-PLAN.md:107-108, 127](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md#L107-L108), [49-15-PLAN.md:190](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-15-PLAN.md#L190), [`crates/devflow-core/src/mode.rs:316`](crates/devflow-core/src/mode.rs#L316)
- **Concrete Failure**:
  - In `49-10-PLAN.md:107`, step 3 requires: *"From the captures, name the stage reached..."*. But step 5 (line 127) specifies the stop record schema as:
    `stage: <the stage of the last gate_open line in $EV/ledger.jsonl>, which 49-15 cross-checks`
  - In DevFlow Auto mode (`mode.rs:316`), `Stage::Validate` is autonomous and has no preflight gate (`assert!(!Mode::Auto.should_gate(Stage::Validate, 0, 0))`).
  - If an attempt fails during the Validate stage (e.g. test failure or test loopback limit), the stage reached is `validate`, but the last `gate_open` in `ledger.jsonl` is `code`.
  - If `outcome.md` truthfully records `stage: validate`, `49-15-PLAN.md:190` (`[ "$so" = "$sl" ]` where `sl` is the stage of the last `gate_open` line in `ledger.jsonl`) fails with `gate_rc=1`. If `outcome.md` writes `stage: code`, it misrepresents the failure stage and contradicts step 3.
  - Furthermore, if an attempt crashes or is aborted before the Define gate opens (e.g. prelaunch refusal or startup crash), `ledger.jsonl` contains zero `gate_open` entries. `sl` evaluates to empty, causing `[ -n "$so" ] && [ "$so" = "$sl" ]` in 49-15 to fail unconditionally.
- **Proof**: Executed fixture test in `mktemp -d`:
  - With `stage: validate` in `outcome.md` and `code` as the last ledger gate, `[ "$so" = "$sl" ]` evaluated to FALSE.
  - With `stage: prelaunch` in `outcome.md` and an empty ledger, `[ -n "$so" ] && [ "$so" = "$sl" ]` evaluated to FALSE.

---

### 7. Medium: Snapshot Gate Readers Decoupled from Ledger and Vulnerable to Stale Ordering
- **Severity**: Medium
- **Citations**: [49-10-PLAN.md:255](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md#L255), [49-11-PLAN.md:123](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-PLAN.md#L123), [49-03-PLAN.md:125-132](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-PLAN.md#L125-L132)
- **Concrete Failure**:
  - `ctx()` in 49-10 Task 2 and 49-11 Task 1 resolves gate context using:
    `find "$EV/gates" -mindepth 3 -maxdepth 4 -path "*/$1-*/devflow/*" -name "49.1-$1.json" -type f 2>/dev/null | sort | head -1`
  - While `gate-watch.sh` records the exact snapshot path and sha256 in `$EV/ledger.jsonl` for each `gate_open` event (`49-03-PLAN.md:129`), `ctx()` completely ignores the ledger.
  - Because `ctx()` selects the lexicographically first directory matching `*/$1-*/devflow/*`, if multiple snapshots exist (e.g. across preflight loopbacks, refreshed requests, or retries), `ctx()` always reads the earliest snapshot directory.
  - If a later snapshot contains an invalid context or leaked config condition, `ctx()` continues to pass by inspecting the earlier snapshot.
- **Proof**: Executed fixture test in `mktemp -d`: Planted `define-100` (valid context) and `define-200` (bad context containing `"GSD config can hold the chain flag"`, recorded in ledger). The plan's `ctx define` selected `define-100` and reported `config_condition_in_refusal=0` (PASS).

---

### 8. Medium: Asymmetric Acceptance Gate in 49-15 Allows False `NOT_MET` Verdicts
- **Severity**: Medium
- **Citations**: [49-15-PLAN.md:190](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-15-PLAN.md#L190)
- **Concrete Failure**:
  - `49-15-PLAN.md:190` validates C1-C3 check lines with:
    ```bash
    [ "${tk[1]}" != MET ] || [ "$o1" -eq 0 ] || rc=1;
    [ "${tk[2]}" != MET ] || [ "$o2" -eq 0 ] || rc=1;
    if [ "${tk[3]}" = MET ]; then [ "$o3" -eq 0 ] || rc=1; else [ "$o3" -ne 0 ] || rc=1; fi;
    ```
  - For C3, the gate implements a true two-sided negative control (`else [ "$o3" -ne 0 ] || rc=1`).
  - For C1 and C2, the check only asserts that if `tk` is `MET`, the check file passed. If `tk[1]` or `tk[2]` is marked `NOT_MET`, `[ "${tk[1]}" != MET ]` evaluates to TRUE, bypassing the check file entirely.
  - If an executor or operator mistakenly records C1 or C2 as `NOT_MET` when the check file actually passed (`CHECK passed=3 failed=0`), the automated gate returns `gate_rc=0`.
- **Proof**: Executed fixture test in `mktemp -d`: A check file ending in `CHECK passed=3 failed=0` paired with `C1: NOT_MET` exited with `rc=0`. The identical scenario for C3 exited with `rc=1`.

---

### 9. Medium: Catch-Up Regex in 49-11 Task 1 Rejects Standard Markdown Lists
- **Severity**: Medium
- **Citations**: [49-11-PLAN.md:123](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-PLAN.md#L123)
- **Concrete Failure**:
  - In `49-11-PLAN.md:123`, the catch-up parser extracts the attempt recorded in `49-10-SUMMARY.md` using:
    `A=$(rg -o -r "\$1" -e "^[ >]*current_attempt=([1-3])[ ]*$" "$S10" 2>/dev/null | sort -u)`
  - If `49-10-SUMMARY.md` quotes the gate output using standard markdown list bullets (e.g. `- current_attempt=1` or `* current_attempt=1`), the leading character does not match `^[ >]*`.
  - `na` evaluates to 0, triggering:
    `catch_up=UNREADABLE: 49-10-SUMMARY.md must quote exactly one current_attempt value`
    and failing the gate with `rc=1`.
- **Proof**: Tested regex in bash against `- current_attempt=1`: 0 matches returned.

---

## SUSPECTED

### 10. Low (Speculation): Duplicate `record-secure` Appends on 49-12 Task 2 Re-presentation
- **Severity**: Low
- **Citations**: [49-12-PLAN.md:165](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md#L165), [49-06-PLAN.md:185-192](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md#L185-L192), [49-06-PLAN.md:238-239](.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md#L238-L239)
- **Mechanism**:
  - In `49-12` Task 2, `record-secure` is run after committing `49.1-SECURITY.md`. It inspects `apply_sha..HEAD` and appends `{"kind":"record-secure", "shas":[...]}` to `accommodations.jsonl`.
  - If 49-12 is re-presented on the same attempt, Task 2 instructs the operator to run `record-secure` again. Because `record-secure` does not check for an existing `record-secure` record, it appends a second identical entry.
  - In `49-06-PLAN.md:238`, `revert` iterates through recorded security commit SHAs: `for the recorded SECURITY commit SHAs (newest first, as one revert commit)... git revert --no-commit <sha>...`.
  - If `shas` are deduplicated before reverting, it succeeds; if passed duplicated SHAs to `git revert`, git may reject the second revert of the same commit.
- **Speculation Label**: Labeled speculation because `ship-accommodations.sh` has not yet been implemented (it is authored in Wave 0, Plan 49-06), so its exact argument handling for duplicate entries depends on the implementation.

---

## Direct Answers to Highest-Value Questions

### 1. Define Stop and Code Stop Trace Under GSD Resume
- **Define Stop**:
  - Stops at 49-10. Operator archives via `archive-attempt.sh <N> --stop-unit`.
  - *Where it breaks*: If the attempt stopped before `feature/phase-49.1` existed, `archive-attempt.sh` writes no archive ref (`49-08:224`). 49-10 Task 1's precondition (`49-10:194`) falsely classifies it as a same-N re-presentation instead of launching N+1, tests whether the stopped gatewatch unit is active, and fails permanently (Finding 3).
- **Code Stop**:
  - Stops at 49-11. Operator archives and lands fix. Operator launches N+1 outside GSD and blindly approves Define preflight gate before any verification tool runs (Finding 5).
  - Operator re-runs `/gsd-execute-phase 49`. GSD skips 49-10 (has summary) and resumes at 49-11.
  - *Where it breaks*: 49-11 Task 1 catch-up regex (`49-11:123`) fails if 49-10's summary formatted the attempt in a standard markdown list (Finding 9). If 49-11 halts mid-plan, re-presentation halts at Task 2 and prompts the operator to approve Code preflight again, triggering `GateError::NoOpenGate` (Finding 2).

### 2. Same-N Re-presentation Rule Edge Cases
- **Skipping what must be redone**: In 49-10 Task 1, if an early launch failed and was archived without an archive ref, it skips the launch command because it misclassifies the state as same-N.
- **Redoing what must not repeat**:
  - `gate.sh approve` in 49-11 Task 2, 49-12 Task 2, and 49-13 Task 1 is re-presented because GSD checkpoints halt unconditionally, failing with `GateError::NoOpenGate` (Finding 2).
  - `undo-ship-hooks.sh` in 49-13 Task 2 runs unconditionally without a same-N bypass check; when re-run after a reset, it fails its layout guards and halts (Finding 4).

### 3. Snapshot-Based Gate Reads vs Live `gate.sh show`
- Snapshot reads (`49-10:255`, `49-11:123`) do **NOT** prove what the live check proved.
- They select `sort | head -1` across directories matching `*/$1-*/devflow/*` without binding to the ledger's recorded snapshot or content hash.
- In fixture testing, when `define-100` was present alongside `define-200` (the actual gate recorded by the ledger), `ctx()` evaluated `define-100`, ignoring the ledger and passing even when `define-200` contained a breaking config condition (Finding 7).

### 4. Gates That Pass Broken Work or Fail Correct Work
- **Fails Correct Work**:
  - `49-18-PLAN.md:193` Task 2 gate: requires `49-18-SUMMARY.md` before it is allowed to exist under GSD execution rules (Finding 1).
  - `49-15-PLAN.md:190`: fails any attempt that stopped during the autonomous `Validate` stage or before Define gate opened (Finding 6).
- **Passes Broken Work**:
  - `49-15-PLAN.md:190`: passes if an attempt falsely reports C1 or C2 as `NOT_MET` despite all tests passing (Finding 8).
  - `49-15-PLAN.md:190`: passes an `outcome.md` with fabricated `plan`, `cause`, `disposition: success`, or `counts_as_attempt` fields as long as `stage` matches the ledger.

### 5. Irreversible / Outward-Facing Steps Missing Immediate Pre-Command Guards
- **`49-11` Code-Stop Retry Define Approval**: `49-10-PLAN.md:144` tells the operator to approve the Define preflight gate outside GSD without running any pre-command automated assertions against Attempt N+1's binary or negative control (Finding 5).
- **`49-18` PR Merge**: In `49-18-PLAN.md:184-186`, `gh pr merge --match-head-commit` guards the commit SHA, but the automated gate requiring the graphify triage heading in SUMMARY deadlocks before the merge can be acknowledged by GSD (Finding 1).

---

## What I Did NOT Check
- Did NOT run `devflow`, `systemd-run`, `systemctl`, `journalctl`, `podman`, `docker`, or full cargo builds (per hard limits).
- Did NOT test live GitHub CLI (`gh`) operations against remote repositories.
- Did NOT execute tool scripts from Wave 0 (`launch-attempt.sh`, `archive-attempt.sh`, `gate-watch.sh`, `undo-ship-hooks.sh`, `ship-accommodations.sh`) as they do not exist in the snapshot; their behavior was audited exclusively from their plan specifications in `49-02` through `49-08`.
