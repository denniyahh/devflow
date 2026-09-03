# Phase 44: Adversarial Multi-Model Plan & Architecture Review Summary

**Date**: 2026-08-26  
**Phase**: 44 (Codex End-to-End Verification & Driver Handoff Hardening)  
**Worktree**: `/var/home/denniyahh/Github/devflow/.worktrees/phase-44`  
**Reviewers**:
1. **OpenAI Codex** (`gpt-5.6-terra`, high reasoning effort via `codex exec -s read-only`)
2. **DeepSeek V4 Pro** (via Hermes Agent v0.20.5 with LiteLLM proxy)
3. **OpenAI Codex Baseline** (`gpt-5.6-luna`, medium reasoning effort)

---

## 1. Executive Summary

- **Source Code Verification**: Line-number citations, struct layouts, and module visibility were verified across both models against live source.
- **Critical Defects Caught & Fixed Before Execution**:
  1. **Shell Splitting Hazard on Unquoted Compound Command (Codex Terra H7)**: The format string inserted `hermes_cron.command` (`cd <root> && devflow resume ...`) unquoted into `hermes cron create`. Bash parses `&&` as a command separator, executing `cd` and then running `devflow resume ... --repeat 1 --name ...` directly in the operator shell. *Fixed in 44-03 Task 2 by wrapping `command` in single quotes: `'{}'`.*
  2. **Struct Field Name Mismatch (Codex Terra H7)**: `HermesCronJob` has `schedule`, `name`, `command`, and `once: bool` — it has NO `repeat` field. *Fixed in 44-03 Task 2 by mapping `once: bool` to `--repeat 1`.*
  3. **Empty Schedule Hint Rendering (DeepSeek H1)**: Unhandled `schedule.is_empty()` branch would emit `hermes cron create  ...` (double space, misparsed positional args), re-creating bug #148. *Fixed in 44-03 Task 2 with explicit manual resume instruction and negative test.*
  4. **Driver Binary Preflight for Codex (Codex Terra H2 & DeepSeek M3)**: `driver.health()` on `CodexDriver` returns `Ok(())` unconditionally. Testing health alone would not fail if `codex` is missing from `PATH`. *Fixed in 44-01 Task 1 by explicitly invoking `ensure_agent_binary(driver.binary_name())`.*
  5. **Resume Argument Ordering (Codex Terra M4)**: The plan specified `resume(project_root, phase, agent, legacy_claude_launch)`, but test call sites were rewritten with arguments flipped. *Fixed in 44-01 Task 1 to `resume(root, phase, None, false)`.*
  6. **Cron Event Deletion Guard (Codex Terra H5)**: Events were emitted whenever the pre-check found a file, even if deletion returned `Err`. *The original plan attempted to fix this via `consume_cron_instructions` but omitted the helper's owning task; execution review corrected that with prerequisite 44-00.*
  7. **Legacy Cron Handling (Codex Terra H6 & DeepSeek M4)**: The original plan named `devflow_core::ship::consume_cron_instructions` without creating it. 44-00 now owns the core helper and its legacy-path controls, avoiding `pub(crate)` leaks and phase mismatch.
  8. **Multi-Argument Cargo Test Filters (Codex Terra M11 / Codex Luna M1)**: Fixed multiple positional test arguments to single valid Cargo filters (`agents::tests::codex` and `resume_`).
  9. **Feedback Latency Optimization**: Intermediate `--workspace` sweeps removed from 44-02 Task 1 and 44-03 Task 1, deferring workspace regression to wave boundaries.

---

## 2. Findings & Remediation Matrix

