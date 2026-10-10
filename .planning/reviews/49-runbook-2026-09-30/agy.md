# Adversarial Review: Phase 49 Operator Runbook & Supporting Harness

## CONFIRMED BLOCKERS

### 1. Dead-Run Collection Deadlock When Process Dies at an Open Gate
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:210-216, 220-234, 242-246`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L210-L246)
  - [`crates/devflow-cli/src/commands.rs:1415-1425`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/crates/devflow-cli/src/commands.rs#L1415-L1425)
- **Failure Condition & Reproducer:**
  1. A run dies, crashes, or is killed (e.g. OOM, host sleep, signal) while waiting at an open gate (e.g., `Define` or `Code`), leaving `.devflow/gates/01-<stage>.json` on disk.
  2. If the operator attempts the runbook's abort instruction ([`49-RUNBOOK.md:212`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L212)):
     ```bash
     "$BIN" gate reject 1 --stage "$STAGE" --note "abort: describe observed blocking defect" --project "$EV/project"
     ```
     `commands.rs:1415-1425` executes:
     ```rust
     let holder_before_response = lock::holder_status(project_root, phase);
     let ship_without_holder =
         stage == Stage::Ship && matches!(holder_before_response, lock::HolderStatus::NoHolder);
     if !holder_before_response.may_be_waiting() && !ship_without_holder {
         return Err(CliError::Message(format!(
             "no confirmed waiter holds phase {phase}'s lock for {stage}; no response was written. {}",
             no_waiter_repair(phase, stage)
         )));
     }
     ```
     Because the waiter process is dead and `stage != Stage::Ship`, `gate reject` errors with `"no confirmed waiter holds phase 1's lock for <stage>; no response was written"`. The gate file is never answered, removed, or closed.
  3. The operator must then fall back to "If the run dies without reaching Ship" ([`49-RUNBOOK.md:218-246`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L218-L246)), but that section commands:
     - Line 220: *"Confirm all three: no workflow_shipped, no waiting gate, and no surviving monitor or agent..."*
     - Line 232: *"If no gate is waiting, no workflow_shipped exists and no process survives in the run unit, record the death..."*
     - Line 242: *"Never pass --allow-state while the monitor or agent is alive or a gate is parked."*
     - Lines 245–246: *"Do not resume DevFlow inside the attempt: resumed progression is no longer unattended, and recovery drops the attempt's provenance."*
  4. The operator cannot reject the gate, cannot satisfy "no waiting gate", is forbidden from passing `--allow-state` while a gate is parked, and is forbidden from running `devflow resume` or `recover`. The operator is completely deadlocked from collecting or recording the dead attempt.

---

### 2. Pre-Monitor Crash Diagnostic Fails and Misclassifies Valid State as Corrupted
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:225, 231-232`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L225-L232)
  - [`crates/devflow-cli/src/commands.rs:693-705`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/crates/devflow-cli/src/commands.rs#L693-L705)
  - [`crates/devflow-core/src/state.rs:288, 584`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/crates/devflow-core/src/state.rs#L288-L584)
- **Failure Condition & Reproducer:**
  1. In `commands.rs:693`, `devflow start` saves state (`workflow::save_state(&state)`) *before* `launch_stage()` spawns a detached monitor. At this point, `state.monitor_pid` is initialized to `None` (`state.rs:584`), and serialized to `.devflow/state-01.json` as `"monitor_pid": null`.
  2. If the `p49-a$N-start` unit fails or dies prior to monitor spawn (e.g. killed during Define preflight checks or crashing before line 700), `state-01.json` exists with `"monitor_pid": null`.
  3. The dead-run diagnostic in [`49-RUNBOOK.md:225`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L225) executes:
     ```bash
     MONITOR=$(jq -er '.monitor_pid | select(type == "number")' "$EV/project/.devflow/state-01.json")
     ps -ww -o pid=,lstart=,args= -p "$MONITOR"
     ```
  4. Because `.monitor_pid` is `null`, `jq -er` outputs nothing and exits with status 4. In Bash under `set -e`, execution aborts immediately; in an interactive shell, `MONITOR` is left empty, causing the subsequent `ps -p ""` to error with `ps: missing list of PIDs`.
  5. The runbook then states ([`49-RUNBOOK.md:231-232`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L231-L232)):
     *"Treat missing or malformed state as missing evidence to diagnose, not zero PID or death."*
     The operator is instructed to treat completely valid state (`monitor_pid: null` before monitor spawn) as "malformed state / missing evidence" and forbidden from recording the run's death.

---

## CONFIRMED WARNINGS

### 3. Contract Omission: `$EV/surv/analysis.txt` Promised but Never Generated
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md:23`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md#L23)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py:21-22, 56, 384-385`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py#L21-L385)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:156-158, 237`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L156-L237)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh:311-328`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh#L311-L328)
- **Failure Condition & Reproducer:**
  1. `EVIDENCE-LAYOUT.md:23` defines the mandatory attempt evidence schema:
     `- $EV/surv/ — watch.jsonl, analysis.txt (from surv-watch.py).`
  2. `surv-watch.py` defines the entry point:
     `analyze <out_dir>: Reads <out_dir>/watch.jsonl and publishes <out_dir>/analysis.txt.`
  3. No script (`launch-attempt.sh`, `collect-attempt.sh`, `check-evidence.sh`) and no runbook step ever runs `python3 "$T/surv-watch.py" analyze "$EV/surv"`. Both Section 7 ([`49-RUNBOOK.md:157`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L157)) and the dead-run path ([`49-RUNBOOK.md:237`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L237)) only stop the watcher unit (`systemctl --user stop "p49-a$N-surv"`).
  4. In every attempt, `$EV/surv/analysis.txt` is missing on disk, violating the published evidence contract.

---

### 4. Contract Drift: Real `attempt.json` Omits Promised `created_at` Field
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md:15`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md#L15)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh:236-238`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh#L236-L238)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh:420-422`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh#L420-L422)
- **Failure Condition & Reproducer:**
  1. `EVIDENCE-LAYOUT.md:15` specifies the exact JSON schema for `attempt.json`:
     `{"attempt":N,"bin":"<abs>","bin_sha256":"<64 hex>","build_commit":"<40 hex>","launch":"<abs $EV/project>","base_branch":"integration","created_at":<epoch>}`
  2. In `launch-attempt.sh:236-238`, `prepare` produces `attempt.json` via:
     ```bash
     jq -n --argjson attempt "$attempt" --arg bin "$binary" --arg bin_sha256 "$sha" --arg launch "$project" \
         --arg build_commit "$(git -C "$repo" rev-parse HEAD)" \
         '{attempt:$attempt,bin:$bin,bin_sha256:$bin_sha256,build_commit:$build_commit,launch:$launch,base_branch:"integration"}' >"$ev/attempt.json"
     ```
     `created_at` is omitted entirely.
  3. In contrast, `collect-attempt.sh:420-422`'s selftest fixture includes `"created_at": 1` because the mock author followed the spec, masking the fact that the live generator produces a non-compliant JSON object.

---

### 5. Orphaned Active Unit and Lingering Children on Stop / Death
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:156-158, 236-238, 293`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L156-L293)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh:309-311, 317-320`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh#L309-L320)
- **Failure Condition & Reproducer:**
  1. `launch-attempt.sh:309` launches `p49-a$attempt-start` with:
     ```bash
     systemd-run --user --unit "p49-a$attempt-start" -p RemainAfterExit=yes -p KillMode=process ...
     ```
  2. On normal collection ([`49-RUNBOOK.md:157`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L157)) and dead-run collection ([`49-RUNBOOK.md:237`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L237)), the operator only runs:
     ```bash
     systemctl --user stop "p49-a$N-surv"
     ```
     `p49-a$N-start` is never stopped or reset.
  3. Because `RemainAfterExit=yes` is set, `p49-a$N-start` remains permanently `active (exited)` in systemd throughout stop handling and retries. `launch-attempt.sh status` reports the unit as `active`.
  4. Because `KillMode=process` is configured, if the main shell process exited or was killed while child processes (e.g. background agents or subprocesses) were running, systemd leaves the cgroup intact. Without `systemctl --user stop "p49-a$N-start"`, untracked child processes remain active during subsequent retry attempts.

---

### 6. Cleanup Step `gate sweep --root` is a Guaranteed No-Op
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:292-295`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L292-L295)
  - [`crates/devflow-cli/src/commands.rs:1513-1575, 1737-1739`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/crates/devflow-cli/src/commands.rs#L1513-L1739)
- **Failure Condition & Reproducer:**
  1. In Section 10 Cleanup ([`49-RUNBOOK.md:292-295`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L292-L295)), the operator runs:
     ```bash
     systemctl --user stop "p49-a$N-start" "p49-a$N-surv"
     systemctl --user reset-failed "p49-a$N-start" "p49-a$N-surv"
     "$BIN" gate sweep --root "$EV/project"
     ```
  2. In `commands.rs:1519, 1552-1555`, `threshold` defaults to `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` (3 days / 259,200s). Any gate created during the attempt has `age < threshold` and is unconditionally skipped (`left_alone += 1; continue`).
  3. Even if `--max-age-secs 0` were passed, line 1557 checks `gate_sweep_may_reap(gate.stage, holder_before_reap)`. Line 1738 requires `matches!(holder, lock::HolderStatus::Live { .. })`. Because the previous line stopped the systemd units, there is no live waiter (`NoHolder`), so `gate_sweep_may_reap` returns `false` and leaves the gate untouched.
  4. Furthermore, passing `--root` causes `gate_sweep` to bypass `registry::prune_missing()`.
  5. The command prints `"left phase ... alone — no confirmed waiter"` and performs zero cleanup.

---

## SUSPECTED

### 7. Inconsistent `--scaffold` Resolution on Fix-Branch Retries
- **Files & Lines:**
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md:267`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L267)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh:179, 192, 280`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh#L179-L280)
  - [`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh:166`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh#L166)
- **Failure Condition & Reproducer:**
  1. [`49-RUNBOOK.md:267`](file:///tmp/p49-runbook-review-wqtn6cmh/review-snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L267) prepares a fix-branch attempt with:
     ```bash
     bash "$T/launch-attempt.sh" prepare --attempt "$N" --repo "$FIX" --scaffold "$W/scripts/scratch-dogfood-repo.sh"
     ```
  2. Neither `--scaffold` nor `--repo` is persisted into `$EV/attempt.json`.
  3. When the operator runs `bash "$T/negctl.sh" --attempt "$N"` (Section 2), `negctl.sh:166` defaults `scaffold="$ROOT/scripts/scratch-dogfood-repo.sh"`.
  4. When the operator runs `bash "$T/launch-attempt.sh" start --attempt "$N"` (Section 4), line 280 executes `bash "$scaffold" --check "$project"`, where `scaffold` defaults to `$repo/scripts/scratch-dogfood-repo.sh` and `repo` defaults to `$ROOT`.
  5. If `$FIX` required changes to the scaffold script (e.g. payload tweaks, GSD config adjustments, or check assertions), `prepare` used `$FIX`, but `negctl.sh` and `start` revert to running the unpatched scaffold from `$ROOT`.

---

## LIMITS & UNPROVEN BOUNDARIES

### Paths Verified Offline
- **Evidence Verification Rules (`check-evidence.sh`):** Verified offline that `check-evidence.sh` strictly enforces:
  - Fork-point triplet consistency: `base_sha` (prelaunch), `worktree_head` (define snapshot), and `merge_base` (ship snapshot) must be identical.
  - Integration branch topology: exactly one merge commit in `integration`'s first-parent history since `base_sha`, with parent 1 being `feature/phase-01`, and `main` left untouched.
  - PR metadata: PR targets `main` from `feature/phase-01`.
  - Negative control assertions: verification of the verbatim refusal text, develop branch provenance, and `workflow_aborted` event presence.
- **Negative Control Isolation (`negctl.sh`):** Verified that `negctl.sh` isolates the negative run using `--no-notify` for `snapshot-hook.sh`, overrides `DEVFLOW_BASE_BRANCH` with `develop`, and deletes `.planning/config.json`.
- **Sanitization Correctness (`sanitize-evidence.py`):** Verified that regex patterns correctly match standard bearer, OpenAI, and GitHub tokens, that username and home directory paths in both POSIX and URL-escaped forms are transformed, and that the selftest passes against planted values.
- **CLI Verbs & Flags Alignment:** Verified that `gate list`, `gate show`, `gate approve`, `gate reject`, `gate sweep`, `status`, `evidence`, and `start` argument signatures match `devflow-cli/src/main.rs`.

### What Remains Unproven
- **Live Systemd Transient Unit & cgroup Accounting:** Offline inspection cannot verify whether `systemd-run --user` behaves consistently across user session restarts or resource limits (memory/CPU cgroup limits) on Kinoite/Fedora Blue under real process load.
- **Anthropic `apiKeyHelper` Daemon Interaction:** No real Anthropic API authentication or Claude CLI stream-json interaction was executed; offline review cannot prove that the background helper daemon returns credentials before the 180s probe timeout expires on a fresh login session.
- **GitHub API Rate Limits / Transient Network Errors:** Behavior under network timeouts during `gh repo create`, `gh repo view`, or `gh pr view` cannot be proven without live network access.
- **Accepted Deferral:** As documented in `49-08-PLAN.md`, the `cap_reached` provenance limitation is explicitly deferred and represents phase failure, not VERIFY-01 success.
