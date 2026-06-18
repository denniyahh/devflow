#!/usr/bin/env bash
# Test script for devflow Hermes skill
# Validates: SKILL.md frontmatter, devflow binary, documented commands
set -euo pipefail

SKILL_DIR="$(cd "$(dirname "$0")" && pwd)"
SKILL_FILE="$SKILL_DIR/SKILL.md"
PASS=0
FAIL=0

green() { echo -e "\033[32m✓ $*\033[0m"; PASS=$((PASS + 1)); }
red()   { echo -e "\033[31m✗ $*\033[0m"; FAIL=$((FAIL + 1)); }

echo "=== DevFlow Skill Validation ==="
echo ""

# --- 1. SKILL.md exists and is valid ---
echo "--- SKILL.md ---"
if [ -f "$SKILL_FILE" ]; then
    green "SKILL.md exists"
else
    red "SKILL.md missing"
fi

# Check frontmatter
if head -1 "$SKILL_FILE" | grep -q '^---$'; then
    green "YAML frontmatter opens with ---"
else
    red "Missing YAML frontmatter opening ---"
fi

if grep -q '^name: devflow' "$SKILL_FILE"; then
    green "name: devflow"
else
    red "name field missing or wrong"
fi

if grep -q '^description:' "$SKILL_FILE"; then
    green "description field present"
else
    red "description field missing"
fi

if grep -q '^version:' "$SKILL_FILE"; then
    green "version field present"
else
    red "version field missing"
fi

echo ""

# --- 2. DevFlow binary ---
echo "--- DevFlow Binary ---"
if command -v devflow &>/dev/null; then
    green "devflow on PATH ($(which devflow))"
    VER=$(devflow --version 2>&1)
    echo "  Version: $VER"
    green "devflow --version works"
else
    red "devflow not found on PATH"
fi

echo ""

# --- 3. Commands work ---
echo "--- Command Validation ---"
if devflow status &>/dev/null; then
    green "devflow status works"
else
    red "devflow status failed"
fi

if devflow config &>/dev/null; then
    green "devflow config works"
else
    red "devflow config failed"
fi

if devflow --help &>/dev/null; then
    green "devflow --help works"
else
    red "devflow --help failed"
fi

# Check subcommand existence in help
HELP=$(devflow --help 2>&1)
for cmd in start check status ship init config recover; do
    if echo "$HELP" | grep -q "$cmd"; then
        green "subcommand '$cmd' documented in --help"
    else
        red "subcommand '$cmd' NOT in --help"
    fi
done

echo ""

# --- 4. Summary ---
echo "============================="
echo "Results: $PASS passed, $FAIL failed"
echo "============================="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
