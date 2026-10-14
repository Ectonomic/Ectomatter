"""Ectodeck's own fonts for a keyboard's glyphs: ⌘ ⌥ ⇧ ⎋ ⏻ and the rest of
the Unicode blocks they come from. They are cut from the fonts the Key lab's
glyph list was made with, so a key's typed legend draws the same, and the list
still shows every glyph, on a computer that has none of those fonts.

  glyph_fonts.py      writes crates/ectodeck-draw/fonts/: the three fonts,
                      their licenses and a README

Each font is renamed: DejaVu's license asks it of a changed font, and Noto is
Google's trademark. Ectodeck reaches them after the computer's own fonts, so
where those have a glyph, they still draw it.
"""
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'crates/ectodeck-draw/fonts'
DEJAVU_LICENSE = Path('/usr/share/licenses/dejavu-sans-fonts/LICENSE')
# the OFL's own words, after the Barlow copyright line the core's copy starts with
OFL = ROOT / 'cores/evga-z10/fonts/OFL.txt'
NOTO_COPYRIGHT = 'Copyright 2022 The Noto Project Authors (https://github.com/notofonts/symbols)'

# The blocks a keyboard's glyphs come from: arrows, mathematical operators,
# technical signs (⌘ ⌥ ⎋ ⏏ ⏻), control pictures (␣), shapes (▲ ◀), symbols
# (☀), and the supplemental arrows; the speakers of the volume keys
# (🔇 🔈 🔉 🔊, and 🕨 🕩 🕪); heavy arrows with wide tips (🠜 🠝 🠞 🠟, and the
# heavy up arrow some Shift keys print, 🡅); and the broken bar (¦).
BLOCKS = [(0x2190, 0x21FF), (0x2200, 0x22FF), (0x2300, 0x23FF), (0x2400, 0x243F), (0x25A0, 0x25FF),
          (0x2600, 0x26FF), (0x27F0, 0x27FF), (0x2900, 0x297F), (0x2B00, 0x2BFF), (0x1F507, 0x1F50A), (0x1F568, 0x1F56A), (0x1F81C, 0x1F81F), (0x1F845, 0x1F845), (0xA6, 0xA6)]

# (file, family, style, PostScript name, source, its name and version, license)
FONTS = [
    ('KeyGlyphs-Bold.ttf', 'Ectodeck Key Glyphs', 'Bold', 'EctodeckKeyGlyphs-Bold',
     Path('/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf'), 'DejaVu Sans Bold 2.37', 'DejaVu-LICENSE.txt'),
    ('KeyGlyphsSymbols-Regular.ttf', 'Ectodeck Key Glyphs Symbols', 'Regular', 'EctodeckKeyGlyphsSymbols-Regular',
     Path('/usr/share/fonts/google-noto-vf/NotoSansSymbols[wght].ttf'), 'Noto Sans Symbols 2.003, at its Regular weight', 'Noto-OFL.txt'),
    ('KeyGlyphsSymbols2-Regular.ttf', 'Ectodeck Key Glyphs Symbols 2', 'Regular', 'EctodeckKeyGlyphsSymbols2-Regular',
     Path('/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf'), 'Noto Sans Symbols 2 2.008', 'Noto-OFL.txt'),
]


def cut(file, family, style, ps, source, origin):
    f = TTFont(source)
    if 'fvar' in f:
        # its default instance, the one a computer's copy draws when no weight is asked
        f = instancer.instantiateVariableFont(f, {a.axisTag: a.defaultValue for a in f['fvar'].axes})
        # its style names now name nothing: one style is left
        del f['STAT']
    opts = subset.Options()
    opts.name_IDs = ['*']
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=[u for lo, hi in BLOCKS for u in range(lo, hi + 1)])
    s.subset(f)
    full = family if style == 'Regular' else f'{family} {style}'
    names = {1: family, 2: style, 3: f'Ectodeck: {full}', 4: full, 6: ps, 10: f'The keyboard glyphs of {origin}, cut for Ectodeck.', 16: family, 17: style}
    table = f['name']
    table.names = [r for r in table.names if r.nameID < 256 and r.nameID not in (21, 22, 25) and (r.nameID not in (16, 17) or r.nameID in names)]
    for i, text in names.items():
        if i in (16, 17) and not any(r.nameID == i for r in table.names):
            continue
        table.removeNames(nameID=i)
        table.setName(text, i, 3, 1, 0x409)
    # (the source's dates kept, so a rebuild writes the same file)
    f.recalcTimestamp = False
    f.save(OUT / file)
    return len(f.getBestCmap())


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    counts = {file: cut(file, family, style, ps, source, origin) for file, family, style, ps, source, origin, _ in FONTS}
    (OUT / 'DejaVu-LICENSE.txt').write_bytes(DEJAVU_LICENSE.read_bytes())
    ofl = OFL.read_text().split('\n')
    body = '\n'.join(ofl[next(i for i, l in enumerate(ofl) if 'This Font Software is licensed' in l):])
    (OUT / 'Noto-OFL.txt').write_text(NOTO_COPYRIGHT + '\n\n' + body)
    lines = [f'- `{file}`: {family}, the glyphs of {origin}, {counts[file]} of them, under {"the Bitstream Vera and Arev licenses" if lic.startswith("DejaVu") else "the SIL Open Font License 1.1"} (`{lic}`).'
             for file, family, style, ps, source, origin, lic in FONTS]
    (OUT / 'README.md').write_text(
        '# Ectodeck\'s fonts for a keyboard\'s glyphs\n\n'
        'The glyphs a key\'s legend can have that its typeface lacks: ⌘ ⌥ ⇧ ⎋ ⏻ and the rest of the Unicode blocks they come from '
        '(arrows, mathematical operators, technical signs, control pictures, shapes, symbols, and the supplemental arrows), the volume keys\' speakers, and heavy arrows with wide tips. '
        'Ectodeck builds them in, so a typed legend draws the same, and the Key lab\'s glyph list still shows every glyph, on a computer that has none of the fonts they were cut from. '
        'The legends reach them after a core\'s own symbols font, and the lab\'s text after the computer\'s own fonts.\n\n'
        + '\n'.join(lines) + '\n\n'
        'Each is cut from its font and renamed, as DejaVu\'s license asks of a changed font and as Google\'s trademark on Noto asks. '
        'Built by `tools/glyph_fonts.py`.\n')
    print('fonts written:', counts)


if __name__ == '__main__':
    main()
