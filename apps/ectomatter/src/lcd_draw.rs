//! The Z10's screen drawn as it looks: 160 by 32 dots, twenty characters of
//! eight by eight that touch, in the keyboard's own font (data/z10font.json,
//! read from the firmware: eight column bytes a character, the top dot in
//! bit 7). Lit dots glow in the screen's light; unlit ones show faintly, as
//! the glass does. The approved mock's drawing (lcd.js), dot for dot.

use std::collections::HashMap;
use std::sync::OnceLock;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const PW: usize = 160;
const PH: usize = 32;

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

/// Which of the 160 by 32 dots the rows light, as the keyboard draws them:
/// the degree sign is the font's backtick, a full block lights its whole
/// cell, anything else it has no glyph for stays dark.
pub fn dots(rows: &[String]) -> Vec<bool> {
    let mut lit = vec![false; PW * PH];
    for r in 0..4 {
        let t: Vec<char> = rows.get(r).map_or("", |s| s.as_str()).chars().take(20).collect();
        for c in 0..20 {
            let ch = t.get(c).copied().unwrap_or(' ');
            if ch == '█' {
                for y in 0..8 {
                    for x in 0..8 {
                        lit[(r * 8 + y) * PW + c * 8 + x] = true;
                    }
                }
                continue;
            }
            let ch = if ch == '°' { '`' } else { ch };
            let Some(g) = font().get(&ch) else { continue };
            for x in 0..8 {
                for y in 0..8 {
                    if (g[x] >> (7 - y)) & 1 == 1 {
                        lit[(r * 8 + y) * PW + c * 8 + x] = true;
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

/// Each pixel along one side: the dots it covers, and how much of it each covers.
fn spans(n_px: usize, n_dots: usize, p: f32, fill: f32) -> Vec<[(usize, f32); 2]> {
    (0..n_px)
        .map(|px| {
            let (a, b) = (px as f32, px as f32 + 1.0);
            let mut out = [(usize::MAX, 0.0f32); 2];
            let first = (a / p).floor() as usize;
            for (k, d) in [first, first + 1].into_iter().enumerate() {
                if d >= n_dots {
                    continue;
                }
                let (s, e) = (d as f32 * p, d as f32 * p + p * fill);
                let cover = (b.min(e) - a.max(s)).max(0.0);
                out[k] = (d, cover);
            }
            out
        })
        .collect()
}

/// The screen's own background, between and under its unlit dots, as one
/// color: what a picture `w` pixels across averages to where nothing is lit,
/// so a surface in it meets the picture without an edge.
pub fn field(rgb: [u8; 3], level: f32, w: u32) -> slint::Color {
    let p = w.max(1) as f32 / PW as f32;
    let fill = if p >= 3.0 { 0.82 } else { 0.9 };
    let cov = fill * fill;
    let back = [10.0f32, 9.0, 8.0];
    let unlit = mix(rgb, 0.075 * (0.4 + level * 0.6));
    let c = [0, 1, 2].map(|k| (back[k] * (1.0 - cov) + unlit[k] * cov).round() as u8);
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

/// The screen showing `rows`, lit in `rgb` at `level` (0 to 1), `w` by `h` pixels.
pub fn picture(rows: &[String], rgb: [u8; 3], level: f32, w: u32, h: u32) -> Image {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let lit = dots(rows);
    let p = w as f32 / PW as f32;
    let fill = if p >= 3.0 { 0.82 } else { 0.9 };
    let (xs, ys) = (spans(w, PW, p, fill), spans(h, PH, h as f32 / PH as f32, fill));
    let back = [10.0f32, 9.0, 8.0];
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
                    if lit[dy * PW + dx] { l += cx * cy } else { d += cx * cy }
                }
            }
            lit_cov[y * w + x] = l.min(1.0);
            dark_cov[y * w + x] = d.min(1.0);
        }
    }
    // the lit dots' glow, large enough to show: a blur of them (a canvas's
    // shadow blur is twice its Gaussian's sigma)
    let glow = (p >= 3.0).then(|| blur(&lit_cov, w, h, p * 0.9 / 2.0));
    let glow_rgb = mix(rgb, 0.55 * level);
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let mut c = back;
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
        *px = Rgba8Pixel { r: c[0].round() as u8, g: c[1].round() as u8, b: c[2].round() as u8, a: 255 };
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

    #[test]
    fn letters_light_the_firmwares_dots() {
        // "I" in the keyboard's font: one column of six dots, rows 1 to 6
        let lit = dots(&["I".into()]);
        let n = lit.iter().filter(|&&b| b).count();
        assert!(n >= 6 && n <= 12, "{n}");
        // the degree sign is drawn, a character outside the font is dark
        assert!(dots(&["°".into()]).iter().any(|&b| b));
        assert!(!dots(&["€".into()]).iter().any(|&b| b));
        // column 7 of every cell stays dark in text
        let all = dots(&["ABCDEFGHIJKLMNOPQRST".into()]);
        assert!((0..8).all(|y| (0..20).all(|c| !all[y * PW + c * 8 + 7])));
    }
}
