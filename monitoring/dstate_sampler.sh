#!/bin/bash
# dstate_sampler.sh — Periodic D-state thread sampler with kernel stack traces
#
# Every N seconds, scans /proc/*/status for State==D (uninterruptible wait)
# and dumps /proc/<pid>/stack for each. This is the holy grail for diagnosing
# X-Plane / FUSE hangs: shows the exact kernel function the thread is stuck in.
#
# Without this, we know "X-Plane is in D-state" but not whether it's:
#   - waiting on FUSE read (fuse_request_send)
#   - waiting on Swap-In (folio_wait_bit_common)
#   - waiting on BTRFS commit (btrfs_commit_transaction)
#   - waiting on Page Writeback (folio_wait_writeback)
#
# REQUIRES SUDO — /proc/<pid>/stack needs CAP_SYS_PTRACE for non-self PIDs.
#
# Usage:
#   sudo bash dstate_sampler.sh -o <run_dir> [-i <interval_sec>] [-d <duration_sec>]
#   default interval: 5 sec
#   default duration: 9000 sec (covers a 90-min run + buffer)
#
# Output: <run_dir>/trace_dstate.log
#   format: time-prefixed blocks per scan

set -u

INTERVAL=5
DURATION=9000
OUT=""

while getopts "o:i:d:" opt; do
    case "$opt" in
        o) OUT="$OPTARG" ;;
        i) INTERVAL="$OPTARG" ;;
        d) DURATION="$OPTARG" ;;
        *) echo "usage: sudo bash $0 -o <run_dir> [-i <sec>] [-d <sec>]" >&2; exit 1 ;;
    esac
done

if [ -z "$OUT" ]; then
    echo "ERROR: -o <run_dir> required" >&2
    exit 1
fi

if [ "$EUID" -ne 0 ]; then
    echo "ERROR: This script needs root for /proc/<pid>/stack access" >&2
    exit 1
fi

mkdir -p "$OUT"
LOG="$OUT/trace_dstate.log"

cleanup() {
    echo "dstate_sampler stopping" >> "$LOG"
    exit 0
}
trap cleanup TERM INT

echo "dstate_sampler: writing to $LOG every ${INTERVAL}s for ${DURATION}s"
echo "=== dstate_sampler started $(date '+%Y-%m-%d %H:%M:%S') interval=${INTERVAL}s ===" >> "$LOG"

START=$(date +%s)
END=$((START + DURATION))

while [ $(date +%s) -lt $END ]; do
    ts=$(date '+%H:%M:%S')

    # Find threads in D-state.
    # /proc/<pid>/task/<tid>/status holds State of each thread.
    # State line format: "State:	D (disk sleep)" or "D" or "D (uninterruptible)"
    found_any=0
    for tid_dir in /proc/[0-9]*/task/[0-9]*; do
        [ ! -d "$tid_dir" ] && continue
        # Read state — fast check, reject early if not D
        state=$(awk '/^State:/{print $2; exit}' "$tid_dir/status" 2>/dev/null)
        [ "$state" != "D" ] && continue

        if [ "$found_any" -eq 0 ]; then
            echo "" >> "$LOG"
            echo "=== $ts D-STATE SCAN ===" >> "$LOG"
            found_any=1
        fi

        tid=$(basename "$tid_dir")
        pid=$(echo "$tid_dir" | awk -F/ '{print $3}')
        comm=$(cat "$tid_dir/comm" 2>/dev/null || echo "?")
        wchan=$(cat "$tid_dir/wchan" 2>/dev/null || echo "?")

        echo "  PID=$pid TID=$tid comm=$comm wchan=$wchan" >> "$LOG"
        echo "  Stack:" >> "$LOG"
        # /proc/<pid>/stack gives full kernel stack (newest frame first)
        cat "$tid_dir/stack" 2>/dev/null | head -20 | sed 's/^/    /' >> "$LOG"
        echo "" >> "$LOG"
    done

    sleep "$INTERVAL"
done

echo "=== dstate_sampler ended $(date '+%Y-%m-%d %H:%M:%S') ===" >> "$LOG"
