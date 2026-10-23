#!/usr/bin/env bash
set -euo pipefail

[ "$#" -eq 2 ] || { echo "usage: $0 <drive script> <out dir>" >&2; exit 64; }
repo=$(cd "$(dirname "$0")/../../../.." && pwd)
script=$(realpath "$1")
out=$(realpath -m "$2")
mkdir -p "$out"

(cd "$repo" && cargo build --release -p bevy_rubrix --example gallery)
target=$(cd "$repo" && cargo metadata --format-version 1 --no-deps | jq -r .target_directory)

cd "$out"
RUBRIX_DRIVE=$script "$target/release/examples/gallery" > log.txt 2>&1 &
pid=$!
echo "$pid" > pid
trap 'kill "$pid" 2>/dev/null || true' INT TERM

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
rm -f pid
cat log.txt
echo "exit=$rc"
exit "$rc"
