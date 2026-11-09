# Milestones

## v3.0.0 Unattended Run Survivability (Shipped: 2026-11-08)

**Phases completed:** 7 phases, 120 plans, 246 tasks

**Closeout type:** `override_closeout`. All seven phase verifications read `passed`; five (47, 48, 49, 50, 50.1) were
`stale` at close because later phases edited files their verifiers had covered — accepted as overrides by the operator
(2026-11-08). No `/gsd-audit-milestone` was run (operator decision). Requirements 18/18 complete.
**Known verification overrides:** 37 newly acknowledged, 9 carried forward (pre-close artifact audit: 2 debug sessions,
46 deferred-item entries; the 11 Phase 46 entries were resolved in place, see the GSD manual-fix incident log).
**Release mapping:** Phases 46-50.1 shipped as `v3.0.0` (2026-11-03/04, crates.io + GitHub Release); Phase 51 merged to
`develop` via PR #234 on 2026-11-08 and ships inside 4.0.0 — 3.1.0 skipped (operator). Timeline 2026-09-05 → 2026-11-07,
1,555 phase-scoped commits.
**Live evidence:** Phase 49 attempt 3 (auto mode, shipped); Phase 50 attempt 1 and fault arm; Phase 50.1 attempt 2 (not
unattended); Phase 51 supervise dogfood (4 by-hand exceptions, `51-RUN-NOTES.md`) and the 2026-11-08 failover probe
(claude → codex, synthetic denial).

