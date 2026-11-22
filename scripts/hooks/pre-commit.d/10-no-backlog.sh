#!/usr/bin/env bash
# Workspace-only pre-commit drop-in: issues are tracked in GitHub Issues only
# (denniyahh/devflow, operator decision 2026-11-22). The ROADMAP.md 999.x
# backlog was migrated and removed; this refuses any commit that brings it back.
set -euo pipefail

roadmap=".planning/ROADMAP.md"
added_entries="$(git diff --cached -U0 -- "$roadmap" | grep -E '^\+(### Phase 999\.|## Backlog)' || true)"
added_dirs="$(git diff --cached --name-only --diff-filter=A -- '.planning/phases/' | grep -E '^\.planning/phases/999\.' || true)"

if [ -n "$added_entries" ] || [ -n "$added_dirs" ]; then
    echo "pre-commit: no-backlog: this commit adds a 999.x backlog entry; file a GitHub issue instead:" >&2
    echo "  gh issue create --repo denniyahh/devflow --title '<title>' --label bug --body-file <file>" >&2
    [ -n "$added_entries" ] && printf '  %s: %s\n' "$roadmap" "$added_entries" >&2
    [ -n "$added_dirs" ] && printf '  %s\n' "$added_dirs" >&2
    exit 1
fi
