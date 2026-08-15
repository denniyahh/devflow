# 36-01 Summary — Pi Adapter Registration

**Plan:** 36-01 (tracer, wave 1) — `AgentKind::Pi` + `PiAgent` + preflight + doctor integration.
**Status:** complete — both tasks verified.

## What landed
- `AgentKind::Pi` in `state.rs` (enum + `Display` → `"pi"` + `FromStr` → `"pi"` + the parse-error
  string now lists pi).
- `crates/devflow-core/src/agents/pi.rs` — `PiAgent`: `exec_command` returns
  `("pi", ["-p", "--no-approve", "--", <prompt>])`; `completion_signal_detected` → `false`
  (process-exit transport); `preflight` shells out to `pi auth check --json` and maps `ready` →
  `Ok`, otherwise a credentialless `Err`.
- `adapter_for(AgentKind::Pi)` arm + `pub mod pi` in `agents/mod.rs`; the `adapter_for_returns_correct_names`
  test extended.
- `doctor`'s `cmd_check` table gains a `pi` row (`agent_program` already derives the binary from the
  adapter's `exec_command`, so no separate mapping was needed).
- Enumeration sites updated: `README.md`, `docs/architecture/agent-model.md`,
  `docs/guides/adding-agent.md`, `docs/guides/configuration.md` now list four agents.

## Decisions made (from the adversarial review)
- `--no-approve` (not `--approve`) — unconditional approve trusts unsandboxed project-local
  extensions; a fresh worktree establishes no trust.
- Health check via `pi auth check`, not env sniffing — `DEVFLOW_PI_PROVIDER` is a provider name,
  not a credential.
- No `--model`/`--provider` wiring (Phase 37); a bare `--` before the positional prompt.

## Verification
- `cargo test -p devflow-core --lib`: **637 passed, 0 failed** (includes 5 new `pi.rs` tests:
  argv shape, `--` delimiter, `classify_auth_check` reject-not-ready / accept-ready /
  reject-ready-with-failed-exit).
- `cargo test -p devflow`: all test binaries **0 failed** (including `build_provenance`, which
  required `pi.rs` to be committed first — it copies only `git ls-files`-tracked paths).

## Not done here (Phase 37)
- End-to-end `devflow start --agent pi` (Code-stage prompt is still Claude-specific).
- Model/provider selection, JSON-mode unwrapper, monitor/`CloseRule` integration.
