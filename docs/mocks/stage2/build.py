"""Build the stage 2 mock's stylesheet from the current editor's.

The mock runs the real editor restyled in the Ectonomic family's language:
every blue-grey neutral becomes the zinc shade nearest it in lightness, the
interface is set in Geist (the keycaps keep their condensed face, which
imitates EVGA's printing), and lime appears only as a signal: focus, the
connection and live dots, and the box around a modified key.

    python3 docs/mocks/stage2/build.py <editor web dir> <out dir>
"""
import colorsys
import re
import shutil
import sys
from pathlib import Path

WEB, OUT = Path(sys.argv[1]), Path(sys.argv[2])
PALETTE = sys.argv[3] if len(sys.argv) > 3 else 'zinc'
# Ectodeck's own greys, Tailwind's neutral scale, as its app draws them
NEUTRAL = ['#0a0a0a', '#171717', '#1c1c1c', '#1f1f1f', '#262626', '#2e2e2e', '#333333', '#404040', '#4a4a4a',
           '#525252', '#5c5c5c', '#737373', '#8a8a8a', '#a3a3a3', '#d4d4d4', '#e5e5e5', '#f5f5f5']
ZINC = ['#09090b', '#0a0a0a', '#111113', '#17171a', '#1c1c20', '#222226', '#27272a', '#2e2e33', '#303036',
        '#3a3a42', '#3f3f46', '#52525b', '#71717a', '#8a8a93', '#a1a1aa', '#d4d4d8', '#e4e4e7', '#fafafa']


def rgb(h):
    h = h.lstrip('#')
    if len(h) == 3:
        h = ''.join(c * 2 for c in h)
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def lum(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def remap(m):
    h = m.group(0)
    c = rgb(h)
    if c[0] == c[1] == c[2]:        # pure greys are the keycaps' own shading: leave them exactly
        return h
    _, l, s = colorsys.rgb_to_hls(*(v / 255 for v in c))
    if s > 0.28 and not (c[2] > c[0] and s < 0.5):   # a real colour (reds, amber, green): keep it
        return h
    return min(NEUTRAL if PALETTE == 'neutral' else ZINC, key=lambda z: abs(lum(rgb(z)) - lum(c)))


css = (WEB / 'style.css').read_text()
css = re.sub(r'#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b', remap, css)
tokens = {
    '--base': '#0a0a0a', '--panel': '#17171a', '--line': '#222226', '--ink': '#fafafa',
    '--quiet': '#a1a1aa', '--focus': '#c8f22e', '--good': '#c8f22e',
} if PALETTE == 'zinc' else {
    '--base': '#1c1c1c', '--panel': '#262626', '--line': '#404040', '--ink': '#f5f5f5',
    '--quiet': '#a3a3a3', '--focus': '#3b82f6', '--good': '#c8f22e',
}
for k, v in tokens.items():
    css = re.sub(rf'{k}:\s*[^;]+;', f'{k}: {v};', css, count=1)
css = css.replace("font: 15px/1.45 'Barlow', system-ui, sans-serif;", "font: 14px/1.45 'Geist', system-ui, sans-serif;")
if PALETTE == 'zinc':
    css = css.replace('rgba(96, 150, 240, .9)', 'rgba(200, 242, 46, .85)')  # the modified-key box, in lime
css = '''@font-face { font-family: 'Geist'; src: url('Geist-Variable-latin.woff2') format('woff2'); font-weight: 100 900; }
@font-face { font-family: 'Geist Mono'; src: url('GeistMono-Variable-latin.woff2') format('woff2'); font-weight: 100 900; }
''' + css
(OUT / f'style-{PALETTE}.css').write_text(css)
for f in ('stage2.html', 'platform.html'):
    shutil.copy(Path(__file__).parent / f, OUT / f)
print('built', OUT / f'style-{PALETTE}.css')
