#!/bin/sh
# Planned-removal gate for devflow-core (Phase 52.2, accept-break).
#
# Runs the minor-release semver check against origin/develop and fails unless
# the run finished (exit 0: no lint failed; exit 100: lints failed), ran a
# non-zero number of checks, and every item it lists under `Failed in:`
# contains one of the fixed-string patterns in LIST_FILE. A removal that no
# plan names is printed as `UNPLANNED:` and fails the gate.
#
# usage: scripts/semver-planned-removals.sh LIST_FILE
set -eu

usage() {
    echo "usage: scripts/semver-planned-removals.sh LIST_FILE" >&2
    exit 2
}

[ "$#" -eq 1 ] || usage
LIST_FILE=$1
if [ ! -f "$LIST_FILE" ] || [ ! -r "$LIST_FILE" ]; then
    echo "error: removal list '$LIST_FILE' is not a readable file" >&2
    usage
fi

if ! command -v cargo-semver-checks >/dev/null 2>&1; then
    echo "error: cargo-semver-checks is not installed" >&2
    exit 1
fi

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT
OUTPUT="$WORK_DIR/semver.out"
PATTERNS="$WORK_DIR/patterns"
ENTRIES="$WORK_DIR/entries"
TAIL_LINES=20

print_tail() {
    tail -n "$TAIL_LINES" "$OUTPUT" >&2
}

status=0
cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop \
    --release-type minor >"$OUTPUT" 2>&1 || status=$?

case "$status" in
    0 | 100) ;;
    *)
        echo "cargo semver-checks exited $status" >&2
        print_tail
        exit 1
        ;;
esac

checks=$(sed -n 's/^[[:space:]]*Checked \[[^]]*\][[:space:]]*\([0-9][0-9]*\) checks.*/\1/p' "$OUTPUT" | head -n 1)
if [ -z "$checks" ] || [ "$checks" -eq 0 ]; then
    echo "semver-checks ran no checks" >&2
    print_tail
    exit 1
fi

# Each non-blank line after a `Failed in:` line, up to the next blank line,
# names one item; leading whitespace is trimmed.
awk '
    /^Failed in:[[:space:]]*$/ { listing = 1; next }
    listing && /^[[:space:]]*$/ { listing = 0; next }
    listing { sub(/^[[:space:]]+/, ""); print }
' "$OUTPUT" >"$ENTRIES"

listed=$(wc -l <"$ENTRIES" | tr -d ' ')
if [ "$status" -eq 100 ] && [ "$listed" -eq 0 ]; then
    echo "semver-checks failed but listed no item" >&2
    print_tail
    exit 1
fi

grep -v '^#' "$LIST_FILE" | grep -v '^[[:space:]]*$' >"$PATTERNS" || true

unplanned=0
while IFS= read -r entry; do
    if [ -s "$PATTERNS" ] && printf '%s\n' "$entry" | grep -F -q -f "$PATTERNS"; then
        echo "planned: $entry"
    else
        echo "UNPLANNED: $entry"
        unplanned=$((unplanned + 1))
    fi
done <"$ENTRIES"

echo "checked $checks checks; $listed listed items, $unplanned unplanned"
if [ "$unplanned" -gt 0 ]; then
    exit 1
fi
