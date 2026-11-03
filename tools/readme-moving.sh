#!/bin/bash
# The README's moving pictures, docs/pictures/<name>.webp, on the desks made
# up for showing Ectomatter (apps/ectomatter/tests/renders.rs,
# Desk::showcase_laid), drawn offscreen at each moment of one turn of the
# light (the_readmes_moving_pictures, 64 frames over its 4 seconds):
#
# - readme-hero.webp: Home on three desks one after another, each fading
#   into the next and the last into the first, so it loops without a seam:
#   laid out as Grant has his, orange on everything, a word typed and the
#   mouse clicked, each press sending its ripple across the desk; on a full
#   mat, a wave of color across the desk; and the mouse at the right on a
#   mousepad with art on it, clicked, then a word typed
# - readme-lighting.webp, readme-keys.webp: the keyboard's Lighting and Keys
#   pages
# - readme-keyboard-lab.webp: the Keyboard lab, the LEDs' Beam dragged
#   through its run and the light following it
# - readme-keycaps.webp: the keyboard's caps made over, three seconds each
#   (renders.rs, KEYCAPS), its wave running on through them
#
# Animated WebP, not GIF: a GIF's 256 colors band the lights' glow. The
# README's still pictures are held to the app by
# the_readmes_pictures_are_the_app_as_it_is; these are made again by hand
# when those pages change.
#   tools/readme-moving.sh      needs bwrap, and Python's Pillow with WebP
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
ECTOMATTER_MOVING_FRAMES=64 cargo test --release -p ectomatter --test renders the_readmes_moving_pictures -- --ignored --quiet
python3 -I - <<'PY'
# (made with libwebp's own animation encoder, through Pillow: each frame keeps
# only what changed, without leaving parts of earlier frames behind)
from pathlib import Path
from PIL import Image
at = Path('target/readme-moving')
def frames(name):
    return [Image.open(f).convert('RGB') for f in sorted((at / name).glob('frame-*.png'))]
def save(name, seq, quality=82):
    out = Path('docs/pictures') / f'{name}.webp'
    # (64 frames over the light's 4 seconds: 16 a second, 62.5 ms each)
    seq[0].save(out, save_all=True, append_images=seq[1:], duration=[62 + i % 2 for i in range(len(seq))], loop=0, quality=quality, method=6)
    print(out, out.stat().st_size, 'bytes,', len(seq), 'frames')
def fade(a, b):
    return [Image.blend(x, y, (k + 0.5) / len(a)) for k, (x, y) in enumerate(zip(a, b))]
for name in ('readme-lighting', 'readme-keys', 'readme-keyboard-lab'):
    save(name, frames(name))
# (twelve seconds, the wave over all of it: a little less quality keeps it
# near 5 MB)
save('readme-keycaps', frames('readme-keycaps'), quality=70)
# (the hero: the first desk from its ninth frame, then the second and the third,
# each fading into the next over half a second, and the third into the first
# desk's first eight frames, so the end fades into where it starts)
mine, mat, pad = frames('readme-hero-mine'), frames('readme-hero-mat'), frames('readme-hero-pad')
a, d = mine[8:], mine[:8]
seq = a[:-8] + fade(a[-8:], mat[:8]) + mat[8:-8] + fade(mat[-8:], pad[:8]) + pad[8:-8] + fade(pad[-8:], d)
# (the ripples change much of each frame: a little less quality keeps the
# README's top picture under 5 MB, the glow unbanded)
save('readme-hero', seq, quality=70)
PY
