---
phase: 49-live-unattended-run-the-milestone-s-instrument
reviewed: 2026-10-09T00:59:54Z
depth: deep
files_reviewed: 15
files_reviewed_list:
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/staleness.rs
  - scripts/scratch-dogfood-repo.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/check-evidence.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/snapshot-hook.sh
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/sanitize-evidence.py
  - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py
findings:
  critical: 2
  warning: 6
  info: 5
  total: 13
status: issues_found
---

# Phase 49: Code Review Report

**Reviewed:** 2026-10-09T00:59:54Z
**Depth:** deep
**Files Reviewed:** 15
**Status:** issues_found

## Summary

Scope: the two merged crate fixes (`git diff 2d28e3f bbfe6df -- crates`: e3f9ddd checkpoint-in-marker detection, 084625c persisted notify command + Ship PR target) traced into their consumers (`pipeline_launch.rs` GateReview arm, `relaunch_checkpoint_session`, `fire_gate_notify` call sites, monitor spawn), plus the phase-49 scripts and tools as they stand.

Two blocking problems:

1. **e3f9ddd adds a free-text substring heuristic to the trigger for checkpoint auto-decide.** The module's own contract says D-02 rejected that kind of predicate. A probe build (a scratch copy of HEAD with extra tests, not committed) confirmed that ordinary failure reasons route into an Auto-mode checkpoint resume. One example is the prose the auto-decide prompt itself asks the agent to write.
2. **The merged crate commits fail `cargo fmt --check`** (gates.rs, state.rs). That breaks `scripts/check.sh all` and the pre-push container gate. Neither commit is on any remote yet.

On the specific focus questions:

- **Serde back-compat of `gate_notify_cmd`.** Correct by construction. It has `#[serde(default)]` plus `skip_serializing_if = "Option::is_none"`, and `State` has no `deny_unknown_fields`. Old state files deserialize to `None`, and older binaries ignore the new key. No test pins this. Verified by reading the code, not by a test.
- **Revert-sensitivity.** The new agent_result tests fail if e3f9ddd is reverted, because the positive case has no `Gate:` line and only the marker path matches it. Reverting the state/monitor/pipeline_gate/staleness half of 084625c fails **no** test (WR-02).
- **What passed.** `cargo clippy --workspace --all-targets -- -D warnings` passed on a clean HEAD snapshot. The targeted lib tests passed (20/20). Neither shows that the heuristic is correct, only that it compiles and that the cases its author chose pass.

Known tool defects 999.147–999.150 were not re-reported: sanitizer email redaction vs JSON escapes, collected `.gitignore *`, the checker hiding a repeated gate, and DECN-03 turn placement read from uncommitted transcripts.

## Critical Issues

### CR-01: Marker-reason substring heuristic auto-resumes ordinary failures as blocking-human checkpoints

**File:** `crates/devflow-core/src/agent_result.rs:762-782` (consumed at `crates/devflow-cli/src/pipeline_launch.rs:1777-1893`)
**Issue:** `marker_reports_human_gate` returns true for any `failed` DEVFLOW_RESULT whose free-text `reason` contains the substring `blocking-human` and any one of `checkpoint`, `gate` or `halted`. `gate` is also a substring of ordinary words such as "investigate", "delegate", "aggregate" and "gateway". The check is wired into `text_reports_human_gate`, so it runs for every top-level stream `result`, for every decoded JSON string in a non-stream capture, and for every plain line. The GateReview arm then auto-resumes the session with `checkpoint_auto_decide_prompt` ("You previously stopped at a blocking-human gate … resolve it yourself") for **any stage**, including Validate, instead of taking the per-stage failure path. It does this up to `MAX_CHECKPOINT_RESUMES` (3) times, emits a `checkpoint_auto_decided` audit event for a checkpoint that was never reached, and only then gates with a misleading "resume ceiling exhausted" reason.

Probe results (scratch copy of HEAD with extra `#[test]`s, run through `blocking_human_checkpoint_reported`):

| Failed-marker reason | Detected as checkpoint |
|---|---|
| `Resolved the blocking-human gate by choosing budget-120; cargo test then failed` | **true** |
| `no blocking-human checkpoint was reached; tests failed` | **true** |
| `verification failed: SUMMARY does not record the blocking-human decision; investigate` | **true** (only via "investi**gate**") |
| `cargo test failed in integration suite` (control) | false |
| `halted at a blocking human checkpoint (Task 1)` | false (a false negative, which is the safe direction) |

