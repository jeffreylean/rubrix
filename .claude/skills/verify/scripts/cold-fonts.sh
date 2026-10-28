#!/usr/bin/env bash
set -euo pipefail

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || { echo "usage: $0 <out dir> [runs, default 4]" >&2; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "$1")
runs=${2:-4}
budget_ms=16.7
mkdir -p "$out"

over=0
for run in $(seq 1 "$runs"); do
  dir="$out/cold$run"
  mkdir -p "$dir"
  {
    date -Is
    uptime
    echo "pgrep -a cargo:"
    pgrep -a cargo || echo none
  } > "$dir/pre.txt"
  python3 -I -c '
import os
evicted = 0
for root, _, files in os.walk("/usr/share/fonts"):
    for name in files:
        try:
            fd = os.open(os.path.join(root, name), os.O_RDONLY)
        except OSError:
            continue
        os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        os.close(fd)
        evicted += 1
print("evicted", evicted, "font files from the page cache")
' >> "$dir/pre.txt"
  acc=0
  "$here/acceptance.sh" "$dir" > "$dir/acceptance.out" 2>&1 || acc=$?
  line="cold$run load $(sed -n 2p "$dir/pre.txt" | sed 's/.*load average: //') acceptance exit $acc"
  for page in hamlet_rich hamlet_code hamlet_diff; do
    csv="$dir/first-paint-$page.csv"
    max=$([ -e "$csv" ] && "$here/frame-stats.sh" "$csv" | sed -n 's/.* max=\([0-9.]*\) .*/\1/p')
    line="$line $page ${max:-none}"
    if [ -z "$max" ] || ! awk -v m="$max" -v b="$budget_ms" 'BEGIN { exit !(m <= b) }'; then
      over=$((over + 1))
    fi
  done
  echo "$line ms"
done

if [ "$over" -ne 0 ]; then
  echo "cold-fonts: $over page switches over $budget_ms ms in $runs runs" >&2
  exit 1
fi
echo "cold-fonts: every Hamlet page switch within $budget_ms ms in $runs runs"
