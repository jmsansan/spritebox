#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo run --release --example night_shift
ffmpeg -hide_banner -loglevel error -y -framerate 24 \
  -i out/night-shift/frames/%03d.png \
  -vf 'split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=none' \
  -loop 0 out/night-shift/night-shift.gif
