#!/bin/sh
# Name-based check of a removed-test ledger (52.2-07, review codex-3).
#
# A net `#[test]` count passes a replaced test with no row and fails a correct
# one-row ledger, so this compares test NAMES. For each FILE (a repo-relative
# path) it takes the test names at the ledger's `ledger-base:` commit and at the
# working tree; the removed names are base minus head. It fails on a removed
# name with no ledger row, and on a ledger row naming a test that was not
# removed from that file.
#
# A test name is the identifier of the first `fn` line after a line that is
# exactly `#[test]` (surrounding whitespace allowed), skipping attribute, `//`
# comment and blank lines in between.
#
# Ledger rows are the markdown table rows under the `## Test ledger` heading of
# LEDGER_MD, up to the next `## ` heading: cell 1 is the test name (backticks
# stripped, a `::`-qualified name compared by its last segment), cell 2 the
# file's repo-relative path.
#
# Exit 0: the ledger covers every removed name. Exit 1: a missing or stray row
# (each printed). Exit 2: usage, or a missing, repeated or unusable base.
set -eu
LC_ALL=C
export LC_ALL

usage() {
    cat >&2 <<'EOF'
usage: scripts/test-ledger-check.sh LEDGER_MD FILE...
       scripts/test-ledger-check.sh --self-test

  LEDGER_MD  a markdown file with one `ledger-base: <40-hex>` line and a
             `## Test ledger` table (test name | file | kind | successor)
  FILE       a repo-relative path whose removed tests the ledger must cover
EOF
    exit 2
}

