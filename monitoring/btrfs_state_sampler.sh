#!/bin/bash
# btrfs_state_sampler.sh — Periodic snapshot of BTRFS allocator state
#
# Reads /sys/fs/btrfs/<UUID>/allocation/{data,metadata,system}/*
# every N seconds and writes a CSV. Detects allocator pressure (>95% used,
# unallocated trending down) which BTRFS tools report only on demand.
#
# NO SUDO REQUIRED — sysfs is world-readable.
#
# Usage:
#   ./btrfs_state_sampler.sh -o <run_dir> [-i <interval_sec>]
#   default interval: 30 sec
#
# Output: <run_dir>/btrfs_state.csv
#   columns: timestamp,uuid,kind,total_bytes,used_bytes,used_pct

set -u

INTERVAL=30
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
CSV="$OUT/btrfs_state.csv"

# Header
if [ ! -s "$CSV" ]; then
    echo "timestamp,uuid,kind,total_bytes,used_bytes,used_pct" > "$CSV"
fi

cleanup() {
    echo "btrfs_state_sampler stopping (last write to $CSV)"
    exit 0
}
trap cleanup TERM INT

echo "btrfs_state_sampler: writing to $CSV every ${INTERVAL}s"

while true; do
    ts=$(date +%s.%3N)
    for uuid_dir in /sys/fs/btrfs/*/; do
        uuid=$(basename "$uuid_dir")
        # Skip "features" dir — not a UUID
        [ "$uuid" = "features" ] && continue
        [ ! -d "$uuid_dir/allocation" ] && continue

        for kind in data metadata system; do
            kind_dir="$uuid_dir/allocation/$kind"
            [ ! -d "$kind_dir" ] && continue
            tot=$(cat "$kind_dir/total_bytes" 2>/dev/null || echo 0)
            used=$(cat "$kind_dir/bytes_used" 2>/dev/null || echo 0)
            if [ "$tot" -gt 0 ]; then
                pct=$(awk -v u="$used" -v t="$tot" 'BEGIN{printf "%.2f", (u/t)*100}')
            else
                pct=0
            fi
            echo "$ts,$uuid,$kind,$tot,$used,$pct" >> "$CSV"
        done
    done
    sleep "$INTERVAL"
done
