#!/bin/bash
# The README's moving pictures, docs/pictures/<name>.webp: Ectomatter's Home,
# the keyboard's Lighting page and its Keys page, on the desk made up for
# showing it (apps/ectomatter/tests/renders.rs, Desk::showcase), drawn
# offscreen at each moment of one turn of the keyboard's wave
# (the_readmes_moving_pictures, 64 frames over its 4 seconds), each made one
# animated WebP that loops (WebP, not GIF: a GIF's 256 colors band the
# lights' glow). The README's still pictures are held to the app by
# the_readmes_pictures_are_the_app_as_it_is; these are made again by hand
# when those pages change.
#   tools/readme-moving.sh      needs bwrap, and ffmpeg with libwebp
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
ECTOMATTER_MOVING_FRAMES=64 cargo test --release -p ectomatter --test renders the_readmes_moving_pictures -- --ignored --quiet
for d in target/readme-moving/*/; do
  name=$(basename "$d")
  # (64 frames over the wave's 4 seconds: 16 a second)
  ffmpeg -loglevel error -y -framerate 16 -pattern_type glob -i "$d/frame-*.png" \
    -loop 0 -c:v libwebp_anim -lossless 0 -q:v 82 -compression_level 6 "docs/pictures/$name.webp"
  echo "docs/pictures/$name.webp $(stat -c %s "docs/pictures/$name.webp") bytes"
done
