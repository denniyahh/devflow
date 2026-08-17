# Phase 36 Research — Pi Adapter Registration + Release Signing

**Gathered:** 2026-08-15 (post adversarial review)

## What was verified (against `develop` @ `a3a4871`, `pi` 0.84.1)

### Pi adapter surface
- `AgentKind` enum at `crates/devflow-core/src/state.rs:384` — three variants (Claude, Codex,
  OpenCode) with `Display` (lowercase) and `FromStr`. Adding Pi = one variant + two match arms.
- `AgentAdapter` trait at `crates/devflow-core/src/agents/mod.rs:12` — `name`, `exec_command`,
  `extra_env` (default empty), `completion_signal_detected`, `preflight` (default no-op).
  `adapter_for(kind)` at `:63` is the dispatch to extend.
- `ClaudeAgent::exec_command` (`claude.rs:40`) returns a prompt-free argv (stdin transport);
  Codex/OpenCode pass the prompt positionally. **Pi is positional** (`pi -p "prompt"`, `-p` is
  boolean, prompt is positional — not stdin like Claude, not `-p <prompt>` like antigravity).
- Non-Claude agents route to `MonitorLaunch::Legacy` (`pipeline_launch.rs:198-208`) — no
  `CloseRule`/drain-gate involvement in this phase. Confirmed: `claude_stream_launch_enabled`
  (`:703`) is Claude-only.

### Pi specifics that differ from the existing three adapters
- Pi defaults provider `google` (`pi --help`: `--provider` default `google`) and reads
  `GEMINI_API_KEY`; with no model/provider surfaced anywhere in `AgentAdapter`, the Pi adapter
  must source them from config/env.
- Non-interactive modes never prompt for project trust; they consult `defaultProjectTrust`
  (default `ask` → project resources silently ignored). `--approve` is **load-bearing** under
  DevFlow's fresh-worktree-per-phase model (no saved trust decision ever matches).
- Pi has **no built-in sandbox** (`docs/security.md`); `extra_writable_roots` is ignored.
- `-p` (print) transport terminates on process exit — no JSON unwrapper needed in this phase.

### Release path (999.96 + 999.104)
- Tag creation at `git.rs:209` / `:226` currently passes `-c tag.gpgSign=false` (unsigned). The
  signing-key override must be made deterministic here.
- `check_ssh_signing_viability` (`git.rs:1112-1150`) + `check_signing_viability` — the capability
  probe to remove. Its `release --check` row lives at `commands.rs:2549-2573` (the
  `TagSigningViabilityCheck` struct).
- The pre-push fingerprint hook is `scripts/hooks/pre-push:40-70` (reads `devflow.releaseSigningKey`,
  compares the pushed tag's fingerprint) — to remove.
- 999.96: `release --check` self-pin row is in `commands.rs`; the new version-bump row compares
  `CHANGELOG.md` top heading vs workspace version (`Cargo.toml:9`). Fixture must be synthetic.

## Pitfalls (carried into the plan)
- `--approve` must be wired, not left to discretion — a headless `pi` run without it silently
  drops project resources and still exits 0.
- The health check ("can execute headless") for Pi means "a provider credential resolves", which
  has no analogue in the existing three adapters' checks.
- The signing probe removal must also delete its `SigningViability` enum arm usage and the
  `release --check` row that calls it — a dangling enum/row would fail `cargo test`/`clippy`.
