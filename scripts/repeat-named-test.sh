#!/bin/sh
# Repeat one named test N times under the runner that failed (52.1 D-08).
#
#   nextest    cargo nextest run TARGET -E 'test(=NAME)'. A run counts only
#              when cargo exits 0 and exactly one output line has PASS as its
#              first field and NAME as its last: nextest's reporter line,
#              `PASS [time] <binary-id> NAME`, which it writes to stderr.
#   inprocess  cargo test TARGET, unfiltered, so the test shares its process
#              with its siblings (the races of 999.221, 999.222 and 999.232
#              need them). A run counts only when cargo exits 0 and exactly
#              one line reads `test NAME ... ok`.
#
# NAME is the full path as the runner prints it. A renamed or missing test
# fails the run instead of passing on zero matches. N passes bound nothing
# about a rate: report them as N runs.
set -eu

usage() {
    echo "usage: scripts/repeat-named-test.sh nextest|inprocess '<cargo target args>' <test name> <count>" >&2
    exit 2
}

[ "$#" -eq 4 ] || usage
mode=$1
target=$2
name=$3
count=$4
case "$mode" in
    nextest | inprocess) ;;
    *) usage ;;
esac
case "$count" in
    '' | *[!0-9]*) usage ;;
esac
[ "$count" -ge 1 ] || usage

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
trap 'exit 130' INT TERM

# Runs the test once into $workdir/run-<i>.log; returns 0 only on a counted pass.
run_once() {
    log="$workdir/run-$1.log"
    # $target is split on purpose: it holds several cargo arguments.
    # shellcheck disable=SC2086
    if [ "$mode" = nextest ]; then
        if cargo nextest run $target --no-tests=fail --retries 0 --status-level pass \
            --final-status-level none --color never -E "test(=$name)" >"$log" 2>&1; then
            rc=0
        else
            rc=$?
        fi
        matches=$(awk -v n="$name" '$1 == "PASS" && $NF == n { c++ } END { print c + 0 }' "$log")
    else
        if cargo test $target >"$log" 2>&1; then
            rc=0
        else
            rc=$?
        fi
        matches=$(awk -v l="test $name ... ok" '$0 == l { c++ } END { print c + 0 }' "$log")
    fi
    if [ "$rc" -ne 0 ]; then
        echo "run $1: cargo exited $rc" >&2
    elif [ "$matches" -ne 1 ]; then
        echo "run $1: named test $name matched $matches times" >&2
    else
        return 0
    fi
    tail -n 20 "$log" >&2
    return 1
}

passed=0
run=1
while [ "$run" -le "$count" ]; do
    if run_once "$run"; then
        passed=$((passed + 1))
    fi
    run=$((run + 1))
done
echo "passed $passed of $count"
[ "$passed" -eq "$count" ] || exit 1
