//! What the page draws over the keyboard, from the Z10 core's art, as the web
//! app did: the two slider knobs, which follow the real sliders; the display,
//! drawn live as the keyboard's panel shows it; and the display's glass.

use std::collections::HashMap;

use ectodeck_draw::pieces;
use resvg::usvg::{self, Tree};
use serde_json::Value;

use crate::Overlay;

/// the display: 20 x 4 cells of 8 x 8 pixels, each glyph 5 x 7 in columns 1 to 5
const COLS: usize = 20;
const ROWS: usize = 4;
const PW: usize = COLS * 8;
const PH: usize = ROWS * 8;

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
    fold: HashMap<char, String>,
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
    /// what the display shows: whether something drives it, its four rows, its backlight
    pub lcd: (bool, Vec<String>, [f64; 3]),
    drawn_lcd: Option<(bool, Vec<String>, [f64; 3], (u32, u32))>,
}

pub struct Panel {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub px: Vec<[f32; 3]>,
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
        let tree = usvg::Tree::from_str(ectodeck_draw::z10::SVG, &usvg::Options::default()).expect("the Z10's art");
        let board: Value = serde_json::from_str(include_str!("../../ectodeck/public/board.json")).unwrap();
        let v = &board["view"];
        let sliders = board["sliders"].as_array().map(|a| a.iter().map(|s| [num(&s[0]), num(&s[1]), num(&s[2]), num(&s[3])]).collect()).unwrap_or_default();
        let g = &board["glass"];
        let font: Value = serde_json::from_str(include_str!("../../ectodeck/public/lcdfont.json")).unwrap();
        let fold: Value = serde_json::from_str(include_str!("../../ectodeck/public/lcdfold.json")).unwrap_or(Value::Null);
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
        let fold = fold.as_object().map(|m| m.iter().filter_map(|(k, v)| Some((k.chars().next()?, v.as_str()?.to_string()))).collect()).unwrap_or_default();
        Overlays {
            tree,
            view: (num(&v[0]), num(&v[1])),
            sliders,
            glass_box: [num(&g[0]), num(&g[1]), num(&g[2]), num(&g[3])],
            font,
            fold,
            size: (0, 0),
            scale: 1.0,
            knobs: Vec::new(),
            glass: None,
            panel: None,
            panel_rev: 0,
            fracs: [None, None],
            lcd: (false, Vec::new(), [90.0; 3]),
            drawn_lcd: None,
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

    /// The display's panel in the board's pixels: 96% of the glass across,
    /// five times as wide as it is tall, in the middle of the glass.
    fn panel_box(&self) -> [f32; 4] {
        let unit = self.size.0 as f32 / 649.0;
        let [gx0, gy0, gx1, gy1] = self.glass_box;
        let (gw, gh) = (gx1 - gx0, gy1 - gy0);
        let (w, h) = (gw * 0.96, gw * 0.96 / 5.0);
        let (x, y) = (gx0 + (gw - w) / 2.0, gy0 + (gh - h) / 2.0);
        [(x - self.view.0) * unit, (y - self.view.1) * unit, w * unit, h * unit]
    }

    /// The display's text as the keyboard draws it: printable ASCII, the
    /// degree sign and the solid block as themselves, anything else spelled
    /// in those first.
    fn lcd_text(&self, s: &str) -> Vec<char> {
        let mut out = Vec::new();
        for c in s.chars() {
            if (' '..='~').contains(&c) || c == '\u{b0}' || c == '\u{2588}' {
                out.push(c);
            } else {
                out.extend(self.fold.get(&c).map_or("?".chars().collect::<Vec<_>>(), |f| f.chars().collect()));
            }
        }
        out
    }

    /// The panel drawn at its size in real pixels, as the web app drew it: a
    /// negative display, the field dark and only the pixels that are on lit
    /// in the backlight's colour, behind glass, so even a lit pixel is well
    /// short of full brightness.
    fn draw_panel(&mut self) {
        let [_, _, w, h] = self.panel_box();
        let (pw, ph) = ((w.round().max(1.0)) as u32, (h.round().max(1.0)) as u32);
        let key = (self.lcd.0, self.lcd.1.clone(), self.lcd.2, (pw, ph));
        if self.drawn_lcd.as_ref() == Some(&key) && self.panel.is_some() {
            return;
        }
        let shade = |c: [f64; 3], f: f64| [(c[0] * f).round().min(255.0) as f32, (c[1] * f).round().min(255.0) as f32, (c[2] * f).round().min(255.0) as f32];
        let hexc = |r: u8, g: u8, b: u8| [r as f32, g as f32, b as f32];
        let (live, rows, back) = (self.lcd.0, &self.lcd.1, self.lcd.2);
        let (field, off, on) = if live { (shade(back, 0.05), shade(back, 0.1), shade(back, 0.72)) } else { (hexc(8, 10, 12), hexc(0x12, 0x16, 0x1a), hexc(0x2a, 0x31, 0x38)) };
        let mut px = vec![field; (pw * ph) as usize];
        let mut lit = vec![false; PW * PH];
        if live {
            for r in 0..ROWS {
                let mut text = self.lcd_text(rows.get(r).map_or("", |s| s.as_str()));
                text.resize(COLS.max(text.len()), ' ');
                for (c, ch) in text.iter().take(COLS).enumerate() {
                    if *ch == '\u{2588}' {
                        for y in 0..8 {
                            for x in 0..8 {
                                lit[(r * 8 + y) * PW + c * 8 + x] = true;
                            }
                        }
                        continue;
                    }
                    let Some(g) = self.font.get(ch) else { continue };
                    for (x, col) in g.iter().enumerate() {
                        for y in 0..7 {
                            if (col >> y) & 1 == 1 {
                                lit[(r * 8 + y) * PW + c * 8 + 1 + x] = true;
                            }
                        }
                    }
                }
            }
        }
        let (cx, cy) = (pw as f32 / PW as f32, ph as f32 / PH as f32);
        // a gap round each pixel where it is big enough to show one
        let dot = if cx >= 2.5 { 0.84 } else { 1.0 };
        // each dot a square laid over the field, its edges shared out by how
        // much of each pixel they cover, as a canvas fills a rectangle
        for y in 0..PH {
            for x in 0..PW {
                let col = if lit[y * PW + x] { on } else { off };
                let (x0, y0) = (x as f32 * cx, y as f32 * cy);
                let (x1, y1) = (x0 + cx * dot, y0 + cy * dot);
                for py in (y0.floor() as u32)..(y1.ceil() as u32).min(ph) {
                    let oy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).max(0.0);
                    for qx in (x0.floor() as u32)..(x1.ceil() as u32).min(pw) {
                        let a = (x1.min(qx as f32 + 1.0) - x0.max(qx as f32)).max(0.0) * oy;
                        let d = &mut px[(py * pw + qx) as usize];
                        for c in 0..3 {
                            d[c] = d[c] * (1.0 - a) + col[c] * a;
                        }
                    }
                }
            }
        }
        let [x, y, _, _] = self.panel_box();
        self.panel = Some(Panel { x: x.round() as i32, y: y.round() as i32, w: pw, h: ph, px: px.iter().map(|p| p.map(|v| v / 255.0)).collect() });
        self.panel_rev += 1;
        self.drawn_lcd = Some(key);
    }

    /// The display's panel, as last drawn, for the HDR picture.
    pub fn panel(&mut self) -> Option<&Panel> {
        if self.size.0 == 0 {
            return None;
        }
        self.draw_panel();
        self.panel.as_ref()
    }

    /// What goes over the keyboard, in the page's logical pixels from the
    /// keyboard's top left: the knobs and the display's glass. The display
    /// itself is in the HDR picture, under the glass.
    pub fn model(&mut self) -> Vec<Overlay> {
        if self.size.0 == 0 {
            return Vec::new();
        }
        self.draw_panel();
        let s = self.scale;
        let unit = self.size.0 as f32 / 649.0;
        let mut out = Vec::new();
        for (i, k) in self.knobs.iter().enumerate() {
            let x = match self.fracs.get(i).copied().flatten() {
                Some(f) => (k.x0 + f * k.travel - self.view.0) * unit,
                None => k.x,
            };
            out.push(Overlay { image: k.image.clone(), x: x / s, y: k.y / s, w: k.w / s, h: k.h / s });
        }
        if let Some((img, [x, y, w, h])) = &self.glass {
            out.push(Overlay { image: img.clone(), x: x / s, y: y / s, w: w / s, h: h / s });
        }
        out
    }
}
