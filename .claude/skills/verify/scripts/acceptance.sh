#!/usr/bin/env bash
set -euo pipefail

[ "$#" -eq 1 ] || { echo "usage: $0 <out dir>" >&2; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
drive="$here/../../../../crates/bevy_rubrix/examples/gallery/drive/acceptance.drive"
out=$(realpath -m "$1")

rc=0
"$here/launch.sh" "$drive" "$out" > /dev/null || rc=$?
grep '^accept: ' "$out/log.txt" || true
for csv in "$out"/first-paint-*.csv "$out"/stress-*.csv; do
  [ -e "$csv" ] || continue
  printf '%s ' "$(basename "$csv" .csv)"
  "$here/frame-stats.sh" "$csv"
done

checks=$(grep -c '^expect ' "$drive")
passed=$(grep -c '^accept: .* pass$' "$out/log.txt" || true)
if [ "$rc" -ne 0 ]; then
  echo "acceptance: the gallery exited with $rc, see $out/log.txt" >&2
  exit "$rc"
fi
if [ "$passed" -ne "$checks" ]; then
  echo "acceptance: $passed of $checks checks pass, see $out/log.txt" >&2
  exit 1
fi
echo "acceptance: $passed of $checks checks pass"
