---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P1 recommendation — split re-scoped by Claude same day
---

# Backlog: Property and Fuzz Testing for Protocol Parsers

## Goal

DevFlow parses agent markers, JSON event streams, rate-limit responses, YAML
frontmatter, shell commands, and git output. Example-based tests are
extensive, but there is no fuzzing or property testing for malformed,
truncated, nested, or adversarial input.

## Re-scoped split (2026-07-21, Claude review)

The original recommendation listed six targets (agent result markers, JSON
envelopes, rate-limit detection, frontmatter extraction, event logs, shell
quoting) as needing both `proptest` and `cargo-fuzz` undifferentiated. Reviewed
each by what actually stresses it:

**`proptest` (format-aware business logic, not byte-level adversarial):**
- Agent result markers / JSON envelopes — structured, produced by controlled
  agent CLIs; the interesting invariants are semantic (missing fields,
  unexpected enum values), which proptest's generators handle naturally.
- Frontmatter structural parsing, event logs, git porcelain output — well-
  structured, line-based formats where varied-but-valid-shaped input is more
  valuable than raw byte fuzzing.

**`cargo-fuzz` (genuine byte-level adversarial target):**
- `shell_quote` — the one clear fuzz candidate on this list. Quoting bugs are
  classically about specific adversarial byte sequences (control characters,
  embedded quotes, unicode edge cases), and a bug here has real
  command-injection implications if fed adversarial phase-name/path content.

Fuzzing the full original list is more investment than the risk profile
justifies — most of it is proptest scope.

## Invariants parsers must hold

Must never panic. Must fail closed on ambiguous approval data (see the
`verify.rs` fix from this session's QA review — exactly this class of bug).
Must preserve documented precedence rules (e.g. Claude's `is_error` overriding
a stale success marker).

## Notes

Promote with `/gsd-review-backlog` when ready.
