# Ectodeck's fonts for a keyboard's glyphs

The glyphs a key's legend can have that its typeface lacks: ⌘ ⌥ ⇧ ⎋ ⏻ and the rest of the Unicode blocks they come from (arrows, mathematical operators, technical signs, control pictures, shapes, symbols, and the supplemental arrows). Ectodeck builds them in, so a typed legend draws the same, and the Light lab's glyph list still shows every glyph, on a computer that has none of the fonts they were cut from. The legends reach them after a core's own symbols font, and the lab's text after the computer's own fonts.

- `KeyGlyphs-Bold.ttf`: Ectodeck Key Glyphs, the glyphs of DejaVu Sans Bold 2.37, 777 of them, under the Bitstream Vera and Arev licences (`DejaVu-LICENSE.txt`).
- `KeyGlyphsSymbols-Regular.ttf`: Ectodeck Key Glyphs Symbols, the glyphs of Noto Sans Symbols 2.003, at its Regular weight, 251 of them, under the SIL Open Font License 1.1 (`Noto-OFL.txt`).
- `KeyGlyphsSymbols2-Regular.ttf`: Ectodeck Key Glyphs Symbols 2, the glyphs of Noto Sans Symbols 2 2.008, 531 of them, under the SIL Open Font License 1.1 (`Noto-OFL.txt`).

Each is cut from its font and renamed, as DejaVu's licence asks of a changed font and as Google's trademark on Noto asks. Built by `tools/glyph_fonts.py`.
