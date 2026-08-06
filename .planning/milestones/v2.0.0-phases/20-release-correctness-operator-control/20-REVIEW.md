---
phase: 20-release-correctness-operator-control
reviewed: 2026-07-23T00:00:00Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - crates/devflow-core/src/version.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/release_check.rs
  - crates/devflow-cli/tests/log_format_env.rs
findings:
  critical: 2
  warning: 3
  info: 1
  total: 6
status: issues_found
---

# Phase 20: Code Review Report

**Reviewed:** 2026-07-23T00:00:00Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Reviewed the version-file self-pin rewrite, the liveness-gated worktree
cleanup with bounded retry, the new `--until` plan-only stop path, the
read-only `release --check` preflight, and the manual `ship --phase`
override. The release-cut preflight is genuinely read-only and
network-independent (no `git fetch`, no mutating git subcommands) — that
part checks out. Two BLOCKER-level correctness bugs were found:

1. `find_version_in_contents` (version.rs) mis-extracts a quoted version
   value when anything follows the closing quote on the same line (a
   trailing TOML comment). This silently corrupts `read_version` /
   `read_major_version`, and — critically for this phase's own
   `release --check` self-pin invariant — `read_workspace_self_pins`'
   `workspace_version` extraction, causing spurious self-pin "drift"
   failures (or outright parse errors) the moment a real Cargo.toml has an
   inline comment next to its version field. `write_version` explicitly
   tests for and preserves this exact pattern (GAP-6 comments/tests), so
   the read path is asymmetric with the write path it's meant to mirror.

2. `devflow cleanup`'s new liveness guard (20b) was never updated to
   account for the new `state.stopped` field this same phase introduces
   (20c, `--until`). A phase halted with `devflow start --until <stage>`
   has `monitor_pid` cleared and its agent already exited by design — this
   reads as `Liveness::Unknown` with `agent_alive == false`, which sails
   straight through the "refuse on live agent" guard. An ordinary
   `devflow cleanup` (no `--force` needed) will silently delete the
   worktree of a phase the operator explicitly parked for a later
   `devflow resume`, stranding the resume path. `doctor`'s own
   reconciliation checks (`check_dead_agent`/`check_dead_monitor`) were
   correctly taught about `facts.stopped` in this same phase — `cleanup`
   was not, and no test exercises the interaction.

Three WARNING-level issues are lower-confidence or narrower in scope but
worth fixing: `print_dry_run` never surfaces `--until` in its preview, a
manual `devflow ship --phase` invocation can block the foreground CLI for
the full gate timeout (default measured in days) if terminal hooks fail,
and `member_depends_on`'s dependency-edge parser silently misses the
`[dependencies.name]` long-form section syntax used by `publish_order`'s
release-safety-critical topo-sort.

## Critical Issues

### CR-01: `find_version_in_contents` corrupts the parsed version when trailing content follows the closing quote

**File:** `crates/devflow-core/src/version.rs:451-476` (the bug is on line 472)
**Issue:**

```rust
let value = value.trim();
if value.starts_with('{') {
    continue;
}
return Some(value.trim_matches(['"', '\'']).to_string());
```

`value.trim_matches(['"', '\''])` only strips quote characters that sit at
the very start/end of the *whole remaining string*. For a line like

```toml
version = "1.7.0"  # pinned release version
```

`value` (after `.trim()`) is `"1.7.0"  # pinned release version`. The
leading `"` is stripped (start matches), but the string's actual last
character is `n` (from "version"), not a quote — so the trailing `"`
right after `1.7.0` is **never stripped**. The function returns
`1.7.0"  # pinned release version` instead of `1.7.0`.

This is not cosmetic:

- `read_version` (line 166) feeds this into `parse_version_str`, which
  splits on `['.', '+', '-']`. The corrupted patch token
  (`0"  # pinned release version`) fails `.parse::<u32>()`, so
  `read_version` returns `Err(VersionError::Parse(_))` for any version
  file with a trailing comment on its version line — a hard failure on a
  pattern this project's own tests treat as supported (see
  `write_version_preserves_trailing_comment_in_toml` /
  `write_version_preserves_trailing_comment_in_single_quoted_toml`, which
  assert `write_version` faithfully *preserves* such a comment).
- `read_workspace_self_pins` (line 393, added this phase for
  `release --check`) calls the same function to extract
  `workspace_version`. If `[workspace.package] version = "1.7.0"  # ...`
  carries any trailing comment, `check_self_pin` (commands.rs:1325) will
  compare a clean pin value (`"1.7.0"`, correctly extracted from the
  inline-table fragment) against the corrupted workspace version
  (`"1.7.0\"  # ..."`) and always report spurious drift — `devflow
  release --check` fails every release the moment anyone adds an inline
  comment next to the workspace version, which is exactly the invariant
  this phase's preflight exists to assert correctly.

