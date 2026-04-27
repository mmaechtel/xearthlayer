#!/bin/bash
# fuse_pending_sampler.sh — Periodic FUSE pending-request count
#
# Reads /sys/fs/fuse/connections/<id>/waiting every 1 second.
# Detects FUSE kernel queue saturation, which manifests as X-Plane hangs
# when concurrent reads exceed fuse.max_background config.
#
# NO SUDO REQUIRED — sysfs is readable for connections owned by the user.
#
# Usage:
#   ./fuse_pending_sampler.sh -o <run_dir> [-i <interval_sec>]
#   default interval: 1 sec (matches sysmon.py slow probe)
#
# Output: <run_dir>/fuse_pending.csv
#   columns: timestamp,connection_id,waiting

set -u

INTERVAL=1
OUT=""

while getopts "o:i:" opt; do
    case "$opt" in
        o) OUT="$OPTARG" ;;
        i) INTERVAL="$OPTARG" ;;
        *) echo "usage: $0 -o <run_dir> [-i <interval_sec>]" >&2; exit 1 ;;
    esac
done

if [ -z "$OUT" ]; then
    echo "ERROR: -o <run_dir> required" >&2
    exit 1
fi

mkdir -p "$OUT"
CSV="$OUT/fuse_pending.csv"

if [ ! -s "$CSV" ]; then
    echo "timestamp,connection_id,waiting" > "$CSV"
fi

cleanup() {
    echo "fuse_pending_sampler stopping"
    exit 0
}
trap cleanup TERM INT

echo "fuse_pending_sampler: writing to $CSV every ${INTERVAL}s"

while true; do
    ts=$(date +%s.%3N)
    for conn_dir in /sys/fs/fuse/connections/*/; do
        [ ! -d "$conn_dir" ] && continue
        cid=$(basename "$conn_dir")
        waiting=$(cat "$conn_dir/waiting" 2>/dev/null || echo "")
        [ -n "$waiting" ] && echo "$ts,$cid,$waiting" >> "$CSV"
    done
    sleep "$INTERVAL"
done
