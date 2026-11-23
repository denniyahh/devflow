## CONFIRMED

### 1. CRITICAL — the #285 preflight warning is printed *before* every `start` refusal, contradicting both its own comment and OPERATIONS.md

`crates/devflow-cli/src/commands.rs:399-404`

```rust
    // #285 (D-13): a warning, never a refusal, said by the dry run and the real start
    // alike, after every refusal above so a refused start prints none.
    if let Some(warning) = gsd_derived_paths_warning(project_root) {
        eprintln!("{warning}");
    }
```

`OPERATIONS.md:37`: "**After its refusals**, `start` (a dry run too) warns on stderr, and does not refuse, when git does not ignore GSD's derived files …".

Every refusal that `start`'s own docs enumerate runs *after* line 402: `refuse_without_a_user_manager` (`commands.rs:411`), `claim_foreground_holder` (`:415`), `gather_guard_facts` (`:418`), `reserve_launch` (`:422` → `launch_guards` at `:1284-1311`: live runner, finished/incomplete state, orphan gate files, leftover worktree/branch), and `start_reserved`'s CONTEXT.md/`--reviews`/leftover-branch refusals (`commands.rs:903-920`, `:966-974`).

Failure scenario: on a repo whose `.gitignore` lacks `.planning/state.json`, `devflow start --phase 53.3` over a live run prints a three-line `.gitignore` warning and *then* `phase 53.3 has a live run … devflow stop`. The operator reads a setup warning on a verb that did nothing, and any script/agent that treats "stderr contained a warning" as "start got past preflight" is now wrong. The only refusals it genuinely follows are the fallback/agent ones above it.

