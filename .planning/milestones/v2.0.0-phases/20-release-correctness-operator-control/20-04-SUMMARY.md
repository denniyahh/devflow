---
phase: 20-release-correctness-operator-control
plan: 04
subsystem: release-tooling
tags: [rust, cargo, git, cli, clap, ssh, gpg, release-engineering]

# Dependency graph
requires:
  - phase: 20-01
    provides: write_version rewrites every local-path [workspace.dependencies] self-pin alongside [workspace.package] version — 20d's self-pin check asserts this fixed invariant
  - phase: 20-03
    provides: shared main.rs/commands.rs Command/Start clap-enum regions (sequential wave ordering to avoid a merge conflict, not a functional dependency)
provides:
  - "devflow release --check: four structured, read-only checks (self-pin, develop/main divergence, crates.io publish order, tag-signing viability) mirroring doctor's Check-list-then-report shape"
  - "Bare devflow release (no --check) rejected toward the deferred release-cut executor (999.25 / DEN-50)"
  - "devflow_core::git::origin_main_ancestor_status / publish_order / classify_ssh_add_status / check_signing_viability — reusable, independently unit-tested helpers"
affects: [999.25-release-cut-executor, future-release-tooling]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "release_check follows doctor's Check-list-then-report shape (same pub(crate) Check struct), keeping the two commands visually consistent"
    - "Ancestor-check reuse: shells out to the exact git merge-base --is-ancestor origin/main HEAD invocation scripts/sync-main-to-develop.sh already uses, but strips the preceding git fetch entirely — read-only preflight must not touch the network"
    - "Publish order derived from data (workspace members + each member's own [dependencies] section, Kahn's-algorithm topo sort), never a hardcoded prose string"
    - "Signing viability redaction: only a SHA256 fingerprint (computed via ssh-keygen -lf on the PUBLIC key) or a generic reason string ever reaches output — never the configured key's filesystem path or private key bytes"
    - "Test hermeticity: HOME/SSH_AUTH_SOCK isolated per test invocation since git config reads fall through to the operator's global ~/.gitconfig even inside a throwaway fixture repo (this dev machine's own gpg.format=ssh is the exact Pattern 4 research finding)"

key-files:
  created:
    - crates/devflow-cli/tests/release_check.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "Self-pin, divergence, and publish-order helpers compare against dynamically-read values (workspace version, workspace members/deps) — never a hardcoded expected version or prose order string, matching must_haves.truths."
  - "Divergence check reuses ONLY the merge-base --is-ancestor command from sync-main-to-develop.sh, deliberately omitting its preceding git fetch — origin/main-absent degrades to an actionable 'run git fetch first' WARN, never a false FAIL and never an implicit fetch."
  - "Signing check computes a public-key fingerprint via ssh-keygen -lf on the .pub file only — never reads or echoes the private key, and never echoes the configured signingkey's filesystem path, matching commands.rs:1195's existing 'no path/username' discipline."
  - "Publish order-order and Check-rendering both live in the same pub(crate) Check struct doctor already defines, rather than introducing a parallel report type."

requirements-completed: [20d]

