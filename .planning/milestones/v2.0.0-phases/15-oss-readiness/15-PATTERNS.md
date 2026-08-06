# Phase 15 (15b): OSS Packaging - Pattern Map

**Mapped:** 2026-07-17
**Files analyzed:** 9 (README.md, ARCHITECTURE.md, CONTRIBUTING.md, SECURITY.md, DEPENDENCIES.md,
docs/guides/{quickstart,configuration}.md, .devcontainer/devcontainer.json [new],
Cargo.toml/LICENSE-APACHE [new])
**Analogs found:** 9 / 9 (all are diff-in-place rewrites against themselves + source-of-truth code,
except `.devcontainer/` which is greenfield)

This phase is documentation/config, not application code. For every existing doc file, the
"analog" is **its own current version** — the planner corrects it in place against named source
files, not against a different file's pattern. Excerpts below are the exact current-state text
to diff against, plus the ground-truth source snippets each doc must match.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `README.md` | doc (config) | transform (prose ↔ CLI surface) | itself (current `README.md`) | exact — small accuracy pass |
| `ARCHITECTURE.md` | doc (config) | transform (prose ↔ source code) | itself (current `ARCHITECTURE.md`) | exact — full rewrite, structure reusable |
| `CONTRIBUTING.md` | doc (config) | transform | itself (current `CONTRIBUTING.md`) | exact — minimal touch |
| `SECURITY.md` | doc (config) | transform | itself (current `SECURITY.md`) | exact — one-line fix |
| `DEPENDENCIES.md` | doc (config) | transform | itself (current `DEPENDENCIES.md`) | exact — accuracy pass (flagged discretionary in RESEARCH) |
| `docs/guides/quickstart.md` | doc (config) | transform | itself | exact — accuracy pass (discretionary) |
| `docs/guides/configuration.md` | doc (config) | transform | itself | exact — accuracy pass (discretionary) |
| `.devcontainer/devcontainer.json` | config | file-I/O (static config, no data flow) | `.github/workflows/ci.yml` (closest existing toolchain-pinning convention) | role-match — no devcontainer precedent exists |
| `Cargo.toml` (license fix) + `LICENSE-APACHE` | config | file-I/O | itself (`Cargo.toml` workspace.package block) + `LICENSE` (existing MIT file, as structural analog for the new Apache file) | exact for Cargo.toml edit; role-match for new LICENSE-APACHE |

## Pattern Assignments

### `README.md` (doc, transform)

**Analog:** itself, current `/README.md` (189 lines) — already accurate from Phase 13 (`4da52e5`).
Only a targeted diff is needed, not a rewrite.

**Current command table** (`README.md:88-118`) is missing `gate` and `logs` (both added in 15a).
Ground truth for what to add — `crates/devflow-cli/src/main.rs:61` `enum Command` and `:226`
`enum GateCmd` (list/approve/reject subcommands, per CONTEXT.md 15a bullet).

