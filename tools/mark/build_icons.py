#!/usr/bin/env python3
"""Builds Ectomatter's mark files from one drawing: the family splat with a ring, centre
(32, 31.8), radius 9.8, the ring the name Ectomatter first carried (chosen for it on 2026-09-29,
and Ectomatter's again since 2026-10-22). The ring's line follows Ectopulse's Mark weights (4.4 at
96 px and up, 5 from 40, 6 from 20, 7 below 20), so its middle stays open at every size. After
Ectowork's brand/build_icons.py, which drew the same ring.

Writes:
  apps/ectomatter/ui/glyphs/mark.svg   the title bar's 16-pixel drawing (line 7), which the app
                                       renders for its title bar, Macro Editor, Screen Editor and labs
  design/mark.svg                      the same drawing, for the mocks' title bars
  apps/ectomatter/ui/icons/ectomatter.svg          the master, at the line icon files use (4.6)
  apps/ectomatter/ui/icons/ectomatter-N.png        each icon size at its own line; the tray takes
                                                   22, 32, 48 and 64, the desktop takes them all

With --install, also puts the icons where the desktop looks for com.ectonomic.ectomatter
(~/.local/share/icons/hicolor), for the taskbar and the window switcher.

Rendered by headless Chrome: every size laid out on one transparent page, then cut apart."""
import pathlib, shutil, subprocess, sys
from PIL import Image

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SPLAT = (HERE / 'splat.txt').read_text().strip()
INK = '#163d0a'
SIZES = [16, 22, 24, 32, 48, 64, 96, 128, 192, 256, 512]
APP_ID = 'com.ectonomic.ectomatter'


def weight(px):
    return 4.4 if px >= 96 else 5 if px >= 40 else 6 if px >= 20 else 7


def svg(stroke, px=None):
    size = f' width="{px}" height="{px}"' if px else ''
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"{size}>'
            f'<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="4" y1="4" x2="60" y2="60">'
            f'<stop offset="0" stop-color="#c8f22e"/><stop offset="1" stop-color="#9be012"/></linearGradient></defs>'
            f'<path d="{SPLAT}" fill="url(#g)"/>'
            f'<circle cx="32" cy="31.8" r="9.8" fill="none" stroke="{INK}" stroke-width="{stroke}"/></svg>')


icons = ROOT / 'apps/ectomatter/ui/icons'
icons.mkdir(parents=True, exist_ok=True)
small = svg(weight(16)) + '\n'
(ROOT / 'apps/ectomatter/ui/glyphs/mark.svg').write_text(small)
(ROOT / 'design/mark.svg').write_text(small)
(icons / 'ectomatter.svg').write_text(svg(4.6) + '\n')

x = 0
html = ''
for px in SIZES:
    html += f'<div style="position:absolute;left:{x}px;top:0;width:{px}px;height:{px}px">{svg(weight(px), px)}</div>'
    x += px + 20
work = HERE / '_render'
work.mkdir(exist_ok=True)
page = work / 'sheet.html'
page.write_text(f'<!doctype html><html><body style="margin:0;background:transparent">{html}</body></html>')
shot = work / 'sheet.png'
subprocess.run(['google-chrome', '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
                f'--user-data-dir={work / "chrome"}', '--default-background-color=00000000',
                f'--window-size={x + 20},{max(SIZES) + 20}', f'--screenshot={shot}', f'file://{page}'],
               capture_output=True, check=True)
sheet = Image.open(shot).convert('RGBA')
x = 0
for px in SIZES:
    sheet.crop((x, 0, x + px, px)).save(icons / f'ectomatter-{px}.png')
    x += px + 20
shutil.rmtree(work)

for f in sorted(icons.iterdir()):
    print(f.relative_to(ROOT), f.stat().st_size)

if '--install' in sys.argv:
    theme = pathlib.Path.home() / '.local/share/icons/hicolor'
    for px in SIZES:
        to = theme / f'{px}x{px}/apps'
        to.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(icons / f'ectomatter-{px}.png', to / f'{APP_ID}.png')
    (theme / 'scalable/apps').mkdir(parents=True, exist_ok=True)
    shutil.copyfile(icons / 'ectomatter.svg', theme / f'scalable/apps/{APP_ID}.svg')
    print(f'installed {len(SIZES)} sizes and the scalable icon as {APP_ID} in {theme}')
