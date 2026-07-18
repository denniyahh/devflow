---
phase: 15-oss-readiness
plan: 03
subsystem: infra
tags: [devcontainer, ci, contributing, docs, rust, oss]

# Dependency graph
requires:
  - phase: 15-oss-readiness (plan 01)
    provides: SECURITY.md/DEPENDENCIES.md refresh, no README/CONTRIBUTING overlap
  - phase: 15-oss-readiness (plan 02)
    provides: ARCHITECTURE.md rewrite + docs/guides accuracy pass
provides:
  - CONTRIBUTING.md required-checks note (mirrors ci.yml's three jobs) and a Dev Container subsection
  - .devcontainer/devcontainer.json — pinned Rust dev environment with cargo/target cache
  - .github/workflows/devcontainer.yml — CI-parity job that builds the container and runs cargo build/test/clippy inside it
affects: [15-oss-readiness (later plans: crates.io publish), any future contributor-onboarding work]

# Tech tracking
tech-stack:
  added: [devcontainers/rust base image (mcr.microsoft.com/devcontainers/rust), devcontainers/ci GitHub Action]
  patterns: [pin external supply-chain references (base image tag, GitHub Action ref) to explicit non-floating versions, verified live against the registry rather than trusted from research]

key-files:
  created:
    - .devcontainer/devcontainer.json
    - .github/workflows/devcontainer.yml
  modified:
    - CONTRIBUTING.md

key-decisions:
  - "Verified the devcontainer base image tag live against mcr.microsoft.com's registry API (401 tags) rather than trusting the plan's illustrative 1-1-bookworm — chose 2.0.13-1-bookworm, the current highest-patch tag in the major-2/bookworm line"
  - "Pinned devcontainers/ci to @v0.3 (an explicit major-version tag confirmed to exist via the GitHub API), matching the plan's 'explicit major version, not a floating ref' requirement"
  - "CODE_OF_CONDUCT.md spot-checked and left unmodified — contact wording ('the project maintainer') is generic and consistent with SECURITY.md's security@dennis.dev, not stale"

requirements-completed: [15b]

coverage:
  - id: D1
    description: "CONTRIBUTING.md documents the three required CI checks (cargo test, cargo clippy -D warnings, cargo fmt --check) in the PR Process section"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg -n 'cargo clippy' CONTRIBUTING.md && rg -n 'cargo fmt' CONTRIBUTING.md && rg -n 'cargo test' CONTRIBUTING.md (all hit in PR Process area)"
        status: pass
    human_judgment: false
  - id: D2
    description: "CONTRIBUTING.md documents the .devcontainer flow alongside the existing Distrobox subsection, without touching the Adding-a-New-Agent anchor pointer"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg -n 'devcontainer' CONTRIBUTING.md (hits); rg -n 'extension-points' CONTRIBUTING.md (single anchor pointer, unchanged)"
        status: pass
    human_judgment: false
  - id: D3
    description: ".devcontainer/devcontainer.json exists, is valid JSON, pins an explicit non-:latest official Rust image, mounts both cargo registry and target/ caches, and installs clippy+rustfmt on create"
    requirement: "15b"
    verification:
      - kind: other
        ref: "node -e JSON.parse(...) exits 0; rg -c ':latest' returns 0; rg 'mcr.microsoft.com/devcontainers/rust' hits; rg 'target'/'registry' both hit; rg 'rustup component add clippy rustfmt' hits"
        status: pass
    human_judgment: false
  - id: D4
    description: "A devcontainer CI job (.github/workflows/devcontainer.yml) builds the container and runs cargo build/test/clippy inside it"
    requirement: "15b"
    verification:
      - kind: other
        ref: "test -f .github/workflows/devcontainer.yml; rg 'cargo build'/'cargo test'/'cargo clippy'/'devcontainer'/'actions/checkout@v4' all hit; yq eval confirms valid YAML"
        status: pass
    human_judgment: true
    rationale: "The workflow's correctness under real GitHub Actions execution (devcontainers/ci actually building the pinned image and running the commands) cannot be proven locally — no Docker/devcontainer CLI available in this sandbox. Static structure (grep + YAML parse) verified; live CI run is the remaining human-observable proof point."
  - id: D5
    description: "CODE_OF_CONDUCT.md present and spot-checked as current"
    requirement: "15b"
    verification:
      - kind: other
        ref: "test -f CODE_OF_CONDUCT.md"
        status: pass
    human_judgment: false

# Metrics
duration: 10min
completed: 2026-07-17
status: complete
---

# Phase 15 Plan 03: CONTRIBUTING refresh + .devcontainer reproducible environment Summary

**Added a pinned `.devcontainer/devcontainer.json` (mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm, cargo+target cache volumes) with a real CI-parity job (`devcontainer.yml`) that builds it and runs `cargo build/test/clippy` inside, plus a CONTRIBUTING.md update listing the three required PR checks and the new devcontainer flow.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-07-17T10:20:00-04:00 (approx, reading/verification included)
- **Completed:** 2026-07-17T10:26:20-04:00
- **Tasks:** 3
- **Files modified:** 3 (1 modified, 2 created)

## Accomplishments
- CONTRIBUTING.md now states the three CI-gating checks (`cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`) verbatim against `ci.yml`, and documents a `.devcontainer` subsection parallel to the existing Distrobox one
- New `.devcontainer/devcontainer.json`: pins an explicit, registry-verified Rust base image tag (not `:latest`), mounts both the cargo registry cache and workspace `target/` as named volumes (avoids full-workspace rebuilds), and runs `rustup component add clippy rustfmt && cargo build --workspace` on create to match `rust-toolchain.toml` explicitly
- New `.github/workflows/devcontainer.yml`: builds the devcontainer via `devcontainers/ci@v0.3` (pinned to an explicit major-version tag, confirmed to exist via the GitHub API) and runs `cargo build/test/clippy` inside it — real CI-parity proof, not just JSON linting

## Task Commits

Each task was committed atomically:

1. **Task 1: CONTRIBUTING.md — required-checks note + devcontainer subsection + CODE_OF_CONDUCT spot-check** - `712ea86` (docs)
2. **Task 2: .devcontainer/devcontainer.json — pinned Rust dev environment with cargo+target cache** - `0aab4bd` (feat)
3. **Task 3: .github/workflows/devcontainer.yml — container-parity CI test** - `a6b3fba` (ci)

**Plan metadata:** (pending — this SUMMARY commit)

## Files Created/Modified
- `CONTRIBUTING.md` - Added required-checks note to PR Process (three CI jobs) and a Dev Container subsection next to Distrobox
- `.devcontainer/devcontainer.json` - New: pinned Rust devcontainer, cargo/target cache mounts, postCreate toolchain install, VS Code extensions
- `.github/workflows/devcontainer.yml` - New: CI job building and testing inside the devcontainer

## Decisions Made
- Verified the devcontainer base image tag live against `mcr.microsoft.com/v2/devcontainers/rust/tags/list` (network access confirmed available in this environment) rather than trusting the plan's illustrative `1-1-bookworm`. The `1-` major line is stale; `2.0.13-1-bookworm` is the current highest-patch tag in the actively-published bookworm line — used that instead.
- Pinned `devcontainers/ci` to `@v0.3`, confirmed via `GET /repos/devcontainers/ci/git/refs/tags/v0.3` to be a real, explicit major-version tag (not a floating branch ref), satisfying both the plan's instruction and threat T-15-03b.
- CODE_OF_CONDUCT.md spot-checked per Task 1's instruction; left unmodified — its "reported to the project maintainer" contact language is generic and doesn't conflict with `SECURITY.md`'s `security@dennis.dev`, so no edit was warranted.
- Removed the literal substring `:latest` from a devcontainer.json comment (originally written as "do not bump to :latest") because it accidentally tripped the plan's own acceptance-criteria grep (`rg -c ':latest'` expected `0`) despite being a negation, not a pin. Reworded to "never use an unversioned floating tag" — same meaning, doesn't collide with the literal-string check.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Devcontainer comment collided with its own `:latest` acceptance check**
- **Found during:** Task 2 (writing `.devcontainer/devcontainer.json`)
- **Issue:** A doc comment explaining the pin ("do not bump to :latest") contained the literal substring `:latest`, which is exactly what the plan's acceptance check (`rg -c ':latest' .devcontainer/devcontainer.json` expected to return `0`) greps for — the comment would have failed the plan's own automated gate despite correctly pinning the image.
- **Fix:** Reworded the comment to avoid the literal string while preserving intent ("never use an unversioned floating tag").
- **Files modified:** `.devcontainer/devcontainer.json`
- **Verification:** `rg -c ':latest' .devcontainer/devcontainer.json` returns `0`; image line still pins `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`.
- **Committed in:** `0aab4bd` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Cosmetic-only fix to satisfy the plan's own literal grep gate; no functional change. No scope creep.

## Issues Encountered
- The research/patterns docs' illustrative base image tag (`mcr.microsoft.com/devcontainers/rust:1-1-bookworm`) is on a stale major-version line (`1-*`) relative to the currently-published `2.0.13-1-bookworm` (verified live against the registry's tag list). Per the plan's explicit A1 flagged assumption ("the executor MUST verify the chosen tag resolves against the registry at build time and pick a current published tag"), the newer tag was used instead of the research's illustrative one.
- No Docker / `devcontainer` CLI available in this execution sandbox to locally build-test the container — per the plan's flagged Environment Availability note, the `.github/workflows/devcontainer.yml` CI job (running on GitHub's runners) is the real acceptance gate for the container actually building and passing `cargo build/test/clippy`; this was not exercised end-to-end in this session (see D4's `human_judgment: true` in coverage).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CONTRIBUTING.md, `.devcontainer/`, and the devcontainer CI job are all in place; no blockers for the remaining 15b plans (crates.io publish work).
- Recommend a first real CI run on this branch's PR to confirm `devcontainer.yml` actually builds `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` and passes cargo build/test/clippy inside it — this is the one verification step that could only be proven live on GitHub's runners, not in this sandbox.

---
*Phase: 15-oss-readiness*
*Completed: 2026-07-17*

## Self-Check: PASSED

All claimed files exist (`CONTRIBUTING.md`, `.devcontainer/devcontainer.json`,
`.github/workflows/devcontainer.yml`, `CODE_OF_CONDUCT.md`) and all three task commits
(`712ea86`, `0aab4bd`, `a6b3fba`) are present in git history.
