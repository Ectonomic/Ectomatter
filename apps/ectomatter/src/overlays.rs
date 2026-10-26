//! What the page draws over the keyboard, from the Z10 core's art, as the web
//! app did: the two slider knobs, which follow the real sliders; the display,
//! drawn live as the keyboard's panel shows it; and the display's glass.

use std::collections::HashMap;

use ectomatter_draw::pieces;
use resvg::usvg::Tree;
use serde_json::Value;

use crate::Overlay;

/// the display: a cell of 8 x 8 pixels for each of the characters its core describes (see
/// lcd_page::size), each glyph 5 x 7 in columns 1 to 5
const CELL: usize = 8;

/// The display's dots across and down, for its characters as its core describes them.
fn dots() -> (usize, usize) {
    let (cols, rows) = crate::lcd_page::size();
    (cols * CELL, rows * CELL)
}

/// Where the real panel's dots sit in the screen's window: the room round
/// them, in dots, on the left, the top, the right and the bottom. They sit
/// low and to the left, not in the middle.
const GUTTER: [f32; 4] = [1.0, 3.0, 3.0, 1.0];

struct Knob {
    image: slint::Image,
    /// where the art has it, and its size, in the board's pixels
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// its slider's travel, in the drawing's units: where it starts, and how far the knob goes
    x0: f32,
    travel: f32,
}

pub struct Overlays {
    tree: Tree,
    view: (f32, f32),
    sliders: Vec<[f32; 4]>,
    glass_box: [f32; 4],
    font: HashMap<char, [u8; 5]>,
    /// the board these were drawn for, in the buffer's pixels, and its scale
    size: (u32, u32),
    scale: f32,
    knobs: Vec<Knob>,
    glass: Option<(slint::Image, [f32; 4])>,
    /// the display's panel, drawn into the HDR picture rather than the page:
    /// where it goes in the board's pixels, and its pixels in sRGB's encoding
    /// (0 to 1), which is lit as a backlight is; and a count of its drawings
    panel: Option<Panel>,
    pub panel_rev: u64,
    /// each slider's place along its travel, 0 to 1, once the keyboard says
    pub fracs: [Option<f32>; 2],
    /// what the display shows: whether something drives it, its rows, its backlight
    pub lcd: (bool, Vec<String>, [f64; 3]),
    /// what it was drawn for: those, the panel's size, its dots' share of their cells, and the
    /// display's own size in characters
    drawn_lcd: Option<(bool, Vec<String>, [f64; 3], (u32, u32), u32, (usize, usize))>,
    /// how strong the glass's sheen is over the display, 1 as the art draws it
    pub glass_opacity: f32,
}

/// The display's panel, for the HDR picture: where it goes in the board's
/// pixels, and, a pixel at a time, how much of it lit dots and unlit dots
/// cover (the rest is the ground between them), with the backlight's color
/// (sRGB, 0 to 1). With nothing driving it, it is dark: `dark` holds its
/// unlit colors in linear light, which only the room's light shows.
pub struct Panel {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub live: bool,
    pub on: Vec<f32>,
    pub off: Vec<f32>,
    pub backlight: [f32; 3],
    pub dark: Vec<[f32; 3]>,
}

fn image(p: &pieces::Piece) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(p.w, p.h);
    buf.make_mut_bytes().copy_from_slice(&p.rgba);
    slint::Image::from_rgba8_premultiplied(buf)
}

