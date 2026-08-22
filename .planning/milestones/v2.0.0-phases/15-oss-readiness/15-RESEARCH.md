# Phase 15 (15b): OSS Packaging - Research

**Researched:** 2026-07-17
**Domain:** OSS project packaging — docs accuracy (README/ARCHITECTURE/CONTRIBUTING), dev container tooling, crates.io publishing, GitHub badges — for a Rust CLI workspace, executed through DevFlow's own pipeline as a dogfood run.
**Confidence:** HIGH (all core facts verified directly against the current codebase; external conventions confirmed against official docs)

<user_constraints>
## User Constraints (from CONTEXT.md)

> CONTEXT.md for this phase is not written in the standard `## Decisions` /
> `## Claude's Discretion` / `## Deferred Ideas` format — it is a locked checklist
> (operator-authored, 2026-07-16 rescope). Reproduced verbatim below, mapped to the nearest
> equivalent structure.

### Locked Decisions (the 15b checklist — treat every bullet as locked scope, not exploratory)

> ## 15b — Open Source Packaging *(run as a DevFlow dogfood)*
>
> - [ ] README: full rewrite against `main.rs` — currently documents
>       `init`/`config`/`verify`/`lint`/`ship`/`confirm`/`rejectpr` (none
>       exist) and `--mode auto|manual` instead of `auto|supervise`.
>       Installation, quickstart, agent support table; link OPERATIONS.md.
> - [ ] ARCHITECTURE.md: full rewrite against current code (Stage enum, GSD
>       prompts, hooks, per-phase state + two-level locking, events.jsonl,
>       monitor ownership). Extension-point docs for agent adapters.
> - [ ] CONTRIBUTING.md: fork → branch → test → PR workflow.
> - [ ] Dev container: `.devcontainer/devcontainer.json`, Dockerfile,
>       container test (`cargo build && cargo test && cargo clippy`).
>       (`devflow devcontainer` subcommand only if it earns its keep.)
> - [ ] CI badge + PR gate status; CODE_OF_CONDUCT/SECURITY refresh if needed.
> - [ ] Publish `devflow` to crates.io (publish-prep landed in 12-06; publish
>       once docs describe the real product).
>
> **Note (this research):** the README bullet's problem description ("currently documents
> `init`/`config`/`verify`/`lint`/`ship`/`confirm`/`rejectpr`... `--mode auto|manual`") is
> now stale — Phase 13 already corrected README's command table and mode flag before this
> CONTEXT.md was written. The locked *deliverable* (a README that's fully accurate against
> `main.rs`, with installation/quickstart/agent table/OPERATIONS.md link) still stands; only
> the size of the remaining gap has shrunk. See Summary and Common Pitfalls below.

### Claude's Discretion

> Not explicitly separated in this CONTEXT.md. Treat unlisted specifics as discretionary
> within the locked checklist above: exact devcontainer base image/tag, exact cache-mount
> shape, CONTRIBUTING.md wording changes (if any), badge placement/styling, and whether
> `DEPENDENCIES.md`/`docs/guides/*.md` accuracy fixes are folded into this phase (flagged as
> Open Question 1 below — recommend confirming with the operator rather than assuming).

### Deferred Ideas (OUT OF SCOPE)

> ## Deferred out of this phase (2026-07-16)
>
> - **Antigravity agent adapter** (old 15c) — unscheduled backlog; revisit
>   after Phase 16 when there's a concrete need.
>
> ## Moved to Phase 14 → 16 (2026-07-14 / 2026-07-16)
>
> - `HermesAgent` adapter, Hermes skill-file rewrite, Hermes plugin — now
>   Phase 16 (Hermes Support).

Also treat 15a (`devflow gate` subcommand, OPERATIONS.md, `.devflow.yaml` decoy removal,
IN-01 lib.rs rustdoc, `--help` snapshot test) as **out of scope — already shipped and
merged** (`90875ef`, `a50ad03`). Do not re-plan or re-verify 15a work in 15b.
</user_constraints>

## Summary

15b is a **documentation-accuracy and packaging** phase, not a code-feature phase. The
codebase itself (`crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/*.rs`) is the
ground truth; every doc file must be checked against it directly rather than assumed. Direct
inspection this session found the actual gap is narrower — but differently shaped — than
CONTEXT.md's checklist implies:

- **README.md is already mostly accurate** (rewritten during Phase 13 dogfood work,
  commit `4da52e5`). It documents the real command set (`start`, `status`, `list`,
  `cleanup`, `parallel`, `sequentagent`, `reference`, `test`, `recover`, `doctor`) and the
  real `--mode auto|supervise` flag. It is missing the two commands 15a added
  (`gate`, `logs`), does not link `OPERATIONS.md`, and its "Configuration" section still
  describes the pre-14a single `.devflow/state.json` file instead of per-phase
  `state-NN.json`.
- **ARCHITECTURE.md is severely stale** and needs the full rewrite CONTEXT.md describes: it
  documents a dead 8-step state machine (`Idle → Branching → Planning → Executing →
  Verifying → Docsing → Shipping → Cleaning → Idle`), a `devflow init`/`.devflow.yaml`
  config file that no longer exists, and `confirm`/`rejectpr` commands that were removed.
  None of this matches the real `Stage` enum (`Define → Plan → Code → Validate → Ship`),
  the hooks model (`hooks.rs`), per-phase state + two-level locking (`workflow.rs`,
  `lock.rs`), `events.jsonl` (`events.rs`), or the gate CLI (`gates.rs` + `main.rs`
  `GateCmd`).
