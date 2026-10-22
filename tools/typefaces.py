"""Ectomatter's own typefaces for legends: the faces keyboards are printed in,
each under a license that lets Ectomatter carry it (SIL Open Font License or
MIT), so a setup looks the same on any computer.

  typefaces.py SOURCES    writes crates/ectomatter-draw/typefaces/ from the
                          font files in SOURCES (as downloaded from their
                          projects, named below), with their licenses and a README

A font is cut to the letters keyboards print (Latin, Greek and Cyrillic,
punctuation, currency, arrows; for the Japanese and Korean faces, the kana,
the jamo and the words on those keyboards' keys), a variable font keeping
every weight and only its default on any other axis; a font whose license
reserves its name (Audiowide, Orbitron) is carried as it comes.
"""
import io
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'crates/ectomatter-draw/typefaces'

# (file written, source file, cut to these letters or None to carry it as it comes, look, project, license)
LATIN = [(0x20, 0x52F), (0x1E00, 0x1EFF), (0x2000, 0x206F), (0x20A0, 0x20CF), (0x2100, 0x21FF)]
# the kana, and the words on a Japanese keyboard's keys
JAPANESE = [(0x20, 0x7E), (0xA5, 0xA5), (0x3000, 0x30FF), (0xFF01, 0xFF5E)] + [(ord(c), ord(c)) for c in '半角全漢字変換無英数']
# the jamo, and the words on a Korean keyboard's keys
KOREAN = [(0x20, 0x7E), (0x20A9, 0x20A9), (0x3130, 0x318F)] + [(ord(c), ord(c)) for c in '한영자']
FACES = [
    ('BrunoAce-Regular.ttf', 'BrunoAce-Regular.ttf', None, 'wide and technical', 'https://github.com/googlefonts/Bruno-ace', 'OFL-1.1'),
    ('Audiowide-Regular.ttf', 'Audiowide-Regular.ttf', None, 'rounded and technical, a slashed zero', 'https://github.com/googlefonts/audiowide', 'OFL-1.1, Reserved Font Name "Audiowide"'),
    ('Orbitron.ttf', 'Orbitron[wght].ttf', None, 'geometric and square, the EVGA Z10\'s own look', 'https://github.com/theleagueof/orbitron', 'OFL-1.1, Reserved Font Name "Orbitron"'),
    ('Michroma-Regular.ttf', 'Michroma-Regular.ttf', None, 'wide and squared, the wide Eurostile look', 'https://github.com/googlefonts/Michroma-font', 'OFL-1.1'),
    ('Saira.ttf', 'Saira[wdth,wght].ttf', LATIN, 'squared, the Eurostile look of gaming keyboards', 'https://github.com/Omnibus-Type/Saira', 'OFL-1.1'),
    ('Rajdhani-Medium.ttf', 'Rajdhani-Medium.ttf', LATIN, 'narrow and squared', 'https://github.com/itfoundry/rajdhani', 'OFL-1.1'),
    ('Rajdhani-SemiBold.ttf', 'Rajdhani-SemiBold.ttf', LATIN, 'narrow and squared', 'https://github.com/itfoundry/rajdhani', 'OFL-1.1'),
    ('Rajdhani-Bold.ttf', 'Rajdhani-Bold.ttf', LATIN, 'narrow and squared', 'https://github.com/itfoundry/rajdhani', 'OFL-1.1'),
    ('Barlow-Regular.ttf', 'Barlow-Regular.ttf', None, 'plain and upright, the DIN look', 'https://github.com/jpt/barlow', 'OFL-1.1'),
    ('Barlow-SemiBold.ttf', 'Barlow-SemiBold.ttf', None, 'plain and upright, the DIN look', 'https://github.com/jpt/barlow', 'OFL-1.1'),
    ('Arimo.ttf', 'arimo.ttf', LATIN, 'the Helvetica and Arial look most keyboards print', 'https://github.com/googlefonts/arimo', 'OFL-1.1'),
    ('Inter.ttf', 'Inter[opsz,wght].ttf', LATIN, 'the clean modern look of laptop keyboards', 'https://github.com/rsms/inter', 'OFL-1.1'),
    ('Nunito.ttf', 'Nunito[wght].ttf', LATIN, 'soft and rounded, the VAG Rounded look of older Mac keyboards', 'https://github.com/googlefonts/nunito', 'OFL-1.1'),
    ('Jost.ttf', 'Jost[wght].ttf', LATIN, 'round and geometric, the Futura look', 'https://github.com/indestructible-type/Jost', 'OFL-1.1'),
    ('OpenGorton-Regular.otf', 'OpenGorton-Regular.otf', None, 'the engraved Gorton of classic keyboards', 'https://github.com/dakotafelder/open-gorton', 'MIT'),
    ('OpenGorton-Bold.otf', 'OpenGorton-Bold.otf', None, 'the engraved Gorton of classic keyboards', 'https://github.com/dakotafelder/open-gorton', 'MIT'),
    ('JetBrainsMono.ttf', 'JetBrainsMono[wght].ttf', LATIN, 'a programmer\'s monospace', 'https://github.com/JetBrains/JetBrainsMono', 'OFL-1.1'),
    ('CourierPrime-Regular.ttf', 'CourierPrime-Regular.ttf', LATIN, 'the typewriter look', 'https://github.com/quoteunquoteapps/CourierPrime', 'OFL-1.1'),
    ('CourierPrime-Bold.ttf', 'CourierPrime-Bold.ttf', LATIN, 'the typewriter look', 'https://github.com/quoteunquoteapps/CourierPrime', 'OFL-1.1'),
    ('NotoSansJP.ttf', 'NotoSansJP[wght].ttf', JAPANESE, 'Japanese keyboards\' kana and keys', 'https://github.com/notofonts/noto-cjk', 'OFL-1.1'),
    ('NotoSansKR.ttf', 'NotoSansKR[wght].ttf', KOREAN, 'Korean keyboards\' jamo and keys', 'https://github.com/notofonts/noto-cjk', 'OFL-1.1'),
]