fn num(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

impl Overlays {
    pub fn new() -> Overlays {
        // (with no keyboard, an empty drawing: no board is ever laid)
        let tree = crate::cores::keyboard().map_or_else(|| resvg::usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg'/>", &resvg::usvg::Options::default()).unwrap(), |c| c.tree());
        let board: Value = serde_json::from_str(include_str!("../data/board.json")).unwrap();
        let v = &board["view"];
        let sliders = board["sliders"].as_array().map(|a| a.iter().map(|s| [num(&s[0]), num(&s[1]), num(&s[2]), num(&s[3])]).collect()).unwrap_or_default();
        let g = &board["glass"];
        let font: Value = serde_json::from_str(include_str!("../data/lcdfont.json")).unwrap();
        let font = font
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, g)| {
                        let c = k.chars().next()?;
                        let cols: Vec<u8> = g.as_array()?.iter().map(|b| b.as_u64().unwrap_or(0) as u8).collect();
                        Some((c, [cols.first().copied()?, *cols.get(1)?, *cols.get(2)?, *cols.get(3)?, *cols.get(4)?]))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Overlays {
            tree,
            view: (num(&v[0]), num(&v[1])),
            sliders,
            glass_box: [num(&g[0]), num(&g[1]), num(&g[2]), num(&g[3])],
            font,
            size: (0, 0),
            scale: 1.0,
            knobs: Vec::new(),
            glass: None,
            panel: None,
            panel_rev: 0,
            fracs: [None, None],
            lcd: (false, Vec::new(), [90.0; 3]),
            drawn_lcd: None,
            glass_opacity: 1.0,
        }
    }

    /// Draw the pieces for a board this size (the buffer's pixels) and scale;
    /// true if anything was drawn again.
    pub fn board(&mut self, w: u32, h: u32, scale: f32) -> bool {
        if (w, h) == self.size && scale == self.scale || w == 0 || h == 0 {
            return false;
        }
        self.size = (w, h);
        self.scale = scale;
        let unit = w as f32 / 649.0;
        self.knobs = ["control_117", "control_118"]
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let p = pieces::draw(&self.tree, &[id], w, h)?;
                let [sx0, _, sx1, _] = *self.sliders.get(i)?;
                let kw = p.w as f32 / unit;
                Some(Knob { image: image(&p), x: p.x as f32, y: p.y as f32, w: p.w as f32, h: p.h as f32, x0: sx0, travel: sx1 - sx0 - kw })
            })
            .collect();
        self.glass = pieces::draw(&self.tree, &["overlay_1"], w, h).map(|p| (image(&p), [p.x as f32, p.y as f32, p.w as f32, p.h as f32]));
        self.drawn_lcd = None;
        true
    }

    /// The display in the pixels of a board `bw` wide: the whole of the
    /// screen's window, from where the bezel ends. Its dots sit in it as the
    /// real panel's do (`GUTTER`), the display's own background round them.
    fn panel_box(&self, bw: u32) -> [f32; 4] {
        let unit = bw as f32 / 649.0;
        let [gx0, gy0, gx1, gy1] = self.glass_box;
        [(gx0 - self.view.0) * unit, (gy0 - self.view.1) * unit, (gx1 - gx0) * unit, (gy1 - gy0) * unit]
    }

    /// The dots' box inside the display, in the display's own pixels: the
    /// dots and the gutters either side of them fill its width, and the room
    /// above and below them is shared as the real panel's gutters share it
    /// (the window is a little taller than the gutters alone would make it).
    /// Dots too many rows for that keep their gutters above and below, and
    /// the room either side is shared as the gutters there share it. `dots`
    /// is how many the display has across and down (see [dots]).
    fn dots_box(&self, pw: u32, ph: u32, (dw, dh): (usize, usize)) -> [f32; 4] {
        let [l, t, r, b] = GUTTER;
        let pitch = (pw as f32 / (dw as f32 + l + r)).min(ph as f32 / (dh as f32 + t + b));
        let (w, h) = (dw as f32 * pitch, dh as f32 * pitch);
        let (room_x, room_y) = (pw as f32 - w, ph as f32 - h);
        [room_x * l / (l + r), room_y * t / (t + b), w, h]
    }

    /// The display's text as the keyboard draws it (lcd_page::spell).
    fn lcd_text(&self, s: &str) -> Vec<char> {
        crate::lcd_page::spell(s).chars().collect()
    }

    /// The panel drawn at its size in real pixels, as the web app drew it: a
    /// negative display, only the pixels that are on lit, each dot a square
    /// filling `dot` of its cell where cells are big enough to show a gap.
    fn draw_panel(&mut self, bw: u32, dot: f32) {
        let [_, _, w, h] = self.panel_box(bw);
        let (pw, ph) = ((w.round().max(1.0)) as u32, (h.round().max(1.0)) as u32);
        let key = (self.lcd.0, self.lcd.1.clone(), self.lcd.2, (pw, ph), (dot * 1000.0).round() as u32, crate::lcd_page::size());
        if self.drawn_lcd.as_ref() == Some(&key) && self.panel.is_some() {
            return;
        }
        let (live, rows, back) = (self.lcd.0, &self.lcd.1, self.lcd.2);
        let ((cols, lines), (dw, dh)) = (crate::lcd_page::size(), dots());
        let mut lit = vec![false; dw * dh];
        if live {
            for r in 0..lines {
                let mut text = self.lcd_text(rows.get(r).map_or("", |s| s.as_str()));
                text.resize(cols.max(text.len()), ' ');
                for (c, ch) in text.iter().take(cols).enumerate() {
                    if *ch == '\u{2588}' {
                        for y in 0..CELL {
                            for x in 0..CELL {
                                lit[(r * CELL + y) * dw + c * CELL + x] = true;
                            }
                        }
                        continue;
                    }
                    let Some(g) = self.font.get(ch) else { continue };
                    for (x, col) in g.iter().enumerate() {
                        for y in 0..7 {
                            if (col >> y) & 1 == 1 {
                                lit[(r * CELL + y) * dw + c * CELL + 1 + x] = true;
                            }
                        }
                    }
                }
            }
        }
        let [mx, my, mw, mh] = self.dots_box(pw, ph, (dw, dh));
        let (cx, cy) = (mw / dw as f32, mh / dh as f32);
        // a gap round each dot where the cells are big enough to show one
        let dot = if cx >= 2.5 { dot } else { 1.0 };
        let n = (pw * ph) as usize;
        let (mut on, mut off) = (vec![0f32; n], vec![0f32; n]);
        // each dot a square, its edges shared out by how much of each pixel they cover
        for y in 0..dh {
            for x in 0..dw {
                let into = if lit[y * dw + x] { &mut on } else { &mut off };
                let (x0, y0) = (mx + x as f32 * cx, my + y as f32 * cy);
                let (x1, y1) = (x0 + cx * dot, y0 + cy * dot);
                for py in (y0.floor() as u32)..(y1.ceil() as u32).min(ph) {
                    let oy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).max(0.0);
                    for qx in (x0.floor() as u32)..(x1.ceil() as u32).min(pw) {
                        let a = (x1.min(qx as f32 + 1.0) - x0.max(qx as f32)).max(0.0) * oy;
                        into[(py * pw + qx) as usize] += a;
                    }
                }
            }
        }
        // unlit, with nothing driving it: its own dark colors, which only the room's light shows
        let shade = |c: [f32; 3], d: [f32; 3], a: f32| [0, 1, 2].map(|i| (c[i] * (1.0 - a) + d[i] * a) / 255.0);
        let dark = if live {
            Vec::new()
        } else {
            // (in linear light, made once here rather than at every frame)
            (0..n).map(|i| shade([8.0, 10.0, 12.0], [18.0, 22.0, 26.0], off[i].min(1.0)).map(|v| v.max(0.0).powf(2.2))).collect()
        };
        let [x, y, _, _] = self.panel_box(bw);
        self.panel = Some(Panel {
            x: x.round() as i32,
            y: y.round() as i32,
            w: pw,
            h: ph,
            live,
            on,
            off,
            backlight: [back[0] as f32 / 255.0, back[1] as f32 / 255.0, back[2] as f32 / 255.0],
            dark,
        });
        self.panel_rev += 1;
        self.drawn_lcd = Some(key);
    }

    /// The display's panel for a keyboard picture `bw` pixels wide, for the
    /// HDR picture: drawn for that picture, which is stretched to the page's
    /// keyboard while a new size is drawn, so the display stays in its glass.
    pub fn panel_for(&mut self, bw: u32, dot: f32) -> Option<&Panel> {
        if bw == 0 {
            return None;
        }
        self.draw_panel(bw, dot);
        self.panel.as_ref()
    }

    /// What goes over the keyboard, in the page's logical pixels from the
    /// keyboard's top left: the knobs and the display's glass. The display
    /// itself is in the HDR picture, under the glass.
    pub fn model(&mut self) -> Vec<Overlay> {
        if self.size.0 == 0 {
            return Vec::new();
        }
        let s = self.scale;
        let unit = self.size.0 as f32 / 649.0;
        let mut out = Vec::new();
        // a knob: the start of its track and how far the slider is along it,
        // which alone glides; before the keyboard says, where the art has it
        for (i, k) in self.knobs.iter().enumerate() {
            let o = match self.fracs.get(i).copied().flatten() {
                Some(f) => Overlay { image: k.image.clone(), x: (k.x0 - self.view.0) * unit / s, y: k.y / s, w: k.w / s, h: k.h / s, travel: k.travel * unit / s, frac: f, opacity: 1.0 },
                None => Overlay { image: k.image.clone(), x: k.x / s, y: k.y / s, w: k.w / s, h: k.h / s, travel: 0.0, frac: 0.0, opacity: 1.0 },
            };
            out.push(o);
        }
        if let Some((img, [x, y, w, h])) = &self.glass {
            out.push(Overlay { image: img.clone(), x: x / s, y: y / s, w: w / s, h: h / s, travel: 0.0, frac: 0.0, opacity: self.glass_opacity });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dots sit in the screen's window as the real panel's do: a dot's
    /// room on the left, three on the right, and the room above and below
    /// them shared three to one, at any size the keyboard is drawn.
    #[test]
    fn the_dots_sit_low_and_to_the_left() {
        let o = Overlays::new();
        for bw in [649, 1264, 2528] {
            let [_, _, w, h] = o.panel_box(bw);
            let (pw, ph) = (w.round() as u32, h.round() as u32);
            let [x, y, dw, dh] = o.dots_box(pw, ph, dots());
            let pitch = dw / dots().0 as f32;
            let (left, top, right, bottom) = (x / pitch, y / pitch, (pw as f32 - x - dw) / pitch, (ph as f32 - y - dh) / pitch);
            eprintln!("board {bw} px: window {pw} x {ph} px, a dot {pitch:.3} px; room left {left:.2}, top {top:.2}, right {right:.2}, bottom {bottom:.2} dots");
            assert!((dh / pitch - dots().1 as f32).abs() < 1e-3, "the dots are square");
            assert!((left - 1.0).abs() < 1e-3 && (right - 3.0).abs() < 1e-3);
            assert!((top / bottom - 3.0).abs() < 1e-3);
        }
    }

    /// A display of another size in the same window: its dots square and inside it, the room
    /// round them shared as the gutters share it, its gutters kept on the side it fills.
    #[test]
    fn a_display_of_another_size_sits_in_the_window_as_the_z10_s_does() {
        let o = Overlays::new();
        let [_, _, w, h] = o.panel_box(1264);
        let (pw, ph) = (w.round() as u32, h.round() as u32);
        for (cols, rows) in [(16, 2), (40, 4), (20, 8), (8, 8)] {
            let [x, y, dw, dh] = o.dots_box(pw, ph, (cols * 8, rows * 8));
            let pitch = dw / (cols * 8) as f32;
            assert!((dh / pitch - (rows * 8) as f32).abs() < 1e-3, "{cols} by {rows}: the dots are square");
            let (left, top, right, bottom) = (x / pitch, y / pitch, (pw as f32 - x - dw) / pitch, (ph as f32 - y - dh) / pitch);
            assert!(left >= 1.0 - 1e-3 && top >= 3.0 - 1e-3 && right >= 3.0 - 1e-3 && bottom >= 1.0 - 1e-3, "{cols} by {rows}: {left} {top} {right} {bottom}");
            assert!((right / left - 3.0).abs() < 1e-3 && (top / bottom - 3.0).abs() < 1e-3, "{cols} by {rows}: {left} {top} {right} {bottom}");
            assert!((left - 1.0).abs() < 1e-3 || (top - 3.0).abs() < 1e-3, "{cols} by {rows}: filling the window one way");
        }
    }
}