This repo's own root `Cargo.toml` has no trailing comment today, so the
bug is currently latent/untriggered in-repo, but it is trivially
triggered by a one-line edit and there is no test covering `read_version`
or `read_workspace_self_pins` against a trailing-comment fixture (only
`write_version`'s output is tested for comment preservation).

**Fix:** Extract the quoted token the same way `replace_version_in_contents`
already does for the write path — anchor on the opening quote and scan
forward for the matching closing quote, ignoring everything after it,
rather than `trim_matches` on the whole tail:

```rust
let value = value.trim();
if value.starts_with('{') {
    continue;
}
let extracted = match value.chars().next() {
    Some(q @ ('"' | '\'')) => value[1..].find(q).map(|end| value[1..1 + end].to_string()),
    _ => {
        let end = value.find([' ', '\t', ',', '#']).unwrap_or(value.len());
        Some(value[..end].to_string())
    }
};
return extracted;
```

Add a fixture test mirroring `write_version_preserves_trailing_comment_in_toml`
but calling `read_version`/`read_workspace_self_pins` on a version line
with a trailing comment, asserting the clean value comes back.

### CR-02: `devflow cleanup` deletes worktrees for phases intentionally halted by `--until`, stranding `devflow resume`

**File:** `crates/devflow-cli/src/commands.rs:380-460` (guard at lines 409-422); interacts with `crates/devflow-cli/src/pipeline_gate.rs:67-86` and `crates/devflow-cli/src/pipeline_launch.rs:213-229`
**Issue:**

`transition()`'s `--until` stop path (pipeline_gate.rs:67-86) intentionally
clears `state.monitor_pid = None` and leaves the agent process exited —
the phase is not finished, `state.stage` stays at the completed stop
stage, and `state` is never cleared (`workflow::clear_state` is not
called), so the phase remains "active" per `workflow::list_states` and
`devflow status`, awaiting `devflow resume`.

`cleanup()`'s liveness guard (commands.rs:402-422) computes:

```rust
let agent_alive = phase.and_then(|p| agent_pid_from_file(project_root, p)).is_some_and(agent::agent_running);
let monitor_pid = matched_state.and_then(|s| s.monitor_pid);
let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
let phase_liveness = liveness(monitor_pid, monitor_alive, agent_alive);

if agent_alive || matches!(phase_liveness, Liveness::Healthy | Liveness::BetweenStages) {
    return Err(...); // refuse
}
```

For a `--until`-stopped phase: `monitor_pid` is `None` (cleared by the
stop path) → `liveness()` returns `Liveness::Unknown` (never `Stuck`,
never `Healthy`/`BetweenStages`); the agent for the just-completed stage
has already exited normally, so `agent_alive == false`. Neither disjunct
of the refusal condition is true, so the guard **does not refuse** — and
critically, unlike the `reference` worktree special case a few lines
above (commands.rs:393-396), there is **no `--force` gate at all** for
ordinary phase worktrees. A bare `devflow cleanup` (no flags) will
proceed straight to `remove_worktree_with_retry`, deleting the worktree
of a phase the operator explicitly parked for later work.

This is a genuine regression introduced by the interaction of 20b (the
liveness guard) and 20c (the new `stopped` state) landing in the same
phase: `doctor`'s reconciliation checks (`check_dead_agent`/
`check_dead_monitor`, commands.rs:1571-1610) were correctly updated to
special-case `facts.stopped` so a stopped phase is never misreported as
a crashed agent or dead monitor — `cleanup`'s guard has no equivalent
check on `matched_state.stopped`. After the worktree is deleted,
`devflow resume --phase N` (pipeline_launch.rs:213-229) will still load
the persisted state (worktree path pointing at a now-deleted directory)
and attempt to relaunch the agent there, failing in a way that is much
harder to diagnose than "the worktree was deleted."

No test in `phase7_cli.rs` exercises `cleanup` against a `--until`-stopped
phase (`start_until_plan_halts_cleanly` never calls `cleanup` afterward);
the closest existing coverage
(`cleanup_force_refuses_on_live_agent_unknown_monitor`) only covers
`Unknown` liveness *with a live agent pid*, not the stopped-phase case
where the agent has legitimately exited.

