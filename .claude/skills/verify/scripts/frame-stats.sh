#!/usr/bin/env bash
set -euo pipefail

[ "$#" -eq 1 ] || { echo "usage: $0 <frames csv>" >&2; exit 64; }

warmup=60
tail -n +2 "$1" | tail -n +$((warmup + 1)) | cut -d, -f2 | sort -g | awk -v warmup="$warmup" '
  { ms[NR] = $1 }
  END {
    if (NR == 0) { print "no frames after the first " warmup > "/dev/stderr"; exit 1 }
    median = NR % 2 ? ms[(NR + 1) / 2] : (ms[NR / 2] + ms[NR / 2 + 1]) / 2
    rank = int(0.95 * NR); if (rank < 0.95 * NR) rank++
    printf "frames=%d median=%.3f p95=%.3f max=%.3f ms\n", NR, median, ms[rank], ms[NR]
  }'
