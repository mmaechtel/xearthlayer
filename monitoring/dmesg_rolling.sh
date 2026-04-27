#!/bin/bash
# dmesg_rolling.sh — Periodic dmesg snapshot during a run
#
# Captures kernel ring buffer messages mid-flight, not just before/after.
# Critical for catching:
#   - "INFO: task hung for more than 120 seconds" (kernel hung-task warnings)
#   - NVMe driver errors
#   - BTRFS transaction warnings ("WARN btrfs_xxx")
#   - OOM killer activity
#   - Watchdog softlockup messages
#
# REQUIRES SUDO — kernel.dmesg_restrict=1 on this system blocks user reads.
#
# Usage:
#   sudo bash dmesg_rolling.sh -o <run_dir> [-i <interval_sec>] [-d <duration_sec>]
#   default interval: 60 sec  (every minute)
#   default duration: 9000 sec
#
# Output: <run_dir>/dmesg_rolling.log
#   appended timestamped blocks; only NEW lines per interval

set -u

INTERVAL=60
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
    echo "ERROR: This script needs root for dmesg access" >&2
    exit 1
fi

mkdir -p "$OUT"
LOG="$OUT/dmesg_rolling.log"

cleanup() {
    echo "=== dmesg_rolling stopping at $(date '+%Y-%m-%d %H:%M:%S') ===" >> "$LOG"
    exit 0
}
trap cleanup TERM INT

# dmesg --since flag plus -T for human-readable timestamps. Each iteration
# captures only events newer than the previous tick.
echo "dmesg_rolling: writing to $LOG every ${INTERVAL}s for ${DURATION}s"
echo "=== dmesg_rolling started $(date '+%Y-%m-%d %H:%M:%S') interval=${INTERVAL}s ===" >> "$LOG"

START=$(date +%s)
END=$((START + DURATION))
LAST_TS=$(date '+%Y-%m-%d %H:%M:%S')

while [ $(date +%s) -lt $END ]; do
    sleep "$INTERVAL"
    NOW=$(date '+%Y-%m-%d %H:%M:%S')
    # Capture only events newer than LAST_TS
    NEW=$(dmesg -T --since "$LAST_TS" 2>/dev/null | tail -200)
    if [ -n "$NEW" ]; then
        echo "" >> "$LOG"
        echo "=== $NOW ===" >> "$LOG"
        echo "$NEW" >> "$LOG"
    fi
    LAST_TS="$NOW"
done

echo "=== dmesg_rolling ended $(date '+%Y-%m-%d %H:%M:%S') ===" >> "$LOG"
