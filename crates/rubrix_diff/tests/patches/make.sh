#!/usr/bin/env bash
# Usage: make.sh [out-dir]   (default: this script's directory)
# Rewrites the patches in out-dir from real git commands run in a
# scratch repository, so rerunning it leaves them byte-identical.
set -euo pipefail

out=$(realpath "${1:-$(dirname "$0")}")
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT

export LC_ALL=C TZ=UTC
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=rubrix GIT_AUTHOR_EMAIL=rubrix@example.com
export GIT_COMMITTER_NAME=rubrix GIT_COMMITTER_EMAIL=rubrix@example.com
export GIT_AUTHOR_DATE='2026-01-01T00:00:00+0000'
export GIT_COMMITTER_DATE='2026-01-01T00:00:00+0000'

cd "$repo"
git init --quiet
git config core.abbrev 12
git config core.quotepath true

seq_lines() { for i in $(seq "$1" "$2"); do echo "line $i"; done; }

{
  echo 'fn main() {'
  seq_lines 1 12
  echo '}'
  echo
  echo 'fn helper() {'
  seq_lines 13 30
  echo '}'
} >lib.rs
seq_lines 1 10 >docs.md
seq_lines 1 6 >keep.txt
echo one >tiny.txt
printf 'first\nsecond' >notes.txt
printf 'accent\n' >"héllo.txt"
printf 'space\n' >"spaced name.txt"
printf 'echo hi\n' >script.sh
printf '\211PNG\r\n\032\n\000\000\000\rone' >logo.png
seq_lines 1 3 >doomed.txt
git add -A
git commit --quiet -m base

reset() {
  git reset --quiet --hard
  git clean --quiet -fd
}

patch() {
  local name=$1
  shift
  git diff --no-color "$@" >"$out/$name.patch"
  reset
}

sed -i 's/^line 3$/line three/; s/^line 25$/line twenty-five/' lib.rs
patch modified -- lib.rs

echo two >tiny.txt
patch one-line -- tiny.txt

git mv docs.md guide.md
sed -i 's/^line 5$/line five/' guide.md
git add -A
patch rename --cached -M

git mv keep.txt kept.txt
patch rename-pure --cached -M

chmod +x script.sh
patch mode

printf 'new\nfile\n' >added.txt
git rm --quiet doomed.txt
git add -A
patch add-delete --cached

: >empty.txt
git add -A
patch empty-add --cached

printf 'last' >>notes.txt
echo >>notes.txt
patch no-newline

printf 'first\nchanged' >notes.txt
patch no-newline-both

printf '\211PNG\r\n\032\n\000\000\000\rtwo' >logo.png
git diff --no-color >"$out/binary.patch"
git diff --no-color --binary >"$out/binary-full.patch"
reset

printf 'brand new\n' >"héllo.txt"
sed -i 's/space/spaced out/' "spaced name.txt"
patch quoted

cp lib.rs util.rs
git add -A
patch copy --cached -C --find-copies-harder

cp lib.rs util.rs
sed -i 's/^line 3$/line three/' util.rs
git add -A
patch copy-edited --cached -C --find-copies-harder

sed -i 's/^line 3$/line three/' lib.rs
sed -i 's/^line 2$/line two/' docs.md
git commit --quiet -am 'Edit two files' -m 'A body line that is not a diff.'
git format-patch --quiet -1 --stdout --signature=2.0.0 \
  >"$out/format-patch.patch"

mkdir old new
seq_lines 1 8 >old/plain.txt
seq_lines 1 8 | sed 's/^line 4$/line four/' >new/plain.txt
touch -d '2026-01-01 00:00:00 +0000' old/plain.txt new/plain.txt
diff -u old/plain.txt new/plain.txt >"$out/plain.patch" || true

mkdir quilt-old quilt-new
seq_lines 1 6 >quilt-old/x.txt
seq_lines 1 6 | sed 's/^line 3$/line three/' >quilt-new/x.txt
seq_lines 1 6 >quilt-old/y.txt
seq_lines 1 6 | sed 's/^line 5$/line five/' >quilt-new/y.txt
touch -d '2026-01-01 00:00:00 +0000' quilt-old/* quilt-new/*
{
  diff -u quilt-old/x.txt quilt-new/x.txt || true
  diff -u quilt-old/y.txt quilt-new/y.txt || true
} >"$out/quilt.patch"

mkdir ru-old ru-new
seq_lines 1 4 >ru-old/t.txt
seq_lines 1 4 | sed 's/^line 2$/line two/' >ru-new/t.txt
printf '\211PNG\000one' >ru-old/i.bin
printf '\211PNG\000two' >ru-new/i.bin
seq_lines 1 3 >ru-new/added.txt
printf '\000only new' >ru-new/added.bin
printf '\000only old' >ru-old/gone.bin
touch -d '2026-01-01 00:00:00 +0000' ru-old/* ru-new/*
diff -ruN ru-old ru-new >"$out/diff-r-binary.patch" || true
diff -u /dev/null ru-new/added.bin >"$out/diff-null-binary.patch" || true

mkdir a b
printf '\211PNG\000one' >a/i.bin
printf '\211PNG\000two' >b/i.bin
git diff --no-color --no-index a/i.bin b/i.bin >"$out/noindex-binary.patch" || true
echo hi >a/m.sh
cp a/m.sh b/m.sh
chmod +x b/m.sh
git diff --no-color --no-index a/m.sh b/m.sh >"$out/noindex-mode.patch" || true
accent=$'h\303\251llo.bin'
printf '\211PNG\000one' >"a/$accent"
printf '\211PNG\000two' >"b/$accent"
printf '\000one' >"a/spaced name.bin"
printf '\000two' >"b/spaced name.bin"
{
  git diff --no-color --no-index "a/$accent" "b/$accent" || true
  git diff --no-color --no-index "a/spaced name.bin" "b/spaced name.bin" || true
} >"$out/noindex-quoted.patch"

printf 'top\n\nmiddle\n\nbottom\n' >blank.txt
git add blank.txt
git commit --quiet -m 'Add blank.txt'
printf 'top\n\nMIDDLE\n\nbottom\n' >blank.txt
git diff --no-color -- blank.txt | sed 's/^ $//' >"$out/blank-context-lf.patch"
sed 's/$/\r/' "$out/blank-context-lf.patch" >"$out/blank-context-crlf.patch"
reset

printf 'top\n-- x\nmid 1\nmid 2\nmid 3\nbottom\n' >dashes.txt
git add dashes.txt
git commit --quiet -m 'Add dashes.txt'
printf 'top\n++ y\nmid 1\nmid 2\nMID 3\nbottom\n' >dashes.txt
git diff --no-color -U0 -- dashes.txt >"$out/dashes.patch"
reset