- **CONTRIBUTING.md is already accurate and current** — fork/branch/test/PR workflow,
  correct project structure, correct logging conventions, and it already points at
  `ARCHITECTURE.md#extension-points--adding-an-agent`. Minimal changes needed here.
- **`DEPENDENCIES.md` and `docs/guides/{quickstart,configuration}.md` are undocumented in
  CONTEXT.md's checklist but are just as stale as old-ARCHITECTURE.md** (`devflow init`,
  `.devflow.yaml`, a `doctor` output sample showing `.devflow.yaml ✓ (found)` and
  `devflow v1.0.0`) — see Open Questions; these are linked from README's "Documentation"
  section and from OPERATIONS.md's file-inventory framing, so leaving them stale
  undermines the accuracy goal of the whole phase even though CONTEXT.md doesn't name them.
- **SECURITY.md** references a `.devflow/audit.log` file that does not exist anywhere in
  the codebase — a phantom reference from an earlier version.
- **crates.io publish-prep is verified still valid today**: `cargo publish --dry-run -p
  devflow-core` and `cargo package --workspace` both pass cleanly against the current tree
  (re-run this session, not just trusted from the 12-06 SUMMARY). The one real gap: `Cargo.toml`
  declares `license = "MIT OR Apache-2.0"` but only a single `LICENSE` (MIT) file exists at
  the repo root — no `LICENSE-APACHE`. This is a genuine dual-license/SPDX mismatch to fix
  before publishing.
- **No `.devcontainer/` exists at all** — this is 100% new work, not a rewrite.
- **CI badge already exists and is correctly formed** in README (`ci.yml/badge.svg` linked
  to the Actions run). "PR gate status badge" has no standard separate GitHub artifact —
  see Open Questions for how to interpret that checklist bullet.
- **This phase is explicitly meant to be executed as a DevFlow dogfood run** — OPERATIONS.md
  already contains a worked example: `devflow start --phase 15 --agent claude --mode auto .`
  This has a structural implication for planning: Code-stage tasks must be fully
  agent-executable without mid-stream human input (the agent runs headless); the human's
  only touchpoint is the Ship gate (`devflow gate approve/reject`), consistent with the 15a
  exit criterion.

**Primary recommendation:** Treat this as two work-shapes, not one: (1) a targeted,
verification-driven doc-accuracy pass (ARCHITECTURE.md full rewrite; README/DEPENDENCIES/
docs-guides/SECURITY smaller corrections; CONTRIBUTING near-untouched) built by re-deriving
every factual claim from the named source file, never by "cleaning up prose"; and (2) new
infrastructure (`.devcontainer/`, actual `cargo publish`) that has no existing artifact to
correct against. Do not let the plan assume CONTEXT.md's problem description of README is
still accurate — verify against `main.rs` for every command/flag claim, the same way 15a's
`--help` snapshot test now guards it going forward.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| README/ARCHITECTURE/CONTRIBUTING accuracy | Docs (repo root) | CLI (`main.rs`) is the source of truth | Docs describe the CLI; CLI is not touched by this phase |
| `.devcontainer/` dev environment | Dev tooling / CI-parity | — | Runs `cargo build/test/clippy` in a container; no product code changes |
| crates.io publish | Packaging / registry | Cargo workspace metadata | `cargo publish` reads `Cargo.toml` metadata already set in Phase 12-06 |
| CI badge / PR gate status | Docs (README) | CI (`.github/workflows/ci.yml`) | Badge markup lives in README; the underlying signal is the existing CI workflow |
| CODE_OF_CONDUCT/SECURITY refresh | Docs (repo root) | — | Governance docs, no code coupling |
| "Run 15b as a dogfood" | DevFlow pipeline (Stage/Gate/Monitor) | Docs (deliverables) | The phase's own deliverables are produced BY DevFlow's Define→Plan→Code→Validate→Ship loop, not just ABOUT it |

## Standard Stack

No new runtime dependencies are introduced by this phase — no `Cargo.toml` [dependencies]
changes are required. The "stack" here is tooling/process, not libraries.

### Core (tooling used by this phase)
| Tool | Version (verified) | Purpose | Why Standard |
|------|---------------------|---------|---------------|
| `cargo publish` / `cargo package` | Cargo 1.97.1 [VERIFIED: `cargo --version` in this environment] | Publish `devflow-core` then `devflow` to crates.io | Built-in Cargo workflow; no third-party publish tool needed [CITED: doc.rust-lang.org/cargo/reference/publishing.html] |
| Dev Containers spec (`devcontainer.json`) | Spec version implied by `mcr.microsoft.com/devcontainers/rust` base image tag | Reproducible contributor/CI-parity environment | De facto standard for VS Code / GitHub Codespaces / `devcontainer CLI` [CITED: containers.dev/implementors/reference] |
| GitHub Actions workflow badge | N/A (static URL convention) | CI status badge in README | Standard `actions/workflows/<file>/badge.svg` URL pattern [CITED: docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/adding-a-workflow-status-badge] |