| # | Severity | Component | Finding Source | Defect Summary | Plan Remediation |
|---|---|---|---|---|---|
| **1** | **High** | CLI Shell Escaping | [Codex Terra H7](review_codex_terra.md#L63) | `hermes cron create` format string emitted `command` unquoted; `&&` splits the shell command, causing immediate execution instead of job creation. | **Resolved in 44-03 Task 2**: Wrapped embedded command in single quotes: `'{}'`. |
| **2** | **High** | Struct Model | [Codex Terra H7](review_codex_terra.md#L63) | Plan referenced `instructions.hermes_cron.repeat`; `HermesCronJob` has `once: bool` and no `repeat` field. | **Resolved in 44-03 Task 2**: Corrected struct field access to map `once` to `--repeat 1`. |
| **3** | **High** | Hint Renderer | [DeepSeek H1](review_deepseek.md#L17) | `cron_hint_line` omitted the empty-schedule branch, which is reachable when `Retry-After` is unparseable (e.g. `"usage limit"`). Emitting `hermes cron create  ...` reproduces bug #148. | **Resolved in 44-03 Task 2**: Added explicit branching for empty schedule rendering a manual resume instruction, plus negative test. |
| **4** | **High** | Driver Preflight | [Codex Terra H2](review_codex_terra.md#L13) | `CodexDriver::health` is a default no-op `Ok(())`. Missing `codex` binary would pass health and mutate state before failing in launch. | **Resolved in 44-01 Task 1**: Mandated `ensure_agent_binary(driver.binary_name())` alongside `driver.health`. |
| **5** | **High** | Audit Integrity | [Codex Terra H5](review_codex_terra.md#L43) | `cron_instructions_consumed` was emitted on pre-existence even if deletion failed (`Err`), creating false audit logs. | **Resolved in 44-01 & 44-02**: Tied event emission strictly to `Ok(Some(path_kind))`. |
| **6** | **High** | Legacy Cron Scope | [Codex Terra H6](review_codex_terra.md#L53) | Calling `legacy_cron_instructions_path` directly is uncompilable (`pub(crate)`) and does not verify phase ownership. | **Resolved by 44-00**: create and test `consume_cron_instructions(project_root, phase)` in core before either CLI consumer runs. |
| **7** | **Medium** | Call Sites | [Codex Terra M4](review_codex_terra.md#L33) / [DeepSeek M1](review_deepseek.md#L39) | Argument order mismatch in `resume()` test rewrites. | **Resolved in 44-01 Task 1**: Standardized on `resume(root, phase, None, false)`. |
| **8** | **Medium** | Quoting Layer | [DeepSeek M5](review_deepseek.md#L80) | `ship::shell_quote` is private in `devflow-core`, and `hermes_cron.command` is already quoted at construction time in `ship.rs:186`. | **Resolved in 44-03 Task 2**: Specified that `command` is already quoted and interpolated directly inside single quotes. |
| **9** | **Medium** | Test Syntax | [Codex Terra M11](review_codex_terra.md#L101) / [Codex Luna M1](review_codex.md#L38) | Multiple positional test names passed to `cargo test` is invalid Cargo syntax. | **Resolved in 44-01 & 44-04**: Replaced with single filters (`agents::tests::codex`, `resume_`). |
| **10** | **Info** | Test Latency | Nyquist compliance | Intermediate tasks embedded redundant ~90s `--workspace` passes. | **Resolved in 44-02 & 44-03**: Removed `--workspace` from intermediate tasks, preserving it on plan terminal tasks. |

---

## 3. Referenced Artifacts

- OpenAI Codex Terra Raw Review: [`review_codex_terra.md`](review_codex_terra.md)
- DeepSeek V4 Pro Raw Review: [`review_deepseek.md`](review_deepseek.md)
- OpenAI Codex Baseline Review: [`review_codex.md`](review_codex.md)
- Phase Context: [`44-CONTEXT.md`](44-CONTEXT.md)
- Hardened Plans: [`44-01-PLAN.md`](44-01-PLAN.md), [`44-02-PLAN.md`](44-02-PLAN.md), [`44-03-PLAN.md`](44-03-PLAN.md), [`44-04-PLAN.md`](44-04-PLAN.md)
