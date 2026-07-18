# Phase 12: Bootstrap + Housekeeping

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

## Goal

Bootstrap tooling (new-project, map-codebase support) and versioning
automation, plus the accumulated housekeeping debt Phase 11 explicitly
deferred here: 11 warning-level and 5 info-level code review findings
(`11-REVIEW.md`), untested orchestration paths (`11-VALIDATION.md`'s Test
Gap Analysis), and getting `devflow` published to crates.io.

Two items Phase 11 originally deferred here were routed elsewhere instead
(overlap with already-scoped phases, see "Explicitly Out of Scope" below):
WR-11 → Phase 14, IN-01 → Phase 13.

---

## 12a — Bootstrap Support

- [ ] `new-project` support — TBD, needs scoping (original ROADMAP.md line
      only says "Bootstrap (new-project, map-codebase)"; no detailed
      requirements exist yet in `.planning/`)
- [ ] `map-codebase` support — TBD, same as above

## 12b — Versioning Automation

- [ ] Extend `version.rs` beyond current scope (was `pyproject.toml`-only
      per the pre-Phase-11 CONCERNS.md #4; confirm current state now that
      `Cargo.toml`/workspace detection exists — `detect_prefers_cargo_then_
      pyproject_then_package_json` test suggests this may already be
      resolved, verify before scoping further work)
- [ ] **(IN-05, Phase 11 code review)** `Cargo.toml` workspace version is
      `1.2.0` but code comments/docs repeatedly say "v2.0.0" — `devflow
      doctor` reports the contradictory `1.2.0` to users. Decide: bump to
      2.0.0, or stop saying "v2.0.0" in code/docs. Confirmed still
      inconsistent (2026-07-08).

## 12c — Publish to crates.io

- [ ] `cargo publish` — name confirmed available on crates.io (checked
      2026-07-08, 404 on `crates.io/api/v1/crates/devflow`)

## 12d — Phase 11 Code Review Debt (Warnings)

All confirmed still present in current code as of 2026-07-08 — none were
touched by any phase since Phase 11 shipped.

- [ ] **WR-01** — `monitor.rs:84-116`: agent command built via shell
      script + string interpolation (`shell_escape` only covers `'`).
      Reviewer-assessed low risk (prompt text is internal, `--agent` is
      enum-constrained) but fragile. Consider `Command::new(program).args()`
      spawning instead of shell script generation — eliminates the
      injection surface entirely.
- [ ] **WR-02** — `agent.rs:67`: `libc::kill(pid as i32, 0)` truncates on
      PIDs > `i32::MAX` (unreachable in practice on Linux/macOS, but should
      use `libc::pid_t` directly and document the assumption). Adjacent to
      the separate PID-reuse-safety concern noted in the original external
      review of this same function — worth addressing together.
- [ ] **WR-03** — `hooks.rs:95-104`: `BranchCleanup` uses non-force delete;
      silently warns (not errors) when a feature branch is unmerged at
      cleanup time. Workflow prints "phase shipped" even when cleanup
      failed. Document intentional non-force behavior, or upgrade to a
      distinguishable warning ("not merged yet" vs. "git error").
- [ ] **WR-04** — `version.rs:182-203`: TOML section tracker doesn't
      handle `[[array-of-tables]]` headers or inline tables; a
      `[workspace.dependencies]` section with a `version` key could be
      misread before `[workspace.package]` in edge-case Cargo.tomls.
- [ ] **WR-05** — `ship.rs:380-396`: RFC 3339 timestamp handling in
      `parse_rfc3339ish` is confusing but reviewer-confirmed not actually
      buggy. Documentation-only fix (clarify why second-restoration is
      timezone-safe).
- [ ] **WR-06** — `main.rs:826-832`: `retry_after_from_reason` falls
      through to the raw reason string when the "rate limited until "
      prefix is absent (`.or(reason)`). An unparseable reason produces a
      cron schedule of `* * * * *` — fires every minute until manually
      removed. Fix: drop the `.or(reason)` fallback, return `"unknown"`
      directly so the cron schedule builder can reject it cleanly.
- [ ] **WR-07** — `workflow.rs:32-39`: `save_state` writes via plain
      `fs::write` (truncate + write, not atomic). A kill mid-write
      (OOM, `kill -9`) leaves `state.json` empty or partial JSON — the
      next `load_state` fails to parse and the workflow is **permanently
      stuck**. Gate files already use atomic temp+rename
      (`write_atomic`); `save_state` needs the same treatment. High
      severity for a "walk away" tool — recommend prioritizing this one.
- [ ] **WR-08** — `ship.rs:450-459`: `shell_quote`'s "safe unquoted"
      character check misses `~ : @ + = %`, etc. False-negative only (falls
      through to safe single-quote wrapping) — low priority, easy fix.
- [ ] **WR-09** — `agent_result.rs:184-195`: `parse_marker_lines` tail
      scan reverses by Unicode char rather than byte. Reviewer-confirmed
      no correctness bug for ASCII `DEVFLOW_RESULT` markers — simplify if
      touching this file for other reasons, not urgent otherwise.
- [ ] **WR-10** — `crates/devflow-cli/tests/phase7_cli.rs:46-53`:
      `write_config` test helper writes a v1-style `.devflow.yaml` into
      temp repos; no test asserts devflow does NOT read it. If parsing is
      ever accidentally reintroduced, nothing would catch the regression.
      Remove `write_config`/`write_last_ship` helpers or add an explicit
      non-regression assertion.

## 12e — Phase 11 Code Review Debt (Info / Cleanup)