### Supporting
| Tool | Purpose | When to Use |
|------|---------|-------------|
| `dtolnay/rust-toolchain@stable` (already used in `.github/workflows/ci.yml`) | Pin/install stable Rust in CI and (optionally) devcontainer parity checks | Already in use; reuse the same action reference inside any devcontainer-driven CI job if one is added |
| `mcr.microsoft.com/devcontainers/rust` base image | Prebuilt Rust dev container image (cargo, rustup, common tooling preinstalled) | Base image for `.devcontainer/devcontainer.json`; avoids hand-rolling a Dockerfile from `rust:slim` [CITED: github.com/microsoft/vscode-remote-try-rust] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `mcr.microsoft.com/devcontainers/rust` prebuilt image | Custom `Dockerfile` from `rust:1-slim` + manual `rustup component add clippy rustfmt` | Custom Dockerfile gives full control (e.g. pinning to `rust-toolchain.toml`'s `stable` channel exactly) but is more to maintain; the prebuilt image is the documented convention and is what the official `vscode-remote-try-rust` sample uses |
| Named Docker volume for `~/.cargo`/`target` cache | No cache (rebuild every time) | A named volume (`{"source": "devcontainer-cargo-cache", "target": "/usr/local/cargo", "type": "volume"}`) meaningfully speeds up repeat builds in a 2-crate workspace; skip only if container rebuild time is a non-issue [CITED: github.com/devcontainers/templates/issues/117] |
| `cargo publish` (2-step: core, then CLI) | A publish helper crate (e.g. `cargo-release`) | Not needed for a 2-crate workspace with a settled version scheme already (git-tag-driven SemVer, Phase 11 decision) — adding a release-automation crate here would be scope creep this phase doesn't need |

**Installation:** No new Cargo dependencies to install. Devcontainer and CI changes are config-only (`.devcontainer/devcontainer.json`, optional `Dockerfile`, no `Cargo.toml` dependency edits).

**Version verification:** `cargo --version` in this environment reports `1.97.1` [VERIFIED: local `cargo --version`], well above the `stable` toolchain pinned in `rust-toolchain.toml`. No package versions need registry verification since no new dependency is being added — only the two workspace crates' own versions (`1.2.0`, workspace-inherited) are being published.

## Package Legitimacy Audit

**Not applicable.** This phase adds no new external dependencies to `Cargo.toml`. It
*publishes* the project's own two crates (`devflow-core`, `devflow`) to crates.io — the
opposite direction of the Package Legitimacy Gate, which screens packages being *installed*.
The `.devcontainer` base image (`mcr.microsoft.com/devcontainers/rust`) is a container image
reference, not an npm/PyPI/cargo package, and is outside the scope of `gsd-tools query
package-legitimacy check`.

**Packages removed due to [SLOP] verdict:** none (n/a — no packages evaluated)
**Packages flagged as suspicious [SUS]:** none (n/a)

## Architecture Patterns

### System Architecture Diagram

This phase does not add a runtime data-flow component — the "system" being documented is
DevFlow's existing pipeline. The diagram below is what ARCHITECTURE.md's rewrite needs to
convey (verified directly against source, not the current stale diagram):

```
devflow start --phase N --agent A --mode M
        │
        ▼
 ┌─────────────────┐   creates .worktrees/phase-NN/ (default)
 │  worktree.rs     │   or branches in place (--no-worktree)
 └────────┬─────────┘
          ▼
 ┌─────────────────────────────┐
 │  workflow.rs / state.rs     │  writes .devflow/state-NN.json
 │  Stage::Define               │  (per-phase since 14a)
 └────────┬─────────────────────┘
          ▼
 ┌─────────────────────────────┐   spawn_monitor() → detached child owns
 │  monitor.rs                  │   the agent process; agent stdout/stderr
 │  launches agents::adapter_for│   captured to phase-NN-stdout/stderr.log
 └────────┬─────────────────────┘
          ▼  (agent exits)
 ┌─────────────────────────────┐
 │  devflow advance --phase N   │  lock.rs per-phase lock →
 │  agent_result.rs (3 layers)  │  agent_result::evaluate_agent_result
 └────────┬─────────────────────┘
          │  success/failure/verdict
          ▼
 ┌─────────────────────────────┐   Define→Plan→Code→Validate→Ship
 │  Stage transition            │   hooks.rs fires BranchCreate/DocsUpdate/
 │  (main.rs transition())      │   ChangelogAppend/VersionBump/BranchCleanup
 └────────┬─────────────────────┘   at the mapped transitions
          │  Validate/Ship reach a gate
          ▼
 ┌─────────────────────────────┐   writes .devflow/gates/NN-<stage>.json,
 │  gates.rs run_gate()         │   fires DEVFLOW_GATE_NOTIFY_CMD, blocks on
 │                               │   poll_response() (exp. backoff, 7d default)
 └────────┬─────────────────────┘
          ▼
   devflow gate approve|reject   ← human answers via CLI (15a)
          │
          ▼
 events.jsonl (every step above also appends one event line, schema v1)
```

### Recommended Project Structure (docs, not code — for this phase)
```
.devcontainer/
├── devcontainer.json   # base image, features, cargo cache mount, extensions
└── Dockerfile           # only if the prebuilt image needs extension (optional)
README.md                 # accuracy pass: add gate/logs rows, link OPERATIONS.md
ARCHITECTURE.md            # full rewrite against Stage/hooks/lock/events/gates
CONTRIBUTING.md            # minor pass only — already accurate
DEPENDENCIES.md            # accuracy pass (out of CONTEXT.md's checklist — flag, see Open Questions)
docs/guides/quickstart.md      # accuracy pass or removal (see Open Questions)
docs/guides/configuration.md   # accuracy pass or removal (see Open Questions)
SECURITY.md                    # drop phantom `.devflow/audit.log` reference
CODE_OF_CONDUCT.md              # spot-check only — no product coupling
```

### Pattern 1: Docs-as-source-of-truth verification
**What:** Every factual claim in a rewritten doc (a command name, a flag, a file path, a
struct/enum name) must be traced to a specific source file and line before being written,
not reconstructed from memory of "what DevFlow used to do."
**When to use:** For every section of ARCHITECTURE.md and every corrected section of README/
DEPENDENCIES.md/docs/guides in this phase.
**Example (this session's verification of the Stage enum):**
```rust
// Source: crates/devflow-core/src/stage.rs
pub enum Stage {
    Define,   // /gsd-discuss-phase {N}
    Plan,     // /gsd-plan-phase {N}
    Code,     // /gsd-execute-phase {N}
    Validate, // /gsd-validate-phase {N}
    Ship,     // /gsd-ship {N}
}
```

### Pattern 2: `--help` snapshot as a docs-drift guard (already landed, 15a)
**What:** `crates/devflow-cli/tests/help_snapshot.rs` diffs live `devflow --help` output
against a committed snapshot (`tests/snapshots/devflow-help.txt`) and fails loudly with an
explicit instruction to update docs first.
**When to use:** Already wired into the test suite; 15b's plan does not need to build this,
only to satisfy it (regenerate the snapshot if the CLI itself changes — it should not,
since 15b is docs-only) and to treat it as the acceptance gate proving README/OPERATIONS.md
match the real `--help` output.
**Example:**
```rust
// Source: crates/devflow-cli/tests/help_snapshot.rs
// "the committed snapshot is the contract between the CLI surface and the docs
//  (OPERATIONS.md, README). If this test fails, the CLI changed — update the
//  docs, then regenerate"
```

### Anti-Patterns to Avoid
- **Rewriting ARCHITECTURE.md from README's summary instead of from source:** README's
  "Architecture" section is a compressed summary and is itself only partially detailed
  (it doesn't mention `events.jsonl`, per-phase locking, or the gate CLI). Rewriting
  ARCHITECTURE.md by expanding README would reproduce gaps rather than close them — go to
  `stage.rs`, `hooks.rs`, `lock.rs`, `events.rs`, `gates.rs`, `workflow.rs`, `monitor.rs`
  directly.
- **Trusting CONTEXT.md's problem description of README as still accurate:** it was written
  before the Phase 13 README rewrite landed (`4da52e5`). Verify current README against
  `main.rs` before deciding what to change — most of the CONTEXT.md-described README gap
  (phantom `init`/`config`/`verify`/`lint`/`ship`/`confirm`/`rejectpr` commands,
  `--mode auto|manual`) no longer exists.
- **Publishing without fixing the LICENSE/Cargo.toml mismatch:** `Cargo.toml` declares
  `MIT OR Apache-2.0` but only `LICENSE` (MIT text) exists at the repo root. crates.io does
  not hard-block on this (the `license` field is a free-form SPDX string, not
  file-validated), but it is a real correctness gap for a project claiming dual licensing —
  either add `LICENSE-APACHE` or change the declared license to `MIT` before publishing.
- **Treating the devcontainer test as "just document it" instead of actually running it:**
  CONTEXT.md explicitly asks for "a container test that runs `cargo build && cargo test &&
  cargo clippy`" — this needs to actually execute (locally via `devcontainer CLI` or as a CI
  job), not just be described in prose, or it provides no real CI-parity guarantee.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Rust dev container base environment | A custom `Dockerfile` starting from `rust:slim` with manually `rustup component add clippy rustfmt` and manually installed VS Code server deps | `mcr.microsoft.com/devcontainers/rust` prebuilt image | Microsoft's official Rust devcontainer image already bundles rustup, cargo, common lints, and is the base image used by the reference `vscode-remote-try-rust` template [CITED: github.com/microsoft/vscode-remote-try-rust] |
| CI status badge | A third-party badge service (shields.io dynamic badge, custom badge generator) | GitHub's native `actions/workflows/<file>/badge.svg` URL (already used in README) | Native badges need no external service dependency and auto-update; README already does this correctly | 
| crates.io publish ordering (leaf-before-dependent) | A manual two-terminal publish script or CI job that guesses ordering | `cargo package --workspace` to verify locally (already proven in 12-06 and re-verified this session), then `cargo publish -p devflow-core` followed by `cargo publish -p devflow` once the core version is live on the registry | Cargo's own workspace tooling already resolves and verifies publish order through a temporary local registry — no custom tooling needed |
| Docs/CLI drift detection | A new bespoke doc-linter or manual changelog checklist | The existing `--help` snapshot test (15a) as the acceptance signal, extended by cross-referencing each doc's command table against `Command`/`GateCmd` in `main.rs` during review | Purpose-built guard already exists; reuse it rather than inventing a second mechanism |

**Key insight:** Every "don't hand-roll" item here already has an existing, working
solution in this exact repo or in the standard Rust/GitHub tooling ecosystem — this phase's
risk is not "which library to pick" but "verifying facts against source before writing
prose," which no library can do for you.

## Common Pitfalls

### Pitfall 1: Assuming CONTEXT.md's README description is current
**What goes wrong:** A plan built directly from CONTEXT.md's "currently documents
`init`/`config`/`verify`/`lint`/`ship`/`confirm`/`rejectpr`" claim would spend effort
"fixing" commands that are already gone from README, and might miss the two commands
(`gate`, `logs`) that actually are missing.
**Why it happens:** CONTEXT.md was written 2026-07-16 as part of the phase rescope, but
README was already corrected in Phase 13 (commit `4da52e5`, "sync README/CHANGELOG with
Phase 13 changes") before that rescope note was written — the note describes an
even-older state of the doc.
**How to avoid:** Diff README's current command table against `main.rs`'s `Command`/
`GateCmd` enums directly (as this research did) before writing any README task.
**Warning signs:** A task titled "remove phantom `init`/`config`/`ship` commands from
README" — if grep for those strings in README.md returns nothing, the task is stale.

### Pitfall 2: ARCHITECTURE.md rewrite scope creep into re-explaining CONTRIBUTING.md content
**What goes wrong:** ARCHITECTURE.md and CONTRIBUTING.md both currently have an
"Extension points / adding an agent" checklist (duplicated, and CONTRIBUTING.md correctly
defers to ARCHITECTURE.md as authoritative). A careless rewrite could re-diverge these two
copies.
**Why it happens:** Both docs independently maintain the same 7-step checklist.
**How to avoid:** Keep CONTRIBUTING.md's copy as the short pointer it already is
("See ARCHITECTURE.md#extension-points--adding-an-agent for the authoritative version")
and only update ARCHITECTURE.md's copy.
**Warning signs:** A diff touching CONTRIBUTING.md's "Adding a New Agent" section beyond
the anchor link.

### Pitfall 3: `cargo publish` failing on an unpublished path dependency
**What goes wrong:** `devflow` (the CLI crate) depends on `devflow-core = { path = "../devflow-core", version = "1.2.0" }`. Publishing the CLI crate before `devflow-core` is live on
crates.io fails, because the registry can't resolve a `1.2.0` version of a crate that
doesn't exist yet remotely.
**Why it happens:** Cargo requires a registry-resolvable version for path deps when
publishing, and crates.io propagation can also lag by a few minutes after `devflow-core`'s
own publish.
**How to avoid:** Publish `devflow-core` first, wait for it to appear on the registry
(`cargo search` is disabled on crates.io's public API, so poll `https://crates.io/crates/devflow-core` or `cargo add devflow-core --dry-run` from a scratch dir), then publish
`devflow`. This exact ordering constraint was already discovered and worked around in
12-06 via `cargo package --workspace`'s temporary local registry (verification only,
not real publish).
**Warning signs:** `cargo publish -p devflow` erroring with "no matching package named
`devflow-core` found" immediately after publishing core.

### Pitfall 4: Declared dual license without both license files
**What goes wrong:** `Cargo.toml`'s `license = "MIT OR Apache-2.0"` is a legal claim that
either license text is available at the user's choice. Only `LICENSE` (MIT) exists at the
repo root — no `LICENSE-APACHE`.
**Why it happens:** Common workspace-metadata copy/paste from a template that assumed dual
licensing without following through on both license files.
**How to avoid:** Either add a real `LICENSE-APACHE` file (Apache 2.0 text, matching the
copyright holder) or narrow `Cargo.toml`'s `license` field to `"MIT"` to match what's
actually present, before `cargo publish`.
**Warning signs:** crates.io's generated page shows "MIT OR Apache-2.0" but the repo has no
Apache license text to click through to.

### Pitfall 5: Devcontainer cargo cache pointing at the wrong path for a workspace
**What goes wrong:** A naive devcontainer cache mount targets `~/.cargo/registry` only,
missing `target/` — for a 2-crate workspace this still leaves every `cargo build` inside
the container recompiling both crates from scratch on every container rebuild.
**Why it happens:** Cargo caches registry downloads separately from build artifacts; only
mounting the registry cache misses the bigger win.
**How to avoid:** Mount both `/usr/local/cargo/registry` (or `~/.cargo/registry` depending
on base image user) and the workspace's `target/` directory as named volumes, or accept
slower rebuilds if simplicity is preferred for a small workspace — a defensible tradeoff to
surface explicitly in the devcontainer task rather than silently under-caching.
**Warning signs:** Every devcontainer rebuild takes as long as a fresh `cargo build` from
an empty target dir.

## Code Examples

### `.devcontainer/devcontainer.json` starting point (adapt, verify against current `rust-toolchain.toml`)
```jsonc
// Source: pattern from github.com/microsoft/vscode-remote-try-rust (official Microsoft sample),
// adapted with a cargo/target cache mount per
// github.com/devcontainers/templates/issues/117
{
  "name": "DevFlow",
  "image": "mcr.microsoft.com/devcontainers/rust:1-1-bookworm",
  "mounts": [
    {
      "source": "devflow-cargo-registry",
      "target": "/usr/local/cargo/registry",
      "type": "volume"
    },
    {
      "source": "devflow-target",
      "target": "${containerWorkspaceFolder}/target",
      "type": "volume"
    }
  ],
  "postCreateCommand": "rustup component add clippy rustfmt && cargo build --workspace",
  "customizations": {
    "vscode": {
      "extensions": [
        "rust-lang.rust-analyzer",
        "vadimcn.vscode-lldb",
        "tamasfe.even-better-toml"
      ]
    }
  }
}
```
**Note:** verify the base image tag (`1-1-bookworm` here is illustrative) against the
image's current published tags and against `rust-toolchain.toml`'s pinned `channel =
"stable"` at plan time — image tags drift independently of this research.

### CI badge markdown (already correct in README — pattern reference only)
```markdown
<!-- Source: docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/adding-a-workflow-status-badge -->
[![CI](https://github.com/OWNER/REPO/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/ci.yml)
```

### Publish ordering (verified this session against the current tree)
```bash
# Source: doc.rust-lang.org/cargo/reference/publishing.html, verified locally 2026-07-17
cargo publish --dry-run -p devflow-core   # already passes clean
# once devflow-core is live on the registry:
cargo publish -p devflow-core
# wait for propagation, then:
cargo publish -p devflow
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Single `.devflow/state.json` | Per-phase `.devflow/state-NN.json` | Phase 14a (2026-07-16) | Every doc mentioning "state.json" (README's Configuration section, DEPENDENCIES.md's doctor sample, SECURITY.md) is now technically imprecise |
| `.devflow.yaml` config file, `devflow init` | No config file; all options are `devflow start` CLI flags | v2.0.0 architecture decision (2026-06-19), decoy file physically removed in 15a | docs/guides/configuration.md and quickstart.md still describe the removed model in full |
| Gate answered by hand-editing `.devflow/gates/NN-stage.response.json` | `devflow gate list/approve/reject` | 15a (2026-07-16) | README/OPERATIONS.md need the command documented; OPERATIONS.md already has it, README does not yet |
| Manual/no dev environment reproduction | (target state, not yet built) `.devcontainer/` | This phase (15b) | No prior art in this repo to correct against — pure greenfield |

**Deprecated/outdated:**
- `devflow confirm` / `devflow rejectpr`: named in ARCHITECTURE.md's "Git and ship model"
  section and DEPENDENCIES.md's "Required for Shipping" section, but neither exists as a
  `Command` variant in current `main.rs` — Ship is now driven entirely through the gate
  protocol (`devflow gate approve <phase> --stage ship`).
- `.devflow/audit.log`: referenced in SECURITY.md, does not exist in the codebase
  (`rg -n "audit.log"` finds only the doc reference, zero source hits).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | The `mcr.microsoft.com/devcontainers/rust:1-1-bookworm` tag is current and available | Code Examples | If the tag is stale/removed, the devcontainer fails to build; plan should verify the current tag at build time rather than hard-coding this research's illustrative tag |
| A2 | "PR gate status badge" (CONTEXT.md wording) refers to the existing CI badge / branch-protection required checks, not a separate distinct badge type | Open Questions | If the operator meant something more specific (e.g. a Codecov-style external gate service), the plan under-delivers on this bullet |
| A3 | `docs/guides/quickstart.md` and `docs/guides/configuration.md` are in-scope collateral damage of the "accuracy" goal even though CONTEXT.md's checklist doesn't name them | Open Questions | If the operator intends to delete these guides outright (rather than fix them) instead of updating in place, the plan should confirm before writing new content into stale files |
| A4 | crates.io still has `devflow` and `devflow-core` names available for this account, unchanged since the 2026-07-08 STATE.md decision ("name confirmed available") | Standard Stack / Common Pitfalls | If a name was claimed by someone else in the interim, the actual `cargo publish` (not just `--dry-run`) will fail with a name-conflict error — the dry-run does not fully validate registry-side name ownership |

**If this table is empty:** N/A — assumptions listed above need brief confirmation, none are load-bearing for the core doc-accuracy work (which is independently verified against source).

## Open Questions (RESOLVED — see .planning/phases/15-oss-readiness/15-0{1,2,3}-PLAN.md)

All three questions below were resolved during planning (2026-07-17) following each
question's own recommendation; each resolution is recorded as a flagged assumption in
the corresponding PLAN.md so the decision is visible to the executor, not just here.

1. **(RESOLVED — folded in, per recommendation.)** **Are `DEPENDENCIES.md`,
   `docs/guides/quickstart.md`, `docs/guides/configuration.md`, and
   `skills/hermes/devflow/SKILL.md` in scope for this phase's accuracy pass?**
   - What we know: all four contain stale-but-plausible content (`devflow init`,
     `.devflow.yaml`, old `doctor` output, `devflow confirm`) at the same severity level
     CONTEXT.md describes for ARCHITECTURE.md, but CONTEXT.md's 15b checklist names only
     README, ARCHITECTURE.md, CONTRIBUTING.md, devcontainer, badges, CODE_OF_CONDUCT/
     SECURITY, and crates.io publish.
   - What's unclear: whether these were deliberately left out (e.g. `skills/hermes/*` is
     arguably Phase 16 territory since Hermes work was explicitly moved there) or simply
     not surfaced when CONTEXT.md was written.
   - Recommendation: `skills/hermes/devflow/SKILL.md` — leave for Phase 16 (Hermes Support
     owns that surface per STATE.md's phase-split decision). `DEPENDENCIES.md` and the two
     `docs/guides/*.md` files — recommend folding a lightweight accuracy pass into this
     phase (they are directly linked from README's "Documentation" list and from
     OPERATIONS.md), but confirm with the operator before the planner commits scope, since
     CONTEXT.md didn't name them explicitly.
   - **Resolution:** folded in — `DEPENDENCIES.md` and the two `docs/guides/*.md` files got
     a lightweight accuracy pass (15-01-T3, 15-02-T2, 15-02-T3); `skills/hermes/devflow/SKILL.md`
     was left for Phase 16, as recommended. Recorded as a flagged assumption in each plan.

2. **(RESOLVED — option (b), per recommendation.)** **What does "PR gate status" badge mean concretely?**
   - What we know: GitHub has no separate native badge type for "PR merge gate /
     required-checks status" distinct from the existing workflow-run badge already in
     README. Branch protection required-status-checks are a repo *setting*, not a
     renderable badge.
   - What's unclear: whether the operator wants (a) nothing more than the existing CI
     badge (already done), (b) a CONTRIBUTING.md note describing which checks are required
     before merge, or (c) a third-party badge (e.g. a Codecov coverage badge, if code
     coverage is tracked — it currently is not in `.github/workflows/ci.yml`).
   - Recommendation: default to (b) — document required checks (test/clippy/fmt, all three
     jobs in `ci.yml`) in CONTRIBUTING.md's PR Process section — since (a) is already
     satisfied and (c) would require adding new CI tooling out of scope for a docs phase.
   - **Resolution:** (b) — CONTRIBUTING.md's PR Process section now documents the required
     checks (test/clippy/fmt) (15-03-T1).

3. **(RESOLVED — skip, per recommendation.)** **Is the `devflow devcontainer` subcommand worth building?**
   - What we know: CONTEXT.md explicitly frames this as conditional — "only if it earns
     its keep." No existing precedent subcommand in `main.rs` does anything comparable
     (closest is `devflow doctor`, which audits rather than launches tooling).
   - What's unclear: what "earns its keep" means operationally — e.g. would it just shell
     out to `devcontainer up`/`devcontainer exec` (requiring the separate `@devcontainers/cli`
     npm package as a new external dependency), or would it be a thin wrapper that mainly
     prints instructions?
   - Recommendation: skip it. A `.devcontainer/devcontainer.json` + documented `devcontainer
     up`/VS Code "Reopen in Container" flow satisfies the phase goal without adding a new
     CLI surface (and a new npm-package dependency) to maintain and to keep the `--help`
     snapshot test in sync with.
   - **Resolution:** skipped — 15-03 ships `.devcontainer/devcontainer.json` only, no new
     `devflow devcontainer` subcommand.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `cargo` | Publish, package verification | Yes | 1.97.1 [VERIFIED: local] | — |
| `rust-toolchain.toml` stable channel + clippy/rustfmt | CI parity, devcontainer postCreate | Yes | stable, components clippy+rustfmt already pinned [VERIFIED: `rust-toolchain.toml`] | — |
| Docker / devcontainer CLI | Building/testing `.devcontainer/` locally before commit | Not probed this session (research phase; verify at execution time) | — | If Docker isn't available in the execution environment, validate `devcontainer.json` schema manually and rely on CI (a GitHub Actions job that runs `devcontainer build`) as the real test surface instead of a local Docker run |
| crates.io registry credentials (`cargo login` / `CARGO_REGISTRY_TOKEN`) | Actual `cargo publish` (not dry-run) | Not probed (credential-gated; out of scope for a research agent to check) | — | Blocking dependency with no fallback — the actual publish step needs a human to hold a crates.io API token; the plan should gate the publish task behind a `checkpoint:human-verify`/manual step, not attempt to script credential acquisition |

**Missing dependencies with no fallback:**
- crates.io publish credentials — must be supplied by the operator at execution time; no
  automatable fallback exists (this is intentional/expected for a first publish).

**Missing dependencies with fallback:**
- Docker/devcontainer CLI (local validation) — CI-based devcontainer build check is a
  workable fallback if local Docker isn't available in the execution sandbox.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (workspace), plus a Rust integration-test-style snapshot guard |
| Config file | none — no `pytest.ini`/`jest.config`; test discovery is Cargo's standard `tests/` convention |
| Quick run command | `cargo test -p devflow-cli --test help_snapshot` |
| Full suite command | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|---------------------|--------------|
| 15b (README/ARCHITECTURE accuracy) | Docs match real CLI surface | manual-only + existing guard | `cargo test -p devflow-cli --test help_snapshot` (guards CLI-vs-docs drift indirectly; does not itself parse README/ARCHITECTURE prose) | ✅ (`crates/devflow-cli/tests/help_snapshot.rs`, 15a) |
| 15b (crates.io publish) | Both crates package/publish cleanly, in dependency order | integration | `cargo publish --dry-run -p devflow-core && cargo package --workspace` | ✅ (already used in 12-06, re-verified this session) |
| 15b (devcontainer) | Container builds and runs `cargo build && cargo test && cargo clippy` cleanly | integration/manual | new: `devcontainer build --workspace-folder .` (or a CI job invoking it) | ❌ Wave 0 — no devcontainer test exists yet |
| 15b (CI badge / PR gate) | Badge renders and links to the real workflow | manual | visual check of rendered README on GitHub | n/a (not automatable meaningfully) |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-cli --test help_snapshot` (fast, catches CLI/doc drift immediately) plus a manual read-through of the specific doc file touched against its named source file(s).
- **Per wave merge:** `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Phase gate:** Full suite green, `cargo publish --dry-run -p devflow-core` clean, devcontainer build test green (if built) before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] A devcontainer build/test check — either a local `devcontainer build --workspace-folder .` step documented in CONTRIBUTING.md, or (recommended, since it's automatable and matches "CI-parity") a new `.github/workflows/devcontainer.yml` job running `cargo build && cargo test && cargo clippy` inside the built container image. Neither currently exists.
- [ ] No test framework gap for the doc-accuracy work itself — the existing `--help` snapshot test is sufficient as the CLI-surface guard; doc-prose accuracy is inherently a human/agent read-and-compare activity, not something to force into an automated test.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|--------------------|
| V2 Authentication | No | This phase touches no auth surface |
| V3 Session Management | No | N/A |
| V4 Access Control | No | N/A |
| V5 Input Validation | No | No new user input handling introduced |
| V6 Cryptography | No | N/A |
| V14 Configuration (supply chain) | Yes | Devcontainer base image provenance (pin to a specific, official `mcr.microsoft.com/devcontainers/rust` tag, not `:latest`); crates.io publish credential handling (never commit a `CARGO_REGISTRY_TOKEN`; use `cargo login` interactively or a CI secret, never a plaintext file in the repo) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| Leaking a crates.io API token into a committed file, CI log, or shell history during first-publish setup | Information Disclosure | Use `cargo login` (writes to `~/.cargo/credentials.toml`, outside the repo) or a GitHub Actions encrypted secret injected as `CARGO_REGISTRY_TOKEN`; never echo the token, never commit `.cargo/credentials.toml` |
| Devcontainer base image supply-chain drift (`:latest` tag silently changing under you) | Tampering | Pin `mcr.microsoft.com/devcontainers/rust` to an explicit version tag (e.g. `1-1-bookworm`), not `:latest`; revisit the pin deliberately, not automatically |
| Stale SECURITY.md pointing responders at a non-existent file (`.devflow/audit.log`) for evidence-gathering during an actual incident | Information Disclosure (indirect — misdirected incident response) | Fix SECURITY.md's "Best Practices" section to reference real state files (`state-NN.json`, `events.jsonl`) as part of this phase's accuracy pass |

## Sources

### Primary (HIGH confidence)
- `crates/devflow-cli/src/main.rs` (this repo, read directly) — real `Command`/`GateCmd` surface, `--mode auto|supervise`
- `crates/devflow-core/src/{stage,gates,hooks,lock,events,workflow}.rs` (this repo, read directly) — Stage enum, gate protocol, hooks model, per-phase locking, events schema, state file naming
- `Cargo.toml`, `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml` (this repo, read directly) — publish metadata already set
- `crates/devflow-cli/tests/help_snapshot.rs` (this repo, read directly) — existing docs-drift guard
- Local `cargo publish --dry-run -p devflow-core` and `cargo package --workspace` runs (executed this session against the current tree) — verified still-passing

### Secondary (MEDIUM confidence)
- [Publishing on crates.io — The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html) — required/recommended metadata fields, dry-run workflow
- [Adding a workflow status badge — GitHub Docs](https://docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/adding-a-workflow-status-badge) — badge URL/markdown convention
- [microsoft/vscode-remote-try-rust `.devcontainer/devcontainer.json`](https://github.com/microsoft/vscode-remote-try-rust) — official Rust devcontainer template pattern
- [devcontainers/templates issue #117 — Rust cargo cache persistence](https://github.com/devcontainers/templates/issues/117) — cache-volume mount pattern for Cargo workspaces

### Tertiary (LOW confidence)
- General WebSearch summaries of crates.io publishing guides (dev.to, calmops.com) — used only to corroborate the Cargo Book's own guidance, not as a standalone source

## Metadata

**Confidence breakdown:**
- Standard stack / tooling: HIGH — no new dependencies; all publish/CI facts verified directly against the local toolchain and repo
- Architecture (doc-rewrite content): HIGH — every claim traced to a specific source file this session
- Devcontainer specifics (exact base image tag, exact cache mount syntax): MEDIUM — pattern confirmed against official Microsoft sample and a documented community issue, but the specific image tag should be re-verified at plan/execution time since tags roll forward
- Pitfalls: HIGH — LICENSE mismatch, audit.log phantom reference, and README's true current state were all independently confirmed by direct inspection this session, not inferred from CONTEXT.md's description

**Research date:** 2026-07-17
**Valid until:** 2026-08-01 (30 days — the source-code facts are stable barring further CLI changes; the devcontainer base image tag and crates.io dry-run result should be re-checked if execution slips past this window)

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|---------------------|
| 15b | OSS packaging: README/ARCHITECTURE rewrite against v2 reality, CONTRIBUTING, dev container, CI/PR-gate badges, CODE_OF_CONDUCT/SECURITY refresh, crates.io publish — run as a DevFlow dogfood | Architecture Patterns (verified current CLI/architecture facts), Code Examples (devcontainer + publish-order patterns), Common Pitfalls (LICENSE mismatch, publish ordering, stale phantom references), Open Questions (scope boundary on DEPENDENCIES.md/docs-guides, "PR gate status" interpretation, `devflow devcontainer` subcommand go/no-go) |
</phase_requirements>
