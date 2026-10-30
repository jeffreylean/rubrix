#!/usr/bin/env bash
# lanes.sh <out dir> [<trunk worktree>]
#
# Runs E1's replay lanes 2 to 10, saves each run's output as
# <out dir>/e1-<slug>.txt, and prints each run's last line after its
# lane number. Given a trunk worktree, it also runs lane 1: the live
# bench at trunk, with CARGO_TARGET_DIR set to TRUNK_TARGET_DIR, and
# here, with this shell's CARGO_TARGET_DIR, compared on changed lines
# row by row, and undo-run.keys here, saved as e1-live-regress.txt.
# Exits 0 only when every lane passes.
set -euo pipefail

if (($# < 1 || $# > 2)); then
  echo "usage: lanes.sh <out dir> [<trunk worktree>]" >&2
  exit 2
fi
mkdir -p "$1"
out=$(cd "$1" && pwd)
trunk=${2:+$(cd "$2" && pwd)}
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."

dir=crates/rubrix_edit/tests/replay
large=bench/fixtures/rust/large.rs.txt
replay=$(cargo build --release -p rubrix_edit --example replay \
  --message-format=json-render-diagnostics |
  jq -r 'select(.reason == "compiler-artifact"
    and .target.name == "replay") | .executable')

failed=0
for lane in \
  "2 undo-run $large undo-run.keys" \
  "3 undo-breaks $large undo-breaks.keys" \
  "4 multi-caret $large multi-caret.keys" \
  "5 merge $large merge.keys" \
  "6 graphemes $dir/graphemes.txt graphemes.keys" \
  "7 words $large words.keys" \
  "8 indent-enter $large indent-enter.keys" \
  "9 crlf $large crlf.keys" \
  "10 stress $large stress-100k.keys"; do
  read -r n slug file script <<<"$lane"
  log=$out/e1-$slug.txt
  "$replay" "$file" "$dir/$script" >"$log" 2>&1 || failed=1
  echo "lane $n: $(tail -n 1 "$log")"
done

if [[ -n $trunk ]]; then
  : "${TRUNK_TARGET_DIR:?set it to the target dir of the trunk worktree}"
  live=(cargo run --locked --release -q -p rubrix_bench --bin live --)
  trunk_table=$(cd "$trunk" &&
    CARGO_TARGET_DIR=$TRUNK_TARGET_DIR "${live[@]}" \
      --json "$out/e1-live-trunk.json")
  head_table=$("${live[@]}" --json "$out/e1-live-head.json")
  comparison=$(jq -rn \
    --slurpfile trunk "$out/e1-live-trunk.json" \
    --slurpfile head "$out/e1-live-head.json" '
    def rows: map({key: "\(.file) | \(.scenario)",
      value: (.["changed lines"] // "-" | tostring)}) | from_entries;
    ($trunk[0] | rows) as $t | ($head[0] | rows) as $h
    | ($t + $h | keys_unsorted[]) as $k
    | [$t, $h] | map(if has($k) then .[$k] else "missing" end)
    | "\($k): trunk \(.[0]), head \(.[1]), "
      + (if .[0] == .[1] then "match" else "differ" end)')
  total=$(grep -c '' <<<"$comparison")
  matched=$(grep -c ', match$' <<<"$comparison" || true)
  undo=pass
  undo_log=$("$replay" "$large" "$dir/undo-run.keys" 2>&1) || undo=fail
  [[ $matched == "$total" && $undo == pass ]] || failed=1
  summary="lane 1: $matched of $total live rows match, undo-run $undo"
  {
    echo "== live at trunk $(git -C "$trunk" describe --always --dirty)"
    echo "$trunk_table"
    echo "== live at head $(git describe --always --dirty)"
    echo "$head_table"
    echo "== changed lines, trunk against head"
    echo "$comparison"
    echo "== undo-run.keys at head"
    echo "$undo_log"
    echo "$summary"
  } >"$out/e1-live-regress.txt"
  echo "$summary"
fi

exit "$failed"
