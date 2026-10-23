#!/usr/bin/env bash
set -uo pipefail

repo=$(cd "$(dirname "$0")/../../../.." && pwd)
status=0
need() {
  if command -v "$1" > /dev/null; then echo "ok   $1"; else echo "MISSING $1: $2"; status=1; fi
}

need cargo "install Rust through rustup"
need jq "needed by launch.sh and check-no-std.sh"
need hyprctl "launch.sh places the window through Hyprland"
need ffmpeg "video.sh needs it"
need ffprobe "counts the frames of a recording"

if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ] && hyprctl version > /dev/null 2>&1; then
  echo "ok   Hyprland $(hyprctl version -j | jq -r .tag)"
else
  echo "MISSING a running Hyprland session (HYPRLAND_INSTANCE_SIGNATURE)"; status=1
fi

want=$(sed -nE 's/^channel = "([^"]+)".*/\1/p' "$repo/rust-toolchain.toml")
have=$(cd "$repo" && rustc --version | cut -d' ' -f2)
if [ "$want" = "$have" ]; then echo "ok   rustc $have"; else echo "MISSING rustc $want (found $have)"; status=1; fi

if [ -f "$repo/crates/bevy_rubrix/examples/gallery/main.rs" ]; then
  echo "ok   gallery source at $repo"
else
  echo "MISSING crates/bevy_rubrix/examples/gallery/main.rs in $repo"; status=1
fi

running=$(pgrep -x gallery | tr '\n' ' ')
echo "info galleries running now: ${running:-none} (never kill one you did not start)"
exit "$status"
