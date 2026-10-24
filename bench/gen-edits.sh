#!/usr/bin/env bash
# Usage: bench/gen-edits.sh [out-dir]   (default: target/edits)
# Each pair changes rate% of the old lines, rounded to a whole line.
set -euo pipefail

cd "$(dirname "$0")/.."

out=${1:-target/edits}
fixtures=bench/fixtures/rust
export LC_ALL=C

edit() {
  awk -v seed="$1" -v rate="$2" -v lines="$(wc -l <"$3")" '
    function next_uniform() {
      state = state * 16807 % 2147483647
      return state / 2147483647
    }
    BEGIN {
      state = seed
      k = 0
      left = lines
      need = int(lines * rate / 100 + 0.5)
    }
    {
      if (next_uniform() * left-- >= need) {
        print
        next
      }
      need--
      k++
      pick = next_uniform()
      if (pick < 0.5) {
        print $0 " // edit " k
      } else if (pick >= 0.75) {
        print $0 " // edit " k
        print "    let edit_" k " = " k ";"
      }
    }
  ' "$3"
}

write_pair() {
  local name=$1 seed=$2 rate=$3 dir=$out/$1
  edit "$seed" "$rate" "$dir/old.rs.txt" >"$dir/new.rs.txt"
  echo "$name old=$(wc -l <"$dir/old.rs.txt" | tr -d ' ') new=$(wc -l <"$dir/new.rs.txt" | tr -d ' ')"
}

seed=0
for tier in tiny small large; do
  seed=$((seed + 1000))
  for rate in 1 10 50; do
    dir=$out/$tier-$rate
    mkdir -p "$dir"
    cp "$fixtures/$tier.rs.txt" "$dir/old.rs.txt"
    write_pair "$tier-$rate" $((seed + rate)) "$rate"
  done
done

large_lines=$(wc -l <"$fixtures/large.rs.txt")
copies=$(((100000 + large_lines - 1) / large_lines))
mkdir -p "$out/stress-1"
for ((i = 0; i < copies; i++)); do
  cat "$fixtures/large.rs.txt"
done >"$out/stress-1/old.rs.txt"
write_pair stress-1 9001 1