def cut(src, keep, out):
    f = TTFont(src, lazy=False)
    if 'fvar' in f:
        # every weight; any other axis at its default
        pins = {a.axisTag: a.defaultValue for a in f['fvar'].axes if a.axisTag != 'wght'}
        if pins:
            instancer.instantiateVariableFont(f, pins, inplace=True, updateFontNames=False)
            # (read back, for the subsetter to see the font as it now is)
            buf = io.BytesIO()
            f.save(buf)
            buf.seek(0)
            f = TTFont(buf, lazy=False)
    opts = subset.Options()
    opts.name_IDs = ['*']
    opts.name_languages = ['*']
    opts.layout_features = ['*']
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=[u for lo, hi in keep for u in range(lo, hi + 1)])
    s.subset(f)
    f.save(out)


def main(sources):
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.iterdir():
        old.unlink()
    for file, src, keep, *_ in FACES:
        src = Path(sources) / src
        if keep is None:
            (OUT / file).write_bytes(src.read_bytes())
        else:
            cut(src, keep, OUT / file)
    # the OFL once, after every OFL font's own copyright
    ofl = [TTFont(OUT / file)['name'].getDebugName(0) for file, _, _, _, _, license in FACES if license.startswith('OFL')]
    body = (Path(sources) / 'OFL.txt').read_text()
    body = body[body.index('This Font Software is licensed'):]
    (OUT / 'OFL.txt').write_text('\n'.join(dict.fromkeys(ofl)) + '\n\n' + body)
    (OUT / 'OpenGorton-LICENSE.txt').write_bytes((Path(sources) / 'OpenGorton-LICENSE.txt').read_bytes())
    lines = [f'- `{file}`: {look}. {project}, {license}.' for file, _, _, look, project, license in FACES]
    (OUT / 'README.md').write_text(
        '# Ectomatter\'s typefaces for legends\n\n'
        'The faces keyboards are printed in, built into Ectomatter so a setup\'s legends look the same on any computer. '
        'Each is under a license that lets Ectomatter carry it: the SIL Open Font License 1.1 (`OFL.txt`, with each font\'s own copyright in the font) or MIT (`OpenGorton-LICENSE.txt`). '
        'Each is cut to the letters keyboards print, a variable font keeping all its weights; Audiowide and Orbitron, whose licenses reserve their names, are carried as they come.\n\n'
        + '\n'.join(lines) + '\n\nBuilt by `tools/typefaces.py`.\n')
    print('typefaces written:', sorted(p.name for p in OUT.iterdir()))


if __name__ == '__main__':
    main(sys.argv[1])
