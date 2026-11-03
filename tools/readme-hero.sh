#!/bin/bash
# The README's moving picture, docs/pictures/readme-hero.webp: Ectomatter's
# Home on the desk made up for showing it (apps/ectomatter/tests/renders.rs,
# Desk::showcase), drawn offscreen at each moment of one turn of the
# keyboard's wave (the_readmes_moving_picture, 64 frames over its 4 seconds),
# then made one animated WebP that loops. The README's still pictures are
# held to the app by the_readmes_pictures_are_the_app_as_it_is; this one is
# made again by hand when Home changes.
#   tools/readme-hero.sh      needs bwrap, and ffmpeg with libwebp
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
ECTOMATTER_HERO_FRAMES=64 cargo test --release -p ectomatter --test renders the_readmes_moving_picture -- --ignored --quiet
# (64 frames over the wave's 4 seconds: 16 a second)
ffmpeg -loglevel error -y -framerate 16 -pattern_type glob -i 'target/readme-hero/frame-*.png' \
  -loop 0 -c:v libwebp_anim -lossless 0 -q:v 82 -compression_level 6 docs/pictures/readme-hero.webp
echo "docs/pictures/readme-hero.webp $(stat -c %s docs/pictures/readme-hero.webp) bytes"
