#!/usr/bin/env bash
set -uo pipefail

special='
bad-script.drive           fail=2       an unknown step is a parse error
driver-ime-bad.drive       fail=2       a preedit cursor past its text is a parse error
driver-summary-fail.drive  fail=1       one of its three checks fails
driver-wtype.drive         fail=2       wtype needs launch.sh --ime-live
resize.drive               skip-darwin  resize asks Hyprland for the size
rich-wrap.drive            skip-darwin  resize asks Hyprland for the size
virtual-list-resize.drive  skip-darwin  resize asks Hyprland for the size
driver-mac.drive           skip-linux   oskey, pbcopy and pbpaste need macOS
editor-mac-cmd.drive       skip-linux   oskey needs macOS
editor-mac-option.drive    skip-linux   oskey needs macOS
editor-mac-emacs.drive     skip-linux   oskey needs macOS
editor-mac-clipboard.drive skip-linux   oskey, pbcopy and pbpaste need macOS
editor-mac-fonts.drive     skip-linux   oskey needs macOS, and CoreText font fallback is the check
editor-ime-live.drive      skip         types through fcitx5 Pinyin under launch.sh --ime-live, a hand check at milestone 1
'

again='
editor-mouse.drive  RUBRIX_SCALE=2  the mouse lane again at a Mac Retina scale
editor-cf-wrap-grow.drive  RUBRIX_SCALE=2  a wrap that grows a row, at a Mac Retina scale
editor-cf-wrap-shrink.drive  RUBRIX_SCALE=2  a wrap that shrinks a row, at a Mac Retina scale
editor-cf-wrap-toggle.drive  RUBRIX_SCALE=2  the wrap toggle, at a Mac Retina scale
'

replay_files='
graphemes.keys  crates/rubrix_edit/tests/replay/graphemes.txt
'

[ "$#" -eq 1 ] || { echo "usage: $0 <out dir>" >&2; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd -P)
os=$(uname -s)

passed=0
ran=0
skipped=0

row() {
  printf '%-5s %-40s %s\n' "$1" "$2" "$3"
}

special_row() {
  awk -v name="$1" '$1 == name { $1 = ""; sub(/^ +/, ""); print; exit }' <<< "$special"
}

judge() {
  local name=$1 rc=$2 want=$3 checked=$4
  ran=$((ran + 1))
  if [ "$rc" -eq "$want" ]; then
    passed=$((passed + 1))
    row pass "$name" "exit=$rc $checked"
  else
    row fail "$name" "exit=$rc, wants $want, $checked, see $out/$name.txt"
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

while read -r name var reason; do
  [ -n "$name" ] || continue
  script="$repo/crates/bevy_rubrix/examples/gallery/drive/$name"
  label="$name@$var"
  rc=0
  env "$var" "$here/launch.sh" "$script" "$out/${name%.drive}@$var" > "$out/$label.txt" 2>&1 || rc=$?
  judge "$label" "$rc" 0 "$(checks "$out/$label.txt")"
done <<< "$again"

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
