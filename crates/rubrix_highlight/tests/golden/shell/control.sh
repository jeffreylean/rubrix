#!/bin/sh
set -eu
if [ -f "$f" ] && [[ $x == y* ]]; then
  echo yes
elif test -d /tmp; then
  echo dir
else
  echo no
fi
case "$1" in
  start|go) run --fast -x ;;
  stop) halt ;;
  *) echo usage; exit 1 ;;
esac
for i in 1 2 3; do echo $i; done
for f in *.txt
do
  while read -r line; do echo "$line"; done < "$f"
done
until false; do break; done
greet() {
  local name=$1
  printf "hello %s\n" "$name"
}
function cleanup { rm -rf "$tmp"; }
echo if then fi
FOO=bar BAZ=1 env | sort &
make -j4 CC=gcc && ./a.out || echo failed
