#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

pin=$(sed -nE 's/^bevy = \{ version = "=([^"]+)".*/\1/p' Cargo.toml)
if [ -z "$pin" ]; then
  echo "no exact bevy pin found in workspace.dependencies" >&2
  exit 1
fi

status=0

duplicates=$(cargo tree --workspace --duplicates --prefix none | grep -E '^bevy' || true)
if [ -n "$duplicates" ]; then
  echo "bevy crates resolve at more than one version:" >&2
  echo "$duplicates" >&2
  status=1
fi

wrong=$(cargo tree --workspace --prefix none --format '{p}' \
  | grep -E '^bevy v' | grep -v -F "bevy v$pin" | sort -u || true)
if [ -n "$wrong" ]; then
  echo "bevy resolves at a version other than the pinned $pin:" >&2
  echo "$wrong" >&2
  status=1
fi

exit "$status"
