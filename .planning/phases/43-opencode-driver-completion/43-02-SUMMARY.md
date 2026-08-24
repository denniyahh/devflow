---
phase: 43-opencode-driver-completion
plan: 02
subsystem: agents
tags: [rust, opencode, agent-driver, health-check, capability-probe, fail-closed]

# Dependency graph
requires: ["43-01"]
provides:
  - "OpenCodeDriver::health — fail-closed credential check via `opencode providers list`"
  - "OpenCodeDriver::capabilities — fail-closed subagent-dispatch probe via `opencode agent list`"
  - "strip_ansi_escapes / opencode_configured_provider_count / opencode_subagent_dispatch_available(_with) / parse_opencode_agent_list_for_subagent — pure, unit-testable free functions in opencode.rs"
affects: [opencode-driver, devflow-cli-doctor, preflight]

# Actuals (#2632)
actuals:
  tokens: 5846
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pi's Command-wrapper + pure-classifier split for health (spawn wrapper tested via a stub binary; classifier tested directly)"
    - "Hermes's mockable `_with(output_fn)` split for capabilities (no subprocess spawned in any of its 7 tests)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agents/opencode.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Stub-binary script prints its body via the `printf` shell BUILTIN, never an external `cat`/`echo` — the PATH-restricted test harness (PATH set to ONLY the stub tempdir) cannot resolve any external binary the script would need beyond its own shebang-resolved /bin/sh; see Deviations below for the debugging trail."

patterns-established:
  - "opencode.rs's first #[cfg(test)] mod (the file had none before this plan): PathGuard/ENV_MUTEX/stub_opencode_on_path copied from pi.rs for health's spawn-path test; Hermes's `_with(output_fn)` split for capabilities' spawn-free tests — both patterns coexist in one file, scoped to the probe that actually needs a real spawn under test."

requirements-completed: [OPCD-03]

coverage:
  - id: D1
    description: "OpenCodeDriver::health spawns `opencode providers list`, strips ANSI escapes with a hand-rolled scanner (no regex crate), and sums terminal count lines — decided from the parsed count, never from output.status.success()"
    requirement: "OPCD-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::preflight_accepts_configured_credentials"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::preflight_rejects_constructed_zero_credential_output"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::health_probe_argv_is_providers_list"
        status: pass
    human_judgment: false
  - id: D2
    description: "A spawn failure (no opencode on PATH) fails closed to Err, never a panic"
    requirement: "OPCD-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::preflight_rejects_when_probe_cannot_run"
        status: pass
    human_judgment: false
  - id: D3
    description: "health's Err string leaks no provider name, auth.json path, or environment-variable name"
    requirement: "OPCD-03 (P-04, T-43-11)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::health_error_leaks_no_provider_detail"
        status: pass
    human_judgment: false
  - id: D4
    description: "The pure credential-count classifier sums real live-verified output to 6 and constructed zero-credential fixtures (three plausible shapes) to 0; bullet provider-name lines never miscount as numbers"
    requirement: "OPCD-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::provider_count_sums_credentials_and_environment"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::provider_count_is_zero_for_constructed_credentialless_output"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::provider_count_ignores_bullet_provider_lines"
        status: pass
    human_judgment: false
  - id: D5
    description: "capabilities probes `opencode agent list`; a header line carrying (subagent) or (all) reports true, the default build (primary) agent never counts, and every failure mode (spawn error, non-zero exit, empty output) fails closed to false with the return type staying DriverCapabilities, never a Result"
    requirement: "OPCD-03 (D-10)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::agent_list_baseline_reports_no_subagent"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::agent_list_with_subagent_mode_reports_true"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::agent_list_with_all_mode_reports_true"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::subagent_probe_fails_closed_on_spawn_error"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::subagent_probe_fails_closed_on_nonzero_exit"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::subagent_probe_fails_closed_on_empty_output"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/opencode.rs#opencode::tests::capabilities_never_refuses_a_launch"
        status: pass
    human_judgment: false
  - id: D6
    description: "The shared default_preflight_is_ok_for_built_in_adapters test no longer asserts on OpenCode's health, so cargo test no longer spawns the real opencode binary; the six-driver conformance suite still passes unchanged (43-REVIEW.md WR-06 correction: every_driver_passes_the_conformance_suite calls test_contract/contract_checks, which only exercises name/render_prompt/build_command, never health/capabilities/parse_completion — real coverage of those three lives in opencode.rs's own #[cfg(test)] module and agent_result.rs's marker/error/torn-tail tests, not in this suite)"
    requirement: "OPCD-03 (D-12, RESEARCH Pitfall 4)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#agents::tests::default_preflight_is_ok_for_built_in_adapters"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#agents::tests::every_driver_passes_the_conformance_suite"
        status: pass
    human_judgment: false
  - id: D7
    description: "devflow doctor's opencode install hint names npm i -g opencode-ai instead of the wrong cargo install opencode"
    requirement: "D-11"
    verification:
      - kind: manual
        ref: "crates/devflow-cli/src/commands.rs doctor_checks() opencode cmd_check entry, plus `rg -n 'cargo install opencode' crates/` returning no match"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-08-23
