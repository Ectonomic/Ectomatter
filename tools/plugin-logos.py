"""Each plugin's logo for the top of its README: its icon (the manifest's
`glyph`, drawn in `currentColor`, which the app colors), copied into
pictures/ in the plugin's own repository in the color of GitHub's text, one
for its light theme and one for its dark, as the README's <picture> shows
them (`every_plugin_shows_its_icon_in_its_readme` holds them to the icon).

  plugin-logos.py      writes plugins/<name>/pictures/logo-light.svg and
                       logo-dark.svg for each plugin linked here
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# (GitHub's text, on its light theme and on its dark one)
INKS = {'light': '#1f2328', 'dark': '#f0f6fc'}


def glyph(folder):
    name = re.search(r'"glyph"\s*:\s*"([^"]+)"', (folder / 'core.json').read_text())
    return folder / name.group(1) if name else None


def main():
    for folder in sorted((ROOT / 'plugins').iterdir()):
        if not (folder / 'core.json').is_file():
            continue
        icon = glyph(folder)
        if icon is None or not icon.is_file():
            sys.exit(f'{folder.name}: its manifest names no icon there is')
        (folder / 'pictures').mkdir(exist_ok=True)
        for theme, ink in INKS.items():
            out = folder / 'pictures' / f'logo-{theme}.svg'
            out.write_text(icon.read_text().replace('currentColor', ink))
            print(out.relative_to(ROOT))


if __name__ == '__main__':
    main()