The first row is exactly the prose `checkpoint_auto_decide_prompt` (`prompt.rs:616-628`) tells the agent to write: "refer to the gate you resolved in plain prose". The prompt's design note (`prompt.rs:602-608`) relies on prose like `Resolved gate: blocking-human` **not** being detected. After one auto-decide, any later unrelated failure that mentions the resolved gate re-triggers auto-decide. The code also contradicts the function's own documented contract at `agent_result.rs:638-642`: "deliberately does NOT widen into a general 'does this look like a checkpoint' heuristic (D-02 rejected that class of predicate)".

`claude_stream_reports_human_gate` (`agent_result.rs:1564-1571`) uses `.any` over **all** top-level results. An earlier turn's failed marker therefore still counts after the session moved on; the probe confirmed this. The new heuristic widens that exposure.

The static `phase_has_blocking_human_checkpoint` guard does not bound this. It is true for the whole phase whenever any plan declares such a task, and that is exactly when agents are primed by `CODE_STAGE_POLICY` to write "blocking-human" in their reasons.

**Fix:** Replace the prose heuristic with a structured, agent-declared field, and accept it only from the final top-level result. For example, extend the marker schema and the completion protocol:
```rust
// AgentResult: add `#[serde(default)] pub checkpoint: Option<String>`
// COMPLETION_PROTOCOL: "If you stopped at a blocking-human checkpoint, emit
//   DEVFLOW_RESULT: {\"status\":\"failed\",\"checkpoint\":\"blocking-human\",\"reason\":\"...\"}"
fn marker_reports_human_gate(text: &str) -> bool {
    parse_marker_lines(text).is_some_and(|r| {
        r.status == AgentStatus::Failed && r.checkpoint.as_deref() == Some(HUMAN_GATE_VALUE)
    })
}
```
If a structured field is not acceptable, at minimum require an anchored token at the start of `reason` (e.g. `reason.starts_with("checkpoint: blocking-human")`), drop the bare `gate` and `halted` alternatives, and add regression tests for the three false-positive rows above. Update the `blocking_human_checkpoint_reported` doc comment and the `prompt.rs:602-608` note so they no longer claim prose is never matched.

### CR-02: Merged fix commits fail `cargo fmt --check` and block the push gate

**File:** `crates/devflow-core/src/gates.rs:402` and `:955-961`, `crates/devflow-core/src/state.rs:603`
**Issue:** `cargo fmt --all -- --check -l` on HEAD lists `gates.rs` and `state.rs`, with 3 diff hunks. `scripts/check.sh all` runs `cargo fmt --check` first (`scripts/check.sh:31-32`), and so does the pre-push container gate. Commit 084625c introduced both files' diffs. `git branch -r --contains 084625c` is empty, so the fixes have not been through the gate yet. The branch cannot be pushed as it stands.
**Fix:**
```bash
cargo fmt --all && cargo fmt --all -- --check && git commit -am "style(49): rustfmt the notify-fallback fix"
```

## Warnings

### WR-01: Persisted notify command has inconsistent precedence and cannot be disabled for an in-flight run

**File:** `crates/devflow-core/src/gates.rs:394-407`, `crates/devflow-core/src/monitor.rs:447-449,572-574`, `crates/devflow-core/src/state.rs:429-432,603`
**Issue:**
- **(a) An empty env var no longer disables notification.** `fire_gate_notify_with_fallback` treats an empty `DEVFLOW_GATE_NOTIFY_CMD` the same as an unset one and falls back to the persisted command. Before this change, an empty value was the documented way to disable notification (`gates.rs:383`). Now the only way to stop or rotate a bad hook for a running phase is to hand-edit `.devflow/state-NN.json`.
- **(b) Precedence differs between the foreground and the monitor chain.** In-process, the env var wins over the state value. But `spawn_monitor_inner` calls `.env("DEVFLOW_GATE_NOTIFY_CMD", state value)` *after* `.envs(...)`, so the state value overrides whatever the operator's environment now holds. Every `devflow advance` the monitor runs, which is where Auto-mode gates fire, uses the stale persisted command even after the operator exports a corrected one.
- **(c) The constructor reads the environment.** `State::new` reads the process environment, so every test that calls `State::new` (dozens across both crates, including `tests/*.rs` e2e binaries) silently captures whatever hook the developer's shell exports.
- **(d) The state file is now an executable input.** `.devflow/state-NN.json` used to hold data only. It now holds a shell string that DevFlow runs via `sh -c`, and that string may embed webhook tokens.

**Fix:** Capture the value at `devflow start` in `commands.rs` (the only production `State::new` caller), not in the constructor. Give the env var three states, so that set-and-empty disables:
```rust
let cmd = match std::env::var_os("DEVFLOW_GATE_NOTIFY_CMD") {
    Some(v) if v.is_empty() => None,                    // explicit disable
    Some(v) => Some(v.to_string_lossy().into_owned()),  // operator override
    None => fallback_cmd.filter(|s| !s.is_empty()).map(str::to_owned),
};
```
In `monitor.rs`, inject the persisted value only when the parent's env var is absent (`std::env::var_os(..).is_none()`), so that both paths use the same precedence.

### WR-02: Nothing fails if the notify-persistence wiring is reverted; the workflow Ship prompt change is untested

**File:** `crates/devflow-core/src/state.rs:603`, `crates/devflow-core/src/monitor.rs:447,572`, `crates/devflow-cli/src/pipeline_gate.rs:400-406`, `crates/devflow-cli/src/staleness.rs:467-473`, `crates/devflow-core/src/prompt.rs:571-574`
**Issue:** `gate_notify_cmd` appears only in production code; `rg` finds no test that references it. The only new notify test (`gates.rs:946-963`) calls the helper directly with an explicit `Some(&cmd)`. Reverting any of the following leaves the suite green:
- the `State::new` capture
- either monitor `.env` injection
- the two call sites switching to the fallback variant

That is the exact live-run defect this commit claims to fix. No test round-trips the new field or loads a pre-084625c state file. Back-compat is correct by reading, but nothing pins it. `ship_prompt_instructs_pr_targeting_main` covers `ship_stage_prompt` only. The `workflow_ship_prompt` edit (`prompt.rs:571-574`) is not asserted anywhere.
**Fix:** Add three tests:
- `run_gate_with_timeout` with the env var removed and `state.gate_notify_cmd = Some("touch <sentinel>")`, asserting the sentinel appears. Include a negative control with `None` that asserts it does not.
- A state.rs test that deserializes a fixture JSON without `gate_notify_cmd` (expects `None`) and round-trips `Some`.
- A workflow-root Ship prompt test that asserts the PR-target sentence.

### WR-03: Ship prompt hard-codes `main` and a fixture-flavoured branch name for every project

**File:** `crates/devflow-core/src/prompt.rs:276-279`, `:571-574`
**Issue:** Every DevFlow project's Ship prompt now says "target base branch `main` … while DevFlow merges into integration locally". The literal duplicates `config::MAIN` instead of interpolating it. "integration" is the phase-49 scratch repo's branch name; in a default project the base is `develop` or a configured `base_branch`. An agent can read the bare word as a branch name. The instruction also overrides a project's own GSD `git.base_branch`. The phase-49 scaffold already sets that key to `main` for exactly this purpose (`scripts/scratch-dogfood-repo.sh:997,1020`), and the prompt text gives the agent no basis to prefer one over the other.
**Fix:** Interpolate the constant and the resolved base:
```rust
format!("When creating the pull request, target base branch `{main}`; DevFlow merges the phase into `{base}` locally.",
        main = crate::config::MAIN, base = resolved_base_branch)
```
State in the doc comment why the PR base overrides GSD's `git.base_branch`, or drop the override and rely on the GSD config.

### WR-04: Evidence sanitizer passes AWS keys, Slack webhooks, PEM private keys and JWTs

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/sanitize-evidence.py:52-75`
**Issue:** The docstring promises "anything that looks like a credential stops the publish", and this tool is "the only path by which attempt evidence enters the repository". The prefix list covers only `sk-`, `gh[pousr]_` and `Bearer`, and named assignments need a KEY/TOKEN/SECRET/PASSWORD name bound with `:` or `=`. A file containing an `AKIA…` key, a `hooks.slack.com/services/T…/B…/…` URL, an OpenSSH PEM private-key block (header line `BEGIN OPENSSH PRIVATE KEY` between five-dash fences) and an `eyJ…` JWT scanned **clean** (`scan` exit 0). A planted `ghp_` token in the same run was refused (exit 1), so the probe does discriminate.
**Fix:** Add at least these patterns to `PREFIX_PATTERNS`, with planted-value selftest cases built from fragments as the existing cases are:
```python
("aws_access_key", re.compile(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b")),
("slack_token_or_webhook", re.compile(r"\bxox[abposr]-[A-Za-z0-9-]{10,}|hooks\.slack\.com/services/[A-Za-z0-9/]{20,}")),
("private_key_block", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
("jwt", re.compile(r"\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}")),
```

### WR-05: `launch-attempt.sh start` wedges the attempt and orphans the watcher if the run unit fails to launch

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh:323-330`
**Issue:** The order is:
1. start the `p49-aN-surv` unit,
2. create the `start.stdout` relaunch sentinel,
3. launch the run unit.

If step 3's `systemd-run` fails (a timeout, or a leftover failed unit with the same name, which `negctl.sh` checks for but `start` does not), `set -e` exits. The watcher is then left running with no run to watch, and every later `start` refuses with `already launched`. The only recovery is deleting the sentinel and stopping the surv unit by hand.
**Fix:** Check `systemctl --user is-active` for both unit names up front. Launch the run unit first, or trap a launch failure to `systemctl --user stop "p49-a$attempt-surv"` and remove `start.stdout`. Create the sentinel only after the run unit's `systemd-run` succeeds.

### WR-06: `surv-watch.py analyze` reports interleaving against dead writers' orphaned temp files

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py:298-323`
**Issue:** A state temp window opens on `create` or `initial` and closes only on `moved_from` or `delete`. In two cases the window never closes:
- A writer killed between `create_new` and `rename` (SIGKILL on the monitor, which the leaked-process history makes plausible).
- A stale `.state-01.json.<pid>.<seq>.tmp` present when the watch attaches (`initial`).

Every later create by another pid is then counted as an `overlap`, which inflates `interleaved_windows`. That answers SURV-01's question ("two processes held an open write at the same time") with a false positive. The tool states it is observation-only, but the count it prints is wrong.
**Fix:** Do not let `initial` records open windows; record them as `stale_at_attach`. Report windows that never close as `unclosed` in a separate field, and do not count them in `interleaved_windows`. Optionally record the creator pid's liveness from `/proc/<pid>` at create time, so a dead holder is visible in the analysis.

## Info

### IN-01: Merged and stale doc comments around the checkpoint matcher

**File:** `crates/devflow-core/src/agent_result.rs:604-615,708-732,777`
**Issue:** The original doc comment of `text_reports_human_gate` (`:708-729`) now sits directly above a second summary paragraph, and both attach to `line_reports_human_gate`. `text_reports_human_gate` (`:777`) has no doc comment at all. The `HUMAN_GATE_VALUE` doc still points readers to "[`text_reports_human_gate`]'s trim set", which now lives in `line_reports_human_gate`.
**Fix:** Move the long block back onto `line_reports_human_gate` without the duplicate summary. Document `text_reports_human_gate` as "line declaration OR marker" and update the cross-references.

### IN-02: `fire_gate_notify` has no production callers left

**File:** `crates/devflow-core/src/gates.rs:381-390`
**Issue:** Both production call sites now use `fire_gate_notify_with_fallback`, and `fire_gate_notify` is referenced only by `gates.rs:942` (a test). A future caller that picks the shorter name silently loses the persisted fallback.
**Fix:** Remove it and point the test at the fallback variant with `None`, or mark it `#[cfg(test)]`.

### IN-03: A kill between `mv` and the ledger append wedges every later snapshot

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/snapshot-hook.sh:266-290`
**Issue:** `seq` is derived from the ledger line count. If `timeout 60` or a crash lands after `mv "$tmp" "$dir"` (`:283`) but before the ledger append (`:290`), every later gate computes the same `seq` and fails "snapshot directory already exists without a ledger line". The window is milliseconds wide.
**Fix:** Derive `seq` from `max(ledger count, highest existing gates/NNN-*) + 1`, or append the ledger line before the `mv` and mark it pending.

### IN-04: `prepare` pins `$repo/target/debug/devflow` regardless of the configured target dir

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh:231-232`
**Issue:** If `CARGO_TARGET_DIR` or `build.target-dir` is set, the build lands elsewhere. A stale `target/debug/devflow` would then be pinned while `identity.txt` records `build_commit=HEAD`. Neither is set on this host today; checked with `env` and `~/.cargo/config.toml`.
**Fix:** Take the binary path from `cargo build --message-format=json-render-diagnostics` (the `executable` field), or from `cargo metadata --format-version 1 | jq -r .target_directory`.

### IN-05: `collect-attempt.sh` publishes `verdict.json` before the collection directory

**File:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh:348-353`
**Issue:** `verdict.json` is moved into place before `collected.tmp` is renamed to `collected`. If the second `mv` fails, `launch-attempt.sh prepare` treats the attempt as judged (it gates only on `verdict.json`), even though no finished collection exists.
**Fix:** Rename `collected.tmp` first and write `verdict.json` last, or have `prepare` also require `collected/DONE`.

---

_Reviewed: 2026-10-09T00:59:54Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
