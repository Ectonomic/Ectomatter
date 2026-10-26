//! The keyboard's screen drawn as it looks: its characters of eight by eight
//! dots that touch, as many as the keyboard's core describes (160 by 32 dots
//! on the Z10's twenty by four), in the keyboard's own font (data/z10font.json,
//! read from the firmware: eight column bytes a character, the top dot in
//! bit 7). Lit dots glow in the screen's light; unlit ones show faintly, as
//! the glass does. The approved mock's drawing (lcd.js), dot for dot.

use std::collections::HashMap;
use std::sync::OnceLock;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use crate::lcd_page::Size;

/// A character's dots across and down, as the keyboard's font draws it.
const CELL: usize = 8;
/// The ground between the dots and under them.
const BACK: [f32; 3] = [10.0, 9.0, 8.0];

fn font() -> &'static HashMap<char, [u8; 8]> {
    static FONT: OnceLock<HashMap<char, [u8; 8]>> = OnceLock::new();
    FONT.get_or_init(|| {
        let v: serde_json::Map<String, serde_json::Value> = serde_json::from_str(include_str!("../data/z10font.json")).unwrap_or_default();
        v.into_iter()
            .filter_map(|(k, g)| {
                let c = k.chars().next()?;
                let a = g.as_array()?;
                Some((c, std::array::from_fn(|i| a.get(i).and_then(|b| b.as_u64()).unwrap_or(0) as u8)))
            })
            .collect()
    })
}

/// A screen of `size` in dots, across and down.
fn dots_of((cols, rows): Size) -> (usize, usize) {
    (cols * CELL, rows * CELL)
}

/// Which of a screen's dots the rows light, row by row (see [dots_of]), as the keyboard draws
/// them: the degree sign is the font's backtick, a full block lights its whole cell, anything
/// else it has no glyph for stays dark, and what lies past the screen's edges isn't drawn.
pub fn dots(rows: &[String], size: Size) -> Vec<bool> {
    let (cols, n) = size;
    let (pw, ph) = dots_of(size);
    let mut lit = vec![false; pw * ph];
    for r in 0..n {
        let t: Vec<char> = rows.get(r).map_or("", |s| s.as_str()).chars().take(cols).collect();
        for c in 0..cols {
            let ch = t.get(c).copied().unwrap_or(' ');
            if ch == '█' {
                for y in 0..CELL {
                    for x in 0..CELL {
                        lit[(r * CELL + y) * pw + c * CELL + x] = true;
                    }
                }
                continue;
            }
            let ch = if ch == '°' { '`' } else { ch };
            let Some(g) = font().get(&ch) else { continue };
            for x in 0..CELL {
                for y in 0..CELL {
                    if (g[x] >> (7 - y)) & 1 == 1 {
                        lit[(r * CELL + y) * pw + c * CELL + x] = true;
                    }
                }
            }
        }
    }
    lit
}

fn mix(rgb: [u8; 3], f: f32) -> [f32; 3] {
    rgb.map(|v| (v as f32 * f).round().min(255.0))
}

/// Where a screen of `size` lies in a picture `w` by `h` pixels: a dot's pitch across and
/// down, and the room left of the dots and above them. The dots fill a picture of the screen's
/// own shape (to within the rounding of its pixels, either way), as every picture of the Z10's
/// is; in a picture of another shape they sit in its middle, square and as large as fit. None
/// for no screen.
fn placed(size: Size, w: usize, h: usize) -> Option<(f32, f32, f32, f32)> {
    let (pw, ph) = dots_of(size);
    if pw == 0 || ph == 0 {
        return None;
    }
    let (sx, sy) = (w as f32 / pw as f32, h as f32 / ph as f32);
    if (w * ph).abs_diff(h * pw) * 2 <= pw + ph {
        return Some((sx, sy, 0.0, 0.0));
    }
    let s = sx.min(sy);
    Some((s, s, (w as f32 - pw as f32 * s) / 2.0, (h as f32 - ph as f32 * s) / 2.0))
}

/// How much of its cell a dot fills at a pitch of `p` pixels: less where the dots are large
/// enough to show the gap between them.
fn fill_at(p: f32) -> f32 {
    if p >= 3.0 { 0.82 } else { 0.9 }
}

/// Each pixel along one side: the dots it covers, and how much of it each covers, the first
/// dot `x0` pixels in.
fn spans(n_px: usize, n_dots: usize, p: f32, fill: f32, x0: f32) -> Vec<[(usize, f32); 2]> {
    (0..n_px)
        .map(|px| {
            let (a, b) = (px as f32, px as f32 + 1.0);
            let mut out = [(usize::MAX, 0.0f32); 2];
            let at = (a - x0) / p;
            if at < -1.0 {
                return out;
            }
            let first = at.floor();
            for (k, d) in [first, first + 1.0].into_iter().enumerate() {
                if d < 0.0 || d as usize >= n_dots {
                    continue;
                }
                let d = d as usize;
                let (s, e) = (x0 + d as f32 * p, x0 + d as f32 * p + p * fill);
                let cover = (b.min(e) - a.max(s)).max(0.0);
                out[k] = (d, cover);
            }
            out
        })
        .collect()
}

