# Phase 50 evidence layout

Where every Phase 50 tool reads and writes evidence. It is the Phase 49 layout
(`../../49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md`, unchanged and still the
definition of every attempt path) at a different home, plus two additions: `supervisor/` inside an attempt and
`fault-<M>/` beside the attempts. Change this file together with the tool that writes the path.

## Home

`P50` is `$HOME/.cache/devflow-phase50`. The Phase 49 tools take it as `--home "$P50"`; their transient unit names stay
`p49-a<N>-*` (hard-coded, accepted for Phase 50). `fault-arm.sh` takes its directory as `--out`, and
`check-supervisor-evidence.sh` takes `P50` as the argument of `--fault-current` and `--attempt-gate`. No
environment variable overrides any of these paths, and none is `DEVFLOW_`-prefixed.

## Attempts (Phase 49 layout, at `P50`)

- `$P50/current-attempt`, `$P50/bin/devflow-<12 hex>`: as in Phase 49.
- `$EV` is `$P50/attempt-<N>`, `N` one digit 1-9. Everything Phase 49 lists under `$EV` keeps its meaning:
  `attempt.json` (its `bin_sha256` is the pinned binary's), `identity.txt`, `project/`, `prelaunch/`, `units.txt`,
  `start.stdout`, `start.stderr`, `gates/` and `ledger.jsonl` (snapshot root `R` = `$EV`), `surv/`, `collected/`
  (with `DONE` written last) and `verdict.json`.
- Phase 50 has no negative-arm requirement, so `$EV/negctl/` is not written and nothing reads it.
- `$EV/supervisor/`, written by `check-supervisor-evidence.sh collect-sidecars "$EV" --project "$EV/project"` after
  the Phase 49 collection: copies of the sidecars `<cache>/sock/*.json` whose `project_root` is the attempt's project
  (`<cache>` is `registry::cache_dir()`: `DEVFLOW_CACHE_DIR`, else `XDG_CACHE_HOME/devflow`, else
  `~/.cache/devflow`), and `status.txt`, the `devflow status` output at collection time. Evidence only: `finish()`
  removes each finished launch's socket and sidecar (50-07), so a correct run to Ship leaves none, and no check
  requires one (RV-02). `--attempt` reports how many are left as `ATTEMPT sidecars_left=<n>`.

## Fault arm (`fault-arm.sh run --out "$P50/fault-<M>"`)

`M` is the next unused number (`fault-10` follows `fault-9`; tools sort with `sort -V`). The fault arm refuses an
existing directory and never rewrites a record; a failing run is kept and the next run takes a new `M`.

- `bin_sha256`: the sha256 (64 hex) of the binary the run used, written before the first case. A re-hash after
  any case that differs stops the run.
- `src_tree`: four lines, the object ids of `crates`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` at the
  commit the binary embeds (the `commit` field of the first `workflow_started` event the binary wrote), resolved in
  the checkout holding `fault-arm.sh`; or the single word `unknown` when that commit is empty or does not resolve
  there (R5-W1).
- `src_matches_checkout`: `true` only when that checkout's crate paths were committed (`git status --porcelain --`
  on the four paths printed nothing) and their `HEAD:` ids equal `src_tree`; else `false`.
- `case-<name>.json`, one per case:
  - live cases (`"live": true`): `alive-stop-stream`, `alive-stop-capture`, `kill9-stale-stop`, `kill9-at-gate`,
    `stop-races-completion`. Every one carries `case`, `live`, `bin_sha256`, `launch_shape`, `socket_path`,
    `launch_id`, `supervisor_pid`, `probe_before`, `probe_after`, `members_before`, `members_after`, `advance_ran`,
    `next_stage_launched`, `stage_launched_count`, `stop_exit`, `stop_output`, `expected_met` and `unmet` (the
    reasons `expected_met` is false). Probes are the word `devflow status` prints on its `supervisor:` line
    (`ALIVE`, `STALE`, `GONE`, or `ABSENT`/`ERROR`). Members are `ps -o pid,pgid,sid,lstart,args` lines for every
    process whose process group (`/proc/<pid>/stat` field 5) is the sidecar's `agent_pgid`; `ps -g` is not used,
    because procps-ng selects sessions with it. Case-specific fields: `probe_after_kill`, `supervisor_killed` and
    `members_after_kill` (the kill cases); `gate_open_before`, `lock_holder_before`, `lock_held_before`,
    `lock_holder_after` (`none`, `live`, `recycled` or `unconfirmable`) and `gate_response_written`
    (`kill9-at-gate`); `state_stopped` (all but `kill9-at-gate`); `next_agents_alive` and `first_agent_exit`
    (`stop-races-completion`).
  - the window pointer (`"live": false`): `publish-unlink-window` with `fixture`, the fixture test that reaches the
    publish/unlink window
    (`pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor`),
    and `reason`. `--fault` prints it `ok pointer`, never `ok live` (RV-09).
- `verdict-*.txt` (optional, written by the operator per the runbook): the `--fault --require-all` output for that
  directory, so the published evidence carries the verdict line.

The fault arm's own scratch (`/tmp/df50f.XXXXXX`: private HOME, XDG and `DEVFLOW_CACHE_DIR`, one scaffolded project
and one fake-agent bin dir per case) is removed by its EXIT trap; nothing under it is evidence.

## Judges

| Mode | Reads | Verdict line | Exit 0 when |
|------|-------|--------------|-------------|
| `--fault <dir> [--require-all]` | `fault-<M>/` | `FAULT verdict=pass\|fail` (and `FAULT complete=yes\|no`) | every record passes, `bin_sha256` is valid (and the full case set is present, each once) |
| `--src-check <dir> [--repo <checkout>]` | `fault-<M>/src_tree`, `src_matches_checkout` | `SRC tree_is_head=… recorded_matches=… expected_matches=…` | `src_tree` is the `HEAD:` crate ids and the flag matches the checkout |
| `--fault-current <P50> [--repo <checkout>]` | every `fault-*/` | `FAULT_CURRENT verdict=pass\|fail` | crate source committed and the newest current-source directory passes `--require-all` |
| `--attempt <EV>` | `$EV/` (Phase 49 positive arm) and `$EV/collected/devflow/events.jsonl` | `ATTEMPT verdict=pass\|fail` | both halves pass |
| `--attempt-gate <P50>` | `current-attempt`, `attempt-*/`, `fault-*/` | `ATTEMPT_GATE verdict=shipped\|cap_reached\|not_ready` | `shipped` or `cap_reached` |