coverage:
  - id: D1
    description: "devflow release --check runs four structured, read-only checks (self-pin, divergence, publish order, signing viability); bare devflow release is rejected toward the deferred executor (DEN-50)"
    requirement: "20d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_passes_when_pins_match"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_flags_self_pin_drift"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_without_check_is_rejected"
        status: pass
    human_judgment: false
  - id: D2
    description: "Develop/main divergence check runs git merge-base --is-ancestor origin/main HEAD against already-fetched refs (no git fetch); degrades to an actionable message when origin/main was never fetched"
    requirement: "20d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_reports_divergence_when_main_not_ancestor"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_divergence_degrades_when_origin_main_absent"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::origin_main_ancestor_status_is_ref_absent_without_a_remote"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::origin_main_ancestor_status_is_ancestor_when_head_is_up_to_date"
        status: pass
    human_judgment: false
  - id: D3
    description: "crates.io publish order (devflow-core before devflow) is a structured check derived from the workspace's own members/dependency graph, not free prose"
    requirement: "20d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_states_publish_order"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::publish_order_derives_core_before_cli_from_a_fixture_workspace"
        status: pass
    human_judgment: false
  - id: D4
    description: "Tag-signing viability is gpg.format-aware (ssh-add exit codes 2/1/0 mapped to distinct messages) and leaks no private key material or filesystem path"
    requirement: "20d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::classify_ssh_add_status_maps_all_three_documented_exit_codes"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_signing_output_leaks_no_key_material_or_path"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_signing_degrades_when_ssh_add_absent"
        status: pass
  - id: D5
    description: "Signing viability against a real ssh-agent (key loaded vs. not loaded) reports the correct pass/fail with no key material in output — manual backstop, CI cannot deterministically provision a live agent"
    verification: []
    human_judgment: true
    rationale: "Plan's own <verification> section designates this a Manual-Only backstop (VALIDATION.md) — CI does not deterministically provision a live ssh-agent with a controllable key-loaded/empty state, so this truth cannot be proven by an automated test in this environment. Requires a human to run `devflow release --check` on a gpg.format=ssh machine with (a) an unlocked key loaded and (b) no key loaded, and confirm both distinct actionable messages plus the absence of key material in the output."
    human_judgment: false

duration: ~55min
completed: 2026-07-23
status: complete
---

# Phase 20 Plan 04: Release-Cut Preflight (`devflow release --check`) Summary

**`devflow release --check` — a read-only, network-independent preflight with four structured checks (self-pin, develop/main divergence, crates.io publish order, `gpg.format`-aware tag-signing viability) mirroring `doctor`'s report shape; bare `devflow release` is rejected toward the deferred release-cut executor (999.25 / DEN-50).**

## Performance

- **Duration:** ~55 min
- **Completed:** 2026-07-23
- **Tasks:** 3
- **Files modified:** 6 (1 created: `release_check.rs`)

## Accomplishments