**Fix:** Thread `stopped` into the guard the same way `doctor` does, and
refuse (or at minimum require an explicit `--force`, mirroring the
`reference` worktree's own precedent) before deleting a stopped phase's
worktree:

```rust
let stopped = matched_state.is_some_and(|s| s.stopped);
if stopped {
    let phase_label = phase.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());
    println!(
        "keeping worktree {} for phase {phase_label} — halted via --until; \
         run `devflow resume --phase {phase_label}` first, or pass --force to discard it",
        wt.path.display()
    );
    if !force {
        continue;
    }
}
if agent_alive || matches!(phase_liveness, Liveness::Healthy | Liveness::BetweenStages) {
    // existing refusal
}
```

Add a `cleanup`-after-`start --until` integration test asserting the
worktree survives a bare `devflow cleanup` and is only removable (if at
all) with an explicit `--force`.

## Warnings

### WR-01: `print_dry_run` never reflects `--until`, so `--dry-run` previews the wrong pipeline

**File:** `crates/devflow-cli/src/pipeline_gate.rs:396-422`
**Issue:** `start()` sets `state.stop_until = until` before checking
`dry_run` (commands.rs:116-122), so `print_dry_run(&state)` has access to
`state.stop_until`, but the function never reads it — it always prints
the full Define→Ship stage list with no indication that the run would
actually halt after the requested stage. `devflow start --phase N --until
plan --dry-run` prints the exact same output as the same command without
`--until`, which defeats the point of previewing what a real invocation
would do.
**Fix:** Have `print_dry_run` annotate the stage matching `state.stop_until`
(e.g. append `" [STOPS HERE — --until]"` after that stage's line) and
print a trailing note when `state.stop_until` is `Some`.

### WR-02: `devflow ship --phase` can block the foreground CLI for the full gate timeout on a terminal-hook failure

**File:** `crates/devflow-cli/src/pipeline_gate.rs:161-195, 292-393`
**Issue:** `ship_override` (line 379) dispatches `GateAction::Advance` to
`finish_workflow`, whose retry loop (lines 161-195) reopens the Ship gate
and calls `run_gate`, which polls `Gates::poll_response` for up to
`gate_timeout_secs()` — the production default is measured in days (per
`concurrent_ship_advances_finish_both_phases_independently`'s comment
referencing the "7-day production default"). Every other caller of
`finish_workflow`/`run_gate` runs inside a *detached monitor* process
(spawned via `monitor::spawn_monitor`), so a multi-day block is invisible
to the operator's terminal. `ship_override` is invoked directly from the
foreground `devflow ship --phase N` CLI command — if the terminal-hook
batch fails (e.g. a merge conflict during version bump), the CLI
invocation itself now blocks the caller's shell for however long the gate
takes to resolve, with no warning printed beforehand.
**Fix:** Either spawn the retry-gate portion of `finish_workflow` onto a
detached monitor when reached via `ship_override` (mirroring the
`GateAction::LoopBack` arm, which already forks a new monitor and says so
explicitly), or print an explicit warning before entering
`finish_workflow` from `ship_override` that a hook failure will block
this foreground invocation rather than a background process.

### WR-03: `member_depends_on` misses the `[dependencies.NAME]` long-form section syntax

**File:** `crates/devflow-core/src/git.rs:626-643`
**Issue:** `member_depends_on` only recognizes a dependency edge when the
`[dependencies]` table is written with inline entries (`name = { ... }`
or `name.workspace = true`) on lines directly under a section header
literally equal to `"dependencies"`:

```rust
if current != "dependencies" {
    continue;
}
```

A manifest using the equally-valid expanded TOML form:

```toml
[dependencies.devflow-core]
workspace = true
```

parses to a section header of `"dependencies.devflow-core"`, which never
equals `"dependencies"`, so this edge is silently dropped. `publish_order`
(git.rs:551-577) is exactly the function `release --check`'s
`check_publish_order` (commands.rs:1416-1433) relies on to state a
crates.io-safe publish order — a workspace member manifest written in
this form would have its dependency edge dropped from the topo-sort,
which can produce (or fail to correct) a wrong publish order without any
`warn`/`fail` signal, since `topo_sort`'s cycle-fallback path degrades
silently to input order on missing edges just as it does on a real cycle.
Not currently triggered by this repo's own manifests (all inline form),
but the function is documented as sourcing "the crates.io publish order
... from the workspace's own `[workspace] members` list and each member's
own `[dependencies]` section" without the long-form caveat.
**Fix:** Recognize `current.starts_with("dependencies.")` as also being
"in the dependencies table for the member named after the dot", or at
minimum document the long-form gap the same way the inline-table
single-line limitation is documented in version.rs.

## Info

### IN-01: `user.signingkey` set to inline key data (not a path) is misclassified as "file does not exist"

**File:** `crates/devflow-core/src/git.rs:765-776`
**Issue:** `check_ssh_signing_viability` treats `user.signingkey` purely
as a filesystem path (`Path::new(&signingkey)`, then `key_path.exists()`).
Some `ssh` signing setups accept a literal public key string (e.g.
`user.signingkey = "ssh-ed25519 AAAA..."`) rather than a path to a key
file. In that case the check reports `NotViable { reason: "... key file
does not exist" }`, which is a slightly misleading diagnostic for an
otherwise-viable configuration (it wouldn't reach `ssh-add -l`/
`ssh-keygen` at all).
**Fix:** Detect a value that looks like an inline key (starts with
`ssh-` or `ecdsa-`/`sk-` prefixes) and produce a distinct, accurate
`Unknown`/informational message rather than reusing the "file does not
exist" wording, or extend the check to compare against `ssh-add -l`'s
listed keys directly without requiring a resolvable file path.

---

_Reviewed: 2026-07-23T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