Fix shape: move the `gsd_derived_paths_warning` call to just after `reserve_launch` succeeds (and, for a real start, after `start_reserved`'s refusals), keeping it before `dry_run` returns — or move the refusals it claims to precede.

### 2. MAJOR — D-14's readiness clause was deleted, and the two tests written for it are now unfalsifiable

`crates/devflow-cli/tests/common/mod.rs:143-145`: "**It takes no lock and reads no descriptor of the command.**"

`53.2-CONTEXT.md` D-14 requires: "the fake `systemd-run` **treats only the launched phase's runner lock, held, as readiness**". The fake no longer reads `.runner-*` at all (the `for lock in "$wd"/.devflow/.runner-*` loop is gone from `FAKE_SYSTEMD_RUN_CALLS`).

`crates/devflow-cli/tests/run_liveness_e2e.rs:1287` `the_fake_launcher_does_not_take_an_unheld_runner_record_as_ready` and `:1308` `the_fake_launcher_does_not_take_another_phases_held_runner_lock_as_ready` both assert only `!out.status.success() && stderr.contains(common::FAKE_START_FAILED)`. Their children (`run_fake_launcher` with `sh -c "exec sleep 2"` / `other_phase_runner_lock_holder_entry`) never send `READY=1`, so the listener always reaches `exited` and the fake prints `FAKE_START_FAILED` (`common/mod.rs:196-201`) **whatever any lock logic would have said**. Re-adding the #296 bug ("any held lock = ready") leaves both tests green.

The verifier knows and left it open: `53.2-VERIFICATION.md`, Anti-Patterns, "prior review WR-01: the D-14 negatives pass because no child sends `READY=1`, not because of the lock … carried forward, unchanged" — yet #296 is still marked `SATISFIED`.

Fix shape: delete the two lock tests (they test a mechanism that no longer exists) or turn them into a mutation control: make the launched child send `READY=1` *and* hold another phase's lock, and assert `ready`; then the test fails if readiness ever becomes lock-based again. Update D-14's text in the phase artifacts so the decision and the code agree.

### 3. MAJOR — `status` silently deletes a run's feature branch from "open branches" when its recorded base cannot be resolved

`crates/devflow-cli/src/commands.rs:5061-5065`

```rust
        if let Err(error) = local_branch_resolves(project_root, base) {
            not_compared.push(not_compared_line(base, &error));
            return;
        }
```

and `:5074-5076`, where every branch owned by a run is filtered *out* of the project-base pass:

```rust
    compare(&flow.develop, "the project's base", &|name| {
        !run_bases.contains_key(name)
    });
```

Failure scenario: phase 7 runs on `workspace`; the operator later deletes `workspace` (`git branch -D workspace`, common after a rebase). `devflow status` now prints, for phase 7:

```
  (branches of runs based on workspace could not be compared: workspace is not a local branch)
```

and **no line at all for `feature/phase-07`** — the branch, which is what "open branches:" exists to inventory, has vanished from the list, and `print_open_branches` prints no header either when nothing survives. The "could not be compared" line does not even name the branch it dropped. Worse, when the failing base is the *project's* base, `not_compared_line` (`:5092-5094`) still says "branches of **runs based on** `develop`", so a repo with no runs at all is told about runs.

The pinning test asserts exactly this loss: `commands::tests::open_branches_report_a_deleted_recorded_base_as_not_compared_...` asserts `!lines.iter().any(|line| line.contains(&feature) …)`, i.e. "a deleted run base read as an even comparison" is fixed by hiding the row instead of reporting it.

Fix shape: emit the branch line with an explicit unknown-currency marker (`  feature/phase-07 — ahead/behind `workspace` unknown: workspace is not a local branch`) instead of skipping the row, and word `not_compared_line` by base origin, not by "runs".

### 4. MAJOR — the D-11 / #338 "kept" message is a false statement whenever the state is absent or unreadable

`crates/devflow-cli/src/commands.rs:590-605`

```rust
    let now = workflow::load_state(project_root, phase)
        .ok()
        .and_then(|state| serde_json::to_value(&state).ok());
    if written.is_some() && now.as_ref() == written {
        return clear_after_failed_launch(project_root, phase, worktree, why);
    }
    CliError::Message(format!(
        "start: phase {phase} was not confirmed launched ({why}), and its run state changed \
         after start wrote it, so it was kept: run `devflow status` to inspect it, and \
         `devflow discard --phase {phase}` to remove it"
    ))
```

Same shape at `crates/devflow-cli/src/pipeline_launch.rs:2314-2330` ("its run state changed after resume wrote it, so it was kept").

`now` is `None` when the state file is missing or unparsable, and `written` is `None` when the capture at `commands.rs:432-434` failed. All three cases fall into the same `else` and print "changed … so it was kept", which is *not* what happened: nothing was kept, and `devflow status`/`devflow discard` will both report nothing for that phase. The doc comment two lines up admits the conflation ("one that does not load, or no `written` keeps it") but the operator-facing text does not.

Failure scenario: a `discard` (or a second operator) removes `.devflow/state-NN.json` while `start`'s `systemd-run` is timing out; `start` exits with a message asserting the state was preserved and naming two verbs that cannot act on it. This is "absent read as false" in operator text.

Fix shape: branch on the load result — `MissingState`/parse error → "its run state is gone or does not load (`<error>`); run `devflow status` and `devflow start` again"; captured snapshot failed → "the state could not be read back after start wrote it; …". Only claim "kept because it changed" when `now` is `Some` and differs.

### 5. MAJOR — `cleanup`'s documented behaviour during a stop was invalidated by D-12 and not updated

`OPERATIONS.md:49` (cleanup row): "Each phase is handled under its foreground holder, after its liveness read: **a unit still stopping is waited out, for up to 15 s, as `discard` does**".

`crates/devflow-cli/src/commands.rs:1747-1750`

```rust
    let _holder = match claim_foreground_holder(project_root, phase) {
        Ok(holder) => holder,
        Err(err) => return unsettled(err.to_string()),
    };
```

plus the new `crates/devflow-cli/src/commands.rs:4467-4481`, where `stop` now *holds* that same holder across `stop_unit`:

```rust
    let phase_hold = liveness::claim_phase_holder(project_root, phase).map_err(|err| match err {
```

Failure scenario: `devflow stop --phase 87` on a unit with a SIGTERM-ignoring straggler (the exact `STRAGGLER_TAIL` fixture in `service_e2e.rs`) holds `.holder-87` for the manager's `TimeoutStopSec` plus the reload. `devflow cleanup` in that window cannot claim the holder, so instead of "waiting out up to 15 s" it keeps the phase and exits non-zero. The `discard` row *was* updated for exactly this ("including a `devflow stop` still waiting for its unit to go down: run `devflow discard` again once that verb returns", `OPERATIONS.md:50`); the cleanup row was not, and the verifier's only OPERATIONS finding is about line 865.

Fix shape: add the same clause to the cleanup row (and to `.holder-NN` at `:865`, which the verifier already flags), or make `cleanup_phase` distinguish "held by a stop" and wait it out as the row promises.

### 6. MINOR — `resume --reviews` reports a missing file when the real error is an ambiguous or unreadable phase directory

`crates/devflow-cli/src/pipeline_launch.rs:2775-2784`

```rust
    let present = devflow_core::gsd_reader::phase_dir(launch_root, phase)
        .is_ok_and(|dir| dir.join(format!("{padded}-REVIEWS.md")).is_file());
    …
    Err(crate::commands::reviews_file_refusal(…, &format!("in the run's checkout {}", launch_root.display())))
```

`gsd_reader::phase_dir` (`crates/devflow-core/src/gsd_reader.rs:248-257`) returns `Err(AmbiguousPhaseDir)` for two matching `NN-*` directories and `Err(PhaseDirNotFound)` for an unreadable `.planning/phases`. `.is_ok_and` turns every error into "the file is not there", and the refusal text then instructs "run the review and commit its REVIEWS.md first" (`commands.rs:723-731`) while the file may be present in one of the two directories.

Failure scenario: a run's worktree carries `.planning/phases/12-test/` and a second `12-*` directory (GSD review/retry dirs land as siblings in some flows). `resume --reviews` refuses with a message that is factually wrong and prescribes a fix that cannot work, and the state is never written so the operator has no other signal.

Fix shape: match the `ReaderFailure` — propagate `AmbiguousPhaseDir`/`PhaseDirNotFound` as their own refusal naming the directories, and reserve `reviews_file_refusal` for a resolvable dir with no REVIEWS.md.

### 7. MINOR — the new `--reviews` refusal is fail-open on any git error

`crates/devflow-cli/src/commands.rs:82-91` (`phase_artifact_on_base`, used unchanged for the new check at `:920-927`)

```rust
    let Ok(out) = output else { return true };
    if !out.status.success() {
        return true;
    }
```

D-04: "It is refused without a `NN-REVIEWS.md` in the phase directory". The check is fail-*open*: a `git ls-tree` that cannot run or fails (dubious ownership, a base that is not a valid tree, an unwritable index lock, `command not found`) reports "present", so `start --reviews` launches a reviews Plan with no REVIEWS.md and the agent plans against nothing. The pre-existing CONTEXT.md use of this helper is documented as fail-open; reusing it for a *new refusal* silently imports that hole into a promised gate with no test that the refusal fires on a git error.

Fix shape: give the reviews probe its own strict variant (`Ok(true/false)` + an explicit "could not check the base" refusal naming git's stderr), or keep fail-open and drop the "refused without" wording from D-04/OPERATIONS.

### 8. MAJOR — two of the three overrides are self-accepted, and one converts an untested behaviour into a "verified" truth

`53.2-VERIFICATION.md:15-22`

```yaml
    accepted_by: "gsd-verifier, on the fixture's documented purpose (operator may strike)"
```
```yaml
    accepted_by: "orchestrating agent, decided from evidence (operator may strike)"
```

Override 2 waives a must-have on the **verifier's own** signature; override 3 waives a must-have on the **orchestrating agent's** signature. The repo rule is that a `human_needed`/override residue goes to the operator, and the entries themselves admit the operator has not been asked. Override 3 is also the only evidence for a *code-order* property ("`exited` is reported only after the pid is gone and the socket drained") that no test in the tree asserts — the 3000/3000 probe (`dgram_probe.py`) is not in the repo, is not rerun by `scripts/check.sh`, and `listen_for_ready`/`drained_verdict` (`tests/common/mod.rs:338-395`) could be reordered tomorrow with nothing failing.

Separately, the same table's evidence for #178 is "Full `cargo test` exit 0 at 31a28f3" (`53.2-VERIFICATION.md:47`) — a green suite cannot detect a leaked scratch root; the 0→0 measurement that justified closing #178 with *no* teardown code lives only in `53.2-11-SUMMARY.md` and was not re-run at HEAD.

Fix shape: get operator signatures for overrides 2 and 3 (or implement the missing test), add a test that pins the drain order (e.g. a child that sends `READY=1` and exits inside one poll, asserting `ready` over 100 iterations — this exists for #337 and could be reused), and use the leak measurement, not the suite, as #178's evidence.

## SUSPECTED

### 9. SUSPECTED (MAJOR) — the fake `systemd-run` now hard-panics the whole suite on a test-binary path containing a space

`crates/devflow-cli/tests/common/mod.rs:236-241` (in `fake_systemd_run_calls`)

```rust
    assert!(
        !exe.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '$' | '`' | '\\')),
        "the test binary's path is written double-quoted into the fake: {exe}"
    );
```

The path is interpolated into a `/bin/sh` script as `"@TESTEXE@"`. A target directory, `$TMPDIR`, or checkout under a path with a space (`/home/u/My Projects/devflow/target/debug/deps/…`) is legal and previously worked for every test that writes the fake; now every such test panics inside the writer, i.e. the suite fails with a harness assertion, not a product failure. Related, same mechanism: the per-launch socket is `"$HOME/service-bin/<unit>.notify-<pid>.sock"`; with a long tempdir/HOME the `AF_UNIX` 108-byte limit turns every fake launch into `error: could not bind …`, which surfaces as `systemd-run` failure. Confirming it: run one `mod common` target with `--target-dir` pointing under a path with a space, and with a 90-character `HOME`.

Fix shape: quote via a single-quoted literal with `'\''` escaping (or refuse with a clear "unsupported test path" at the same assert), and bind the notify socket in a short `XDG_RUNTIME_DIR`-style temp path, asserting its length with a message before `bind`.

### 10. SUSPECTED (MINOR) — `/proc/<pid>/stat` parsed by whitespace in the fake `systemctl` can misread `ActiveState`

`crates/devflow-cli/tests/common/mod.rs:57-62`

```sh
alive() {
  s=""
  if [ -r "/proc/$1/stat" ]; then read -r a b s rest <"/proc/$1/stat"; fi
  [ -n "$s" ] && [ "$s" != Z ]
}
```

`comm` (field 2) is parenthesised but may contain whitespace, which shifts `read`'s fields so `s` (state, field 3) takes a later token; the result is a false "alive" or a false "dead". Since this now decides `active` for every unit read, a misread makes `status`/`discard`/`stop`/`start` wait out a 15 s stopping unit or classify a dead runner as `Stopping`. Same family: a stale `U.pid` whose pid is recycled by an unrelated process reports `active` forever. Not reproduced; both are timing/window dependent.

Fix shape: parse after the last `)`: `stat=${stat#*) }` then the first field of that remainder is the state — an awk-free, space-safe read.

### 11. SUSPECTED (MINOR) — `stop`'s holder is taken before the state write lock is even considered, so a `status` run by the operator now sees a "held phase" during every stop

`crates/devflow-cli/src/commands.rs:4467` holds `.holder-NN` for the whole `systemctl --user stop` + reload (up to `TimeoutStopSec` + 10 s). Nothing in the diff updates `status`'s holder rendering or the `.holder-NN` inventory row beyond the D-12 sentence at `OPERATIONS.md:865`, which the verifier already records as narrower than the code. Any operator script that runs `status` in parallel with `stop` and asserts no holder will now see one. Not verified against a live system; speculative until someone runs `devflow stop` with a straggler and reads `status` mid-stop.

### 12. SUSPECTED (MINOR) — the `#279` fix converts the flake into a hard failure rather than diagnosing it

`crates/devflow-cli/tests/service_e2e.rs:311-321` (`settle_unit_reads`) returns `Err` on the first `failed` read:

```rust
            "failed" => {
                return Err(format!(
                    "unit read failed before inactive: ActiveState=failed, Result={}, ExecMainStatus={}",
```

If #279's real cause is that a stopped unit legitimately passes through `failed` (its main process exits non-zero on SIGTERM), `stop_on_a_live_unit_records_stopped` now fails deterministically instead of flaking — D-15 asked for exactly this ("assert `Result`/`ExecMainStatus` in the message"), so it is faithful to the decision, but the phase records #279 as *fixed* rather than *diagnosed*. One host run of `scripts/check.sh service` cannot distinguish "green because the flake is gone" from "green because it did not recur". Fix shape: keep the hard error, and record in the issue what a `failed` read there would mean (exit code + `Result`) so the next occurrence is a diagnosis, not a rerun.