- `Command::Release { check: bool, project: PathBuf }` in `main.rs`: `--check` is required. A bare `devflow release` is rejected with a message naming the deferred release-cut executor (merge PR → tag → sync develop → publish, DEN-50, already filed as backlog item `999.25`) — never silently treated as a valid check run.
- `commands::release_check` runs four `Check`-shaped, read-only checks (reusing `doctor`'s own `Check` struct) and fails the process (non-zero exit) only when at least one check is a hard `fail`; a `warn` (degraded/unknown) never blocks.
- **Self-pin check** (asserts 20a's invariant): new `devflow_core::version::read_workspace_self_pins` reuses `version.rs`'s existing section-scan/inline-table helpers to compare every local-path `[workspace.dependencies]` pin against `[workspace.package] version` dynamically — never a hardcoded expected value.
- **Divergence check**: new `devflow_core::git::origin_main_ancestor_status` reuses the exact `git merge-base --is-ancestor origin/main HEAD` invocation `scripts/sync-main-to-develop.sh` already runs, but strips its preceding `git fetch` entirely. `origin/main` absent (never fetched) degrades to an actionable `"origin/main not fetched — run git fetch first"` WARN rather than a false divergence FAIL or a crash — the check issues no network call at all.
- **Publish-order check**: new `devflow_core::git::publish_order` derives `devflow-core` before `devflow` from the workspace's own `[workspace] members` list and each member's own `[dependencies]` section (a small Kahn's-algorithm topo sort with a cycle fallback) — never a hardcoded prose string.
- **Signing-viability check**: new `devflow_core::git::classify_ssh_add_status` (pure) maps `ssh-add -l`'s three documented exit codes (2 = no agent, 1 = agent-but-empty, 0 = keys listed); `check_signing_viability` branches on `git config gpg.format` (`ssh` vs. unset/`openpgp`), computes a matching public-key fingerprint via `ssh-keygen -lf` on the `.pub` file only, and reports ONLY a boolean + optional `SHA256:...` fingerprint — never private key bytes or the configured key's filesystem path. Absent tooling or unset config degrades to an actionable message, never a panic.
- Regenerated `crates/devflow-cli/tests/snapshots/devflow-help.txt` and added the `release --check` row to `OPERATIONS.md`.

## Task Commits

Each task was committed atomically:

1. **Task 1 (tracer, tdd): End-to-end release --check slice — command + self-pin check** - `45ac63e` (feat)
2. **Task 2 (tdd): Divergence check (reuse ancestor command) + publish-order check** - `67715d2` (feat)
3. **Task 3 (tdd): gpg.format-aware signing-viability check + help/docs** - `62cd6e9` (feat)

_Task 1 is `type="tracer"`: committed as a real, production-quality end-to-end slice, then re-verified (`cargo test -p devflow --test release_check` → GREEN) before expanding into Tasks 2/3, per the tracer feedback gate._

## Files Created/Modified

- `crates/devflow-cli/tests/release_check.rs` (created) — 12 integration tests driving the real `devflow` binary against temp-workspace fixtures: self-pin pass/fail, `--check`-required rejection, divergence fail/degrade, publish-order, and two signing-viability tests (leak-proof + tool-absent fail-soft).
- `crates/devflow-cli/src/main.rs` — `Command::Release { check, project }`; dispatch rejects `check == false` with a DEN-50 message, otherwise calls `commands::release_check`.
- `crates/devflow-cli/src/commands.rs` — `release_check` + four `check_*` helpers (`check_self_pin`, `check_divergence`, `check_publish_order`, `check_signing`), all read-only, all rendering into the existing `pub(crate) Check` struct.
- `crates/devflow-core/src/git.rs` — `AncestorStatus`/`origin_main_ancestor_status`, `publish_order` (+ `workspace_member_paths`/`package_name`/`member_depends_on`/`topo_sort` helpers), `SigningStatus`/`classify_ssh_add_status`, `SigningViability`/`check_signing_viability` (+ `git_config`/`public_key_fingerprint`/`check_ssh_signing_viability`/`check_gpg_signing_viability` helpers); 8 new unit tests.
- `crates/devflow-core/src/version.rs` — `SelfPin` struct + `read_workspace_self_pins`, reusing the module's existing private section-scan helpers.
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated (`release` row added to the top-level command list).
- `OPERATIONS.md` — new `devflow release --check` row in the command table.

## Decisions Made

- Kept the self-pin/divergence/publish-order checks' comparisons entirely data-derived (workspace version, workspace members + `[dependencies]` sections) rather than any hardcoded expected value or prose string, per the plan's `must_haves.truths` and cross-AI review incorporation (Codex HIGH: read-only must not depend on the network).
- `check_signing_viability`'s SSH branch verifies a loaded key belongs to the CONFIGURED `user.signingkey` by comparing `ssh-keygen -lf`'s fingerprint (computed on the public key file only) against `ssh-add -l`'s output, rather than trusting "any key loaded" as sufficient — this is the difference between "an agent has keys" and "an agent has THIS key," which the plan's Pattern 4 research explicitly calls out (`ssh-add -l` exit 0 with a non-matching key must still be an actionable failure).
- Test hermeticity: `release_check.rs`'s `run_release` helper isolates `HOME` (fresh empty dir, no `.gitconfig`) and removes `SSH_AUTH_SOCK`/`SSH_AGENT_PID` for every invocation. Discovered mid-Task-3 that this dev machine's own global `~/.gitconfig` sets `gpg.format=ssh` and `user.signingkey` (confirming the plan's own Pattern 4 research finding) — without isolation, every fixture repo (even ones with no local git config at all) would inherit that global config through git's normal config-resolution fallthrough, making test outcomes depend on the ambient host environment. Applied uniformly to all 12 tests in the file, not just the new signing ones, since Task 1/2 tests would otherwise non-deterministically trip the newly-added signing check's status too.
- The `ssh-add`-absent fail-soft test needed a `PATH` containing a symlink to only the real `git` binary rather than a directory restriction like `/usr/bin` — this Fedora machine ships `ssh-add`/`ssh-keygen` in `/usr/bin` alongside `git` itself, so a naive directory-based PATH restriction did not actually remove tool availability.

## Deviations from Plan

None architecturally — plan executed as written across all three tasks; no Rule 4 (architectural) decisions were needed.

**Minor, non-blocking:** the plan's own `<verification>` "Phase checks" section lists `cargo test -p devflow-core classify_ssh_add_status git::` as a single command; `cargo test` only accepts one `TESTNAME` positional filter, so that exact invocation errors (`unexpected argument 'git::' found`). Verified the intent instead via `cargo test -p devflow-core --lib git::` (28 passed, 0 failed — the substring match already includes `classify_ssh_add_status_maps_all_three_documented_exit_codes`) and separately via `cargo test -p devflow-core classify_ssh_add_status` (1 passed). No code or test change was needed; flagging only so a future plan doesn't copy the same non-executable command literally.

**Rule 1 (auto-fixed, in-flight):** the `Release` command's initial clap doc comment lacked a blank-line paragraph break, which caused clap to render the ENTIRE multi-sentence doc comment (including the D-03 ceiling explanation) as the top-level `--help` one-line summary — inconsistent with every sibling command's single-line summary. Split into a short summary + a blank-line-separated detail paragraph (matching `Resume`'s existing doc-comment shape) before regenerating the snapshot.

## Issues Encountered

None blocking. The ambient-git-config discovery (see Decisions Made) required adding test isolation but did not require any production-code change — `check_signing_viability` correctly reads whatever `git config` resolves to; the issue was purely in achieving deterministic *test* fixtures against a machine whose own global config happens to match the Pattern 4 research finding.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- 20d is fully resolved: `devflow release --check` is real, tested, read-only, and network-independent. The self-pin check asserts 20a's now-fixed invariant (`write_version` rewrites both `[workspace.package] version` and every local-path self-pin) rather than encoding today's manually-patched value.
- The release-cut EXECUTOR (merge/tag/sync/publish) remains explicitly out of scope per D-03 — already filed as backlog item `999.25-release-cut-executor` (DEN-50) prior to this plan's execution (commit `e2df150`), so no new backlog filing is needed from this plan.
- **Manual backstop still required before the next real release cut** (VALIDATION.md Manual-Only, coverage D5 above): run `devflow release --check` on a `gpg.format=ssh` machine with (a) an unlocked signing key loaded in the agent and (b) no key loaded, and confirm the two distinct actionable messages plus the absence of any key material in the printed output. Automated tests in this plan cover all the deterministic branches (no agent, agent-empty by simulated exit code, tool absent, gpg.format unset) but cannot deterministically provision a live agent with a real loaded key.
- `cargo test --workspace` passes with 0 failed across every target (382 total including the new 12 `release_check.rs` + 8 new `git.rs` unit tests); `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are both clean.

---
*Phase: 20-release-correctness-operator-control*
*Completed: 2026-07-23*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/tests/release_check.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/src/commands.rs
- FOUND: crates/devflow-core/src/git.rs
- FOUND: crates/devflow-core/src/version.rs
- FOUND: crates/devflow-cli/tests/snapshots/devflow-help.txt
- FOUND: OPERATIONS.md
- FOUND commit: 45ac63e (feat: Task 1 — self-pin check + --check gate)
- FOUND commit: 67715d2 (feat: Task 2 — divergence + publish-order checks)
- FOUND commit: 62cd6e9 (feat: Task 3 — signing-viability check + help/docs)
