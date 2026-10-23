#!/usr/bin/env bash
set -euo pipefail

[ "$#" -ge 2 ] || { echo "usage: $0 <record dir> <out.mp4> [fps]" >&2; exit 64; }

ffmpeg -y -loglevel error -framerate "${3:-60}" -i "$1/frame-%05d.png" \
  -c:v libx264 -pix_fmt yuv420p -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" "$2"
