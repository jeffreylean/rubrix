#!/usr/bin/env bash
set -euo pipefail

ime_live=0
if [ "${1:-}" = "--ime-live" ]; then
  ime_live=1
  shift
fi
[ "$#" -eq 2 ] || { echo "usage: $0 [--ime-live] <drive script> <out dir>" >&2; exit 64; }
repo=$(cd "$(dirname "$0")/../../../.." && pwd)
script=$(realpath "$1")
out=$(realpath -m "$2")
mkdir -p "$out"

(cd "$repo" && cargo build --release -p bevy_rubrix --example gallery)
target=$(cd "$repo" && cargo metadata --format-version 1 --no-deps | jq -r .target_directory)

cd "$out"
if [ "$ime_live" -eq 1 ]; then
  echo "launch: --ime-live gives the gallery keyboard focus until it exits, so keys you type go to it" >&2
  before=$(hyprctl activewindow -j | jq -r '.address // empty')
fi
RUBRIX_IME_LIVE=$ime_live RUBRIX_DRIVE=$script "$target/release/examples/gallery" > log.txt 2>&1 &
pid=$!
echo "$pid" > pid
settled=0
stop_gallery() {
  local rc=$?
  if [ "$settled" -eq 0 ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    rm -f pid
    cat log.txt
    echo "launch failed with exit $rc, gallery $pid stopped" >&2
  fi
}
trap stop_gallery EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

addr=""
for _ in $(seq 1 100); do
  kill -0 "$pid" 2>/dev/null || break
  addr=$(hyprctl clients -j | jq -r ".[] | select(.pid == $pid) | .address")
  [ -n "$addr" ] && break
  sleep 0.1
done

if [ -n "$addr" ]; then
  w="address:$addr"
  floating=$(hyprctl clients -j | jq -r ".[] | select(.pid == $pid) | .floating")
  if [ "$floating" != "true" ]; then
    hyprctl dispatch "hl.dsp.window.float({ window = \"$w\", action = \"toggle\" })" > /dev/null
  fi
  hyprctl dispatch "hl.dsp.window.resize({ window = \"$w\", x = 1280, y = 800 })" > /dev/null
  hyprctl dispatch "hl.dsp.window.move({ window = \"$w\", x = 80, y = 80 })" > /dev/null
  if [ "$ime_live" -eq 1 ]; then
    hyprctl dispatch "hl.dsp.focus({ window = \"$w\" })" > /dev/null
    if [ "$(hyprctl activewindow -j | jq -r .pid)" != "$pid" ]; then
      echo "launch: Hyprland did not focus the gallery, so a wtype step exits with code 2" >&2
    fi
  fi
  hyprctl clients -j | jq -c ".[] | select(.pid == $pid) | {at, size, floating}" > window.json
elif kill -0 "$pid" 2>/dev/null; then
  echo "no window for pid $pid" >&2
fi

if ! timeout "${TIMEOUT:-300}" tail --pid="$pid" -f /dev/null; then
  echo "timeout after ${TIMEOUT:-300}s, killing pid $pid" >&2
  kill "$pid" 2>/dev/null || true
fi
rc=0
wait "$pid" || rc=$?
settled=1
rm -f pid
if [ "$ime_live" -eq 1 ] && [ -n "$before" ]; then
  hyprctl dispatch "hl.dsp.focus({ window = \"address:$before\" })" > /dev/null || true
fi
cat log.txt
echo "exit=$rc"
exit "$rc"