/// How much of each pixel along one side lies over the screen, which starts `x0` pixels in
/// and runs `len`.
fn over(n_px: usize, x0: f32, len: f32) -> Vec<f32> {
    (0..n_px).map(|px| (((px + 1) as f32).min(x0 + len) - (px as f32).max(x0)).clamp(0.0, 1.0)).collect()
}

/// The screen's own background, between and under its unlit dots, as one color: what a
/// picture `w` by `h` pixels of the keyboard's screen averages to where nothing is lit, so a
/// surface in it meets the picture without an edge.
pub fn field(rgb: [u8; 3], level: f32, w: u32, h: u32) -> slint::Color {
    field_on(crate::lcd_page::size(), rgb, level, w, h)
}

/// [field], for a picture of a screen of `size`.
pub fn field_on(size: Size, rgb: [u8; 3], level: f32, w: u32, h: u32) -> slint::Color {
    let c = ground(size, rgb, level, w.max(1) as usize, h.max(1) as usize).map(|v| v.round() as u8);
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

/// [field]'s color, unrounded: the ground, and over it the share of each cell an unlit dot
/// takes (none with no screen).
fn ground(size: Size, rgb: [u8; 3], level: f32, w: usize, h: usize) -> [f32; 3] {
    let Some((p, ..)) = placed(size, w, h) else { return BACK };
    let fill = fill_at(p);
    let cov = fill * fill;
    let unlit = mix(rgb, 0.075 * (0.4 + level * 0.6));
    [0, 1, 2].map(|k| BACK[k] * (1.0 - cov) + unlit[k] * cov)
}

/// The keyboard's screen showing `rows`, lit in `rgb` at `level` (0 to 1), `w` by `h` pixels.
pub fn picture(rows: &[String], rgb: [u8; 3], level: f32, w: u32, h: u32) -> Image {
    picture_on(rows, crate::lcd_page::size(), rgb, level, w, h)
}

/// [picture], for a screen of `size`: the dots placed in the picture as [placed] says, and
/// round them, in a picture of another shape, the screen's own background ([field]). With no
/// screen, that background alone.
pub fn picture_on(rows: &[String], size: Size, rgb: [u8; 3], level: f32, w: u32, h: u32) -> Image {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    let Some((px, py, x0, y0)) = placed(size, w, h) else {
        let c = ground(size, rgb, level, w, h).map(|v| v.round() as u8);
        buf.make_mut_slice().fill(Rgba8Pixel { r: c[0], g: c[1], b: c[2], a: 255 });
        return Image::from_rgba8(buf);
    };
    let (pw, ph) = dots_of(size);
    let lit = dots(rows, size);
    let fill = fill_at(px);
    let (xs, ys) = (spans(w, pw, px, fill, x0), spans(h, ph, py, fill, y0));
    // (in a picture of another shape, how much of each pixel the screen takes, and round it its background)
    let framed = (x0, y0) != (0.0, 0.0);
    let (in_x, in_y) = (over(w, x0, pw as f32 * px), over(h, y0, ph as f32 * py));
    let around = ground(size, rgb, level, w, h);
    let back = BACK;
    let unlit = mix(rgb, 0.075 * (0.4 + level * 0.6));
    let on = mix(rgb, 0.25 + 0.75 * level);
    // how much of each pixel is lit dot, and how much unlit dot
    let mut lit_cov = vec![0f32; w * h];
    let mut dark_cov = vec![0f32; w * h];
    for (y, sy) in ys.iter().enumerate() {
        for (x, sx) in xs.iter().enumerate() {
            let (mut l, mut d) = (0.0, 0.0);
            for &(dy, cy) in sy {
                if dy == usize::MAX || cy == 0.0 {
                    continue;
                }
                for &(dx, cx) in sx {
                    if dx == usize::MAX || cx == 0.0 {
                        continue;
                    }
                    if lit[dy * pw + dx] { l += cx * cy } else { d += cx * cy }
                }
            }
            lit_cov[y * w + x] = l.min(1.0);
            dark_cov[y * w + x] = d.min(1.0);
        }
    }
    // the lit dots' glow, large enough to show: a blur of them (a canvas's
    // shadow blur is twice its Gaussian's sigma)
    let glow = (px >= 3.0).then(|| blur(&lit_cov, w, h, px * 0.9 / 2.0));
    let glow_rgb = mix(rgb, 0.55 * level);
    for (i, out) in buf.make_mut_slice().iter_mut().enumerate() {
        let mut c = back;
        // (round the screen, its background)
        if framed {
            let a = in_x[i % w] * in_y[i / w];
            for k in 0..3 {
                c[k] = around[k] * (1.0 - a) + back[k] * a;
            }
        }
        let d = dark_cov[i];
        for k in 0..3 {
            c[k] = c[k] * (1.0 - d) + unlit[k] * d;
        }
        if let Some(g) = &glow {
            let a = g[i].min(1.0);
            for k in 0..3 {
                c[k] = c[k] * (1.0 - a) + glow_rgb[k] * a;
            }
        }
        let l = lit_cov[i];
        for k in 0..3 {
            c[k] = c[k] * (1.0 - l) + on[k] * l;
        }
        *out = Rgba8Pixel { r: c[0].round() as u8, g: c[1].round() as u8, b: c[2].round() as u8, a: 255 };
    }
    Image::from_rgba8(buf)
}

fn blur(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let k = (sigma * 3.0).ceil().max(1.0) as i32;
    let weights: Vec<f32> = (-k..=k).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = weights.iter().sum();
    let pass = |s: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0f32; w * h];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let mut a = 0.0;
                for (j, wt) in weights.iter().enumerate() {
                    let d = j as i32 - k;
                    let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                    if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                        a += wt * s[sy as usize * w + sx as usize];
                    }
                }
                out[y as usize * w + x as usize] = a / sum;
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z10's screen.
    const Z10: Size = (20, 4);

    #[test]
    fn letters_light_the_firmwares_dots() {
        // "I" in the keyboard's font: one column of six dots, rows 1 to 6
        let lit = dots(&["I".into()], Z10);
        let n = lit.iter().filter(|&&b| b).count();
        assert!(n >= 6 && n <= 12, "{n}");
        // the degree sign is drawn, a character outside the font is dark
        assert!(dots(&["°".into()], Z10).iter().any(|&b| b));
        assert!(!dots(&["€".into()], Z10).iter().any(|&b| b));
        // column 7 of every cell stays dark in text
        let all = dots(&["ABCDEFGHIJKLMNOPQRST".into()], Z10);
        assert!((0..8).all(|y| (0..20).all(|c| !all[y * 160 + c * 8 + 7])));
        // the dots of a screen of another size: its cells, and nothing past them
        assert_eq!(dots(&[], (16, 2)).len(), 128 * 16);
        let wide = dots(&["ABCDEFGHIJKLMNOPQRSTUVWXYZ".into(), "Z".into(), "past the foot".into()], (16, 2));
        assert_eq!(wide, dots(&["ABCDEFGHIJKLMNOP".into(), "Z".into()], (16, 2)));
        assert!(dots(&["I".into()], (0, 0)).is_empty());
    }

    #[test]
    fn a_screen_fills_a_picture_of_its_shape_and_sits_in_the_middle_of_another() {
        // the Z10's in its cards' pictures and the tiles', their pixels rounded: filling them
        assert_eq!(placed(Z10, 260, 52), Some((1.625, 1.625, 0.0, 0.0)));
        assert_eq!(placed(Z10, 113, 23), Some((113.0 / 160.0, 23.0 / 32.0, 0.0, 0.0)));
        // two rows of sixteen in the same card: as wide as the picture, in its middle, square
        let (px, py, x0, y0) = placed((16, 2), 260, 52).unwrap();
        assert_eq!((px, py, x0, y0), (260.0 / 128.0, 260.0 / 128.0, 0.0, 9.75));
        // forty by four: as wide; twenty by eight: as tall
        assert_eq!(placed((40, 4), 260, 52).map(|p| p.3), Some((52.0 - 32.0 * 260.0 / 320.0) / 2.0));
        assert_eq!(placed((20, 8), 260, 52).map(|p| (p.0, p.2)), Some((52.0 / 64.0, (260.0 - 160.0 * 52.0 / 64.0) / 2.0)));
        assert_eq!(placed((0, 0), 260, 52), None);
        // the picture: round the screen its background, the field, so the bay meets it without an
        // edge; the letters lit inside it
        let rgb = [255, 32, 0];
        let pixel = |img: &Image, x: usize, y: usize| {
            let b = img.to_rgba8().unwrap();
            let p = b.as_slice()[y * b.width() as usize + x];
            [p.r, p.g, p.b]
        };
        let field = field_on((16, 2), rgb, 1.0, 260, 52);
        let img = picture_on(&["████████████████".into(), "████████████████".into()], (16, 2), rgb, 1.0, 260, 52);
        assert_eq!(pixel(&img, 130, 3), [field.red(), field.green(), field.blue()], "above the screen");
        assert_eq!(pixel(&img, 130, 48), [field.red(), field.green(), field.blue()], "below it");
        let on = mix(rgb, 1.0);
        assert_eq!(pixel(&img, 130, 26), on.map(|v| v as u8), "a lit cell in it");
        // no screen: its background alone
        let none = picture_on(&["ABC".into()], (0, 0), rgb, 1.0, 40, 10);
        assert!((0..40).all(|x| (0..10).all(|y| pixel(&none, x, y) == [10, 9, 8])));
    }
}
