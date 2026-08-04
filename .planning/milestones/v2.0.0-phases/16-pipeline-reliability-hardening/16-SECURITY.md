---
phase: 16
slug: pipeline-reliability-hardening
status: verified
threats_open: 0
asvs_level: 1
created: 2026-07-18
---

# Phase 16 - Security

> Consolidated verification of the threat registers authored in Plans 16-01 through 16-07.

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Agent and operator artifacts -> completion evaluation | Agent output is untrusted; PLAN frontmatter and project configuration are operator-authored | Completion markers, verification commands, review instructions |
| DevFlow -> Git checkout and release refs | Merge, version, and tag hooks mutate shared repository state | Branch refs, commits, tags, changelog entries |
| `.devflow/` runtime state -> Git and operator views | Captures and events can contain session data and untrusted text | Logs, gate context, retained history |
| CLI paths and arguments -> filesystem/workflow actions | Incorrect binding or root selection can target the wrong workflow | Project roots, phases, stages, gate responses |
| Documentation -> deterministic checkers | Documentation is input for comparison only and must never become executable input | Commands, flags, paths, environment names |

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation / Evidence | Status |
|-----------|----------|-----------|----------|-------------|-----------------------|--------|
| T-16-01 | Spoofing | Terminal merge hook | high | mitigate | Merge effect is required and missing branches fail closed; `merge_fails_closed_when_branch_absent` | closed |
| T-16-02 | Tampering | Version/tag ordering | high | mitigate | Merge precedes versioning; `terminal_hooks_version_post_merge_develop` verifies the post-merge tag | closed |
| T-16-03 | Tampering / DoS | Repeated merge | medium | mitigate | Merged-branch guard makes retries idempotent | closed |
| T-16-04 | Repudiation | Release history | low | mitigate | Corrupt Phase 16 release entries were removed and tag history was rechecked | closed |
| T-16-05 | Elevation / injection | Git invocation | low | mitigate | Git operations use structured `Command` arguments and phase-derived branch names | closed |
| T-16-06 | DoS | Malformed `devflow.toml` | medium | mitigate | Config parsing warns and falls back to defaults; covered by malformed-config tests | closed |
| T-16-07 | Tampering | Operator configuration | low | accept | Git-flow constants remain outside configurable `DevflowConfig` | closed |
| T-16-08 | Information disclosure | Config path | low | mitigate | Only `<project_root>/devflow.toml` is read | closed |
| T-16-09 | Spoofing | Agent success marker | high | mitigate | Declared Layer 0 probes run before agent self-report and commit heuristics | closed |
| T-16-10 | Tampering / injection | Verification command source | high | mitigate | `external_verify_commands` reads PLAN frontmatter only; stdout injection tests pass | closed |
| T-16-11 | Information disclosure | Capture history | high | mitigate | History paths are covered by the constructor-derived Git ignore invariant | closed |
| T-16-12 | DoS | Capture retention | medium | mitigate | `archive_phase_files` prunes to configured retention and rolls back partial archives on error | closed |
| T-16-13 | Tampering | Operator verification shell | low | accept | `sh -c` is limited to operator-authored PLAN commands and is required for pipelines | closed |
| T-16-14 | Spoofing | Ship review coverage | high | mitigate | Five-angle review prompt and configurable overrides are snapshot-tested | closed |
| T-16-15 | Repudiation | Review harness drift | medium | mitigate | Prompt specifies parallel-capable and sequential fallback execution | closed |
| T-16-16 | DoS | Incremental review | medium | mitigate | Code-stage review is explicitly advisory and non-interactive | closed |
| T-16-17 | Tampering | Review-angle configuration | low | accept | Values are operator-authored and share CLI-flag trust | closed |
| T-16-18 | Information disclosure | Runtime paths | high | mitigate | `gitignore_covers_all_devflow_paths` derives and verifies every runtime path | closed |
| T-16-19 | Tampering / injection | Documentation checker | medium | mitigate | Checker parses and compares strings only; no scanned content is executed | closed |
| T-16-20 | Spoofing | Documentation claims | medium | mitigate | Bidirectional source/document checks and pinned claims fail on drift | closed |
| T-16-21 | Repudiation | Checker exceptions | low | mitigate | Every allowlist entry requires a non-empty reason | closed |
| T-16-22 | Spoofing | Project-root resolution | medium | mitigate | Resolver returns the nearest `.devflow` ancestor; walk-up behavior is tested | closed |
| T-16-23 | Repudiation | Gate CLI binding | medium | mitigate | Stage positional and explicit `--project` parsing have regression tests | closed |
| T-16-24 | Availability | Filesystem walk-up | low | mitigate | Parent traversal terminates at filesystem root | closed |
| T-16-25 | Repudiation | Legacy-state warning | low | mitigate | Warning names the actionable `recover --clean` command | closed |
| T-16-26 | Spoofing | Gate notification | high | mitigate | Persistent status banner is independent of notification-command success | closed |
| T-16-27 | Information disclosure / DoS | Gate rendering | medium | mitigate | Gate context is sanitized and truncated before terminal rendering | closed |
| T-16-28 | Information disclosure | History rendering | medium | mitigate | History references local, ignored captures and does not export their contents | closed |
| T-16-29 | Repudiation | Attempt history | low | mitigate | Timeline correlates ordered events, capture generations, and review artifacts | closed |
| T-16-SC | Supply-chain tampering | `toml` dependency | high | mitigate | Human approved the official `toml-rs/toml` crate and the `toml = "1"` constraint before addition; no other package installs occurred | closed |

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-16-01 | T-16-07 | Project configuration is operator-authored; safety-critical Git-flow constants remain hardcoded | Phase 16 plan contract | 2026-07-18 |
| AR-16-02 | T-16-13 | Shell composition is required for operator-authored external probes and never accepts agent stdout | Phase 16 plan contract | 2026-07-18 |
| AR-16-03 | T-16-17 | Review angles are trusted operator instructions equivalent to CLI input | Phase 16 plan contract | 2026-07-18 |

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-18 | 30 | 30 | 0 | Codex inline ASVS L1 audit |

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-18
