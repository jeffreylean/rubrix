#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

crates=$(cargo metadata --format-version 1 --no-deps | jq -r '
  .packages[]
  | select(.metadata.rubrix.no_std == true)
  | .name')

if [ -z "$crates" ]; then
  echo "no workspace crate sets [package.metadata.rubrix] no_std = true" >&2
  exit 1
fi

for crate in $crates; do
  echo "== $crate"
  cargo build --target x86_64-unknown-none -p "$crate"
done
