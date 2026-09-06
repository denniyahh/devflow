#!/usr/bin/env bash
# Decide whether the CPU pin actually narrowed the visible core count.
#
#   scripts/assert-cpu-pin.sh <cpu_list> <unpinned_nproc> <pinned_nproc>
#
# This script MEASURES NOTHING. The caller does the real `nproc` and the real
# pinned `nproc` and hands both over. That split is the entire reason the
# failing direction is testable: a decider embedded in .github/workflows/ci.yml
# could only ever be observed on a runner that happened to disagree with
# itself, so its interesting branch would never run. Here every branch runs in
# `cargo test -p devflow --test ci_parity_guards`.
#
# Why it exists (46-REVIEWS.md C-03): the CI step this backs used to be three
# bare `echo`s under a comment asserting that the unpinned count was the
# negative control for the pinned one. Nothing compared them and the step
# exited 0 either way. Documenting a gate is not implementing one.
#
# Outcomes:
#   PASSED  — pinned < unpinned. The pin narrowed what one child process sees.
#   FAILED  — pinned >= unpinned, exit 1. Before this script no command in the
#             repository could produce this.
#   SKIPPED — cpu_list is `all` (no pin exists to narrow anything; see C-06),
#             or the host reports <= 2 CPUs (a 2-CPU pin cannot narrow a 2-CPU
#             machine, so a failure would be an artefact of the runner).
#
# Both skip paths PRINT their reason. A silent skip and a pass are
# indistinguishable to anyone scanning a job log, which is C-03 in another
# shape.
#
# WHAT A PASS DOES NOT ESTABLISH: that rustc or the test-harness threads were
# actually confined, or that the runner was allocated 2 cores. It shows only
# that the pin narrowed the core count VISIBLE to one child process. The
# mechanism being relied on is that CPU affinity is inherited across
# fork/exec, and `nproc` does not measure inheritance. `nproc` is diagnostic
# here, not the sole oracle.
#
# Every `backtick` below is Markdown-style prose inside a printf FORMAT
# string, never a command substitution, so SC2016's advice would be wrong.
# This must sit above the first command to apply file-wide.
# shellcheck disable=SC2016
set -euo pipefail

usage() {
    printf 'usage: %s <cpu_list> <unpinned_nproc> <pinned_nproc>\n' "$0" >&2
    printf '  cpu_list       the value of CPUS, e.g. `0,1` or `all`\n' >&2
    printf '  unpinned_nproc `nproc` with no pin applied\n' >&2
    printf '  pinned_nproc   `nproc` under the pin\n' >&2
}

if [ "$#" -ne 3 ]; then
    printf 'assert-cpu-pin: expected 3 arguments, got %s\n' "$#" >&2
    usage
    exit 2
fi

cpu_list="$1"
unpinned="$2"
pinned="$3"

# Reject non-numeric counts explicitly. Left to `[ ... -lt ... ]` this would
# die under `set -e` with "integer expression expected", which reads as a
# broken script rather than as bad input.
for pair in "unpinned_nproc=$unpinned" "pinned_nproc=$pinned"; do
    value="${pair#*=}"
    case "$value" in
        '' | *[!0-9]*)
            printf 'assert-cpu-pin: %s is not a non-negative integer: %s\n' \
                "${pair%%=*}" "$value" >&2
            usage
            exit 2
            ;;
    esac
done

if [ "$cpu_list" = "all" ]; then
    printf 'assert-cpu-pin: SKIPPED — cpu_list is `all`, so no pin was applied '
    printf 'and there is nothing to narrow (unpinned_nproc=%s pinned_nproc=%s).\n' \
        "$unpinned" "$pinned"
    exit 0
fi

if [ "$unpinned" -le 2 ]; then
    printf 'assert-cpu-pin: SKIPPED — this is a 2-CPU host (unpinned_nproc=%s), ' \
        "$unpinned"
    printf 'and a 2-CPU pin cannot narrow a 2-CPU machine, so a failure here '
    printf 'would be an artefact of the runner (pinned_nproc=%s).\n' "$pinned"
    exit 0
fi

if [ "$pinned" -lt "$unpinned" ]; then
    printf 'assert-cpu-pin: PASSED — the pin `%s` narrowed the visible core ' \
        "$cpu_list"
    printf 'count from unpinned_nproc=%s to pinned_nproc=%s.\n' "$unpinned" "$pinned"
    exit 0
fi

printf 'assert-cpu-pin: FAILED — the pin `%s` did not narrow the visible core ' \
    "$cpu_list" >&2
printf 'count: unpinned_nproc=%s, pinned_nproc=%s. A partially-invalid CPU ' \
    "$unpinned" "$pinned" >&2
printf 'list narrows SILENTLY and still exits 0, so the counts matching means '>&2
printf 'the job is NOT running under the load shape it claims.\n' >&2
exit 1