status: complete
---

# Phase 43 Plan 02: OpenCode Fail-Closed Health Check and Capability Discovery Summary

**`OpenCodeDriver::health` now refuses an OpenCode launch on a machine with zero configured provider credentials (parsed from `opencode providers list`, never from exit code), and `capabilities` reports `subagent_dispatch` from a real, fail-closed `opencode agent list` probe.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 3
- **Files modified:** 3 (`opencode.rs`, `agents/mod.rs`, `commands.rs`)

## Accomplishments

- `OpenCodeDriver::health` spawns `opencode providers list`, strips ANSI SGR escape codes with a hand-rolled char-scan (no `regex` crate — matches this workspace's `strip_corruption_padding` precedent), and sums the terminal `N credentials` / `N environment variables` count lines. Readiness is decided **solely** from the parsed count — the subcommand exits 0 regardless of credential state (verified live in RESEARCH), so any exit-code-based design would false-green every run.
- `health`'s `Err` is a fixed message naming only the derived state ("no OpenCode provider credential configured") — it never interpolates raw provider-list stdout, a provider name, an `auth.json` path, or an environment-variable name (P-04, T-43-11). Verified by a test whose stub deliberately includes a real-looking provider name and env-var token in its output.
- `OpenCodeDriver::capabilities` probes `opencode agent list` and reports `subagent_dispatch: true` only when a header line carries the `(subagent)` or `(all)` mode marker; the default `build (primary)` agent never counts. Every failure mode (spawn error, non-zero exit, empty stdout) fails closed to `false`, and the return type is `DriverCapabilities` (never a `Result`), so this probe structurally cannot refuse a launch — only `health` can.
- Removed the OpenCode assertion from `default_preflight_is_ok_for_built_in_adapters` (`agents/mod.rs`) — this shared test previously relied on the trait's default no-op `health`; now that `health` does a real spawn, asserting on it there would make `cargo test` spawn the operator's live `opencode` binary and fail on any credential-less machine. OpenCode's own `health` behavior is now covered inside `opencode.rs`'s new test module using the stub-binary pattern.
- `crates/devflow-cli/src/commands.rs`'s `doctor_checks()` opencode entry now recommends `npm i -g opencode-ai` instead of the stale, actively-wrong `cargo install opencode` (D-11) — the installed binary resolves via Homebrew (a JS/Bun CLI, not a Rust crate), and a same-named-but-unrelated crate exists on crates.io.
- Re-ran `every_driver_passes_the_conformance_suite` — it still passes unchanged (D-12). **Correction (43-REVIEW.md WR-06):** this suite calls `test_contract()`/`contract_checks()`, which only exercises `.name()`, `.render_prompt()`, and `.build_command()` — it never calls `.health()`, `.capabilities()`, or `.parse_completion()`. Real coverage of those three lives in `opencode.rs`'s own `#[cfg(test)]` module (health/capabilities tests) and `agent_result.rs`'s marker/error/torn-tail tests, not in this suite.

**The zero-credential fail-closed path is proven only against a constructed fixture, never against a live run (P-05, A1).** The exact stdout shape of `opencode providers list` on a machine with genuinely zero configured credentials was never observed — no destructive test against a credential-less environment was performed this session or in 43-01/RESEARCH. `opencode_configured_provider_count`'s zero-credential behavior is exercised by three separately-labelled SYNTHETIC fixtures (`provider_count_is_zero_for_constructed_credentialless_output`, `preflight_rejects_constructed_zero_credential_output`) covering the plausible shapes the RESEARCH doc leaves open (section absent entirely, section present but empty, explicit `0 credentials`/`0 environment variables` lines) — reasoned from the live positive-credential capture, not captured from a real negative run. Confirming this needs a genuinely credential-less environment (a scratch container, a CI runner, or a throwaway `HOME`/`XDG_DATA_HOME` override with no `auth.json` and no provider env vars). Every doc comment, test name, and this summary say "constructed fixture," never "live-verified," for this specific path.

## Task Commits

1. **Task 1: Fail-closed credential health check for OpenCode** - `9d90d9c` (feat)
2. **Task 2: Fail-closed subagent-dispatch capability probe** - `8bc27c1` (feat)
3. **Task 3: Conformance re-verification, doctor install hint, and phase gate** - `eb4dd6c` (fix)

_Tasks 1 and 2 followed RED-then-GREEN TDD in spirit (tests were written alongside their implementation and confirmed passing against the intended behavior before commit); Task 3 was a re-verification + one-line doctor-hint fix with no new behavior to gate on RED first._

## Files Created/Modified

- `crates/devflow-core/src/agents/opencode.rs` - real `health` (D-07/D-08/D-09) + `capabilities` (D-10); new free functions `strip_ansi_escapes`, `opencode_configured_provider_count`, `opencode_subagent_dispatch_available`/`_with`, `parse_opencode_agent_list_for_subagent`; first `#[cfg(test)] mod tests` for this file (16 new tests)
- `crates/devflow-core/src/agents/mod.rs` - removed the OpenCode assertion from `default_preflight_is_ok_for_built_in_adapters`; corrected its doc comment
- `crates/devflow-cli/src/commands.rs` - corrected the opencode `doctor_checks()` install hint (D-11)

## Decisions Made

- The stub-binary test harness prints its body via the `printf` shell **builtin**, never `cat`/`echo` as external binaries. See Deviations below — this was found and fixed during Task 1's own test run, not assumed in advance.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Stub-binary script used `cat` (an external binary), which silently produced empty stdout under the PATH-restricted test harness**
- **Found during:** Task 1, first `cargo test` run of the new `health` tests
- **Issue:** The initial stub-script design wrote the body to a sibling `body.txt` file and had the script `cat` it. `preflight_accepts_configured_credentials` and three other tests failed — not because the classifier or `health` logic was wrong, but because the tests deliberately set `PATH` to point ONLY at the stub tempdir (so the operator's live `opencode` is never consulted). `cat` is an external binary resolved via `$PATH`; with `PATH` restricted to just the stub dir, the shell's own `cat` invocation silently failed (no `cat` on that restricted `PATH`), so the script's `cat '{body}'` line produced no output while `exit 0` still ran — `health` observed an empty stdout, computed a provider count of 0, and returned `Err`, poisoning the shared `ENV_MUTEX` for later tests in the same run (visible as a cascade of `PoisonError` panics after the first real failure).
- **Fix:** Switched the stub script to print its body via `printf '%s' '{body}'` — a POSIX shell **builtin** (confirmed via `/bin/sh -c 'type printf'` -> "printf is a shell builtin" on this machine, `/bin/sh` -> `bash`) rather than an external program resolved through `$PATH`. The body is embedded directly into the script inside single quotes (it contains no literal single-quote characters), avoiding the shell-quoting hazard of interpolating multi-line ANSI-laden text through a `printf`-format-string argument.
- **Files modified:** `crates/devflow-core/src/agents/opencode.rs` (test module only — no production-code change; `health`'s spawn/parse logic was correct throughout)
- **Verification:** All 16 `agents::opencode::` tests pass; isolated a minimal Rust reproduction (deleted after use, never committed) to confirm the root cause was the restricted-`PATH` + external-`cat` interaction before applying the fix.
- **Committed in:** `9d90d9c` (Task 1 commit) — the bug was caught and fixed before any commit, so no separate fix commit exists.

---

**Total deviations:** 1 auto-fixed (1 bug fix, test-infrastructure only)
**Impact on plan:** No impact on scope or requirements. The bug was in the test harness the plan asked to be modeled on `pi.rs`'s pattern, not in the driver logic itself; `pi.rs`'s own stub uses `echo` (also a shell builtin), so this workspace already had the correct precedent — the deviation here was an implementation slip during the initial write, caught immediately by the plan's own TDD verify step and fixed before commit.

## Issues Encountered

None beyond the deviation above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `OpenCodeDriver` is now a complete driver: real launch argv (43-01), real completion parsing (43-01), and real fail-closed health/capabilities (43-02). All three OPCD requirements (OPCD-01, OPCD-02, OPCD-03) are satisfied.
- The zero-credential health-check path remains genuinely unverified against a real credential-less machine (A1, P-05) — a future task could close this by running the health-check tests (or a live `opencode providers list` probe) inside a scratch container / CI runner / throwaway `HOME` override with no `auth.json` and no provider env vars, to confirm the constructed fixtures match reality.
- No blockers.

## Self-Check: PASSED

All 3 modified source files confirmed present on disk with the expected changes. All 3 task commit hashes (`9d90d9c`, `8bc27c1`, `eb4dd6c`) confirmed present in `git log --oneline`. Full verification suite re-run and confirmed green immediately before writing this summary: `cargo test -p devflow-core --lib agents::opencode::` (16 passed), `cargo test -p devflow-core --lib agents::` (69 passed), `cargo test -p devflow-core --lib agents::tests::every_driver_passes_the_conformance_suite -- --exact` (1 passed), `cargo test -p devflow --bin devflow` (335 passed), `cargo test --workspace --no-fail-fast` (zero `FAILED` lines across all listed suites), and `scripts/check.sh all` (`all OK`).

---
*Phase: 43-opencode-driver-completion*
*Completed: 2026-08-23*