**Current "Configuration" section to correct** (`README.md:120-122`):
```markdown
## Configuration

DevFlow stores runtime state in `.devflow/state.json` (git-ignored). No config file or init step is required — all workflow options are supplied as CLI flags to `devflow start`.
```
This must change to per-phase `state-NN.json` (State of the Art table in RESEARCH.md: "Phase 14a
(2026-07-16)" per-phase state). Ground truth: `crates/devflow-core/src/workflow.rs` /
`crates/devflow-core/src/state.rs` state file naming.

**Documentation section to extend** (`README.md:176-181`) — add an `OPERATIONS.md` link:
```markdown
## Documentation

- [DEPENDENCIES.md](DEPENDENCIES.md) — full dependency matrix
- [ARCHITECTURE.md](ARCHITECTURE.md) — design documentation
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute
- [CHANGELOG.md](CHANGELOG.md) — version history
```
Insert an `[OPERATIONS.md](OPERATIONS.md) — operator reference (gate protocol, env vars, file
inventory)` row — `OPERATIONS.md` already exists at repo root (15a, `/OPERATIONS.md`, 120 lines).

**CI badge already correct, reuse verbatim as pattern reference** (`README.md:5`):
```markdown
[![CI](https://github.com/denniyahh/devflow/actions/workflows/ci.yml/badge.svg)](https://github.com/denniyahh/devflow/actions/workflows/ci.yml)
```

---

### `ARCHITECTURE.md` (doc, transform — full rewrite)

**Analog:** itself, current `/ARCHITECTURE.md` (230 lines) — the *structure* (Crates → State
machine → Agent model → Completion evaluation → Monitor → Worktree → Git/ship → Configuration →
Logging → Extension points) is a reusable section skeleton; nearly every factual claim inside
it is stale and must be re-derived from source.

**Stale content to replace, section by section:**

1. **State machine** (`ARCHITECTURE.md:20-38`) describes a dead 8-step machine:
```
Idle → Branching → Planning → Executing → Verifying → Docsing → Shipping → Cleaning → Idle
```
Replace with the real `Stage` enum — ground truth `crates/devflow-core/src/stage.rs:16-26`:
```rust
pub enum Stage {
    /// Gather requirements via adaptive Q&A (`/gsd-discuss-phase`).
    Define,
    /// Research + plan + verify loop (`/gsd-plan-phase`).
    Plan,
    /// Wave-based parallel execution (`/gsd-execute-phase`).
    Code,
    /// Nyquist coverage audit (`/gsd-validate-phase`).
    Validate,
    /// PR + review + merge prep (`/gsd-ship`).
    Ship,
}
```
Also cover (per RESEARCH.md's diagram): `hooks.rs` (BranchCreate/DocsUpdate/ChangelogAppend/
VersionBump/BranchCleanup at mapped transitions), `workflow.rs` (per-phase state), `lock.rs`
(two-level locking), `events.rs` (`events.jsonl`, schema v1), `gates.rs` (`run_gate()`,
`.devflow/gates/NN-<stage>.json`, `DEVFLOW_GATE_NOTIFY_CMD`, `poll_response()`), `monitor.rs`
(`spawn_monitor()` detached child).

2. **Configuration section** (`ARCHITECTURE.md:127-158`) documents a removed `.devflow.yaml` /
`devflow init` model with a full YAML schema — this file/command no longer exists (removed as
part of v2.0.0; decoy physically deleted in 15a per CONTEXT.md). Replace with: no config file,
all options are `devflow start` CLI flags (mirror README's current, already-correct
"Configuration" framing, cross-referencing the corrected per-phase state file naming).

3. **Git and ship model** (`ARCHITECTURE.md:108-125`) references `confirm` / `rejectpr` commands
that are **not present** in `main.rs`'s `enum Command` (verified via `rg -n "enum Command"
crates/devflow-cli/src/main.rs` → line 61; no `Confirm`/`RejectPr` variant). Replace with the
gate-driven Ship model: `devflow gate approve <phase> --stage ship` (ground truth
`crates/devflow-cli/src/main.rs:226` `enum GateCmd`, `Gates::respond` atomic write).

4. **Extension points — adding an agent** (`ARCHITECTURE.md:216-231`) — checklist itself is
still accurate structurally; only update the final "Update docs" line's file list if doc set
changes. **Do not duplicate this checklist's content into CONTRIBUTING.md** — CONTRIBUTING.md
already correctly defers to this section via anchor link (`CONTRIBUTING.md:170-171`); keep that
pointer as-is (RESEARCH.md Pitfall 2).

**Sections already accurate, reuse verbatim as structural pattern:** Crates table
(`ARCHITECTURE.md:8-18`), Agent model (`ARCHITECTURE.md:40-56`), Completion evaluation
(`ARCHITECTURE.md:58-73`, three-layer, matches README's Agent Protocol table), Logging section
(`ARCHITECTURE.md:160-215`, matches CONTRIBUTING.md's Logging Conventions section verbatim in
spirit — keep both in sync).

---

### `CONTRIBUTING.md` (doc, transform — minimal touch)

**Analog:** itself, current `/CONTRIBUTING.md` (175 lines) — already accurate (fork → branch →
test → PR workflow present at lines 79-92). Minimal changes only.

**PR Process section to extend** (`CONTRIBUTING.md:79-92`) — per RESEARCH.md's recommended
resolution of the "PR gate status" checklist item (Open Question 2), add a note listing the
three required CI checks, mirroring `.github/workflows/ci.yml`'s three jobs verbatim:
```yaml
# Source: .github/workflows/ci.yml:14-40
jobs:
  test:    # cargo test
  clippy:  # cargo clippy -- -D warnings
  fmt:     # cargo fmt --check
```

**Do not touch** the "Adding a New Agent" section (`CONTRIBUTING.md:156-171`) beyond, at most,
its trailing docs-file list — it already correctly points at
`ARCHITECTURE.md#extension-points--adding-an-agent` as authoritative (line 170-171).

**Distrobox section** (`CONTRIBUTING.md:14-22`) is an existing "isolated dev environment" pattern
already in this doc — useful as the tone/level-of-detail analog when documenting the new
`.devcontainer/` alternative (add a short parallel subsection rather than replacing distrobox).

---

### `SECURITY.md` (doc, transform — one-line fix)

**Analog:** itself, current `/SECURITY.md` (39 lines).

**Phantom reference to fix** (`SECURITY.md:37`):
```markdown
- Do not expose `.devflow/state.json` or `.devflow/audit.log` to untrusted contexts
```
`audit.log` does not exist anywhere in the codebase (RESEARCH.md verified via
`rg -n "audit.log"` — zero source hits). Replace with real state files: per-phase
`.devflow/state-NN.json` and `.devflow/events.jsonl` (ground truth: `events.rs`, `workflow.rs`).

**Version table** (`SECURITY.md:17-20`) currently says `v1.0.0+`; cross-check against
`Cargo.toml:9` `version = "1.2.0"` before publish — align "Supported Versions" wording if stale.

---

### `DEPENDENCIES.md` (doc, transform — discretionary accuracy pass)

**Analog:** itself, current `/DEPENDENCIES.md` (57 lines).

**Stale `doctor` output sample to correct** (`DEPENDENCIES.md:46-57`):
```
$ devflow doctor
  ...
  devflow v1.0.0      ✓
  .devflow.yaml       ✓ (found)
```
`.devflow.yaml` no longer exists (decoy removed in 15a) and version should track
`Cargo.toml`'s `1.2.0`. Also `"Required for Shipping"` section header
(`DEPENDENCIES.md:15-17`) says "Needed for `devflow ship` / `devflow confirm`" —
`devflow confirm` is not a real `Command` variant (see ARCHITECTURE.md note above); drop the
`confirm` reference.

---

### `docs/guides/quickstart.md`, `docs/guides/configuration.md` (doc, transform — discretionary)

**Analog:** each file, itself. Not yet read in this pass (flagged Open Question 1 in
RESEARCH.md as needing operator confirmation before committing scope) — same stale-content
class as old ARCHITECTURE.md (`devflow init`, `.devflow.yaml`, old `doctor` sample). Planner
should re-verify current content against `main.rs` before scoping a rewrite task, same
methodology as README/ARCHITECTURE (Pattern 1: Docs-as-source-of-truth verification).

---

### `.devcontainer/devcontainer.json` (config, file-I/O — greenfield, no repo analog)

**Analog:** `.github/workflows/ci.yml` (closest existing toolchain-pinning convention in this
repo) — extract the Rust toolchain pinning pattern to keep consistent:

**Toolchain pinning pattern** (`.github/workflows/ci.yml:17-30`):
```yaml
- uses: dtolnay/rust-toolchain@stable
  with:
    components: clippy
```
And `rust-toolchain.toml` (repo root, 3 lines):
```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
```
The devcontainer's `postCreateCommand` must install matching components (`rustup component add
clippy rustfmt`) rather than assuming the base image includes them, mirroring CI's explicit
`components:` declaration rather than relying on implicit defaults.

**Starting point (from RESEARCH.md Code Examples, verify base image tag at build time — A1
assumption, MEDIUM confidence):**
```jsonc
{
  "name": "DevFlow",
  "image": "mcr.microsoft.com/devcontainers/rust:1-1-bookworm",
  "mounts": [
    { "source": "devflow-cargo-registry", "target": "/usr/local/cargo/registry", "type": "volume" },
    { "source": "devflow-target", "target": "${containerWorkspaceFolder}/target", "type": "volume" }
  ],
  "postCreateCommand": "rustup component add clippy rustfmt && cargo build --workspace",
  "customizations": {
    "vscode": { "extensions": ["rust-lang.rust-analyzer", "vadimcn.vscode-lldb", "tamasfe.even-better-toml"] }
  }
}
```
Per RESEARCH.md Anti-Patterns/Pitfall 5: mount both the registry cache AND `target/`, not just
the registry — a workspace with 2 crates otherwise recompiles from scratch every rebuild.

**Container test:** RESEARCH.md's Wave 0 Gap recommends a `.github/workflows/devcontainer.yml`
job (new file, same job-shape analog as `ci.yml`'s existing jobs) running
`cargo build && cargo test && cargo clippy` inside the built container image, rather than only
documenting a manual local `devcontainer build` step.

---

### `Cargo.toml` license fix + `LICENSE-APACHE` (config, file-I/O)

**Analog:** `Cargo.toml:11` current declaration, and the existing `/LICENSE` (MIT text) file as
the structural analog for the new file.

**Current mismatch** (`Cargo.toml:11`):
```toml
license = "MIT OR Apache-2.0"
```
Only `/LICENSE` (MIT) exists at repo root — no `LICENSE-APACHE`. RESEARCH.md Pitfall 4: either
add real Apache-2.0 license text as `LICENSE-APACHE` (matching the copyright holder in the
existing `LICENSE` file), or narrow `Cargo.toml:11` to `license = "MIT"`. This is a genuine
correctness gap to close before `cargo publish`, not a docs-only issue.

**README badge to keep in sync if license choice changes** (`README.md:6`, `:187-189`):
```markdown
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
...
## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
```

## Shared Patterns

### Docs-as-source-of-truth verification
**Source:** RESEARCH.md Pattern 1 (Architecture Patterns section); worked example already
applied in this PATTERNS.md above (Stage enum, GateCmd, Command enum).
**Apply to:** Every doc task in this phase (README, ARCHITECTURE, CONTRIBUTING, DEPENDENCIES,
docs/guides). Every factual claim (command name, flag, file path, struct/enum name) must trace
to a specific source file + line before being written — never reconstruct from memory of "what
DevFlow used to do."

### `--help` snapshot as acceptance signal (already landed, 15a — do not rebuild)
**Source:** `crates/devflow-cli/tests/help_snapshot.rs`, `crates/devflow-cli/tests/snapshots/devflow-help.txt`
**Apply to:** Treat as the CLI-surface guard proving README/OPERATIONS.md match real `--help`
output. Run `cargo test -p devflow-cli --test help_snapshot` after any README command-table edit;
if it fails, the CLI changed unexpectedly (out of scope for a docs-only phase) — investigate
before touching docs further.

### Cross-doc consistency: "Extension points / adding an agent" checklist
**Source:** `ARCHITECTURE.md:216-231` (authoritative) vs. `CONTRIBUTING.md:156-171` (pointer)
**Apply to:** ARCHITECTURE.md and CONTRIBUTING.md. Only update ARCHITECTURE.md's copy; keep
CONTRIBUTING.md's copy as the short anchor-link pointer it already is. Avoid re-diverging.

### CI job shape for any new devcontainer test workflow
**Source:** `.github/workflows/ci.yml:13-40` (three parallel jobs: test/clippy/fmt, each
`actions/checkout@v4` + `dtolnay/rust-toolchain@stable`)
**Apply to:** `.github/workflows/devcontainer.yml` (new, if built) — reuse the same
`actions/checkout@v4` + toolchain-pin structure, adding a container-build step ahead of the
`cargo build && cargo test && cargo clippy` invocation.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `.devcontainer/devcontainer.json` | config | file-I/O | No devcontainer convention exists anywhere in this repo — pure greenfield. Closest available analog is `.github/workflows/ci.yml`'s toolchain-pinning pattern (used above), not a same-role match. |
| `.devcontainer/Dockerfile` (optional) | config | file-I/O | Same as above — build only if the prebuilt `mcr.microsoft.com/devcontainers/rust` image needs extension; no existing Dockerfile in repo to pattern-match against. |
| `LICENSE-APACHE` (if added) | config | file-I/O | No existing Apache-2.0 text in repo; must be sourced from the canonical Apache Software Foundation license text, not derived from `LICENSE` (which is MIT text, a different license). |

## Metadata

**Analog search scope:** repo root (README.md, ARCHITECTURE.md, CONTRIBUTING.md, SECURITY.md,
DEPENDENCIES.md, Cargo.toml, OPERATIONS.md, LICENSE), `.github/workflows/ci.yml`, `docs/guides/`,
`crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/stage.rs`, `rust-toolchain.toml`
**Files scanned:** 13
**Pattern extraction date:** 2026-07-17
</content>
