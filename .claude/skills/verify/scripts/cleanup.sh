#!/usr/bin/env bash
set -uo pipefail

for out in "$@"; do
  pidfile="$out/pid"
  [ -f "$pidfile" ] || { echo "no pid file in $out"; continue; }
  pid=$(cat "$pidfile")
  if [ "$(cat "/proc/$pid/comm" 2> /dev/null)" = "gallery" ]; then
    kill "$pid" && echo "stopped gallery pid $pid from $out"
  else
    echo "pid $pid from $out is not a gallery any more"
  fi
  rm -f "$pidfile"
done
