---
phase: 36
title: Pi Adapter Registration + Release Signing
status: locked (post adversarial review, 2026-08-15)
items: [Pi adapter registration, 999.96, 999.104]
---

# 36-SPEC — Pi Adapter Registration + Release Signing

## What this phase delivers

DevFlow registers **Pi** as a fourth, *selectable* agent adapter — `AgentKind::Pi` + a `PiAgent`
implementing the existing `AgentAdapter` trait with the correct headless invocation, and a
preflight health check that distinguishes "Pi binary installed" from "Pi can actually execute
headless". It does **not** claim an end-to-end `devflow start --agent pi` run: the Code-stage
prompt is still Claude-specific (`/gsd-execute-phase`) and non-Claude agents still route through
the legacy launch path, so making Pi *work* end-to-end is Phase 37's job. This phase also closes
two release-path items: 999.96 (a `release --check` version-bump row) and 999.104 (deterministic
release signing key, replacing the fragile key checks).

## Scope

### A. Pi adapter registration + health check

`AgentKind::Pi` + `PiAgent` (`crates/devflow-core/src/agents/pi.rs`) registered in `adapter_for`.
The adapter wires the interface established from Pi docs v0.84.1:

- `name()` — a stable human-readable name (e.g. `"Pi"`).
- `exec_command()` — `("pi", vec!["-p", "--no-approve", "--", <prompt>])` (print mode,
  positional prompt, `--` delimiter). No `--model`/`--provider` wiring — model/provider selection
  is Phase 37 (the `AgentDriver` contract has the config access `AgentAdapter` lacks), and an
  env-sourced value would not survive detached-monitor stage launches. `--no-approve` because
  `--approve` trusts project-local extensions that execute unsandboxed.
- `preflight()` — health check backed by `pi auth check` (Pi's authoritative verb), **not**
  env-var sniffing (`DEVFLOW_PI_PROVIDER` is a provider *name*, not a credential): `ready` →
  headless-capable, otherwise a distinct credentialless `Err`. The "binary absent" case is
  `ensure_agent_binary`'s job (runs before preflight).

### B. 999.96 — `release --check` version-bump row (S)

A `release --check` row compares `CHANGELOG.md`'s top `## <version>` heading against the workspace
version, reporting NOT viable (with direction) when they disagree. Fixture is **synthetic** — an
explicit mismatched fixture plus missing/malformed/duplicate-heading cases — *not* "the current
tree" (the v2.5.0 cut already consumed that skew; `Cargo.toml` and `CHANGELOG.md` are both `2.5.0`).

### C. 999.104 — deterministic release signing key (S–M)

The two-key model is resolved by making the release path deterministic, not by more checks:

1. The release/tag signing path signs with **`devflow.releaseSigningKey`** (the maintainer key)
   via the `git -c user.signingkey=` override **applied in code**, so there is no
   operator-remembered override to forget.
2. **Remove** the `release --check` tag-signing-viability probe (`check_signing_viability`,
   `git.rs:1099`) — it answers "can this key sign", not "is this the maintainer's key", and it
   probes `user.signingkey` (the agent's key), which is the wrong target.
3. **Remove** the pre-push fingerprint comparison (`scripts/hooks/pre-push`) — it sources the
   expected fingerprint from the same config it validates, so it is tautological.
4. A missing `devflow.releaseSigningKey` fails loudly at release time (a config validation, not a
   key check) rather than silently signing with the wrong identity.

## Boundaries / non-goals

- **No end-to-end Pi run.** Pi reaches terminal completion only after Phase 37 de-Claude-ifies the
  prompt and integrates Pi's JSON-mode transport into the monitor. AC #1 from the prior draft
  (end-to-end) is struck.
- **999.67 dropped** — already shipped: `parse_devflow_result` normalizes both arms
  (`agent_result.rs:166-180`) with the mirror test at `:4343`. Marked resolved; not re-done here.
- No `AgentDriver` / `StageIntent` work (Phase 37). No prompt de-Claude-ification (Phase 37).
- No Pi JSON-mode event unwrapper (Phase 37 — the `-p` transport used here terminates on process
  exit and needs no unwrapper).

## Acceptance criteria

1. `devflow doctor` (or the equivalent preflight) reports Pi's presence and headless-capability,
   with a negative control: a machine with the `pi` binary but no provider credential reports
   "installed but not headless-capable".
2. 999.96: the `release --check` row reports NOT viable on a synthetic mismatched fixture and
   viable when they agree, with direction stated.
3. 999.104: `scripts/cut-release.sh` fails loudly when `devflow.releaseSigningKey` is unset or its
   file is unreadable; the capability-only signing-viability probe is deleted (clippy-clean, the
   whole private cluster); the pre-push fingerprint hook is **retained** — it is the only check that
   distinguishes the agent key from the maintainer key on a hand-cut release tag.

## Deferred (explicitly not here)

- 999.31 (AgentDriver + StageIntent) — Phase 37.
- Pi as a full working driver (JSON-mode unwrapper, monitor/`CloseRule` integration) — Phase 37.
- 999.94 (unattended `decision` checkpoint first-option) — Phase 37.
- 999.101 (upstream Claude Code) — observation for Phase 37's driver contract.