- [ ] **IN-02** — `state.rs:46-51`: `agent_result`/`agent_stdout_path`
      fields on `State` are `#[serde(skip)]` but never populated anywhere.
      Remove, or add a tracked TODO explaining why they're reserved.
- [ ] **IN-03** — `state.rs`/`agents/mod.rs`: `AgentKind` is a type alias
      (`pub type AgentKind = Agent`) creating naming confusion against the
      `Agent` adapter trait. Rename the enum to `AgentKind` directly and
      the trait to `AgentAdapter`.
- [ ] **IN-04** — `main.rs:1175`: `test_cmd` invokes `cargo fmt -- --check`
      (non-idiomatic); canonical form is `cargo fmt --check`.

## 12f — Test Coverage Gaps (from `11-VALIDATION.md`)

Untested orchestration-core paths, all still open:

- [ ] `advance()`'s full orchestration (stage transitions, validate-outcome
      handling) — currently only exercised via manual/e2e flows
- [ ] `consecutive_failures` reaching `MAX_CONSECUTIVE_FAILURES` end-to-end
      (unit tests cover `should_gate()` logic in isolation only)
- [ ] `transition()` hook firing — no test confirms `Validate→Ship` fires
      `DocsUpdate`/`ChangelogAppend`
- [ ] Gate timeout path at the real 7-day value (only `timeout_secs=0` is
      tested)
- [ ] `abort` code path (gate response `approved: false` with an "abort"
      note)
- [ ] `list_feature_branches` ahead/behind count correctness (only display
      formatting is tested)
- [ ] `version::write_version` against a workspace `Cargo.toml` (only a
      simple `[package]` Cargo.toml is tested)
- [ ] `parse_rfc3339ish` with negative UTC offsets (only `Z`/UTC tested)
- [ ] Monitor behavior when the `devflow advance` call itself fails
      (missing state file, corrupt JSON)

## 12g — Manual-Only Verifications Never Executed (from `11-VALIDATION.md`)

These were marked "manual-only" at Phase 11 sign-off with no record they
were ever actually run:

- [ ] Hermes gate delivery: `.devflow/gates/*.json` → human → response
      file → `devflow` ack, end-to-end through a live Hermes session
- [ ] Real agent launch through Claude/Codex/OpenCode CLIs (tests use fake
      shell agents; no real paid/credentialed agent CLI has been exercised)
- [ ] Full Ship review/merge workflow (blocked on the `ship.rs` GSD-native
      rewrite — see Explicitly Out of Scope)
- [ ] Docs hook (`DocsUpdate`) side effects in a real workspace — it's
      intentionally fail-soft; verify the skip path is actually exercised
      and visible to the user, not just silent

## Explicitly Out of Scope (this phase)

- **WR-11** (silent halt + no gate on non-Validate stage failure) — routed
  to **Phase 14** (14d), same failure class as the gate-notify work
  already scoped there.
- **IN-01** (stale `lib.rs` rustdoc examples: `devflow check`/`devflow
  ship`) — routed to **Phase 13** (13b), same class as the other
  docs-accuracy work already scoped there.
- **`ship.rs` GSD-native rewrite** (`ship_phase()`, `/gsd-ship` +
  `/gsd-code-review` integration, `ReviewFailed`/`AgentFailed` handling) —
  `11-VALIDATION.md`'s largest coverage gap (11h-1 through 11h-4). Not
  claimed by any phase yet — needs a decision on whether it's Phase 12,
  its own phase, or folds into Phase 14. Flagging rather than assigning;
  this is a real architectural gap, not routine housekeeping.
- **`capture_agent_output()` blocking path** used by `sequentagent`
  (11i-5, still public/in use) — same status as above: a real open
  architectural question (should `sequentagent` keep a synchronous path,
  or move behind monitor-owned execution?), not yet claimed by any phase.
- `devflow.toml` / configurable pipeline — shelved per 2026-07-08 decision
  (see `STATE.md`), not this phase.

---

## Planning-Time Decisions (2026-07-08, /gsd-plan-phase 12)

Three scope forks resolved with the operator before planning:

- **Version (IN-05) → correct docs to 1.2.0, do NOT claim v2.0.0 yet.**
  1.2.0 is the real current version; v2.0.0 is the *target* that ships at the
  end of the Phase 11–14 arc (Phase 14 is not done). Fix `devflow doctor` and
  code/docs so they report/say 1.2.0 (or "v2.0.0 (in progress)" where a target
  reference is appropriate) — remove premature "v2.0.0" *current-version*
  claims. Bump the workspace version to 2.0.0 only when the v2 line actually
  ships (a later phase, not here).

- **crates.io publish (12c) → PREP ONLY; hold the actual publish.**
  Do all publish-readiness work: complete `Cargo.toml` package metadata
  (`description`, `license`, `repository`, `readme`, `keywords`, `categories`),
  `cargo publish --dry-run`, and `cargo package` verification. Do **NOT** run
  `cargo publish`. Publishing is irreversible (a version can never be reused or
  unpublished); the first public crate should land after Phase 13 (OSS docs)
  and Phase 14 (reliability) — not before.

- **12a Bootstrap (new-project / map-codebase) → DEFERRED out of Phase 12.**
  Genuinely unscoped greenfield ("no detailed requirements exist yet");
  inventing requirements now would be speculative. Scope it later via its own
  `/gsd-discuss-phase`. Phase 12 stays focused on the well-diagnosed debt:
  versioning (12b), publish-prep (12c, no publish), the WR/IN code-review
  fixes (12d/12e), and the test-coverage + manual-verification gaps (12f/12g).
