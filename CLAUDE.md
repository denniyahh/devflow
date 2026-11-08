# DevFlow — Project Instructions

Deliberately minimal. Global agent rules live in `~/.config/agents/AGENTS.md`; this file holds only constraints specific to how *this* repository is built, verified, and worked on.

## Crate Topology
- `crates/devflow-core`: Core state machine, pipeline engines, and workflow abstractions (`lib`).
- `crates/devflow-cli`: CLI interface and driver commands (`bin: devflow`).
- `devflow-core` is published on crates.io, so changing or removing a `pub` item or variant is a semver change. Prefer an additive change, and run `cargo semver-checks check-release -p devflow-core` before committing one.

## Canonical Verification & Test Commands
Always run targeted package tests first, then full workspace validation:
- Test `devflow-core` library: `cargo test -p devflow-core --lib`
- Test `devflow` CLI binary: `cargo test -p devflow --bin devflow`
  *(Note: `devflow` is binary-only; running `cargo test -p devflow --lib` verifies nothing and fails).*
- Full check suite (formatting, clippy, tests): `scripts/check.sh all`
- Container parity check: `scripts/check-in-container.sh`

## Repo Traps (verification gates)
Shapes that have shipped a false green, or a gate that fails correct work, here. `scripts/lint-plan-bashisms.sh` (pre-commit, staged `*PLAN.md`) refuses the first three inside `<automated>` blocks.
- `cargo test <filter>` exits 0 when the filter matches nothing. Gate on one named test: `cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=module::path::name)'` (`--bin devflow` for the CLI). One test per command, chained with `&&`: `-E 'test(=a) | test(=b)'` skips a wrong name and exits 0.
- `rg -c` / `--count` print nothing, not `0`, on zero matches. Check "no X left" or "every Y documented" with a Rust source-scan test (like `doc_check.rs`), not a shell count.
- The CLI package is `devflow`, not `devflow-cli`.
- Prefer a named Rust test to any shell pipeline; keep shell to the invocation.
- Every literal `std::env::var("DEVFLOW_…"` / `var_os(` / `env_value(` read under `crates/` (tests included) and every `Command` variant must be named in README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md or `docs/guides/`, or allowlisted in `doc-check-allowlist.toml`, in the same change; otherwise `doc_check` fails `scripts/check.sh all`.
- `crates/devflow-cli/build.rs` embeds the commit and dirty flag, so a rebuilt binary's sha256 changes after every commit: never compare it with a stored one.
- `core.fileMode=false`: set an executable bit with `git add <f> && git update-index --chmod=+x <f>`; `--chmod` alone exits 128 on an untracked file.

## Workspace & Worktrees
- Development phases run inside dedicated worktrees under `.worktrees/phase-N`.
- Use `scripts/phase-worktree.sh <phase>` to initialize or attach to a phase worktree.
- Never run concurrent git operations on the main checkout while an executor holds the working tree.
- `graphify-out/` is ignored and never committed (since 2026-11-08). The operator's global auto-refresh hook rebuilds each checkout's graph after commits, and seeds a new worktree from the main checkout's graph. Do not delete the main checkout's `graphify-out/`: it holds the only copy of the LLM-derived semantic nodes.

## Dogfooding (operator rule, 2026-11-04)
- From Phase 51 on, DevFlow drives its own phases: run them with `devflow start --phase N --mode <auto|supervise>` and the pipeline's gates, not by hand-running GSD commands in an agent session. A stage run by hand is an exception: say so and give the reason in the phase's notes.
- Run the installed release binary. Check `devflow --version` against the version you mean to test before a run, and reinstall (`cargo install --path crates/devflow-cli --locked --force`) after any source change the run depends on.
- Phase 51 conventions (operator, 2026-11-04): Discuss runs in GSD auto (`/gsd-discuss-phase 51 --auto`) in an agent session BEFORE `devflow start`, because DevFlow's Define stage never runs discuss (D-14, `define_stage_prompt` in `crates/devflow-core/src/prompt.rs`); then `devflow start --phase 51 --agent claude --mode supervise --until plan`, since supervise gates Validate, not the Plan→Code boundary; after Plan, run an external review with the adversarial lens on codex and agy (see the `external-review` skill) before Code starts; the Phase 50 conventions carry over: Ship stays blocked during the run (`yes_ship` false) and close-out is done by hand with `scripts/cut-pr-branch.sh` from `feature/phase-N`.
- Phase 51 scope (operator, 2026-11-05): fold in backlog 999.102 (idle-timeout failure message drops cause fields), 999.71 (torn capture-line risk to rate-limit event parsing) and 999.154 (no resume test with the gate-notify variable unset).
- Defects found while dogfooding (operator rule, 2026-11-08). The point of dogfooding is to prove DevFlow works and to improve it, so a defect is the most valuable thing a run produces:
  - **Learn it, then tell the operator.** When DevFlow misbehaves, errors, or has a gap, find out what it actually does and why (read the source of the installed version, reproduce it), and put it in front of the operator in the next report, in plain words, under its own item. Do not let it pass as a side note.
  - **Do not bypass it on your own.** Do not route around a defect with independent action (hand-editing `.devflow/` state, hand-running a stage, killing a process, answering a gate a way that dodges the behavior) unless the operator approves that step. Stop and present the defect with options instead. If a step must be taken outside DevFlow, it is the operator's call and gets recorded as an exception in the phase notes.
  - **Log it right away.** File every defect as a `999.x` backlog entry in the same session: what happened, the evidence, what is not established, and a fix shape. Defects are fixed as soon as practically possible, not parked: say which upcoming phase or fix branch should take it.
  - **Propose enhancements.** When a run shows a weakness, an awkward step, a missing signal, or a gap (not only a hard bug), suggest the improvement to the operator, with a recommendation.
- Phase 50.1 was executed with `/gsd-execute-phase` in a session, not by DevFlow. Its last live run used an earlier build, and no run has exercised the final 3.0.0 code.

## Development Setup Checklist
- When modifying CI workflows, hooks, devcontainers, or tooling dependencies, keep `.planning/user/DEV-SETUP-CHECKLIST.md` updated in the same commit (warned by `scripts/hooks/post-commit`).
