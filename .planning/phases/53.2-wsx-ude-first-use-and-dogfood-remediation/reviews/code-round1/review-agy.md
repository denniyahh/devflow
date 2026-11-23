## CONFIRMED

### 1. [CRITICAL] `#285` warning fires BEFORE almost every `start` and `start --dry-run` refusal, contradicting `OPERATIONS.md` and its own comment
- **Location:** [commands.rs:400-427](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L400-L427), [commands.rs:679-704](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L679-L704), [commands.rs:834-925](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L834-L925), [OPERATIONS.md:36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/OPERATIONS.md#L36)
- **Quoted text:**
  - `OPERATIONS.md:36`: `After its refusals, start (a dry run too) warns on stderr, and does not refuse, when git does not ignore GSD's derived files`
  - `commands.rs:400-408`:
    ```rust
    // #285 (D-13): a warning, never a refusal, said by the dry run and the real start
    // alike, after every refusal above so a refused start prints none.
    if let Some(warning) = gsd_derived_paths_warning(project_root) {
        eprintln!("{warning}");
    }

    if dry_run {
        return start_dry_run(project_root, &state);
    }
    ```
- **Failure scenario:** The only refusal above line 402 is `validate_fallback_agent`. Every actual start/dry-run refusal — `refuse_without_a_user_manager`, `claim_foreground_holder`, `launch_guards` / `reserve_launch` (live, finished, incomplete, unloadable, orphan gates, leftover worktree/branch), `ensure_agent_binary`, `ensure_ask_registration`, `resolve_start_base`, `ensure_base_ref_current`, `ensure_phase_reachable_on_base`, `start_requires_phase_context`, and `--reviews`'s `REVIEWS.md` check — executes *after* line 402. Any refused `start` or `start --dry-run` on a repo lacking the `.gitignore` lines prints the warning anyway. None of the `#285` tests ([phase7_cli.rs:1229-1306](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/phase7_cli.rs#L1229-L1306)) tests a refused start as a negative control.
- **Fix shape:** Move `gsd_derived_paths_warning` to immediately before `print_dry_run` inside `start_dry_run` (after `resolve_start_base`) and after the final refusal inside `start_reserved` (after line 925, before worktree/branch mutation), and add a negative-control test asserting a refused `start` prints no `.gitignore` warning.

---

### 2. [CRITICAL] `re_mark_stopped_after_failed_launch` (`#338` WR-01 / `#258`) lies when the runner already stopped, mutates a changed state while reporting "it was kept", and corrupts `LaunchError::Failed` when `written` is `None`
- **Location:** [pipeline_launch.rs:2228-2275](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/pipeline_launch.rs#L2228-L2275), [pipeline_launch.rs:2300-2331](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/pipeline_launch.rs#L2300-L2331)
- **Quoted text:**
  ```rust
  let err = CliError::Message(format!("resume: phase {phase} was not launched: {why}"));
  let mut kept = false;
  let _ = commit(state, |s| {
      if s.stopped {
          return;
      }
      let untouched = written.is_some() && written == serde_json::to_value(&*s).ok().as_ref();
      s.stopped = true;
      if reviews && untouched {
          s.stop_reason = Some(crate::pipeline_gate::until_halt_reason(Stage::Plan));
          s.reviews_pass = false;
      } else {
          s.stop_reason = Some(format!("resume launch failed: {err}"));
          kept = !untouched;
      }
  });
  if kept {
      return CliError::Message(format!(
          "resume: phase {phase} was not confirmed launched ({why}), and its run state \
           changed after resume wrote it, so it was kept: run `devflow status` to inspect \
           it, and `devflow discard --phase {phase}` to remove it"
      ));
  }
  err
  ```
- **Failure scenario:**
  1. **Runner ran, recorded its own stop (e.g., `--until` halt or normal finish), and exited before `read_timed_out_launch`:** `s.stopped` is `true`, so line 2311 returns early with `kept == false`. `resume` returns `"resume: phase N was not launched: <why>"` — falsely telling the operator nothing launched and omitting `devflow status` / `devflow discard`. (`a_timed_out_resume_keeps_the_until_halt_its_runner_recorded` at [pipeline_launch.rs:2354](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/PHASE-53.2-CODE.diff#L2354) only asserts `result.is_err()`, never checking the error text.)
  2. **Runner ran, changed state (e.g. set `session_id` or advanced stage), and crashed (`s.stopped == false`):** `untouched` is `false`, so lines 2314 and 2319 **overwrite** `s.stopped = true` and `s.stop_reason = Some("resume launch failed: resume: phase N was not launched: ...")` — turning a crashed run into a stopped run whose reason claims it never launched — while line 2326 simultaneously tells the operator `"its run state changed after resume wrote it, so it was kept"`.
  3. **`LaunchError::Failed` with `written == None` (transient read failure at line 2228) or failed `commit`:** `written.is_some()` is `false` -> `untouched` is `false` -> even when `ensure_agent_binary` or `archive_prior_capture` failed before `launch_runner_unit` ever ran (lines 2238-2242), `resume --reviews` refuses to restore the `--until plan` halt and emits `"its run state changed after resume wrote it"`. If `commit` itself returns `Err`, `let _ =` discards it, leaving `stopped == false` on disk while returning `"was not launched"`.
- **Fix shape:** Split `LaunchError::Failed` (nothing launched; unconditionally restore the `--until plan` halt on `reviews`) from `TimedOutLaunch::NotLive`. Serialize `written` directly from the `State` returned by `resume_loaded` (no redundant disk load that can yield `None`). In `NotLive`, set `kept = true` whenever `s.stopped` is already true OR `!untouched`; if `!untouched && !s.stopped`, either leave disk untouched (matching `start`'s `not_live_after_timed_out_launch`) or write a non-contradictory stop reason instead of `"resume launch failed: ... was not launched"`.

---

### 3. [MAJOR] Override #3 does not test `drained_verdict`, `#337` silently contradicts D-14 (`#296`) leaving two vacuous negative tests, and the fake harness has `EINTR` and `/proc/stat` parsing bugs
- **Location:** [53.2-VERIFICATION.md:19-22, 114](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-VERIFICATION.md#L19-L22), [53.2-CONTEXT.md:105-107](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-CONTEXT.md#L105-L107), [common/mod.rs:62-66, 143, 186, 302-352](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/common/mod.rs#L302-L352), [run_liveness_e2e.rs:1284-1333, 1357-1395](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/run_liveness_e2e.rs#L1284-L1333)
- **Quoted text:**
  - `53.2-VERIFICATION.md:19-21`: `accepted_by: "orchestrating agent, decided from evidence (operator may strike)" ... dgram_probe.py binds an AF_UNIX SOCK_DGRAM socket, a forked child sendto()s READY=1 and _exit(0)s, the parent waitpid()s it and then does a non-blocking recv; 3000/3000 drained READY=1`
  - `53.2-CONTEXT.md:105-107`: `D-14 (auto): #296: the fake systemctl reports active while the recorded runner pid lives, and the fake systemd-run treats only the launched phase's runner lock, held, as readiness.`
  - `common/mod.rs:143`: `It takes no lock and reads no descriptor of the command.`
  - `common/mod.rs:64`: `if [ -r "/proc/$1/stat" ]; then read -r a b s rest <"/proc/$1/stat"; fi`
  - `common/mod.rs:330, 349`: `Err(err) => return format!("error: {err}"),`
- **Failure scenario:**
  1. **Override #3 is ungrounded:** `dgram_probe.py` only probes Linux's `AF_UNIX SOCK_DGRAM` kernel queue; it never executes `common::drained_verdict` or `common::listen_for_ready`. In `the_fake_launcher_reads_a_runner_that_sent_ready_and_exited_at_once_as_started` (`run_liveness_e2e.rs:1357`), the listener is already blocked in `recv()` at `common/mod.rs:321` *before* the child spawns, so line 322 consumes `READY=1` directly and `drained_verdict` (line 327) is not exercised. Replacing `drained_verdict` with `return "exited".to_string()` would almost never fail that test.
  2. **D-14 contradiction & vacuous tests:** Plan 18 (`#337`) removed runner-lock verification from `FAKE_SYSTEMD_RUN_CALLS` without updating D-14 or the two D-14 negative tests (`the_fake_launcher_does_not_take_an_unheld_runner_record_as_ready`, `the_fake_launcher_does_not_take_another_phases_held_runner_lock_as_ready`, `run_liveness_e2e.rs:1284-1333`). Both still cite `/// D-14 (#296)` and call `assert_no_proc_read_error` (dead against the current fake), and both pass solely because neither child sends `READY=1` — a child holding **no** lock (or another phase's lock) that sends `READY=1` is accepted as ready.
  3. **Code bugs in the fake:**
     - `listen_for_ready` (`common/mod.rs:330`) and `drained_verdict` (`common/mod.rs:349`) treat `ErrorKind::Interrupted` (`EINTR`) as a fatal launch error instead of retrying `recv`.
     - `FAKE_SYSTEMCTL_CALLS` (`common/mod.rs:64`) and `FAKE_SYSTEMD_RUN_CALLS` (`common/mod.rs:186`) parse `/proc/<pid>/stat` with whitespace splitting (`read -r a b s rest`), which breaks whenever `comm` contains a space — the exact bug `crates/devflow-core/src/agent.rs:228-231` explicitly warns against.
- **Fix shape:** Unit-test `drained_verdict` directly (bind a `UnixDatagram`, send `READY=1` from an unbound socket, drop the sender, call `drained_verdict` -> `"ready"`; negative control with `STATUS=starting` -> `"exited"`). Retry `ErrorKind::Interrupted` in both `recv` loops. Parse `/proc/<pid>/stat` after the last `)` (e.g. `rest=${line##*)}`). Replace the two vacuous D-14 tests with explicit `Type=notify` negative controls (exit without `READY=1`, non-`READY=1` datagram) and record the D-14 supersession explicitly in context/verification instead of carrying WR-01 as verified.

---

### 4. [MAJOR] `service_e2e.rs` marks its three "run everywhere" `#279` pure tests `#[ignore]`, while `common::fake_notify_listener_entry` pads `scripts/check.sh service`'s `16/16` count with a no-op
- **Location:** [service_e2e.rs:3-6, 15, 372-425](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/service_e2e.rs#L3-L6), [common/mod.rs:265-277](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/common/mod.rs#L265-L277), [scripts/check.sh:114-115](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/scripts/check.sh#L114-L115), [53.2-VERIFICATION.md:48, 86](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-VERIFICATION.md#L48)
- **Quoted text:**
  - `service_e2e.rs:3-5`: `Every test here that talks to a unit is #[ignore] and runs only under scripts/check.sh service, which needs a running systemd user manager; the container suite has none. The pure tests of [settle_unit_reads] run everywhere.`
  - `service_e2e.rs:372-374` (repeated at `:394-396` and `:410-412`):
    ```rust
    #[test]
    #[ignore = "service_e2e.rs is the host-only suite; run by scripts/check.sh service"]
    fn a_failed_read_before_inactive_is_reported_with_its_result() {
    ```
  - `53.2-VERIFICATION.md:48`: `scripts/check.sh service exit 0, 16 tests run: 16 passed, 0 skipped`
- **Failure scenario:**
  1. All three pure tests of `settle_unit_reads` carry `#[ignore]`, so they are **never executed** by `scripts/check.sh all`, the devcontainer, or GitHub Actions CI — directly contradicting `service_e2e.rs:5` and `53.2-08-SUMMARY.md:16`.
  2. `service_e2e.rs` defines only **15** `#[test]` functions (3 pure + 12 host). Because `service_e2e.rs:15` declares `mod common;`, `scripts/check.sh service` (`--run-ignored only`) also executes `common::fake_notify_listener_entry` (`common/mod.rs:265-270`), which sees `FAKE_NOTIFY_FILES` unset and immediately returns `()`. That phantom no-op is the 16th "passed" test in `53.2-VERIFICATION.md:48, 86` (and adds an ignored no-op test to every integration test binary that includes `mod common;`).
- **Fix shape:** Remove `#[ignore]` from the three pure `settle_unit_reads` tests at `service_e2e.rs:373, 395, 411`. Filter out `common::fake_notify_listener_entry` in `scripts/check.sh:115` (`-E 'not test(=common::fake_notify_listener_entry)'`) — or run the listener through a dedicated helper binary/subcommand rather than an `#[ignore]` `#[test]` in `common/mod.rs`.

---

### 5. [MAJOR] `start --reviews` and `resume --reviews` enforce inconsistent `REVIEWS.md` rules, `start --reviews` fails open on git errors, and `resume --reviews` masks ambiguity/I/O errors as missing file
- **Location:** [commands.rs:66-91, 919-925](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L66-L91), [pipeline_launch.rs:2773-2785](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/pipeline_launch.rs#L2773-L2785), [gsd_reader.rs:231-258](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-core/src/gsd_reader.rs#L231-L258)
- **Quoted text:**
  - `commands.rs:83-90`:
    ```rust
    let Ok(out) = output else { return true };
    if !out.status.success() {
        return true;
    }
    String::from_utf8_lossy(&out.stdout).lines().any(|path| {
        path.strip_prefix(&prefix)
            .is_some_and(|rest| rest.contains('/') && rest.ends_with(suffix))
    })
    ```
  - `commands.rs:919`: `if state.reviews_pass && !phase_artifact_on_base(project_root, phase, "-REVIEWS.md", base)`
  - `pipeline_launch.rs:2775-2776`:
    ```rust
    let present = devflow_core::gsd_reader::phase_dir(launch_root, phase)
        .is_ok_and(|dir| dir.join(format!("{padded}-REVIEWS.md")).is_file());
    ```
- **Failure scenario:**
  1. `start --reviews` promises in `reviews_file_refusal` to require `.planning/phases/{padded}-*/{padded}-REVIEWS.md`, matching D-04 (`53.2-CONTEXT.md:70`) and `resume --reviews`. Because `phase_artifact_on_base` checks `git ls-tree -r` with only `rest.contains('/') && rest.ends_with("-REVIEWS.md")`, any nested file like `.planning/phases/53.2-foo/reviews/draft-REVIEWS.md` or wrong-prefix file like `01-REVIEWS.md` satisfies `start --reviews` while `resume --reviews` refuses the exact same tree.
  2. `phase_artifact_on_base` intentionally fails **open** (`return true` on `Err` or non-zero `git ls-tree`) for advisory Codex pre-flight, so reusing it for `start --reviews` silently bypasses the D-04 refusal on any `git ls-tree` failure.
  3. In `require_reviews_file_in_checkout`, `.is_ok_and(...)` collapses `ReaderFailure::AmbiguousPhaseDir` (two `{padded}-*` dirs) and directory read errors into `present = false`, telling the operator `"and it is not there; run the review and commit its REVIEWS.md first"` even when the file exists in an ambiguous phase dir.
- **Fix shape:** For `start --reviews`, match exact path shape `.planning/phases/{padded}-<slug>/{padded}-REVIEWS.md` (single `/` after `{padded}-`, exact basename `{padded}-REVIEWS.md`) and fail closed on `git ls-tree` error. In `require_reviews_file_in_checkout`, match `phase_dir`'s `Err` variants and surface `AmbiguousPhaseDir` / I/O errors directly.

---

### 6. [MAJOR] `gsd_derived_paths_warning` (`#285` / `#338` WR-02) misses tracked files under `.gsd/`, mislabels git-probe errors as missing `.gitignore` lines, and checks the primary checkout rather than the forked base
- **Location:** [commands.rs:483-567](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L483-L567)
- **Quoted text:**
  - `commands.rs:483-487`:
    ```rust
    const GSD_DERIVED_PATHS: &[(&str, &str)] = &[
        (".planning/state.json", ".planning/state.json"),
        (".planning/milestone.lock", ".planning/milestone.lock"),
        (".gsd/dispatch-isolation-sentinel.json", ".gsd/"),
    ];
    ```
  - `commands.rs:521-527`:
    ```rust
    match git_command(project_root)
        .args(["ls-files", "--error-unmatch", "--", probe])
        .output()
    {
        Ok(out) if out.status.success() => {
            untrack.push(format!("untrack it: git rm --cached {probe}"));
        }
    ```
  - `commands.rs:543-555`:
    ```rust
    if !to_add.is_empty() || !unchecked.is_empty() {
        let mut warning = String::from(
            "warning: GSD writes derived files that git does not ignore here; add these lines \
             to .gitignore:",
        );
    ```
- **Failure scenario:**
  1. `git ls-files --error-unmatch -- .gsd/dispatch-isolation-sentinel.json` only detects that one hardcoded sentinel file. If a repo tracked any other file under `.gsd/` (e.g. `.gsd/tdd-evidence/...` or `.gsd/config.json`), WR-02's tracked-file check is completely silent.
  2. If all three paths are ignored (`to_add.is_empty()`) and any `git check-ignore` or `git ls-files` call returns an error (`unchecked` non-empty), lines 543-554 print `"warning: GSD writes derived files that git does not ignore here; add these lines to .gitignore:"` followed only by `"could not check ..."`, listing zero `.gitignore` lines.
  3. `gsd_derived_paths_warning` probes `project_root`'s working tree and index, whereas worktree-mode `start` forks from the resolved `base` branch. If the primary checkout has uncommitted `.gitignore` edits or sits on a branch other than `base`, `start` can stay silent while the new phase worktree lacks the ignore rules (or warn when `base` already has them).
- **Fix shape:** For `.gsd/`, check tracked files with `git ls-files -- .gsd/` (non-empty stdout -> advise `git rm -r --cached .gsd/`). Emit `unchecked` errors in a separate `"warning: could not verify GSD ignore/tracking rules:"` block instead of under `"add these lines to .gitignore:"`. Either check the resolved `base` tree or document explicitly that the check reflects the primary checkout.

---

### 7. [MAJOR] `devflow status` liveness line (`RunStatus::Live`) never reports a live question gate, contradicting `OPERATIONS.md:239`; `OPERATIONS.md:865` contradicts D-12 (`#297`)
- **Location:** [commands.rs:2548-2564, 2640-2646, 3332-3351](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L2548-L2564), [OPERATIONS.md:239, 865](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/OPERATIONS.md#L239)
- **Quoted text:**
  - `OPERATIONS.md:239`: `| live | The runner lock is held. The line gives the runner's pid when its record reads, and the stage; a run waiting at a gate says so | At a gate, devflow gate approve N or devflow gate reject N --note <reason>; at a question gate, devflow gate answer N --gate-id <id> (--option <id> | --text <answer>), with the ids devflow status prints; otherwise watch it with devflow logs --phase N -f. End it with devflow stop --phase N |`
  - `commands.rs:2640-2646`:
    ```rust
    Ok(RunnerLiveness::Live { pid }) => {
        return Some(RunStatus::Live {
            pid,
            stage: loaded.map(|state| state.stage),
            gate_pending: loaded.is_some_and(|state| state.gate_pending),
        });
    }
    ```
  - `OPERATIONS.md:865`: `stop refuses while it is held and no runner is live`
- **Failure scenario:**
  1. For a question gate opened by `__ask`, `state.gate_pending` is intentionally `false` (`commands.rs:3385`). Consequently `run_status` builds `RunStatus::Live { gate_pending: false, .. }`, and the phase's first line (`commands.rs:2557-2564`) prints `live (runner pid ...) in plan — watch it with devflow logs --phase N -f; end it with devflow stop --phase N`. It never says the run is waiting at a gate and never prints `devflow gate answer`, contradicting the updated `OPERATIONS.md:239` table row.
  2. `OPERATIONS.md:865` still says "`stop` refuses while it is held and no runner is live", contradicting `OPERATIONS.md:48` ("live runner or not") and `commands.rs:4463-4472` (which claims the holder and refuses even when a runner is live). `53.2-VERIFICATION.md:56, 112` acknowledges this as WR-01 yet still marks Truth 36 VERIFIED.
- **Fix shape:** Pass the open question gate (if any) into `RunStatus::Live` so the liveness line reports waiting at a question gate with `devflow gate answer`, or align `OPERATIONS.md:239` with what the liveness line actually prints. Fix `OPERATIONS.md:865` (and `:48` / `:49` per `53.2-REVIEW.md:75-78`) to match `stop_report`'s actual holder refusal.

---

### 8. [MAJOR] `53.2-VERIFICATION.md` violates D-16 by marking `#265` and `#310` `VERIFIED`/`SATISFIED`, and Overrides #1 and #2 have concrete evidentiary/guard holes
- **Location:** [53.2-VERIFICATION.md:11-18, 47, 104](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-VERIFICATION.md#L11-L18), [53.2-CONTEXT.md:114-117, 215-216](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-CONTEXT.md#L114-L117), [53.2-02-SUMMARY.md:23-25](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-02-SUMMARY.md#L23-L25), [53.2-09-SUMMARY.md:34, 40](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-09-SUMMARY.md#L34), [test_support.rs:1281-1346](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-core/src/test_support.rs#L1281-L1346), [53.2-16-SUMMARY.md:36, 55, 61-62](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-16-SUMMARY.md#L55)
- **Quoted text:**
  - `53.2-CONTEXT.md:116-117`: `If it does not, apply the condition-wait fix the issue names, leave the issue open with the run counts, and do not claim it fixed: a green run is not evidence of a fix.`
  - `53.2-VERIFICATION.md:47, 104`: `Truths 1-23 ... (#265, #310) | VERIFIED`; `all other scoped issues | 01-17 | SATISFIED`
  - `test_support.rs:1331-1335`:
    ```rust
    match kept.iter().position(|(kept_path, fragment, _)| {
        kept_path == path && line.contains(fragment)
    }) {
        Some(index) => used[index] = true,
    ```
- **Failure scenario:**
  1. **D-16 contradiction:** Neither `#265` (`53.2-02-SUMMARY.md:23`) nor `#310` (`53.2-09-SUMMARY.md:34`) reproduced, so both issues remain OPEN on GitHub. Yet both summaries list them in `requirements-completed:` (`53.2-02-SUMMARY.md:25`, `53.2-09-SUMMARY.md:40`) and `53.2-VERIFICATION.md:47, 104` grades them `VERIFIED` / `SATISFIED` ("Flaky tests stabilized").
  2. **Override #2 challenge (self-approved by `gsd-verifier`, never operator-approved):** While keeping `launcher_whose_writer_a_fork_holds` in `gsd_reader.rs:1023` is technically sound, the guard test cited to justify it (`test_support.rs:1281-1346`) has two bypasses: (a) `used[index] = true` allows **unlimited** additional `set_permissions(&launcher, ...0o755)` / `(&stub, ...0o755)` lines in `gsd_reader.rs` or `test_support.rs` without failing; (b) scanning only for `set_permissions(` misses `OpenOptionsExt::mode(0o755)`, `std::fs::copy` of executables (the exact ETXTBSY hazard manually patched at `service_e2e.rs:781-795`), and shell `chmod`.
  3. **Override #1 challenge (D-03 live proof):** Operator-approved, but its scope is narrower than claimed: D-02 is scoped to `claude` **and** `pi` (`53.2-CONTEXT.md:215-216`), yet only `claude` ran live (`53.2-16-SUMMARY.md:42`), with `research`, `plan_check`, and `verifier` disabled (`:36`), the scripted marker check failed as written (`:55`), and the scratch root containing `events.jsonl`, `phase-01-stdout`, and `01-CONTEXT.md` was deleted (`:62`), leaving zero auditable artifacts.
- **Fix shape:** Mark `#265` and `#310` as open/mitigated-unverified (not `SATISFIED`) in `53.2-VERIFICATION.md` and the summaries. In `permission_setters` (`test_support.rs:1324-1345`), count exact occurrences (`count[index] += 1`, fail when `count != 1`) and also scan for `.mode(0o7` / `fs::copy` in tests. Scope D-03's verified claim explicitly to `claude` only.

---

### 9. [MAJOR] `scripts/hooks/post-commit` (unchanged in diff, explicitly in review focus) exits before chaining global/repo hooks on every non-workspace branch
- **Location:** [scripts/hooks/post-commit:27-31, 122-135](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/scripts/hooks/post-commit#L27-L31)
- **Quoted text:**
  - `scripts/hooks/post-commit:31`:
    ```bash
    [ -f "$(git rev-parse --show-toplevel 2>/dev/null)/.workspace-divergence" ] || exit 0
    ```
  - `scripts/hooks/post-commit:122-135`:
    ```bash
    # Chain to whatever post-commit was active before core.hooksPath displaced it,
    # matching the pre-commit hook's delegation rather than shadowing silently.
    global_hooks=$(git config --global --get core.hooksPath || true)
    if [ -n "$global_hooks" ]; then
      global_hooks="${global_hooks/#\~/$HOME}"
      if [ -x "$global_hooks/post-commit" ]; then
        exec "$global_hooks/post-commit" "$@"
      fi
    fi
    ```
- **Failure scenario:** `scripts/hooks/post-commit` was not touched by `PHASE-53.2-CODE.diff`, and its early `exit 0` at line 31 fires on every `develop`-based or `feature/phase-N` worktree that lacks `.workspace-divergence`, **skipping lines 122-135**. Any global `post-commit` hook (such as the operator's auto-refresh hook referenced in `AGENTS.md`) or displaced repo-local `.git/hooks/post-commit` is silently shadowed on every commit in those worktrees, contradicting the comment at lines 122-123. (Secondary bug in the same file: line 74 interpolates `status_line` into JSON with only outer-quote stripping `sed 's/^"//; s/"$//; s/.*/"&"/'`, producing invalid JSON if `status:` contains an inner `"` or `\`.)
- **Fix shape:** Wrap lines 33-120 in `if [ -f "$(git rev-parse --show-toplevel 2>/dev/null)/.workspace-divergence" ]; then ... fi` so the chaining block at lines 122-135 always runs on every branch/worktree.

---

### 10. [MINOR] `open_branch_lines` (`#248`) runs redundant `git rev-list` passes before checking base existence, uses bare `rev-parse --verify`, and mislabels unowned branches
- **Location:** [commands.rs:5035-5113](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L5035-L5113), [commands.rs:117-120](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L117-L120)
- **Quoted text:**
  - `commands.rs:5043-5064`: calls `git.list_feature_branches()` (running `git rev-list --count` twice per branch for *all* feature branches in the repo per distinct base) *before* checking `local_branch_resolves(project_root, base)`.
  - `commands.rs:5092-5094`: `format!("  (branches of runs based on {base} could not be compared: {error})")`
  - `commands.rs:5098-5104`: `git rev-parse --verify --quiet refs/heads/{base}`
- **Failure scenario:** (a) When a project has unowned `feature/phase-*` branches and `flow.develop` (`develop`) does not exist locally (e.g., a `main`/`workspace` repo with `DEVFLOW_BASE_BRANCH` unset at `status` time), line 5093 prints `(branches of runs based on develop could not be compared: ...)` even though those branches have *no run*, and omits the branch names. (b) `local_branch_resolves` uses bare `rev-parse --verify refs/heads/{base}`, which accepts revision suffixes (`develop~1`, `develop@{0}`) — the exact pitfall `commands.rs:117-120` documents and guards against elsewhere. (c) With `K` distinct bases and `B` branches, it spawns `O(K * B)` `git rev-list` processes even for branches immediately filtered out by `owns`.
- **Fix shape:** List branches once, group them by owning base, check `git show-ref --verify --quiet refs/heads/{base}` per base before running `rev-list` only for owned branches, and parameterize `not_compared_line` on `source` (and list the affected branch names).

---

### 11. [MINOR] The only new `unsafe` block (`local_utc_offset_secs`, `#249`) has zero committed unit tests; `#341` host tests retain a 500 ms setup sleep, leak spawned children on panic, and hardcode `/bin/cp`
- **Location:** [commands.rs:3062-3085, 6608-6636](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/src/commands.rs#L3062-L3085), [53.2-09-SUMMARY.md:89](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-09-SUMMARY.md#L89), [service_e2e.rs:784, 1112, 1118-1129, 1145-1209, 1241-1275](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/service_e2e.rs#L1112)
- **Quoted text:**
  - `commands.rs:3067-3071`: `let (converted, tm) = unsafe { let mut tm: libc::tm = std::mem::zeroed(); let converted = !libc::localtime_r(&time, &mut tm).is_null(); (converted, tm) };`
  - `service_e2e.rs:784`: `let cp = "/bin/cp";`
  - `service_e2e.rs:1112`: `std::thread::sleep(Duration::from_millis(500));`
- **Failure scenario:** (a) `#249`'s tests only pass a fake closure `|_| Some(3600)` to `started_line` (`53.2-09-SUMMARY.md:89` notes the real `TZ` probe was deleted before commit), leaving the `unsafe` `localtime_r` path untested on explicit `TZ` offsets (and uncompilable on 32-bit Unix where `tm_gmtoff` is `i32`). (b) `start_a_run_with_a_straggler` sleeps 500 ms instead of waiting for a trap-ready marker file, so under parallel nextest load SIGTERM can kill the straggler before `trap "" TERM` runs, making both `#341` tests time out after 60 s; any panic mid-test or timeout in `wait_for_child` leaks the spawned `devflow stop` / `devflow discard` child; and `service_e2e.rs:784` hardcodes `/bin/cp` instead of checking `["/usr/bin/cp", "/bin/cp"]` like `real_systemctl` at `:802`.
- **Fix shape:** Add a subprocess-isolated unit test calling `local_utc_offset_secs(0)` with `TZ=UTC` (`Some(0)`), `TZ=Asia/Kolkata` (`Some(19_800)`), and `TZ=America/New_York` (`Some(-18_000)`). Replace the 500 ms sleep with a ready file written after `trap "" TERM`, wrap spawned `Child`s in a kill-and-reap `Drop` guard, and resolve `cp` across `["/usr/bin/cp", "/bin/cp"]`.

---

## SUSPECTED

### S1. [MAJOR] Speculation: `FAKE_SYSTEMD_RUN_CALLS`'s `@SETSID@ "$@" &; pid=$!` records the wrong PID if `setsid` is ever spawned as a process group leader
- **Location:** [common/mod.rs:198-201](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/common/mod.rs#L198-L201)
- **Quoted text:**
  ```sh
  NOTIFY_SOCKET="$sock" @SETSID@ "$@" </dev/null >"$dir/$unit.out" 2>&1 &
  pid=$!
  echo "$pid" >"$dir/$unit.pid"
  echo "$pid" >"$note.launched"
  ```
- **Failure scenario (speculation):** Under `util-linux` `setsid(1)`, if the process is already a process group leader (`getpid() == getpgrp()`), `setsid` forks and the parent exits immediately with status 0. In a non-interactive `/bin/sh` background job (`&`), job control is normally off so `setsid` is not a pgrp leader and `exec`s in place; however, if any caller runs the fake under an environment/shell where `setsid` becomes pgrp leader, `$pid` will be the exited `setsid` parent rather than the runner — causing `listen_for_ready` to observe `!agent_running(pid)` immediately (spurious `exited` verdict) and `FAKE_SYSTEMCTL_CALLS` to read `inactive` and signal the wrong PID.
- **Fix shape:** Verify under the pinned container's `/bin/sh` (`dash`) or have the child record its own PID / avoid relying on `setsid` not forking.

### S2. [MAJOR] Speculation: `pi` workflow-style discuss-before-plan (`render_workflow_style`) has never been run live and may fail on real GSD `discuss-phase.md`
- **Location:** [prompt.rs:493-512, 685-712](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-core/src/prompt.rs#L493-L512), [53.2-CONTEXT.md:215-216](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-CONTEXT.md#L215-L216)
- **Quoted text:**
  - `prompt.rs:691-694`: `"Read and follow the GSD workflow file at {workflow_root}/discuss-phase.md for phase {phase}.\n\n"`
  - `53.2-CONTEXT.md:215`: `D-02 is scoped to claude and pi`
- **Failure scenario (speculation):** Only `claude` (slash command `/gsd-discuss-phase`) was tested live in Plan 53.2-16, and with `research`, `plan_check`, and `verifier` disabled. `pi` receives prose instructing it to read `discuss-phase.md` directly and route every question through `devflow_ask` (`ask-NN.mcp.ts` extension); whether `pi` actually honors the `no_plan_branch` interactive instructions without ending its turn or tripping GSD's nested `AskUserQuestion` behavior (#1009) is completely unverified.
- **Fix shape:** Either run the D-03 live proof with `--agent pi` as well, or narrow D-02's claimed live support to `claude` until `pi` is exercised live.

### S3. [MINOR] Speculation: New fake shell scripts (`FAKE_SYSTEMCTL_CALLS`, `FAKE_SYSTEMD_RUN_CALLS`) have not been verified under the pinned devcontainer's `/bin/sh` (`dash`)
- **Location:** [common/mod.rs:62-215](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/crates/devflow-cli/tests/common/mod.rs#L62-L215), [53.2-08-SUMMARY.md:131](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/003b4d34-4bd1-477e-9ed3-df7f1e44da65/scratchpad/review53.2-code/snap/.planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-08-SUMMARY.md#L131)
- **Quoted text:**
  - `53.2-08-SUMMARY.md:131`: `Not run: scripts/check-in-container.sh. The fake shell scripts have still not run under the container's /bin/sh.`
  - `common/mod.rs:186`: `{ read -r p comm state rest <"/proc/$lpid/stat"; } 2>/dev/null`
- **Failure scenario (speculation):** In `dash`, a failed redirection on a compound command (`<"/proc/$lpid/stat"` when `$lpid` vanishes between the existence check and open) can behave differently from `bash` (and `53.2-VERIFICATION.md` ran `scripts/check.sh all` on the host where `/bin/sh` is `bash`, never running `scripts/check-in-container.sh`).
- **Fix shape:** Run `scripts/check-in-container.sh all` before opening the PR to verify all rewritten fake scripts under Debian `dash`.
