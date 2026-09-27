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
    return min(ZINC, key=lambda z: abs(lum(rgb(z)) - lum(c)))


css = (WEB / 'style.css').read_text()
css = re.sub(r'#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b', remap, css)
tokens = {
    '--base': '#0a0a0a', '--panel': '#17171a', '--line': '#222226', '--ink': '#fafafa',
    '--quiet': '#a1a1aa', '--focus': '#c8f22e', '--good': '#c8f22e',
}
for k, v in tokens.items():
    css = re.sub(rf'{k}:\s*[^;]+;', f'{k}: {v};', css, count=1)
css = css.replace("font: 15px/1.45 'Barlow', system-ui, sans-serif;", "font: 14px/1.45 'Geist', system-ui, sans-serif;")
css = css.replace('rgba(96, 150, 240, .9)', 'rgba(200, 242, 46, .85)')      # the modified-key box, in lime
css = '''@font-face { font-family: 'Geist'; src: url('Geist-Variable-latin.woff2') format('woff2'); font-weight: 100 900; }
@font-face { font-family: 'Geist Mono'; src: url('GeistMono-Variable-latin.woff2') format('woff2'); font-weight: 100 900; }
''' + css
(OUT / 'style-zinc.css').write_text(css)
for f in ('stage2.html',):
    shutil.copy(Path(__file__).parent / f, OUT / f)
print('built', OUT / 'style-zinc.css')