**Key accomplishments:**
- An advisory `Sequential 2-CPU check` CI job that runs the whole suite under a `taskset` pin from a single shared definition site — and which, on its first local exercise, deterministically surfaced a latent defect the repo's normal 4-vCPU CI cannot see.
- The CPU-pin `all` case now has one home instead of one-and-a-half; the pinned/unpinned core-count comparison is an assertion instead of three `echo`s; and the parity guard is a fixture-driven function over the `sequential` job's own body instead of a `contains` search over the whole file.
- The plan-bashism guard is now a block-parsing scanner whose allow rule is containment inside a `bash -c '…'` single-quoted region rather than the presence of the string `bash -c`, backed by 11 executing tests and nine fixtures; both bypasses an external reviewer committed through the live hook are refused by a real commit, and the legitimate form still commits.
- All five remaining `pipeline_outcomes.rs` empty-PATH tests now execute only in child processes, with the two-cycle Validate case persisting and rechecking its State between the child and the parent; `NoGitPath` is gone and clippy enforces that it has no callers.
- The 47 direct-spawner locks added by d525f9a are gone after all seven empty-PATH tests moved into child processes; 81 genuine environment-mutation locks remain, and three pinned, contended workspace runs completed with 0 failures.
- The plan-bashism scanner now evaluates only the plan bytes Git will commit, including renamed destinations, with four hermetic disposable-repository regressions that prove both dangerous and inverse staged/worktree directions.
- `insta` snapshot guard over the Claude/OpenCode loop-back prompt, with `run_test` pinned by `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`, observed on this repository to turn the force-update re-bless from exit 0 into exit 101
- `fix_prompt`'s FullExecute arm now carries `CODE_STAGE_POLICY` on the four `render_claude_style` adapters, decided by a helper extracted from `workflow_code_prompt` whose output stayed byte-identical. A six-adapter test that was red on Claude, OpenCode, Antigravity and Hermes now passes.
- Named `insta` baselines for all six adapters' FullExecute fix prompts and for the two delivered resume turns, captured separately from the production constructors and taken against the final 47-03 text. The full `scripts/check.sh all` is green with them in place.
- Corrected unattended-gate guidance, a claude-only Phase 49 evidence standard, the real four/two adapter split, and two separately actionable preflight defects.
- Closed CR-01 with wording-only prompt changes, preserved the parser, and prevented a resumed agent's reasoning from being mistaken for a new blocking-human checkpoint.
- Made the phase-worktree guard harness reliable under hostile global hooks, SSH signing, and environment-injected git configuration.
- Child-process test support now gives Pi and OpenCode fixtures an exact-test, command-local PATH instead of racing the process-global environment.
- All pipeline-outcome PATH fixtures now run under a directory-only child process with exact-test vacuity guards.
- Pipeline-launch PATH fixtures and the child-output guard repair are verified and recorded through an approved normal-hook commit on the intended phase branch.
- All TEST-01 fixtures now use child-local PATHs, abort fixtures prove child mode, and obsolete PATH-prepend helpers are removed.
- State and gate writes now use collision-retrying sibling temps, while gate responses publish first-answer-wins through a hard link and cleanup collects only matching orphan files.
- `recover --clean --phase N` now serializes all destructive cleanup with the phase lock, while doctor names the same recovery command for gates with no possible waiter.
- Non-Ship gate answers now require a lock holder that may be waiting, while Ship retains its explicit recovery path and #200 has wedge-versus-live-consumer E2E coverage.
- The testing guide and stale comments now describe command-local, directory-only PATH fixtures guarded by exact child-test evidence, alongside bounded post-conversion suite measurements.
- Two e2e arms drive a real `devflow start --mode auto` parked at its Define preflight gate. An interrupted `start` leaves a gate that `gate reject` refuses and `recover --clean` clears. A live `start` consumes the same rejection and exits 0. A one-token lock-drop mutation turns the live arm red.
- `start` now refuses, under the per-phase lock and before it touches any gate, branch or state, when the phase's recorded monitor or agent pid is alive (`--force` included), with a refusal that says `stop` would not end this lock-free run and to SIGTERM the monitor first after `ps -p`; a dead leftover still restarts exactly as before.
- `devflow resume` now refuses a live recorded monitor or agent under its phase lock, preserving the existing start refusal byte-for-byte.
- VERIFY-01 now restates amended criterion 1, ROADMAP criterion 2 names the Define/Code preflight gates and the non-waiting `--yes-ship` Ship gate, the close-out gate drops Phase 49.1 and the landing hop, and the superseded 49.1 payload plan is deleted so no execute-phase run can pick it up.
- `scratch-dogfood-repo.sh` gains `--live-payload` (main + integration, devflow.toml on integration, serde-pretty GSD config), a read-only eight-line `--check`, a ten-case `--selftest` with negative controls, and a destination guard widened to the main checkout; the default fixture's tree hash is unchanged.
- A `DEVFLOW_GATE_NOTIFY_CMD` hook that copies each phase-1 gate's state, gate files, events, captures and git heads into a hashed, ledger-indexed snapshot, and a collector that gathers `.devflow/`, the Claude session, the Ship merge found in `integration`'s history and the PR, then writes a verdict derived only from DevFlow's `workflow_shipped` event.
- Stdlib-only ctypes inotify watcher for `<launch>/.devflow/` plus an analysis that flags one pid creating `.state-01.json.<pid>.<seq>.tmp` while another pid's temp is still open, and reports overflows, a missing stop and an empty log as blind spots, never as a verdict.
- `sanitize-evidence.py` publishes a copy of an evidence tree with home spellings, Claude escaped paths, emails and UUIDs redacted, and refuses the whole publish on anything it cannot vouch for. Its credential scan runs on JSON-decoded strings. That is what lets the real phase-43 Code captures publish, which 30c's raw pattern would have refused, while a credential hidden in a JSON-escaped string is still caught.
- The operator's third live `devflow start --mode auto --agent claude --yes-ship` attempt went through Define, Code and Ship. Its evidence passes `check-evidence.sh --attempt-gate` (`verdict=shipped attempt=3`; positive arm 11/0, negative arm 5/0, pair 3/0). Attempt 1 stopped. Attempt 2 shows `shipped`, and it is not on record why attempt 3 was run after it.
- `49-LIVE-RUN.md` gives VERIFY-01 as met on attempt 3 and DECN-03 as Followed, and records SURV-01's no-interleaving observation without claiming anything from it. The evidence behind it is committed sanitized under `49-evidence/`, and attempt 3's copy still passes positive 11/0, negative 5/0 and pair 3/0. Four non-blocking findings are filed as 999.147 to 999.150.
- A staged semver gate that compares the real `cargo semver-checks` 0.51.0 run, item by item, with R-9's named breaks for each plan stage, plus ROADMAP Phase 50 criteria 4-8 and the folded-backlog markers.
- Eight more e2e suites now spawn `devflow` only through `common::hermetic_devflow`, with a `LaunchedRunReaper` after every run-launching spawn. A new permanent source scan in `gate_sweep_e2e.rs` refuses any non-dry-run `--reap-strays` pass in a suite on the builder.
- `acquire_after_existing` now decides reclaim with `classify_holder_record` (pid liveness plus recorded start time). A lock whose pid was reused by another process is reclaimed, so `devflow recover --clean --phase N` now works at a recycled-pid lock. An identity-confirmed live holder, or one whose identity cannot be checked, still keeps its lock.
- A `devflow_core::supervisor` module that builds a per-launch socket path under `cache_dir/sock` with the `sun_path` limit enforced by name, publishes a listener by bind-to-`.part` then hard link, and probes a recorded path into exactly Gone, Stale or Alive (or a named probe error) without reading any pid.
- Each socket now has a 0600 JSON sidecar, and a read-only `sweep_for_in(cache, root, phase)` finds a live supervisor with no state file. When the sidecar is broken or missing it falls back to the socket's name, or to the ping reply's phase. `remove_launch_files` is the one acting function. `request_shutdown` reports `Stopped` only after the final stopped line. `State` gains a persisted `supervisor: Option<SupervisorRecord>`.
- `devflow_core::supervisor::runtime`: a supervisor that serves `ping` and `shutdown` on its socket, turns SIGTERM and SIGINT into stop events through a self-pipe, ends its agent's process group (SIGTERM, then SIGKILL, with leftover members killed while the zombie leader still holds the pgid), and takes every stop/exit decision under one owner mutex, so a stop that races the agent's natural exit never advances the pipeline.
- `monitor::run_supervised_stage` runs the capture shape (the Legacy `sh` script's Rust replacement) and the stream shape (PipeOwning with an address) on one supervisor body. Each puts the agent in its own process group and checks that before recording anything. Each serves ping and shutdown on a per-launch socket, writes 143 and reports `Stopped` when SIGTERM or SIGINT reaches the supervisor, and meets every R-A…R-M responsibility under a non-Claude adapter.
- `devflow_core::supervisor::backstop` ends an orphaned agent group behind a STALE socket only when the group's leader is present, has the recorded start time and still leads the group. It signals through a pidfd opened on that leader, re-reading the leader after opening, with `PIDFD_SIGNAL_PROCESS_GROUP` (SIGTERM, wait, SIGKILL). A recycled leader pid, an empty group and a previous boot remove the files and signal nothing. A group whose leader is gone is refused: its members are reported and never signalled. Off Linux the backstop refuses before reading anything, and no signalling path exists there.
- `spawn_monitor` now takes one `LaunchContext` (cache dir + monitor executable). Inside the CLI's unit-test binary a stream launch re-enters the test binary at an `#[ignore]`d harness entry that runs the real `run_monitor` body, so in-crate launches really supervise their fake agent. Every in-crate child test gets its own `/tmp/dfu…` cache dir, and registration goes only through `register_in`. A full `scripts/check.sh all` run added no entry to the operator's real `~/.cache/devflow/roots` (50-21 measured 33).
- `status`, `cleanup` and `doctor` now answer ALIVE / STALE / GONE from the socket in `state.supervisor`, with no pid read, and each non-ALIVE answer names its repair. `doctor` also lists supervisor sockets that no state claims (no state, superseded, deleted root, missing or broken sidecar). Each of those names `devflow stop --phase N --launch <id>`, and the sweep changes nothing on disk.
- A supervisor now hands its stage to the next one by publishing the successor before it unlinks itself, and a stop or SIGTERM that lands mid-advance or at a gate ends the run at a safe point with the lock released and nothing launched.
- `devflow stop` now ends a run through its supervisor socket (shutdown when ALIVE, the validated backstop when STALE, the lock holder only when GONE), `--launch <id>` tears down one named socket from any directory, and `recover --clean` refuses a live run.
- `start` and `resume` now find a live run through its supervisor socket: the state's record when the state loads, the sidecars (or the socket's file name) when it does not, and they refuse over an unlistable socket directory. No `monitor_pid` is read.
- `State::monitor_pid` is gone from devflow-core, so the supervisor socket record is the only liveness source a state file holds. A permanent `doc_check` scan with planted controls guards against the field coming back, and the semver gate reports `verdict=match` for the full named R-9 set.
- OPERATIONS.md now explains run liveness the way the code answers it: ALIVE / STALE / GONE from the recorded socket, the repair verb for each, the `devflow stop` arms in code order, where the sockets live and their 107-byte bound, plus the macOS caveat and the 2.x install rule. ARCHITECTURE.md, the unattended-mode guide and the lifecycle diagram no longer describe a shell monitor or a forked `devflow advance`. Two permanent `doc_check` scans keep it that way.
- `fault-arm.sh` runs D-17/R-10's five live fault cases on a built devflow binary with fake agents and a private cache, and all five passed on the release binary of `83dac2a` (`~/.cache/devflow-phase50/fault-1`, `FAULT verdict=pass`, `complete=yes`). `check-supervisor-evidence.sh` judges those records, the crate source they came from, and 50-19's live attempt. `50-RUNBOOK.md` reuses Phase 49's rig at `~/.cache/devflow-phase50` and runs the fault arm on each attempt's pinned binary.
- The operator's first live `devflow start --mode auto --agent claude --yes-ship` attempt on binary `devflow-9b0ec70c7190` (built clean from `1deaa4d`) went through Define, Code and Ship and used both launch shapes (5 stream, 1 capture). Its evidence passes `check-supervisor-evidence.sh --attempt-gate` with `verdict=shipped attempt=1`. The fault arm `fault-5`, run on that same binary, passed with `--require-all`.
- Attempt 1 and fault-5 are committed as sanitized, curated copies under `50-evidence/`, each carrying the checker output saved on the raw tree (`ATTEMPT_GATE verdict=shipped attempt=1`, `FAULT verdict=pass`, `ATTEMPT verdict=pass`). `50-LIVE-RUN.md` quotes those lines, maps criteria 1-8 to 26 named tests (all re-run by exact name, all passing) and to the live records, lists what the evidence does not establish, records no non-blocking defect, and carries the 3.0.0 release handoff. The macOS compile gate (V-56) passed.
- The last five e2e suites now spawn `devflow` only through `common::hermetic_devflow`. A new test target, `hermetic_spawn_sites.rs`, holds two permanent scans: one pins all five suites to the builder, and one fails if any file under `tests/` other than `common/mod.rs` names the binary's env var.
- The `devflow` CLI now records, reads and reports only `state.supervisor`. The interim monitor-pid write, the `stage_launched` `monitor_pid` key, the `--until` clear and the stray-reap reachability input are gone, so 50-16 can delete the field.
- A launch that fails or is refused now clears the supervisor record. A publish save that fails shuts the unrecorded successor down, or names the `stop --launch` repair when that shutdown can't be confirmed. A stop that beats the publish is recorded as a stop, and a stop that reaches N after safe point 3 is forwarded to N+1.
- A launcher's `wait_for_publish` now either returns the supervisor's ping reply or ends the supervisor and every agent group it started (by stop-snapshot-kill on timeout, or through the launch's validated sidecar after a SIGKILL), and `run_supervised_stage` ends its own agent group on any error or panic before it publishes.
- Four append-only judges (`--published`, `--live-run`, `--handoff`, `--defects`) in `check-supervisor-evidence.sh`, each a one-call gate for 50-20 whose exit code is the verdict, pinned by 23 named selftest cases. Every failing case was shown to fail for its intended reason by a mutation of the judge.
- A stream supervisor now ends its stream at the agent's exit (exit-watch thread, `waitid(WNOWAIT)`) and waits at most 3 s for its pipe threads, so a process left holding the agent's stdout no longer keeps a completed or stopped supervisor alive; `devflow stop` sent while it is `advancing` ends the run at safe point 1 on the real binary.
- `check-supervisor-evidence.sh --attempt-gate --current-source` refuses any attempt whose paired fault record was not taken from the checkout's committed crate tree (attempt 1 is refused on the real home, the plain gate still ships it), and 50-RUNBOOK.md now records the operator's 2026-10-21 delegation of attempt 2, separate from the planner's adaptations, closing attempt 2 on that gate for 50-28.
- Live attempt 2 ran `devflow start --mode auto --agent claude --yes-ship` on binary `devflow-182ab8b546a8`, built clean from `bca7c35` (which contains the CR-01 fix `373f1e6` and the WR-03 fix `493156b`). It went through Define, Code and Ship and used both launch shapes (5 stream, 1 capture). Its evidence passes `check-supervisor-evidence.sh --attempt-gate --current-source` with `current_source=yes` and `verdict=shipped attempt=2`. The fault arm `fault-7`, run on that same binary, passed all six cases with `--require-all`.
- Attempt 2 (fixed binary `182ab8b5…`, built from `bca7c35`) and its paired fault record `fault-7` are published under `50-evidence/` with their raw-tree verdicts (`ATTEMPT_GATE current_source=yes`, `ATTEMPT_GATE verdict=shipped attempt=2`, `FAULT verdict=pass`). Three made-up credentials the live agent planted as a gitleaks control were redacted in the published copy by operator decision. 50-LIVE-RUN.md now takes its verdict from attempt 2 and keeps attempt 1 as pre-fix history. 50-VALIDATION.md has V-58..V-68, and V-28/V-29 quote attempt 2. WR-03 is recorded fixed, and the macOS compile gate passes on the final tree.
- Every state and gate write, publish and removal in `devflow-core` now goes through `durable.rs`, which fsyncs the file and the directory, and a source scan keeps it that way. The tests prove the fsync calls are made in order; they do not prove survival of a real power loss.
- An EMFILE on the supervisor's control socket now costs 7 accept attempts a second instead of 654,532, and the last `kill(agent, 0)` liveness probe in the publish-failure test is replaced by `process_is_live`, with a subreaper control showing why.
- Five new requirement IDs (SUPV-03, SUPV-04, SUPV-05, SURV-03, SURV-04) with traceability to Phase 50.1, coverage 18/18, and GSD now reports them as the phase's `phase_req_ids`
- `devflow-core` now has one state write path: a short `flock` on the never-replaced `.devflow/.state-NN.lock`, a re-read, the caller's closure and a durable replace, with a create-capable form, an owner-fenced form and a bounded form that returned in at most 500.6 ms against a 500 ms deadline, naming a stopped holder.
- A per-launch `flock` that a launcher hands to its supervisor at fd 3, probed live, abandoned or missing with no pid read, whose file name exists only once the lock is held.
- A gate answer now decides only the gate it names: fresh 16-hex gate ids, answers keyed and bound to them, wrong or id-less answers moved aside under names no other process can take, and a claim that can succeed only while its gate is open.
- The launcher now reserves a launch id, socket and held liveness file before it spawns, the real `__monitor` re-exec adopts that lock at fd 3 before it starts its agent, and the launcher drops its copy after the publish.
- A gate's waiter now registers with a held flock before its gate file is published, and its wait moves no file, backs off to one read a minute, still cancels within about a second, and lets a stop beat an answer.
- The unlocked `state.json` rename on every read is deleted, and a file an old binary left behind is now ignored by every read, `clear_state` and `recover --clean`.
- `pipeline_launch::commit` replaces every `save_state` in the two pipeline files, so a stop or a `yes_ship` revoke written during a multi-day gate wait is no longer overwritten when the wait ends.
- Every production state write in both crates now re-reads under the write lock, two source scans fail on any production `save_state`, and `save_state` is gone from the default-feature `devflow-core` API.
- The CLI's unit tests now answer gates through `preanswer_gate` or the live gate id, and legacy gate-API usages in test code fell from 103 to 22, each remaining one named below with the plan that takes it.
- `gate approve`, `gate reject`, `gate sweep` and `stop` now publish through `Gates::answer` / `Gates::reap_answer` for the gate id they saw, so a decision about one gate can no longer land on the next; the operator can see the id in `gate list` and `gate show` and pass it back with `--gate-id`.
- Every CLI gate wait now accepts only its own gate id's answer and consumes it through one first-wins claim, `devflow ship` claims the same way, and every gate cleanup sets unused answers aside.
- The single per-stage answer slot now exists only as something to set aside: nine `Gates` functions and the transition bridge are deleted, `GateFile.gate_id` is required, and `recover` quarantines an unconsumed answer instead of deleting it.
- `start` and `resume` now reserve at the top, with the live-run guard and the `pending_supervisor` write in one write-lock closure, and hold no long lock. A launcher SIGKILLed during its publish wait leaves a supervisor that a second launch refuses, naming only `devflow stop`.
- Stage N now records N+1 as pending before spawning it, so a SIGKILLed S(N) leaves a state that names the live N+1 and `resume` refuses. Each window this opens has a tested end, and `doctor` no longer calls a pending next supervisor an orphan.
- `status` and `doctor` now name `devflow stop`, never `devflow resume`, while a pending supervisor lives, and `stop` ends one through its socket or clears an abandoned reservation with a report. Both also name an orphaned agent's manual kill and a held state write lock.
- Only the supervisor the state records can now change a run. Each gate side effect shares one write-lock hold with its ownership check, a supervisor decides beside its agent whether it was promoted, and `advance` holds no long lock.
- `gate respond`, `gate sweep`, `stop` and `doctor` now read a waiter only from its gate registration, no CLI path takes the per-phase lock, `stop`'s write gives up after 10 s naming the holder, and `stop` refuses to mark a run beside a launch none of its arms addressed.
- `recover --clean` now decides on a fresh read under the state write lock from the supervisor sockets, the reservation, the gate registrations and the D-17 holder, so it no longer deletes a live supervisor's run (999.166 item 2), and the per-phase pid-lock API is deleted from `devflow-core`.
- A stop now lands before every finalization pass and never inside one: a real `devflow ship` stopped inside `Merge` finishes all four hooks and ships, a stopped `--yes-ship` run sends no notify, and ARCHITECTURE and the `stop` help say what the code does.
- `resume` now closes every stage's gate before it relaunches and names each answer it set aside or found claimed, and `status` and `doctor` list set-aside answers. However, R2-05's premise fails in three reachable cases, in which `resume` calls an applied answer "never applied".
- `stop` now reports a safe-point stop, a pending stop and an orphaned agent as what they are, and the exit file records 143 after a safe-point stop. One finding remains: a `stopped` flag written to disk during a gate wait does not stop the run, and this plan does not cover it.
- The docs now tell an operator how to write a gate answer the code accepts, `status` no longer sends an answer through an `advance` that sets it aside, and the crash table records all 46 windows with 50.1's outcomes.
- The Phase 50 fault arm now passes on the 50.1 binary, reading the Define gate's waiter from `doctor --json`, and `50.1-RUNBOOK.md` gives the procedure for the one live attempt.
- Attempt 1 passed the fault arm and shipped on the 50.1 binary, but only after parking at an unplanned Ship gate that the operator resolved by hand, and it passes the judge only under a rule changed after the attempt started.
- `50.1-LIVE-RUN.md` records attempt 1 as shipped from sanitized published evidence, under a selftested `--attempt-record` judge, and states at its top that the run shipped only after an operator-settled Ship failure gate and under a judge rule changed after the attempt began. Every published-API break is tabled, one of them undeclared.
- `stop`'s third arm now signals the identity-confirmed holder of `.devflow/.holder-NN`, held by `start`, `resume`, `devflow ship` and a hand-run `advance`. Each catches the signal and ends at its next safe point. A hand-run `advance` and `devflow ship` now refuse beside a live supervisor.
- `devflow doctor --json` now names each phase's open gate and whether its registered waiter is live, on every `reconciliation` entry, which is the fact 50.1-26's fault arm reads in place of `status --json`, a command with no `--json` flag.
- A rate-limited Code stage now continues under a configured fallback agent, started by its own supervisor in the same tree on the same branch. Without a fallback, or at any other stage, the existing cron resume still fires. A refused handoff emits an event and then takes that same cron path.
- A `cargo test` gate now fails if `rebase_in` returns anywhere under `crates/`, or if a `sequentagent` or `n` verb returns in any clap `Subcommand` enum, whether as a variant, a `name` or any alias. It reads comment-stripped source and compares values whole, so `name = "__monitor"` does not trip it.
- An idle-timeout failure now says why the agent stopped. The monitor reads `terminal_reason`, `errors` and `origin.kind` from the capture's last top-level `result` event and stores them in the record through a crate-private flattened wrapper. The reason then puts them in one bounded clause right after `idle timeout`, which still fits inside the 300-character gate context. The published `IdleTimeoutRecord` is unchanged.
- An operator can now arm a fallback agent with `devflow start --fallback-agent <AGENT>` and re-arm a spent one with `devflow resume --fallback-agent <AGENT>`. Both verbs refuse a fallback equal to the agent, and in `--mode auto` one outside the Claude/Antigravity launch shape, before they write anything. On `resume` that includes the notify command (review B-3).
- A capture that ends in a torn JSON line is no longer read as a clean silence. The rate-limit check now has a third answer, "torn after the last event". The monitor writes and flags the idle-timeout record for that answer. The reason then opens with `idle timeout (torn final line, a quota denial cannot be ruled out…`. Separately, only top-level events now decide a quota denial: a subagent's `rate_limit_event` or `result` can no longer classify a stage `RateLimited` and so cannot trigger 51-01's failover.
- Per-agent evidence no longer crosses the hop, because the handoff commit clears the canary and the session id. The `agent_handoff` event now records the tree it hands over. A relaunch that fails after the handoff commit now emits `agent_handoff_relaunch_failed` and stops at the never-silent Code gate, inside a real supervisor. Before this plan, that run ended with no gate, nothing running and nothing scheduled. The Phase 50 ownership fence is unchanged.
- A Code failover, a `devflow resume` or an `Ambiguous` re-drive in the middle of an audit-fix, gaps-only or full-execute pass now relaunches that pass, not the default execute-phase prompt. `State.code_fix` holds the fix type. The prompt is rendered at launch by the current agent's driver. A Codex fallback therefore gets the workflow-style audit-fix instruction and never Claude's slash command. The tests check the argv that a spawned stub actually received.
- `devflow resume` with the notify variable unset is now pinned to keep the saved command, shown red on the WR-13 defect itself; WR-12 and IN-10 no longer reproduce at gate entry; both notify doc comments are pinned to the code by source scans.
- Failover depends on SUPV-01's publish wait exactly as an ordinary Code launch does. With the probe answer withheld for one Code launch, neither the failover relaunch nor an ordinary Plan → Code transition publishes a successor, and no agent group survives. Failover's failure is visible (an event, then the never-silent Code gate). The ordinary transition's is not: it leaves no record of why. The cron arm's later `resume` reads the recorded socket exactly as every `resume` does and fails closed. The evidence file's trace rows are now checked row by row by a Rust test.
- The operator docs now say how to arm a Code-stage fallback by flag or by `devflow.toml`, what it does and does not do, which events it writes, and how to recover when its relaunch fails. A row-scoped `doc_check` test pins this. The phase gate is green on the host and in the pinned container. The scan found no advisories, and semver-checks reports no update required.
- Three tests through the real `devflow` binary show the auto-mode failover at Code. The first runs `devflow start --mode auto --until code --fallback-agent antigravity`: Claude runs Define, Plan and Code, is rate-limited on its Code turn, and Antigravity runs its own delivery canary inside the supervisor and then completes Code on the same branch. In the second, Antigravity's canary withholds the token and the run ends at the Code gate, not on the cron arm. In the third, without a fallback, the same rate limit writes cron instructions and `agy` is never invoked.
- Every liveness read the two rate-limit arms make has now been withheld with an ordinary control, or recorded as a gap with its reason. Each one disposes failover and an ordinary launch, or a cron resume and an ordinary resume, identically. The only difference is what follows a failed in-supervisor launch: failover opens the never-silent gate, and an ordinary transition is left ungated. The evidence file is checked in complete mode by a named Rust test.
- A standing `fallback_agent = "<agent>"` in `devflow.toml` now arms the rate-limit fallback at `devflow start`. A typed `--fallback-agent` wins the value. A key-only fallback prints a notice naming `devflow.toml`. A broken key refuses `start` even when a valid flag is given. No environment variable can select the fallback. `DevflowConfig` is unchanged.

---

## v2.8.0 Remaining Harness Support + Pi Dogfood (Shipped: 2026-09-03)

**Phases completed:** 6 phases, 16 plans, 26 tasks

**Closed as `override_closeout`.** Known verification overrides: 0 newly acknowledged, 9 carried
forward from a prior close (see STATE.md Deferred Items).

### Known Gaps

- **DECN-01** (999.94) — partially delivered. `CODE_STAGE_POLICY` reaches the first Code pass for
  both agent families but not the Claude/OpenCode `fix_prompt` loop-back, and is contradicted by
  `checkpoint_auto_decide_prompt` for blocking-human gates. Deferred by operator decision to
  backlog **999.115** / **999.116**.
- **AUTO-01** live `devflow start --mode auto` end-to-end run — not performed (no committed
  `base_branch` in this repo). Deferred to backlog **999.119**; the code is unit/integration-verified
  with a fork-point negative control.

**Key accomplishments:**

- **Pi dogfooded** (Phase 40) — the shipped v2.7.0 Pi driver survived a real supervised
  Define→Validate run with a witnessed reviewer-subagent dispatch and a live gate; the two 999.85
  stale comments rewritten (MAINT-01); 3 Pi-transport regression tests (marker-less, non-zero-exit,
  hung).
- **Antigravity CLI driver** (Phase 41) — `--agent antigravity` launches `agy` headless
  (stream-json, `--print-timeout 60m`, prompt on stdin), agent-aware `CloseRule`/idle-timeout,
  6-driver conformance enrolled. Plus HYG-01 (phase-7 tests reap their own monitors: 0 leaked, was
  43) and HYG-02 (`check-in-container.sh` under uid 0 from worktree + main checkout).
- **Hermes driver** (Phase 42) — `hermes -z "<prompt>" --yolo --accept-hooks` headless on a Legacy
  monitor, process-exit + `DEVFLOW_RESULT` completion contract. The Antigravity supervised dogfood
  in this phase unlocked `--mode auto` for Antigravity at the C2 preflight gate (ANTG-04).
- **OpenCode stub completed** (Phase 43, 28 → 569 lines) — real `opencode run … --auto --format
  json` argv; `parse_opencode_event_result` (torn-tail → error-anywhere → last-marker)
  regression-tested against three real live captures; fail-closed `health` (exit-success AND
  positive credential count) and header-anchored `capabilities` probe. A code review found 4
  fail-closed gaps — all fixed same-phase.
- **Codex verified end-to-end** (Phase 44) — `devflow resume --phase 900 --agent codex` drove a
  throwaway phase's Code and Validate stages to a clean finish (2 real commits, 0-finding review,
  passing validation); surfaced stale `phase7_cli.rs` / `pre_push_signing_policy.rs` assertions
  closed; driver contract byte-unchanged (D-04); 7 post-hoc review findings all fixed. Shipped
  v2.11.0 (PR #154).
- **Unattended `--mode auto` hardening** (Phase 45) — `config::base_branch` resolver (env > file >
  `develop`) resolved once and fed to both the worktree fork point and the git-flow merge target
  (AUTO-01); `affects_compiled_binary` scoped to Cargo workspace members, ignoring
  `.planning/spikes/` (AUTO-02); shared merit-based `CODE_STAGE_POLICY` decision-checkpoint
  constant (DECN-01, partial). Shipped v2.12.0 (PR #203).

**Released incrementally:** v2.9.0 … v2.12.0. **Milestone audit:**
`.planning/milestones/v2.8.0-MILESTONE-AUDIT.md` (status `tech_debt`). **Follow-up backlog:**
999.115, 999.116, 999.118, 999.119, 999.120, 999.121.

---

## v2.7.0 Pi End-to-End + Driver Contract Completion (Shipped: 2026-08-18)

**Phases completed:** 3 phases, 2 plans, 15 tasks

**Key accomplishments:**

- Pi subagent dispatch is VIABLE: `@bacnh85/pi-subagent` works as-shipped (synchronous, in-process, fails closed headless), so the full-dispatch arm is re-scoped to `MonitorLaunch::Legacy` with no `CloseRule`/drain gate.
- Deleted the legacy `AgentAdapter` surface, migrated all call sites to `AgentDriver`, wired the driver-driven `InteractivityMode` gate, and fixed both 999.107 Codex-parser defects — Claude launch argv byte-identical, 633 + 322 tests green.
- Pi driver health reads the active provider from settings.json (google fallback), Pi pinned to Legacy launch with a discriminating regression test, and capability detection matching only the vetted @bacnh85/pi-subagent — with a FIX-FIRST adversarial review closed.

---

## v2.6.0 Multi-Agent Adapter Migration (Shipped: 2026-08-16)

**Phases completed:** 2 phases, 6 plans, 0 tasks

**Key accomplishments:**

- 36-01 (tracer, wave 1) — `AgentKind::Pi` + `PiAgent` + preflight + doctor integration.
- 36-02 (wave 2) — 999.104 (deterministic release signing + probe removal) + 999.96
- 37-01 (tracer, wave 1) — `StageIntent` enum + per-driver `render_prompt` for all five stages + `fix_prompt`.
- 37-02 (wave 2) — `AgentDriver` contract + `ClaudeDriver`/`OpenCodeDriver` with zero regression.
- 37-03 (wave 3) — migrate Codex and Pi onto `AgentDriver`; all four agents now resolve through it.
- 37-04 (wave 4) — `test_contract()`, `DriverHealth`/`InteractivityMode`, docs, and the

---

## v2.5.0 Loop-Termination and Release Hardening (Shipped: 2026-08-15)

**Phases completed:** 4 phases, 15 plans, 22 tasks
**Closeout type:** override_closeout — work shipped and released, but Phases 35.1 and 35.2 were
marked complete without formal `-VERIFICATION.md` artifacts (their verification lives in
`-DRILL.md` / `-VALIDATION.md`). Recorded here rather than silently treated as `verified_closeout`.

**Key accomplishments:**

- 999.77 (criterion 1).
- `advance_with_worktree_declared_checkpoint_reads_the_execution_root`
- `release --check`'s tag-signing preflight now signs throwaway bytes with `ssh-keygen -Y sign` and reports the exit code, replacing the `ssh-add -l` predictor that false-negatived live on two release cuts with the correct key present.
- 999.78/WR-01 — the bound.
- 999.79 / criterion 3 — the rule.
- The deliverable here is an enumeration, and the failure mode is enumerating from memory.
- A real `devflow __monitor` process now holds GSD's `workflow._auto_chain_active` true for exactly the lifetime of a `Mode::Auto` Code-stage child, proven by the child's own reading of the config file while it ran, with a supervise-mode arm that reports the opposite.
- A real `SIGKILL` is shown to leave GSD's `workflow._auto_chain_active` set — and the next real `devflow resume` is shown to clear it in the working tree, clear it at the branch tip with a scoped commit, refuse that commit when the file carries an operator's edit, and announce every repair on stdout and in `events.jsonl`.
- An unattended launch is now refused, before any agent is spawned, when the GSD config cannot hold the chain flag, the Code stage would not launch on the pipe-owning arm, or a plan declares a checkpoint no mode auto-approves — with each of the three watched refusing a fixture built to make it refuse, and each paired with a passing counterpart one change away.
- The chain-flag mechanism works. The drill was run end-to-end against a real Claude agent. Auto-approval is blocked by a GSD orchestrator behavior (upstream #3370) that conflates `gate="blocking"` with `gate="blocking-human"` — a narrow edge case in practice because GSD's default `end-of-phase` mode suppresses the checkpoint tasks that trigger it.
- `verification_authored_this_run` now refuses to read provenance from a content fingerprint alone: a `verification_run_nonce` DevFlow stamps per Validate dispatch gates the decision, and its absence forces a full execute — closing 999.89's residual without changing the artifact format or asking GSD to write anything.
- The cross-repo branch-naming coincidence that keeps `{N}-VERIFICATION.md` from being routinely replaced is now pinned by two tests in `phase_id.rs` — and the pin revealed the assumption behind it was already safe: production code was using `padded()` all along.
- complete — 2/2 tasks, 4 captures committed.
- complete — 1/1 task.
- complete — 2/2 tasks.

---

## v2.4.0 Resume Unattended Dogfooding (Shipped: 2026-08-06)

**Phases completed:** 2 phases (33, 34), 12 plans, 25 tasks
**Closeout type:** override_closeout — see Known Gaps below. All phases verified
(`phase_complete: true`, `verification_status: passed`) and the pre-close artifact audit was clear,
but DOGFOOD-04's traceability row remains `Pending`.

**Delivered:** the structural defects blocking unattended, multi-wave `devflow start` runs are
closed — the loop can now survive a healthy 3+ wave phase, the Validate stage's reported outcome
reflects derived status rather than the agent's self-report, and Layer 0 verification is no longer
inert in worktree mode.

**Key accomplishments:**

- **999.66 / DOGFOOD-02 — the loop no longer false-gates on healthy work.**
  `handle_validate_outcome`'s consecutive-failures counter resets on a real new commit and only
  accumulates when a Validate→Code loop produces nothing, so a healthy 3+ wave phase reaches the
  end while a genuinely stuck loop still hits `MAX_CONSECUTIVE_FAILURES`.

- **999.65 / DOGFOOD-01 — loop-back fix selection reads the right root.** `select_loop_back_fix`
  reads `{N}-VERIFICATION.md` from the phase's worktree rather than the main checkout, making
  `FixType::GapsOnly` reachable on the Validate path in worktree mode for the first time. Proven by
  a test that failed with the inverted value against unchanged code.

- **999.74 / DOGFOOD-04 — the Validate trust boundary.** `reconcile_layer0_verdict` now consults
  Layer 1's own status before transplanting its verdict, and `classify_validate_outcome` was
  rewritten as an exhaustive match naming all seven `AgentStatus` variants with no wildcard in the
  status position. An agent-written `verdict: pass` attached to its own `status: failed` no longer
  advances to Ship unattended — the exploit was reproduced against the real cascade before the fix,
  and pinned by a regression test with a matched positive control.

- **999.73 / DOGFOOD-03 — every stream-json stage joined on real evidence.** All five `Stage`
  variants widened onto the stream-json launch path against committed, PII-scrubbed production
  captures with per-stage drain analysis, rather than on assumption. **The campaign refuted its own
  premise** — zero `background_tasks_changed` events across 1063 events despite 8 concurrent
  sub-agent dispatches — and that was filed as backlog 999.83 rather than quietly absorbed.

- **999.76 — Layer 0 works in DevFlow's default operating shape.** External-verification discovery
  reads the execution root, so a correctly-declared probe set no longer silently never executes in
  worktree mode.

### Known Gaps

Recorded rather than waved through — this is an `override_closeout`.

- **DOGFOOD-04 traceability row is `Pending`, not `Complete`.** Criteria 3 and 4 close the core
  self-report-vs-derived-status guarantee directly, with live tests and negative controls. What
  remains is 999.76's second call site — `phase_has_blocking_human_checkpoint` under
  `Action::GateReview` — which is correct by construction but has **no regression guard**:
  reverting its root argument leaves the full 279-test binary suite green. Phase 34's UAT closed
  this by operator attestation rather than demonstration. Tracked as **999.84 / DEN-106**.

- **999.85 / DEN-107** — two in-source comments (`idle_timeout_result`'s doc comment and a residual
  instance inside `stream_success_cannot_stand_against_nonzero_exit_code`) still justify themselves
  by mechanisms this milestone's own fixes deleted. Conclusions correct, reasons stale. Surfaced by
  the Phase 34 security audit.

- **999.83** — the drain gate, the safety mechanism the widened stages' unattended behaviour
  depends on, is currently proven *not* to see sub-agent concurrency on Claude CLI 2.1.222.

- **No `/gsd-audit-milestone` was run** before this close. Requirements coverage was checked against
  REQUIREMENTS.md's traceability table and all phases were independently verified, but the
  cross-phase integration and E2E-flow audit that `/gsd-audit-milestone` performs did not happen.

### Release status

**This milestone close is a planning-state operation only — v2.4.0 is not released.** At close:
the workspace version is still `2.3.0`, `CHANGELOG.md` has no 2.4.0 section, the work sits on
`feature/phase-34` (66 commits ahead of `develop`, unmerged), and no `v2.4.0` tag exists. Cutting
the release requires the project's real release path — PRs through both protected-branch hops, the
version bump, the changelog entry, a signed tag, and publishing core before cli.

---

## gsd-hygiene GSD Workflow Hygiene (Shipped: 2026-08-04)

**Phases completed:** 1 phases, 1 plans, 3 tasks

**Key accomplishments:**

- Confirmed the ROADMAP.md layout fix (HYGIENE-01/02/03) had already landed as a side effect of creating this milestone, then closed the loop: flipped the 999.72/999.72a backlog entries, documented a durability convention, and independently re-verified the phase goal from scratch.

---

## v2.3.0 the unattended run (Shipped: 2026-08-04)

**Phases completed:** 2 phases (30-31), 10 plans

**Key accomplishments:**

- A Claude `--output-format stream-json` JSONL capture now produces a real Layer-1 verdict through `evaluate_layer1` — last-result-wins across turns, with `decided_by_layer` derived rather than trusted — while both capture shapes that ship today are proven untouched by runtime assertion.
- `task-notification` delivery survives DevFlow's production launch environment — 8 trials, 3 environments, 0 refutations — so Phase 31 could be planned; but 8 successes bound reliability only above ~69%, so it was planned with a retry/timeout path, not a happy path.
- Rate-limit classification, `is_error` failure attribution and `session_id` now survive a Claude `stream-json` capture with the single-document path's precedence and read discipline intact.
- The 0.38s exit figure was refuted by direct measurement (169.5–279.7 ms, median 242.0 ms), closing stdin with pending tasks turned out to be benign rather than undefined, and the ~12s idle-timeout floor was raised to ≥30s after 2 of 7 trials showed a quiet gap above it.
- A Claude stage now launches with the bidirectional `stream-json` argv and is supervised by a std-only Rust monitor that owns both pipes, releasing stdin only when a `DEVFLOW_RESULT` marker has landed in a top-level `result` AND the background-task list has drained — making Phase 30's stream parser reachable from a production launch path for the first time.
- A nonce-canary guard now distinguishes "the behaviour is gone" from "the guard could not run," and a stream-vs-exit-code arbitration layer resolves disagreements between the two signals, with `--legacy-claude-launch` as a documented, off-by-default escape hatch.
- Live acceptance run **passed on attempt 1** (D-18: both plans produced a SUMMARY.md and both merged), independently verified from git — two merge commits with independent-forking parents — closing the 999.64 arc that has blocked every prior unattended-run attempt in this project's history.

**Review:** one adversarial plan review + one peer code review across the milestone; 3 CRITICAL/HIGH findings fixed with mutation-proven tests (`522e905`), 1 filed to backlog (999.75, itself resolved 2026-08-04 in `2c20ab4`/PR #82).

**Known verification overrides:** 2 (see `STATE.md` § Deferred Items — an unrelated gsd-core tooling debug session, and a Phase 24 UAT audit false-positive, both acknowledged at close).

Full detail: `.planning/milestones/v2.3.0-ROADMAP.md`

## v2.0.0 (open-ended label, spanned releases 1.2.0-2.2.0) (Closed: 2026-08-02, archived retroactively 2026-08-04)

**Phases completed:** 16 phases (12-25, 27, 28), 125 plans. Phase 26 (closed partial, not shipped)
and Phase 29 (aborted) are explicitly excluded — see `.planning/superseded/26-release-cut-automation/`
and the Phase 29 prose still in `ROADMAP.md`.

**Key accomplishments (representative — see `.planning/milestones/v2.0.0-ROADMAP.md` for full detail):**

- The GSD-native architecture replaced the original tmux-based agent launcher with direct process
  spawning and a monitor daemon, and the CLI surface was rebuilt and then substantially hardened
  across a dozen phases of reliability work (outcome typing, build provenance, preflight gates,
  hermetic git invocation, `main.rs` decomposed from 8,487 to 478 lines with zero behavioral change).

- The `sequentagent` CLI verb was removed as the breaking change the v2.0.0 slot was held open for,
  replaced by `devflow parallel` and a single-agent rate-limit resume path.

- `devflow start --phase N --agent claude --mode auto --yes-ship` reached a completed Ship stage
  unattended for the first time in this project's history (Phase 25), after Phase 23 proved the
  goal unreachable and closed four specific, individually-evidenced blockers.

- A `blocking-human` checkpoint stopped being a dead end for unattended runs — DevFlow can now
  relaunch the exact exited session and resolve the checkpoint itself, with a recorded audit trail
  (Phase 28).

- All 41 production `git` invocations made hermetic against a hostile `GIT_DIR` (Phase 27),
  unblocking both the release executor goal and `devflow sync`.

**Milestone-level decision:** this label was deliberately left open-ended (2026-07-23) rather than
bounded to a phase count, which let it drift across three releases (2.0.0, 2.1.0, 2.2.0) and 16
phases before being closed by operator decision on 2026-08-02. That experience is why the
successor milestone (v2.3.0) was declared bounded from the start.

**Not archived here:** Phase 26 (CLOSED PARTIAL — two review rounds found Critical defects no test
caught; re-opened as backlog 999.25) and Phase 29 (ABORTED after independent cross-AI review found
5 Criticals + 1 High). Neither shipped, so neither belongs in a "shipped" milestone archive.

Full detail: `.planning/milestones/v2.0.0-ROADMAP.md`

## v1.0 Core Workflow, Versioning, and the GSD-Native Rewrite (retroactive label; declared and archived 2026-08-04)

**Phases completed:** 11 phases (1-11), 12 plans (single-plan phases — this project hadn't yet
adopted multi-plan waves).

**No milestone was ever declared for this era at the time** — DevFlow didn't use the "milestone"
concept until v2.0.0 (2026-07-23). This label is a retroactive convenience grouping, applied by
operator decision on 2026-08-04 so this history follows the same archival convention as v2.0.0
and v2.3.0 rather than sitting permanently un-grouped. Not to be confused with the pre-existing
`v1.0-ASSESSMENT.md` (2026-06-17), a one-time feature-comparison snapshot unrelated to this
archive.

**Key accomplishments:**

- Core workflow, versioning, and state machine established (phases 1-5, versions 0.1.0-0.6.0).
- Agent completion protocol, worktree + PR integration, docs/onboarding, and open-source polish
  shipped incrementally (phases 6-9, versions 1.0.0-1.2.0).

- **The GSD-native rewrite (Phase 11)** replaced the original `tmux`-based agent launcher entirely
  with direct process spawning and a monitor daemon — `tmux` stopped being a runtime dependency
  from this point on, and the CLI surface was substantially rebuilt, setting the architecture every
  later phase (12 onward) built on.

Full detail: `.planning/milestones/v1.0-ROADMAP.md`
