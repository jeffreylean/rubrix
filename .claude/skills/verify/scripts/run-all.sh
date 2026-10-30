#!/usr/bin/env bash
set -uo pipefail

# Scripts that do not simply pass, one per row: the script, its kind,
# and the reason. A kind is `fail=<code>`, the exit code the script
# must end with, or `skip-darwin`, `skip-linux`, or `skip` everywhere.
# A PR that adds such a script adds its row here.
special='
bad-script.drive           fail=2       an unknown step is a parse error
driver-ime-bad.drive       fail=2       a preedit cursor past its text is a parse error
driver-summary-fail.drive  fail=1       one of its three checks fails
driver-wtype.drive         fail=2       wtype needs launch.sh --ime-live
resize.drive               skip-darwin  resize asks Hyprland for the size
rich-wrap.drive            skip-darwin  resize asks Hyprland for the size
virtual-list-resize.drive  skip-darwin  resize asks Hyprland for the size
'

# The file a replay script edits, when it is not large.rs.txt.
replay_files='
graphemes.keys  crates/rubrix_edit/tests/replay/graphemes.txt
'

[ "$#" -eq 1 ] || { echo "usage: $0 <out dir>" >&2; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
out=$(realpath -m "$1")
mkdir -p "$out"
os=$(uname -s)

passed=0
ran=0
skipped=0

row() {
  printf '%-5s %-40s %s\n' "$1" "$2" "$3"
}

# Prints the kind and the reason of a script's row, or nothing.
special_row() {
  awk -v name="$1" '$1 == name { $1 = ""; sub(/^ +/, ""); print; exit }' <<< "$special"
}

# Counts a script that ran: $1 is its name, $2 its exit code, $3 the
# exit code it must end with, and $4 its check count.
judge() {
  ran=$((ran + 1))
  if [ "$2" -eq "$3" ]; then
    passed=$((passed + 1))
    row pass "$1" "exit=$2 $4"
  else
    row fail "$1" "exit=$2, wants $3, $4, see $out/$1.txt"
  fi
}

checks() {
  sed -nE 's/^(drive|replay): ([0-9]+ of [0-9]+ checks pass)$/\2/p' "$1" | tail -1
}

for script in "$repo"/crates/bevy_rubrix/examples/gallery/drive/*.drive; do
  name=$(basename "$script")
  read -r kind reason <<< "$(special_row "$name")"
  case "${kind:-}:$os" in
    skip:* | skip-darwin:Darwin | skip-linux:Linux)
      skipped=$((skipped + 1))
      row skip "$name" "$reason"
      continue
      ;;
  esac
  want=0
  case "${kind:-}" in fail=*) want=${kind#fail=} ;; esac
  rc=0
  "$here/launch.sh" "$script" "$out/${name%.drive}" > "$out/$name.txt" 2>&1 || rc=$?
  judge "$name" "$rc" "$want" "$(checks "$out/$name.txt")"
done

replays="$repo/crates/rubrix_edit/tests/replay"
if [ -d "$replays" ]; then
  (cd "$repo" && cargo build --release -q -p rubrix_edit --example replay)
  for script in "$replays"/*.keys; do
    name=$(basename "$script")
    file=$(awk -v name="$name" '$1 == name { print $2 }' <<< "$replay_files")
    rc=0
    (cd "$repo" && cargo run --release -q -p rubrix_edit --example replay -- \
      "${file:-bench/fixtures/rust/large.rs.txt}" "$script") > "$out/$name.txt" 2>&1 || rc=$?
    judge "$name" "$rc" 0 "$(checks "$out/$name.txt")"
  done
else
  skipped=$((skipped + 1))
  row skip "crates/rubrix_edit/tests/replay/" "missing until E1 merges"
fi

echo "run-all: $passed of $ran scripts pass, $skipped skipped"
[ "$passed" -eq "$ran" ]
