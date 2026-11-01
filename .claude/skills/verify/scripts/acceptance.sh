#!/usr/bin/env bash
set -euo pipefail

[ "$#" -eq 1 ] || { echo "usage: $0 <out dir>" >&2; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
drive="$here/../../../../crates/bevy_rubrix/examples/gallery/drive/acceptance.drive"
mkdir -p "$1"
out=$(cd "$1" && pwd -P)

budget_ms=3.0

rc=0
"$here/launch.sh" "$drive" "$out" > /dev/null || rc=$?
grep '^accept: ' "$out/log.txt" || true
for csv in "$out"/first-paint-*.csv; do
  [ -e "$csv" ] || continue
  printf '%s ' "$(basename "$csv" .csv)"
  "$here/frame-stats.sh" "$csv"
done

echo "p95 budget $budget_ms ms, from R12 and R15: a loaded machine fails it, so run acceptance alone"
p95_checks=0
p95_passed=0
for view in code diff; do
  p95_checks=$((p95_checks + 1))
  csv="$out/stress-$view.csv"
  stats=$([ -e "$csv" ] && "$here/frame-stats.sh" "$csv" || echo "no frames")
  echo "stress-$view $stats"
  p95=$(sed -n 's/.* p95=\([0-9.]*\) .*/\1/p' <<< "$stats")
  if [ -n "$p95" ] && awk -v p="$p95" -v b="$budget_ms" 'BEGIN { exit !(p <= b) }'; then
    echo "accept: stress-$view-p95 pass"
    p95_passed=$((p95_passed + 1))
  else
    echo "accept: stress-$view-p95 fail"
  fi
done

checks=$(($(grep -c '^expect ' "$drive") + p95_checks))
passed=$(($(grep -c '^accept: .* pass$' "$out/log.txt" || true) + p95_passed))
if [ "$rc" -ne 0 ]; then
  echo "acceptance: the gallery exited with $rc, see $out/log.txt" >&2
  exit "$rc"
fi
if [ "$passed" -ne "$checks" ]; then
  echo "acceptance: $passed of $checks checks pass, see $out/log.txt" >&2
  exit 1
fi
echo "acceptance: $passed of $checks checks pass"
