---
phase: "50"
slug: "addressable-monitor-liveness"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-10-20"
---

# Phase 50 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Built by `/gsd-secure-phase 50` (State B) from the `<threat_model>` STRIDE tables of all 25 plans (118 rows, counted two ways) and the one SUMMARY threat flag (50-06). Threat IDs are plan-scoped: the same ID can appear in more than one plan, so every row names its plan.

---

## Trust Boundaries

| Boundary | Description | First declared in |
|----------|-------------|-------------------|
| plan gate → cargo semver-checks | the semver gate's pass/fail decides whether a `devflow-core` break may be committed; a vacuous pass ships an unnamed break | 50-01 |
| executor edit → ROADMAP.md | roadmap parsers (`roadmap.analyze`, `init.plan-phase`) read this file for every later GSD command | 50-01 |
| test process → spawned `devflow` child | the child inherits the developer's environment unless each Command overrides it | 50-02 |
| child → `~/.cache/devflow` | the real machine registry (and, from 50-11, sockets) the operator's live runs rely on | 50-02 |
| test process → `devflow` child | inherited HOME/XDG would point the child at the operator's real cache | 50-03 |
| `gate sweep` / stray pass → machine-wide processes | registry-scoped vs machine-wide action | 50-03 |
| lock file on disk → reclaim decision | a pid and start time read from disk decide whether another writer is live | 50-04 |
| path on disk → probe | anything can sit at a recorded path; the probe decides whether the backstop may later arm | 50-05 |
| other local users → socket | a same-machine process could connect and send requests | 50-05 |
| request bytes → server framing | an oversized or slow request could wedge a control thread | 50-05 |
| sidecar file → teardown decision | a pgid, start time and boot id read from disk later decide whether the backstop may signal | 50-06 |
| sweep caller → filesystem | read-only commands must not mutate machine state | 50-06 |
| signal delivery → supervisor | asynchronous; work in a handler is unsafe | 50-07 |
| supervisor → agent process group | a group kill must target only the agent's own group | 50-07 |
| socket client → shutdown | a request decides whether a run is killed | 50-07 |
| supervisor → agent spawn | the agent's process group decides what a later group kill reaches | 50-08 |
| adapter env → agent | values must reach the agent without being exposed more widely | 50-08 |
| sidecar/state on disk → kill decision | pgid, start time and boot id are read from files the operator's account can write | 50-09 |
| /proc snapshot → signal | the process table can change between reading and signalling | 50-09 |
| test build vs release build | a test-only exec path must never be reachable from the released binary | 50-10 |
| in-crate tests → real cache | the test harness process carries the developer's HOME | 50-10 |
| launcher → new supervisor | the launcher records a socket only after it answers; a failed launch must not leave an agent (50-24's teardown, called here) | 50-11 |
| `__monitor` argv → process table | argv is visible to every local user via ps | 50-11 |
| test suite → real cache | sockets now bind under the cache dir on every launch | 50-11 |
| read-only commands → machine state | status/doctor must observe without acting (D-06) | 50-12 |
| doctor output → bug reports | findings get pasted into issues; they must not carry paths or usernames | 50-12 |
| stop request → in-flight advance | a stop must not interrupt a state write or strand the per-phase lock | 50-13 |
| state handle across a handoff | a reader between saves must always find a live address | 50-13 |
| cfg(test) hold points → production | test-only waits must not exist in the released binary | 50-13 |
| operator → `devflow stop` | the one verb that kills processes and removes sockets | 50-14 |
| state/sidecar on disk → stop's targets | values read from disk choose what stop acts on | 50-14 |
| socket client → supervisor | a shutdown request ends a run | 50-14 |
| launch verb → existing run | a launch must not run a second agent over a live one | 50-15 |
| corrupt state → guard | the state that would name the live run is the thing that cannot be read | 50-15 |
| devflow-core public API → crates.io consumers | a published contract; breaks must be named | 50-16 |
| gate script → release decision | a vacuous semver pass would hide a break | 50-16 |
| docs → operator action | an operator follows the documented repair verb on a real run | 50-17 |
| fault arm → host processes | it SIGKILLs supervisors and must never touch anything it did not start | 50-18 |
| rig tools → operator's real cache | the live run uses it; the tools only read and copy matching sidecars | 50-18 |
| operator terminal → live agent run | real agent quota and a real GitHub repository | 50-19 |
| operator reply → plan closure | a reply must not substitute for evidence | 50-19 |
| raw evidence → public repository | captures can carry home paths, session ids and credentials | 50-20 |
| checker output → verdict record | a verdict must come from evidence, not recollection | 50-20 |
| stray-reap test → machine-wide processes | registry-scoped vs machine-wide action | 50-21 |
| CLI → `State` liveness fields | a leftover pid read would be a second liveness source | 50-22 |
| stray census safety set → live runs | dropping an input must not expose a live process to a reap | 50-22 |
| launcher → state record | the record decides which repair `status` and `doctor` print | 50-23 |
| launcher → `/proc` | the teardown chooses its targets from `/proc` entries another process could recycle | 50-24 |
| raw evidence → committed `50-evidence/` | the published tree is the only copy of the live run that enters the repository | 50-25 |
| judge verdict → 50-20 gates | a gate trusts the judge's exit code alone | 50-25 |

---

## Threat Register

| Plan | Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|------|-----------|----------|-----------|----------|-------------|------------|--------|
| 50-01 | T-50-02 | Tampering | `.planning/ROADMAP.md` | medium | mitigate | Scoped `Edit` only; folded items on a separate line so `phase_req_ids` stays `SUPV-01`; gate runs `roadmap.analyze` and `init.plan-phase` | closed |
| 50-01 | T-50-42 | Repudiation | `tools/semver-gate.sh` false green hiding an unnamed `devflow-core` break | high | mitigate | item-level comparison against one literal named-break table, staged per plan; selftest proves unlisted, unexpected-item, missing, not-yet-happened and empty runs fail; the real run matches `baseline` and mismatches `--through 50-08` on the same output (moved from 50-16 with the tool) | closed |
| 50-01 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package is installed by this plan (RESEARCH Package Legitimacy Audit: none); any new crate would need a `checkpoint:human-verify` and `cargo deny check` | closed |
| 50-02 | T-50-03 | Tampering | e2e children writing the real cache | high | mitigate | `hermetic_devflow` sets HOME, XDG_CACHE_HOME, XDG_CONFIG_HOME and DEVFLOW_CACHE_DIR per child and strips GIT_*; sentinel-HOME gate proves no leak; V-26 proves the check can fail | closed |
| 50-02 | T-50-04 | Denial of Service | monitors leaked by e2e runs | medium | mitigate | `LaunchedRunReaper` bound right after each launching spawn; the home outlives the reaper | closed |
| 50-02 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed; `tempfile` is an existing dev-dependency | closed |
| 50-03 | T-50-05 | Tampering | e2e children reaching the real cache, or a real stray reap acting on the operator's processes | high | mitigate | Hermetic builder on every spawn; sentinel-HOME gate per task; `no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` refuses any non-dry-run `--reap-strays` in a suite on the builder | closed |
| 50-03 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-04 | T-50-06 | Tampering | `acquire_after_existing` reclaiming a live writer's lock | high | mitigate | Reclaim only on NoHolder/Recycled from `classify_holder_record`; Live and Unconfirmable stay contended; two control tests pin it | closed |
| 50-04 | T-50-07 | Tampering | two reclaimers racing | medium | mitigate | Change stays inside the coordination flock; `stale_reclaim_is_serialized_before_it_removes_the_lock_path` gated unchanged | closed |
| 50-04 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-05 | T-50-08 | Elevation of Privilege | `probe` classifying a planted file as STALE | high | mitigate | `symlink_metadata` + `is_socket()` + uid == euid before connect; only ECONNREFUSED is Stale; V-05 pins regular file, directory, symlink, mode 000 | closed |
| 50-05 | T-50-09 | Tampering | publication replacing a live socket | high | mitigate | hard_link publish (EEXIST is an outcome), never rename; V-11 proves a second publish fails and the first still answers | closed |
| 50-05 | T-50-10 | Spoofing | another user connecting | medium | mitigate | `sock/` 0700, socket 0600, `peer_is_same_user` on the server (SO_PEERCRED on Linux; `getpeereid` on macOS, compile-checked only; reject elsewhere, T-50-87); symlinked or foreign `sock/` refused | closed |
| 50-05 | T-50-11 | Denial of Service | oversized or slow request | medium | mitigate | 4 KiB line cap and 2 s read/write timeouts in `protocol` | closed |
| 50-05 | T-50-87 | Spoofing | the peer uid check compiled out, or skipped, on a target without `SO_PEERCRED` (MAC-1) | medium | mitigate | `cfg` bodies: `SO_PEERCRED` on Linux, `getpeereid` on macOS, and `false` on every other target, so the server rejects every peer there and the probe reads `NoReply`, never `Stale`; no body returns true without a uid read; the macOS compile gate (V-54) fails on an ungated Linux-only item. Residual: the macOS body's runtime behaviour is unverified (no macOS host, backlog 999.160) | closed |
| 50-05 | T-50-12 | Tampering | silent sun_path truncation | medium | mitigate | `SocketPath::new` refuses >107 bytes by name before any bind; the temporary `<stem>.part` is exactly as long as the final name, so the bound is the same for both (RV-01); V-06 pins 107/108 and the equal lengths | closed |
| 50-05 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed; `libc`, `serde`, `serde_json`, `thiserror`, `tempfile` already present | closed |
| 50-06 | T-50-13 | Tampering | sidecar naming a victim pgid | high | mitigate | sidecar mode 0600 in a 0700 dir owned by the user; identity fields are only an input to 50-09's validation (boot id + leader start-time match, own-uid members), never a direct kill target | closed |
| 50-06 | T-50-14 | Tampering | sweep mutating sock/ (D-06) | high | mitigate | `sweep_in`/`sweep_for_in` call only probe and read; V-22 snapshot test with a control proving removal is detectable | closed |
| 50-06 | T-50-15 | Information Disclosure | sidecar contents (project path) | low | mitigate | 0600 file, 0700 directory | closed |
| 50-06 | T-50-71 | Elevation of Privilege | a broken or missing sidecar hiding a live socket from `sweep_for_in`, so D-09's guard fails open (RV-05) | high | mitigate | an entry with no usable sidecar is attributed by its file name's root digest and phase, or (name outside the scheme) by its ping reply's phase; `a_socket_whose_sidecar_will_not_parse_is_still_found_by_its_name` with another-root and another-phase controls; the sweep stays read-only (the name only widens what is reported) | closed |
| 50-06 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-07 | T-50-16 | Denial of Service | signal handling | high | mitigate | Self-pipe handler with only getpid+write; no blocked mask (agents would become immune to the group TERM); test asserts the agent's SigBlk/SigCgt | closed |
| 50-07 | T-50-17 | Tampering | stop/completion race advancing a stopped run | high | mitigate | All decisions under one owner mutex; V-14 enumerates interleavings with a control; mutants on the decision fns | closed |
| 50-07 | T-50-18 | Elevation of Privilege | group kill hitting pgid 0/1 or the supervisor's own group | high | mitigate | One guard fn (`pid_t` fit, > 1, ≠ getpgrp); group kills only while the leader is unreaped | closed |
| 50-07 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed; libc only | closed |
| 50-08 | T-50-19 | Elevation of Privilege | agent left in the launcher's group | high | mitigate | `process_group(0)` in both shapes plus a `getpgid == pid` assertion before anything is recorded; V-10 with a control | closed |
| 50-08 | T-50-20 | Information Disclosure | adapter env values | medium | mitigate | values passed in `StageSpec.envs` (process env), never argv; 50-11 passes names with `--keep-env` | closed |
| 50-08 | T-50-21 | Denial of Service | TERM orphaning the agent (999.134) | high | mitigate | both shapes on the runtime's self-pipe; V-16 for TERM and INT with an observability control | closed |
| 50-08 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-09 | T-50-22 | Spoofing | reused pgid signalled | critical | mitigate | Step 5 (recycled leader → nothing signalled); V-20 with a control; uid filter on members | closed |
| 50-09 | T-50-23 | Spoofing | leader-gone group signalled | critical | mitigate | Step 6 refuses and reports; V-19 asserts members survive | closed |
| 50-09 | T-50-24 | Tampering | validate→kill TOCTOU | high | mitigate | pidfd opened on the leader, start time re-read after opening, `PIDFD_SIGNAL_PROCESS_GROUP`; documented killpg fallback only on EINVAL; both Linux only, with no signalling path off Linux (T-50-88) | closed |
| 50-09 | T-50-25 | Elevation of Privilege | unsafe target (pgid ≤ 1, own group) or another user's process | high | mitigate | `RefuseUnsafeTarget` before any I/O; own-uid filter first in enumeration; unsafe-target test | closed |
| 50-09 | T-50-88 | Denial of Service | off Linux the STALE backstop declines to signal (MAC-2), so an orphaned agent group behind a STALE socket keeps running | medium | accept | Accepted residual, tied to backlog 999.158: off Linux there is no `/proc` start time, census or boot id, and signalling or removing on facts that cannot be gathered would reopen T-50-22/T-50-23 (an empty census read as `EmptyGroup` would also delete the files of a live group). By design: `identity_provable: false` gives `RefuseIdentityUnavailable` right after the unsafe-target guard (classify table rows, mutation run), the non-Linux signal body sends nothing, and the refusal text gives the manual repair; the operator must end such a group by hand. Compile-checked only off Linux (V-55) | closed |
| 50-09 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed (libc pidfd constants already exported) | closed |
| 50-10 | T-50-26 | Elevation of Privilege | `MonitorExec::HarnessEntry` reachable in production | high | mitigate | Variant behind `cfg(feature = "test-support")`, selected only by CLI `#[cfg(test)]` code; gate asserts the entry names are absent from a release binary | closed |
| 50-10 | T-50-27 | Tampering | in-crate tests writing the real registry/socket dir | high | mitigate | `register_in(ctx.cache_dir)`; cfg(test) launch requires a per-child DEVFLOW_CACHE_DIR; sentinel-HOME gate over the whole in-crate suite | closed |
| 50-10 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-11 | T-50-29 | Repudiation | deleting the shell monitor's TERM tests drops coverage silently | medium | mitigate | deletion only against 50-08's mapping to V-16/V-10/V-08; SUMMARY lists each pair | closed |
| 50-11 | T-50-30 | Tampering | launcher unlinking a live socket | high | mitigate | per-launch names; launcher never removes a path; hard-link publish refuses an existing name | closed |
| 50-11 | T-50-41 | Information Disclosure | adapter env values on the `__monitor` argv (the launch-path half of 50-08's T-50-20) | medium | mitigate | `--keep-env NAME` only; values travel on the Command environment | closed |
| 50-11 | T-50-55 | Repudiation | the `/proc` stray census going blind with green tests once the shell shapes are deleted | medium | mitigate | no `__monitor` matcher is added (it would make live supervisors reap candidates after 50-22); the gap is recorded in the SUMMARY and closed by 50-12's documentation, census test and deleted-root sweep finding | closed |
| 50-11 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-12 | T-50-31 | Tampering | status/doctor mutating sockets or processes | high | mitigate | only `probe`, `sweep_in` and `backstop::assess` are called; snapshot test with a control; gate refuses acting calls in `collect_orphan_supervisors` | closed |
| 50-12 | T-50-32 | Information Disclosure | socket path (contains $HOME) in doctor output | medium | mitigate | findings and JSON identify supervisors by phase + launch id; JSON test asserts no `/` in supervisor fields | closed |
| 50-12 | T-50-51 | Denial of Service | an orphan repair verb that stops the live successor instead of the orphan | high | mitigate | every orphan finding names `devflow stop --phase N --launch <id>`; the test asserts each repair carries `--launch`; 50-14 makes that form act on exactly one socket | closed |
| 50-12 | T-50-84 | Repudiation | a live or stale supervisor whose sidecar is missing or broken never reported by doctor, because the orphan rules read the parsed sidecar's `project_root` (R5-I3) | medium | mitigate | rule (3) lists sidecar-less sockets that `sweep_for_in` attributes to this root by file name, named by `launch_id_in_name`; fixtures (e) ALIVE broken and (f) STALE missing are listed, (g) another root's and (h) a state-matched one are not; gate requires the `sweep_for_in` call | closed |
| 50-12 | T-50-52 | Repudiation | the `/proc` stray census going blind silently after 50-11 | medium | mitigate | census documented as 2.x-only, a test pins that it ignores the supervisor argv, and the deleted-root role moves to the socket sweep finding | closed |
| 50-12 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-13 | T-50-33 | Denial of Service | lock held past exit / abandoned state write on stop | high | mitigate | stops honoured only at safe points under the existing lock guard; gate cancel maps to the existing Abort unwind; V-15 asserts NoHolder | closed |
| 50-13 | T-50-34 | Tampering | GONE gap mid-handoff letting a second launch in | high | mitigate | one save replaces the record (no pre-clear); N unlinks only after advance returns; V-13 with an observer control | closed |
| 50-13 | T-50-35 | Elevation of Privilege | test hold points reachable in production | medium | mitigate | `test_hold` is `#[cfg(test)]`; the env var is read only there | closed |
| 50-13 | T-50-54 | Denial of Service | safe point 3 and the stopped N+1 each waiting on the other (lock vs final line) | high | mitigate | a supervisor takes the lock to record its stop only when state names its launch id; the safe-point-3 test bounds the stop below both waits | closed |
| 50-13 | T-50-68 | Denial of Service | a stopped supervisor whose state will not load skipping `finish()`: its stop caller sits out the final timeout, the socket lingers, and the D-09 repair verb exits non-zero | medium | mitigate | the `Stopped` branch never returns before `finish()`; a load error takes the no-write path and a lock or save error is logged; `a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped` with a loadable-state control | closed |
| 50-13 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-14 | T-50-36 | Repudiation | stop claiming a stop it did not perform | high | mitigate | `persist_stopped_state` records only a performed or confirmed-not-live stop; refusals exit non-zero; the 999.133 class is tested by the probe-error and refusal cases; a `handed_off` reply is retried against the reloaded record at most 3 times and then refused non-zero with nothing recorded (`commands::tests::stop_gives_up_after_three_handed_off_retries_and_records_nothing`, V-57) | closed |
| 50-14 | T-50-37 | Spoofing | shutdown from another local user | medium | mitigate | socket 0600 in a 0700 dir and SO_PEERCRED on the server (50-05); `request_shutdown` checks socket type and uid before connecting | closed |
| 50-14 | T-50-38 | Elevation of Privilege | stop signalling a pid read from state | high | mitigate | stop never signals `monitor_pid`; the rewritten test keeps a planted live process alive; STALE goes only through the validated backstop | closed |
| 50-14 | T-50-65 | Repudiation | a stop on a corrupt state that ends the run but exits non-zero, or a refusal whose named repair does not work | medium | mitigate | the sidecar-found stop skips the state write it cannot make and says so, exiting 0; the several-sockets refusal prints `--launch <id>` forms and a test runs the printed line on the same corrupt fixture | closed |
| 50-14 | T-50-53 | Denial of Service | a teardown aimed at an orphan stopping the live successor | high | mitigate | `--launch <id>` selects one sidecar by launch id, refuses a phase mismatch or another existing project, and in targeted mode writes no state, answers no gate and signals no lock holder; the bare-stop control shows the two forms differ. Exception accepted in 50-23 as T-50-75: a targeted stop reaching N in its post-save remainder, while N is still `Advancing`, is forwarded by N and stops N+1 (the token keeps no stop source; a source-aware forward needs a 50-07 change) | closed |
| 50-14 | T-50-76 | Denial of Service | a live run whose state and sidecar both will not load staying unstoppable because every printed `--launch <id>` repair answers "nothing to stop" (RV-05b) | medium | mitigate | `--launch` also selects an entry with no usable sidecar by `launch_id_in_name`, only when `sweep_for_in` attributes it to this root and phase; V-39 runs the printed line on that fixture after a non-matching id and a wrong phase changed nothing | closed |
| 50-14 | T-50-77 | Elevation of Privilege | a targeted stop signalling an unidentified process group behind a STALE socket that has no usable sidecar | high | mitigate | that case is refused with nothing signalled or removed (no pgid, start time or boot id to validate); V-40 refuses a broken and a missing sidecar while a parsed-sidecar STALE socket in the same fixture is torn down | closed |
| 50-14 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-15 | T-50-39 | Tampering | corrupt state hiding a live run (999.139) → two agents writing one worktree | high | mitigate | unloadable state → `sweep_for_in` by root+phase; V-23 for start and resume with a control | closed |
| 50-15 | T-50-72 | Tampering | corrupt state AND a corrupt or missing sidecar hiding a live run from the guard (RV-05) | high | mitigate | 50-06 attributes such a socket by its file name (or its ping reply's phase); the guard refuses any such entry that is not Gone and names `devflow stop --phase N`; V-37 runs the printed stop, and its other-root control proves the refusal is attributed, not blanket | closed |
| 50-15 | T-50-40 | Elevation of Privilege | fail-open on a probe error or an unprovable STALE group | high | mitigate | ProbeError and any non-provably-gone `assess` decision refuse; only PreviousBoot/EmptyGroup/LeaderRecycled proceed | closed |
| 50-15 | T-50-90 | Tampering | a launcher (`start`/`resume`) that dies during its publish wait, before saving the record (SIGKILL, or a terminal Ctrl-C, which reaches only the launcher because `__monitor` has its own group): a live supervisor with no record, or under the previous GONE record; with a loadable state this guard probes only `state.supervisor`, so a second launch is not refused and `status` may suggest `devflow resume` → two agents in one worktree (50-CRASH-TABLE.md row A7) | medium | accept | operator decision 2026-10-16: resolved in Phase 50.1 (ownership handoff), whose success criteria carry it; until then `devflow doctor` lists such a socket (50-12) | closed |
| 50-15 | T-50-91 | Tampering | stage N's supervisor SIGKILLed after spawning N+1 and before saving N+1's record: N+1 live and unrecorded; `stop` on N's STALE record finds N's group empty, removes its files and reports the phase resumable, and the guard then lets `resume` launch a second N+1 (50-CRASH-TABLE.md row C4) | medium | accept | same disposition as T-50-90: Phase 50.1; `doctor` lists the first N+1 as superseded (50-12) | closed |
| 50-15 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-16 | T-50-43 | Tampering | a leftover `monitor_pid` path becoming a second liveness source | medium | mitigate | the permanent source scan `doc_check::no_monitor_pid_field_json_key_or_help_mention_remains` (comment-stripped field declaration and JSON key across both crates, plus the help snapshot) finds nothing, with planted controls that it must flag | closed |
| 50-16 | T-50-60 | Repudiation | the named-break list widened after it was written, so an unnamed break passes the final stage | high | mitigate | Task 2 fails unless every commit touching `tools/semver-gate.sh` is a 50-01 commit; the semver-gate false-green threat itself is T-50-42 in 50-01 | closed |
| 50-16 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed; cargo-semver-checks 0.51.0 is already on the host | closed |
| 50-17 | T-50-44 | Repudiation | docs overclaiming (macOS support, a migration path) | low | mitigate | explicit "unverified on macOS" and D-20 sentences; gate rejects any macOS-support claim | closed |
| 50-17 | T-50-66 | Repudiation | the architecture or unattended-mode guide still describing the deleted shell monitor or a forked `devflow advance` | low | mitigate | permanent doc scan `doc_check::docs_describe_the_socket_supervisor_not_the_shell_monitor`: section-scoped ARCHITECTURE.md check (no `devflow advance`, names `in-process` and `socket`) and a wrap-tolerant shell-monitor/shell-script check on the guide, both failing on today's text | closed |
| 50-17 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-18 | T-50-45 | Tampering | fault arm acting on the operator's real cache or unrelated processes | high | mitigate | private DEVFLOW_CACHE_DIR and scratch project under /tmp; kills only recorded pids after a start-time check; gate rejects pkill/killall | closed |
| 50-18 | T-50-46 | Repudiation | a fault verdict claimed without records | medium | mitigate | the checker derives the verdict from per-case records; selftest planted failures | closed |
| 50-18 | T-50-56 | Repudiation | a fault verdict on one binary standing in for the attempt's rebuilt binary | medium | mitigate | the fault dir records `bin_sha256` and every case repeats it; `--attempt-gate` gives `shipped` only on a sha match; the selftest plants a mismatch | closed |
| 50-18 | T-50-78 | Denial of Service | the attempt judge failing a correct run to Ship because it asks for sidecars that `finish()` already removed, costing one of the three capped attempts (RV-02) | high | mitigate | per-launch evidence comes from the `stage_launched` fields (launch id named in the socket file name); no sidecar is required; the selftest's zero-sidecar attempt must pass and a launch id missing from its socket name must fail | closed |
| 50-18 | T-50-79 | Repudiation | Task 3's fault verdict taken from an older directory recorded from different crate source (RV-07) | medium | mitigate | the gate judges only the newest `fault-*` directory whose recorded `src_tree` equals HEAD's crate source with `src_matches_checkout` `true`, and refuses an uncommitted crate tree (`--fault-current`); selftest case `fault current picks the current-source directory over a newer other-source one` proves the selector skips a newer other-source copy, and `fault current fails with only other-source directories` that a passing directory from other source gives `dir=ABSENT` and fails | closed |
| 50-18 | T-50-81 | Denial of Service | V-42's re-check failing a correct fault run after any later commit, because build.rs embeds the commit and the rebuilt binary's sha256 no longer matches (R5-W1) | medium | mitigate | the directory is identified by the crate source of the binary it ran (`src_tree` from the commit in its `workflow_started` event), not by sha256; tested: a re-check after a `.planning/`-only commit passes. Residual (accepted, low): a binary built from uncommitted crate edits that were reverted before the run records the commit's source, since the embedded dirty flag covers the whole tree and cannot be used without failing clean runs | closed |
| 50-18 | T-50-82 | Repudiation | a partial fault directory (one case, or a `--cases` subset) on the attempt's binary letting `--attempt-gate` report `shipped` without D-17/R-10's full case set (R5-W2) | medium | mitigate | `--attempt-gate` counts a directory only through `--fault --require-all` (five `ok live` cases and the window `ok pointer`); selftest cases `fault require-all fails a partial directory` and `attempt gate refuses a partial fault directory`, each pinned by name in the Task 2 gate | closed |
| 50-18 | T-50-80 | Repudiation | a fixture pointer counted as a live fault case (RV-09) | low | mitigate | `--fault` prints `ok live` and `ok pointer` distinctly; `--require-all`, which Task 3's gate reaches through `--fault-current`, requires each of the five live cases as `ok live` and the window as `ok pointer`, never `ok live` (selftest case `fault require-all fails a window record marked live`) | closed |
| 50-18 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed; jq/python3/shellcheck already on the host | closed |
| 50-19 | T-50-47 | Tampering | live run aimed at this repository | high | mitigate | the Phase 49 launcher refuses this repository and a non-private remote; Claude takes no GitHub action in this task | closed |
| 50-19 | T-50-48 | Repudiation | closing on the operator's word | medium | mitigate | the verify reads the attempt gate's derived verdict and refuses a commit before the SUMMARY | closed |
| 50-19 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-20 | T-50-49 | Information Disclosure | unsanitized evidence committed | high | mitigate | Phase 49 sanitizer with its fail-closed secret scan; Task 1's gate (`check-supervisor-evidence.sh --published`) rejects any published file naming $HOME, hidden and ignored files included, and any symlink | closed |
| 50-20 | T-50-50 | Repudiation | verdict written from memory | medium | mitigate | verdicts derived on raw evidence, saved as files, quoted in 50-LIVE-RUN.md | closed |
| 50-20 | T-50-86 | Repudiation | the 3.0.0 release note "unverified on macOS" dropped by a squash merge of the phase PR or by a hand-written CHANGELOG section, so the release claims more than Phase 50 established (R6-D) | low | mitigate | 50-LIVE-RUN.md's release handoff: the merge-commit rule with its reason, the tested 3.0.0-section check (exit 1 on omission) to run before the release PR and after the release, and the repair; Task 2's second gate requires the check line verbatim and each handoff element | closed |
| 50-20 | T-50-89 | Denial of Service | a plan after 50-05 and 50-09 adds an ungated Linux-only libc item, so DevFlow 3.0.0 stops compiling on macOS against the operator's decision (MAC-3) | low | mitigate | Task 3's phase-end gate (V-56) runs `cargo check --target aarch64-apple-darwin` for both packages with `--all-targets` on the final tree; `E0425` is a blocking defect, not a backlog entry. It proves compilation only (T-50-87 and T-50-88 carry the run-time residuals) | closed |
| 50-20 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-21 | T-50-57 | Tampering | the five split-off e2e suites reaching the real cache, or the stray-reap suite acting on the operator's processes | high | mitigate | Hermetic builder on every spawn; sentinel-HOME gate per task; `reap_strays_e2e.rs` keeps its never-destructive property | closed |
| 50-21 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-22 | T-50-58 | Tampering | a leftover CLI read of the monitor pid acting as a second liveness source | medium | mitigate | field-access and JSON-key gate over all of `crates/devflow-cli` with an inline pattern control; renamed persistence test asserts `state.supervisor` | closed |
| 50-22 | T-50-59 | Elevation of Privilege | dropping the monitor pid from `registry_reachable_pids` exposing a live supervisor to `--reap-strays` | high | mitigate | the census cannot match the supervisor argv (50-12's pinned test); lock holders stay in the safety set | closed |
| 50-22 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-23 | T-50-62 | Repudiation | a refused or failed launch leaving a record that sends the operator to `devflow resume` (WR-04 class) | medium | mitigate | `record_failed_launch` on every error return of `spawn_agent_and_record` and in `refuse_launch`; three tests including a spawn-time failure with a succeeding control | closed |
| 50-23 | T-50-63 | Denial of Service | a published supervisor left running after the save that should record it failed | high | mitigate | `publish_record` shuts it down through its socket before returning the error; when that shutdown is not confirmed (RV-08) it keeps the record, saves nothing and names `devflow stop --phase N --launch <id>` in the error; tests: confirmed shutdown, unconfirmed shutdown (refused socket), successful save | closed |
| 50-23 | T-50-73 | Denial of Service | a SIGTERM/SIGINT to stage N after safe point 3 dropped, so N+1 runs on after the operator asked the run to stop (RV-04) | high | mitigate | 50-13 keeps N `Advancing` until its advance returns; `run_monitor` forwards a flagged stop to the recorded successor from a check right after the handed-off marking (R5-I1) and answers a parked caller `stopped` only on a confirmed forward; V-35 (signal before and after the save, socket stop) with a no-stop control | closed |
| 50-23 | T-50-83 | Denial of Service | a stop that lands between a post-advance check placed before the handed-off marking and that marking: flagged (N still `Advancing`) but never read, so a signal is dropped (R5-I1) | medium | mitigate | the check runs right after `enter(OwnerPhase::HandedOff)`, so 50-07's owner mutex leaves no flagged-but-unread window; run (d) of V-35's test (V-48) holds N at `before_handoff_mark` and signals it; an order source-scan test over comment-stripped `run_monitor` (`run_monitor_checks_for_a_stop_right_after_the_handed_off_marking`, with planted controls) requires hold, then marking, then check | closed |
| 50-23 | T-50-74 | Denial of Service | a signal landing after N's handed-off marking (and so after its post-advance check) and before `finish` stops N's control thread (RV-04 residual) | low | accept | the span is one phase change plus `finish`'s reply step; a socket stop there is answered `handed_off` and retried; forwarding a signal there needs 50-07's runtime (reported to the orchestrator); stated in the SUMMARY | closed |
| 50-23 | T-50-75 | Denial of Service | a targeted `stop --launch <N's id>` reaching a still-live N after the save being forwarded and stopping N+1 (side effect of the RV-04 forward; 50-14's T-50-53 class) | low | accept | 50-07's token does not expose a stop's source, so N cannot tell a targeted socket stop from a signal; the span is N's post-save remainder (milliseconds) and the forward is reported on N's stderr; a source-aware forward needs a 50-07 change (reported to the orchestrator) | closed |
| 50-23 | T-50-64 | Repudiation | a performed stop recorded as gone-while-active because the stopped successor could not write state | medium | mitigate | post-save re-probe records `stopped` when the successor already ended; a successor still in its kill grace records its own stop because the save precedes the probe; a crashed (STALE) successor is left recorded as found; held-publish tests for a carried-out stop and an in-grace stop (ordered by sync points, with a reversed-release control run, R6-E), with a no-stop control | closed |
| 50-23 | T-50-85 | Elevation of Privilege | the stopped-outcome test hold (`DEVFLOW_TEST_STOPPED_OUTCOME_HOLD`) reachable in a production supervisor, parking a stopped run for up to 60 s | medium | mitigate | `stopped_outcome_test_hold` and its call in `Supervisor::wait_agent` compile only under `cfg(any(test, feature = "test-support"))`, which release builds do not enable; it acts only when the variable names a path and a stop was requested; bounded at 60 s; Task 2's source-scan test (`supervisor::runtime::tests::the_stopped_outcome_hold_is_test_only_and_read_literally`, with planted controls) requires the attribute directly above the fn, and the allowlist reason records it as test-only | closed |
| 50-23 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-24 | T-50-28 | Denial of Service | unpublished agent left running after a publish-wait timeout or an early `__monitor` exit | high | mitigate | on timeout `wait_for_publish` sends SIGTERM, waits, then SIGSTOPs its own child, snapshots that child's direct children, SIGKILLs the child's group and each snapshotted child's own group; `run_supervised_stage`'s `AgentGroupGuard` ends the agent group on any error or panic before publish; `a_supervisor_that_never_publishes_is_ended_with_its_agent_group` (control: a bare SIGKILL orphans the sleeper) and `a_supervisor_that_fails_to_publish_ends_its_agent_group` (control: a published supervisor is left running) | closed |
| 50-24 | T-50-61 | Elevation of Privilege | the NotReady teardown signalling a group it does not own | high | mitigate | children are read from `/proc` only while the launcher's own child is SIGSTOPped (it cannot spawn or reap meanwhile), each child's ppid, pgid and start time come from one read of its `/proc/<pid>/stat` (fields 4, 5 and 22, pinned by `proc_stat_parse_reads_ppid_pgid_and_start_time_after_the_last_paren`), each target must lead its own group and keep its snapshotted start time (`agent::is_same_process`), and every group signal refuses a pgid that does not fit `pid_t`, pgid ≤ 1 and the caller's own group | closed |
| 50-24 | T-50-67 | Elevation of Privilege | `AgentGroupGuard` signalling a recycled pgid after its leader was reaped | medium | mitigate | the guard owns the agent's unreaped `Child` and is armed only after the pgid check passes (the `PgidMismatch` branch keeps its own pid kill), so its group SIGKILL always precedes the reap that would free the pgid | closed |
| 50-24 | T-50-69 | Denial of Service | agent orphaned when `__monitor` is SIGKILLed after spawning it and before publishing (RV-03; no `Drop`, no signal handler runs) | high | mitigate | `run_supervised_stage` writes the sidecar right after the pgid check and pid file; the launcher's ending step reads the expected launch's sidecar and SIGKILLs its group only on `backstop::assess` = `LeaderPresentSignal` with a fresh `agent::is_same_process` re-check; `a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar` (controls: sidecar removed → agent survives; wrong start time → nothing signalled) | closed |
| 50-24 | T-50-70 | Denial of Service | the same SIGKILL landing between the agent's fork and the sidecar's rename (RV-03 residual) | low | accept | no record of the agent's identity exists yet, and signalling an unvalidated pgid would break D-04's rule; the span is `spawn`'s fork-to-exec plus a few syscalls and one small file write; closing it needs a child subreaper (process-wide reaping hazards in the CLI, the supervisor and parallel tests) or an agent self-record in `pre_exec` (async-signal-safe unsafe code), both rejected here; stated in the SUMMARY | closed |
| 50-24 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |
| 50-25 | T-50-92 | Information Disclosure | `--published` passing a tree that still names the operator's home path in a hidden, ignored or binary file | high | mitigate | the search is `rg -uuu` over every file, an empty `$HOME` or an rg error fails, and symlinks fail; selftest cases `published fails a hidden file naming the home path`, `published fails a file naming the home path` and `published fails a symlink` | closed |
| 50-25 | T-50-93 | Repudiation | a judge exiting 0 on a missing input, so a 50-20 gate passes vacuously | medium | mitigate | each judge's absent-input case must fail (`live run fails an absent file`, `handoff fails an absent live run file`, `defects fails outside a git repository`, `published fails without an attempt directory`) | closed |
| 50-25 | T-50-94 | Tampering | this plan's edit changing how `--attempt-gate` judges while 50-19's live attempt uses it | medium | mitigate | ordering: depends_on 50-19, so the edit starts only after the live attempt closes; append-only rule in the context and the acceptance criteria (no removed line over this plan's commits); Task 2's gate re-pins 50-18's 31 cases by name | closed |
| 50-25 | T-50-SC | Tampering | npm/pip/cargo installs | low | accept | No package installed | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-50-01 | 50-01 T-50-SC (low) | No package is installed by this plan (RESEARCH Package Legitimacy Audit: none); any new crate would need a `checkpoint:human-verify` and `cargo deny check` | plan 50-01 threat register | 2026-10-20 (audited) |
| AR-50-02 | 50-02 T-50-SC (low) | No package installed; `tempfile` is an existing dev-dependency | plan 50-02 threat register | 2026-10-20 (audited) |
| AR-50-03 | 50-03 T-50-SC (low) | No package installed | plan 50-03 threat register | 2026-10-20 (audited) |
| AR-50-04 | 50-04 T-50-SC (low) | No package installed | plan 50-04 threat register | 2026-10-20 (audited) |
| AR-50-05 | 50-05 T-50-SC (low) | No package installed; `libc`, `serde`, `serde_json`, `thiserror`, `tempfile` already present | plan 50-05 threat register | 2026-10-20 (audited) |
| AR-50-06 | 50-06 T-50-SC (low) | No package installed | plan 50-06 threat register | 2026-10-20 (audited) |
| AR-50-07 | 50-07 T-50-SC (low) | No package installed; libc only | plan 50-07 threat register | 2026-10-20 (audited) |
| AR-50-08 | 50-08 T-50-SC (low) | No package installed | plan 50-08 threat register | 2026-10-20 (audited) |
| AR-50-09 | 50-09 T-50-88 (medium) | Accepted residual, tied to backlog 999.158: off Linux there is no `/proc` start time, census or boot id, and signalling or removing on facts that cannot be gathered would reopen T-50-22/T-50-23 (an empty census read as `EmptyGroup` would also delete the files of a live group). By design: `identity_provable: false` gives `RefuseIdentityUnavailable` right after the unsafe-target guard (classify table rows, mutation run), the non-Linux signal body sends nothing, and the refusal text gives the manual repair; the operator must end such a group by hand. Compile-checked only off Linux (V-55) | plan 50-09 threat register | 2026-10-20 (audited) |
| AR-50-10 | 50-09 T-50-SC (low) | No package installed (libc pidfd constants already exported) | plan 50-09 threat register | 2026-10-20 (audited) |
| AR-50-11 | 50-10 T-50-SC (low) | No package installed | plan 50-10 threat register | 2026-10-20 (audited) |
| AR-50-12 | 50-11 T-50-SC (low) | No package installed | plan 50-11 threat register | 2026-10-20 (audited) |
| AR-50-13 | 50-12 T-50-SC (low) | No package installed | plan 50-12 threat register | 2026-10-20 (audited) |
| AR-50-14 | 50-13 T-50-SC (low) | No package installed | plan 50-13 threat register | 2026-10-20 (audited) |
| AR-50-15 | 50-14 T-50-SC (low) | No package installed | plan 50-14 threat register | 2026-10-20 (audited) |
| AR-50-16 | 50-15 T-50-90 (medium) | operator decision 2026-10-16: resolved in Phase 50.1 (ownership handoff), whose success criteria carry it; until then `devflow doctor` lists such a socket (50-12) | plan 50-15 threat register | 2026-10-20 (audited) |
| AR-50-17 | 50-15 T-50-91 (medium) | same disposition as T-50-90: Phase 50.1; `doctor` lists the first N+1 as superseded (50-12) | plan 50-15 threat register | 2026-10-20 (audited) |
| AR-50-18 | 50-15 T-50-SC (low) | No package installed | plan 50-15 threat register | 2026-10-20 (audited) |
| AR-50-19 | 50-16 T-50-SC (low) | No package installed; cargo-semver-checks 0.51.0 is already on the host | plan 50-16 threat register | 2026-10-20 (audited) |
| AR-50-20 | 50-17 T-50-SC (low) | No package installed | plan 50-17 threat register | 2026-10-20 (audited) |
| AR-50-21 | 50-18 T-50-SC (low) | No package installed; jq/python3/shellcheck already on the host | plan 50-18 threat register | 2026-10-20 (audited) |
| AR-50-22 | 50-19 T-50-SC (low) | No package installed | plan 50-19 threat register | 2026-10-20 (audited) |
| AR-50-23 | 50-20 T-50-SC (low) | No package installed | plan 50-20 threat register | 2026-10-20 (audited) |
| AR-50-24 | 50-21 T-50-SC (low) | No package installed | plan 50-21 threat register | 2026-10-20 (audited) |
| AR-50-25 | 50-22 T-50-SC (low) | No package installed | plan 50-22 threat register | 2026-10-20 (audited) |
| AR-50-26 | 50-23 T-50-74 (low) | the span is one phase change plus `finish`'s reply step; a socket stop there is answered `handed_off` and retried; forwarding a signal there needs 50-07's runtime (reported to the orchestrator); stated in the SUMMARY | plan 50-23 threat register | 2026-10-20 (audited) |
| AR-50-27 | 50-23 T-50-75 (low) | 50-07's token does not expose a stop's source, so N cannot tell a targeted socket stop from a signal; the span is N's post-save remainder (milliseconds) and the forward is reported on N's stderr; a source-aware forward needs a 50-07 change (reported to the orchestrator) | plan 50-23 threat register | 2026-10-20 (audited) |
| AR-50-28 | 50-23 T-50-SC (low) | No package installed | plan 50-23 threat register | 2026-10-20 (audited) |
| AR-50-29 | 50-24 T-50-70 (low) | no record of the agent's identity exists yet, and signalling an unvalidated pgid would break D-04's rule; the span is `spawn`'s fork-to-exec plus a few syscalls and one small file write; closing it needs a child subreaper (process-wide reaping hazards in the CLI, the supervisor and parallel tests) or an agent self-record in `pre_exec` (async-signal-safe unsafe code), both rejected here; stated in the SUMMARY | plan 50-24 threat register | 2026-10-20 (audited) |
| AR-50-30 | 50-24 T-50-SC (low) | No package installed | plan 50-24 threat register | 2026-10-20 (audited) |
| AR-50-31 | 50-25 T-50-SC (low) | No package installed | plan 50-25 threat register | 2026-10-20 (audited) |

*Accepted risks do not resurface in future audit runs.*

---

## SUMMARY Threat Flags

| Plan | Flag | Verdict |
|------|------|---------|
| 50-06 | fail-open-enumeration: `sweep_in` returns an empty Vec when `sock/` cannot be listed | closed: the launch guard and `recover --clean` call `socket_dir_unlistable` before trusting an empty sweep and refuse (`commands.rs` `possibly_live_sockets`); `the_launch_guard_refuses_an_unlistable_socket_dir_over_an_unloadable_state` and `…_when_the_state_file_is_missing` pass, with the missing-directory control |

---

## Audit Notes (2026-10-20, gsd-security-auditor, L1)

- Evidence per mitigate row (file:line or named test) is in the auditor report; 78 named tests passed in its run (78/78, counted two ways). Mutants, sentinel-HOME suite runs, the real semver-gate `check` and the container `scripts/check.sh all` were taken from SUMMARY records, not re-run.
- **T-50-49 (high, unsanitized evidence committed):** closed on its declared mitigation, but the Phase 49 sanitizer let fragments of the operator's email through; they were redacted by hand in `eb7ba4a` (raw tree unchanged) and the gap is open in `deferred-items.md` (2026-10-20).
- **Outside the register:** the unlistable-`sock/` check covers the launch guard and `recover --clean` only. `stop --launch` and the corrupt-state `stop` sweep without it; on an unlistable directory `stop --launch` reports nothing to stop and returns Ok. Low impact (the launch guard still refuses a relaunch); recorded in `deferred-items.md`.
- **T-50-30 wording:** "the launcher never removes a path" stopped being literally true in 50-24 (`wait_for_publish` removes its own launch's files after reaping their only server); the threat stays closed by per-launch names and a publish that refuses an existing name.
- **T-50-SC (50-11):** "no package installed" — `39c2af8` added a `libc` dev-dependency to devflow-cli, already in the lockfile, so no new package entered.
- Not established by an L1 audit: bypass tracing (L2/L3), stability of timing-dependent tests (each ran once), macOS runtime behaviour (compile-only; 999.158, 999.160), V-53 (at the 3.0.0 cut).

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-20 | 118 | 118 | 0 | gsd-security-auditor (L1, block_on high) via /gsd-secure-phase 50 |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-20
