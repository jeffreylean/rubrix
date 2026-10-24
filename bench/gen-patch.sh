#!/usr/bin/env bash
# Usage: bench/gen-patch.sh [fynix-clone]
# Writes bench/fixtures/patch/10k.patch and 100k.patch
# from `git log -p` ending at fynix commit 6aea143, read from a
# clone of https://github.com/voxell-tech/fynix.git. The clone is
# $1, else $RUBRIX_FYNIX_CLONE, else ~/.cache/rubrix-fixtures/fynix,
# and is made there when missing. Nothing is ever pushed to it.
set -euo pipefail

cd "$(dirname "$0")/.."

url=https://github.com/voxell-tech/fynix.git
commit=6aea143ad9fd406e87043e2f9a9b373064378107
clone=${1:-${RUBRIX_FYNIX_CLONE:-${XDG_CACHE_HOME:-$HOME/.cache}/rubrix-fixtures/fynix}}
out=bench/fixtures/patch
large_lines=10000
stress_lines=100000
stress_slack=1000

export LC_ALL=C GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

if [ ! -d "$clone/.git" ]; then
  mkdir -p "$(dirname "$clone")"
  git clone --quiet --no-tags "$url" "$clone"
fi
git -C "$clone" cat-file -e "$commit^{commit}"

mkdir -p "$out"
git -C "$clone" log -p --format=email --abbrev=12 -M50% \
    --no-color --no-ext-diff --no-textconv --diff-algorithm=myers \
    --src-prefix=a/ --dst-prefix=b/ -U3 "$commit" |
  awk -v large="$out/10k.patch" -v stress="$out/100k.patch" \
    -v large_lines="$large_lines" -v stress_lines="$stress_lines" \
    -v slack="$stress_slack" '
    /^From [0-9a-f]+ Mon Sep 17 00:00:00 2001$/ { commits++ }
    { text[commits] = text[commits] $0 "\n"; size[commits]++ }
    END {
      total = 0
      for (i = 1; i <= commits && total < large_lines; i++) {
        printf "%s", text[i] > large
        total += size[i]
      }
      close(large)

      total = 0
      for (round = 0; total < stress_lines && round < 100; round++) {
        for (i = 1; i <= commits && total < stress_lines; i++) {
          if (total + size[i] > stress_lines + slack) continue
          printf "%s", text[i] > stress
          total += size[i]
        }
      }
      close(stress)
    }'

wc -l "$out/10k.patch" "$out/100k.patch"
