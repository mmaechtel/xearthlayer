#!/usr/bin/env bash
# Reproduces the level 0 sharpening benchmark (level0-bench).
#
#   scripts/level0-bench.sh synth   synthetic scenes, no network (default)
#   scripts/level0-bench.sh real    fetch real provider chunks, then run
#   scripts/level0-bench.sh probe   test one chunk per provider
#
# Chunks are cached under level0-bench/data (gitignored), so a second
# run fetches nothing. Real results go to level0-bench/results, synthetic
# ones to level0-bench/results-synth.
set -euo pipefail

cd "$(dirname "$0")/.."
tool="level0-bench"
bench=(cargo run --release --quiet -p level0-bench --)

case "${1:-synth}" in
    probe)
        "${bench[@]}" probe
        ;;
    synth)
        rm -rf "$tool/data-synth"
        "${bench[@]}" synth --data "$tool/data-synth"
        "${bench[@]}" run --data "$tool/data-synth" --out "$tool/results-synth" \
            --notes "$tool/NOTES-synth.md"
        ;;
    real)
        "${bench[@]}" fetch --data "$tool/data" --providers bing,arcgis,apple
        "${bench[@]}" run --data "$tool/data" --out "$tool/results" \
            --notes "$tool/NOTES.md"
        ;;
    *)
        echo "usage: $0 [synth|real|probe]" >&2
        exit 2
        ;;
esac