# The test names in Rust source on stdin, one per line.
test_names() {
    awk '
        /^[ \t]*#\[test\][ \t]*$/ { want = 1; next }
        want && (/^[ \t]*#\[/ || /^[ \t]*\/\// || /^[ \t]*$/) { next }
        want {
            if (match($0, /(^|[^A-Za-z0-9_])fn[ \t]+[A-Za-z_][A-Za-z0-9_]*/)) {
                name = substr($0, RSTART, RLENGTH)
                sub(/^.*fn[ \t]+/, "", name)
                print name
            }
            want = 0
        }
    '
}

# The `FILE NAME` pairs of the ledger rows in LEDGER_MD ($1), one per line.
ledger_rows() {
    awk -F'|' '
        /^## / { in_ledger = ($0 ~ /^## Test ledger[ \t]*$/); next }
        in_ledger && /^[ \t]*\|/ {
            name = $2; file = $3
            gsub(/`/, "", name)
            gsub(/^[ \t]+|[ \t]+$/, "", name)
            gsub(/^[ \t]+|[ \t]+$/, "", file)
            count = split(name, parts, "::")
            name = parts[count]
            # The header row and the separator row name no test.
            if (name !~ /^[A-Za-z_][A-Za-z0-9_]*$/) next
            print file " " name
        }
    ' "$1"
}

check() {
    ledger=$1
    shift
    [ -f "$ledger" ] || { echo "test-ledger-check: no ledger file at $ledger" >&2; exit 2; }
    bases=$(grep -E '^ledger-base: [0-9a-f]{40}$' "$ledger" || true)
    base_count=$(printf '%s' "$bases" | grep -c . || true)
    if [ "$base_count" -ne 1 ]; then
        echo "test-ledger-check: $ledger must hold exactly one \`ledger-base: <40-hex>\` line; it holds $base_count" >&2
        exit 2
    fi
    base=${bases#ledger-base: }
    if ! git merge-base --is-ancestor "$base" HEAD 2>/dev/null; then
        echo "test-ledger-check: ledger-base $base is not an ancestor of HEAD" >&2
        exit 2
    fi

    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    ledger_rows "$ledger" >"$work/rows"
    failed=0
    removed_total=0
    file_count=0
    for file in "$@"; do
        file_count=$((file_count + 1))
        if git cat-file -e "$base:$file" 2>/dev/null; then
            git show "$base:$file" | test_names | sort >"$work/base"
        else
            : >"$work/base"
        fi
        if [ -f "$file" ]; then
            test_names <"$file" | sort >"$work/head"
        else
            : >"$work/head"
        fi
        comm -23 "$work/base" "$work/head" >"$work/removed"
        removed_total=$((removed_total + $(grep -c . "$work/removed" || true)))
        sort -u "$work/removed" >"$work/removed.u"
        awk -v file="$file" '$1 == file { print $2 }' "$work/rows" | sort -u >"$work/named"
        for name in $(comm -23 "$work/removed.u" "$work/named"); do
            echo "REMOVED-WITHOUT-ROW: $file $name"
            failed=1
        done
        for name in $(comm -13 "$work/removed.u" "$work/named"); do
            echo "ROW-NOT-REMOVED: $file $name"
            failed=1
        done
    done
    [ "$failed" -eq 0 ] || exit 1
    echo "ledger covers $removed_total removed tests in $file_count files"
}

# A scratch repo whose base has tests `kept`, `replaced` and `renamed_old`, and whose
# head replaces one and renames another, so the file's `#[test]` count is unchanged.
self_test() {
    script=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
    scratch=$(mktemp -d)
    trap 'rm -rf "$scratch"' EXIT
    unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_CONFIG GIT_PREFIX
    cd "$scratch"
    git init -q
    git config user.name "ledger self-test"
    git config user.email "ledger-self-test@example.invalid"
    git config commit.gpgsign false
    git config core.hooksPath /dev/null
    printf '%s\n' '#[test]' 'fn kept() {}' '' '#[test]' 'fn replaced() {}' '' \
        '#[test]' '#[ignore]' 'fn renamed_old() {}' >a.rs
    git add a.rs
    git commit -q -m base
    base=$(git rev-parse HEAD)
    printf '%s\n' '#[test]' 'fn kept() {}' '' '#[test]' 'fn brand_new() {}' '' \
        '#[test]' '#[ignore]' 'fn renamed_new() {}' >a.rs
    git commit -q -am head

    header=$(printf '%s\n' "ledger-base: $base" '' '## Test ledger' '' \
        '| test name | file | kind | successor test or D-NN |' \
        '|-----------|------|------|------------------------|')
    replaced_row='| replaced | a.rs | subject removed | D-00: the self-test |'
    renamed_row='| renamed_old | a.rs | renamed | renamed_new |'
    kept_row='| kept | a.rs | subject removed | D-00: a stray row |'

    printf '%s\n' "$header" >empty.md
    printf '%s\n' "$header" "$replaced_row" "$renamed_row" >covered.md
    printf '%s\n' "$header" "$replaced_row" "$renamed_row" "$kept_row" >stray.md

    ok=1
    status=0
    out=$("$script" empty.md a.rs) || status=$?
    if [ "$status" -ne 1 ] \
        || ! printf '%s\n' "$out" | grep -qx 'REMOVED-WITHOUT-ROW: a.rs replaced' \
        || ! printf '%s\n' "$out" | grep -qx 'REMOVED-WITHOUT-ROW: a.rs renamed_old' \
        || printf '%s\n' "$out" | grep -qw kept; then
        echo "self-test: the empty ledger did not fail on the replaced and renamed tests (exit $status):" >&2
        printf '%s\n' "$out" >&2
        ok=0
    fi
    status=0
    out=$("$script" covered.md a.rs) || status=$?
    if [ "$status" -ne 0 ]; then
        echo "self-test: the covering ledger did not pass (exit $status):" >&2
        printf '%s\n' "$out" >&2
        ok=0
    fi
    status=0
    out=$("$script" stray.md a.rs) || status=$?
    if [ "$status" -ne 1 ] || ! printf '%s\n' "$out" | grep -qx 'ROW-NOT-REMOVED: a.rs kept'; then
        echo "self-test: the stray row for a kept test did not fail (exit $status):" >&2
        printf '%s\n' "$out" >&2
        ok=0
    fi
    [ "$ok" -eq 1 ] || exit 1
    echo "self-test: replace, rename and stray-row controls bite"
}

[ "$#" -ge 1 ] || usage
if [ "$1" = "--self-test" ]; then
    [ "$#" -eq 1 ] || usage
    self_test
    exit 0
fi
[ "$#" -ge 2 ] || usage
ledger=$1
case "$ledger" in
    /*) ;;
    *) ledger=$(pwd)/$ledger ;;
esac
shift
top=$(git rev-parse --show-toplevel) || { echo "test-ledger-check: not inside a git work tree" >&2; exit 2; }
cd "$top"
check "$ledger" "$@"
