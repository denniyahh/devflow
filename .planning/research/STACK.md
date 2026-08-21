# Technology Stack — Milestone v2.8.0 (Remaining Harness Support)

**Project:** DevFlow
**Researched:** 2026-08-18
**Method:** Direct probing of installed CLIs (`--help`/`--version`) + source read of the four
existing drivers (`claude.rs`, `codex.rs`, `opencode.rs`, `pi.rs`) and the `AgentDriver` contract
(`agents/mod.rs`). **HIGH confidence** — no web search needed; the CLIs are installed locally and
their argv is authoritative.

## Recommended Stack

### Agent CLIs to onboard / complete

| CLI | Version | Headless launch | Auto-approve | Output signal | Driver family |
|-----|---------|-----------------|--------------|---------------|---------------|
| Antigravity CLI (`agycli` → `antigravity-cli`) | 1.1.14 | `-p "<prompt>"` / `--print` | `--dangerously-skip-permissions` (agycli adds it by default) | `--output-format text\|json\|stream-json`; `--input-format stream-json` (NDJSON turn on stdin) | **Claude-style** (stream-json capable) |
| Hermes | 0.20.4 (Python) | `-z "<prompt>"` / `--oneshot` — prints **only the final response text** to stdout, no banner/spinner | `--yolo` | process-exit + prompt-embedded `DEVFLOW_RESULT` contract (no JSON stream for oneshot) | **Pi-style positional** (single-document) |
| OpenCode | 1.18.18 | `opencode run "<prompt>"` | `--auto` | `--format json` (raw JSON events, JSONL) | **Codex-style JSONL** (completion-parsable) |

### Binary-name resolution (Antigravity — do not guess)

- `antigravity` — native ELF, **v1.1.13**, lacks `--input-format stream-json`.
- `agycli` — shell wrapper: `exec antigravity-cli --dangerously-skip-permissions "$@"` — **v1.1.14**,
  has `--input-format stream-json`. This is the canonical "antigravity-cli" binary named in the
  Phase 19 cross-AI review ("the working `antigravity-cli` binary, not the broken `agy`→GUI wrapper").
- `agy` — shell wrapper: `exec antigravity "$@"` (the 1.1.13 binary, no skip-permissions).

**Decision:** the driver targets the `antigravity` binary name on `PATH` but must resolve the
*vetted* binary (1.1.14 `antigravity-cli` surface). This is a per-driver `build_command` /
`ensure_agent_binary` decision — capture it in the driver, not a guess at plan time.

### No new Rust dependencies

All three integrations are argv/process-level. No crate additions, no network deps. The existing
`DriverCapabilities` / `SandboxRequirements` / `DriverHealth` enums already carry everything new
drivers need (`#[non_exhaustive] + Default` by design, 999.31 D-01).

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Antigravity binary | `agycli` (→ `antigravity-cli` 1.1.14) | `antigravity` (1.1.13) | older, lacks `--input-format stream-json`; 1.1.14 is what the review vetted |
| Hermes entry point | `hermes -z "<prompt>" --yolo` | `hermes send` | `send` is a messaging-platform pipe (no LLM/agent loop) — wrong tool |
| OpenCode output | `--format json` (JSONL events) | default formatted text | unparseable for completion/verdict detection |

## Installation

No new installs required — all three CLIs are already on `PATH` (`~/.local/bin`). DevFlow's
`ensure_agent_binary` preflight already fails loud when a configured agent binary is absent.

## Sources

- `antigravity --help` / `agycli --help` (installed CLIs, v1.1.13 / v1.1.14) — 2026-08-18
- `hermes -h` (installed, v0.20.4) — 2026-08-18
- `opencode run --help` (installed, v1.18.18) — 2026-08-18
- `crates/devflow-core/src/agents/{claude,codex,opencode,pi}.rs` + `mod.rs` (driver contract)
- `~/.local/bin/{agy,agycli}` wrapper scripts (binary-name resolution evidence)
