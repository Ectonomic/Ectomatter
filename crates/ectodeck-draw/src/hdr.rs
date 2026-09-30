//! The keyboard as one HDR picture, drawn from the Z10 core's art.
//!
//! Everything the drawing shows under and on the keys is flattened into one
//! picture: the body and its print, the light each key throws on the deck, the
//! caps, the light on each cap, the glow round each legend, and the legends.
//! What never changes is drawn once for the keyboard's size; each key's light
//! is a shape cut from its own pieces, drawn once too, and filled with the
//! key's colour each time the lighting changes. Light is added in linear light,
//! and the light pieces and the lit legends go brighter than the desktop's
//! white, up to the glow's brightness.
//!
//! The picture comes out as the HDR layer wants it: PQ, BT.2020, 16 bits a
//! channel, premultiplied. SDR colours are decoded as KWin decodes the page's
//! own (gamma 2.2 at the reference white), so the keyboard meets the page
//! round it without a seam.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};
use serde_json::Value;

/// One key's light, as the page has it: its colour (sRGB) and how strongly it
/// is lit (0 to 1), and whether the key is remapped, which steps its legend back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: u32,
    pub rgb: [u8; 3],
    pub strength: f32,
    pub dim: bool,
}

/// A shape cut from one piece of the drawing: how much of it covers each
/// pixel, in its own small box on the board.
struct Shape {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    cover: Vec<f32>,
}

/// A lit key's pieces.
struct KeyArt {
    pos: u32,
    under: Option<Shape>,
    cap: Option<Shape>,
    glow: Option<Shape>,
    legend: Option<Shape>,
    /// the grey its legend is printed in, in sRGB's encoding
    print: [f32; 3],
}

/// The keyboard at one size, ready to light.
pub struct Board {
    pub w: u32,
    pub h: u32,
    /// the body and its print over the ground's colour: opaque, sRGB-encoded
    under: Vec<f32>,
    /// the caps and the legends of unlit keys: premultiplied, sRGB-encoded
    caps: Vec<f32>,
    keys: Vec<KeyArt>,
}

/// sRGB (as the page's colours are) to linear light, as KWin decodes the page: gamma 2.2.
fn lin(v: u8) -> f32 {
    (v as f32 / 255.0).powf(2.2)
}

impl Board {
    /// Draw what never changes, and cut each lit key's shapes, for a board
    /// `w` x `h` pixels: the drawing fitted into it, keeping its proportions.
    pub fn new(tree: &Tree, device: &Value, base: [u8; 3], w: u32, h: u32) -> Result<Board, String> {
        let size = tree.size();
        let k = (w as f32 / size.width()).min(h as f32 / size.height());
        let (tx, ty) = ((w as f32 - size.width() * k) / 2.0, (h as f32 - size.height() * k) / 2.0);
        let node = |id: &str| tree.node_by_id(id);
        // Where a piece goes, through `onto` (the drawing's canvas to pixels).
        // resvg draws a lone piece with its own transform only, not those of
        // the groups it sits in, and first shifts it so its box starts at 0,0;
        // this gives it the groups' transforms and cancels the shift.
        let placed = |n: &usvg::Node, onto: Transform| -> Option<Transform> {
            let usvg::Node::Group(g) = n else { return None };
            let groups = g.abs_transform().pre_concat(g.transform().invert()?);
            let bb = n.abs_layer_bounding_box()?;
            Some(onto.pre_concat(groups).pre_translate(bb.x(), bb.y()))
        };
        let board = Transform::from_row(k, 0.0, 0.0, k, tx, ty);
        // draw one piece onto a pixmap the size of the board
        let draw = |pix: &mut Pixmap, id: &str| {
            if let Some(n) = node(id) {
                if let Some(t) = placed(n, board) {
                    resvg::render_node(n, t, &mut pix.as_mut());
                }
            }
        };
        // cut one piece's shape, in its own box
        let shape = |id: &str| -> Option<Shape> {
            let n = node(id)?;
            let bb = n.abs_layer_bounding_box()?;
            let (fx, fy) = (tx + k * bb.x(), ty + k * bb.y());
            let (x, y) = (fx.floor() as i32, fy.floor() as i32);
            let (sw, sh) = ((k * bb.width()).ceil() as u32 + 2, (k * bb.height()).ceil() as u32 + 2);
            let mut pix = Pixmap::new(sw, sh)?;
            let t = placed(n, Transform::from_row(k, 0.0, 0.0, k, tx - x as f32, ty - y as f32))?;
            resvg::render_node(n, t, &mut pix.as_mut());
            let cover = pix.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
            Some(Shape { x, y, w: sw, h: sh, cover })
        };

        let controls = device["controls"].as_array().ok_or("the device names no controls")?;
        let lit = |c: &Value| c["illumination"].is_array();
        let sliders = |c: &Value| c.get("slider").is_some();

        // under the light: the body and its print, over the ground
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        pix.fill(resvg::tiny_skia::Color::from_rgba8(base[0], base[1], base[2], 255));
        draw(&mut pix, "body");
        draw(&mut pix, "legend-body");
        // kept in sRGB's own encoding: the light is mixed over it as the web page's CSS mixed it
        let under = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();

        // over it: every cap but the slider knobs (the page moves those), and the
        // legends of keys with no light behind them
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        for c in controls.iter().filter(|c| !sliders(c)) {
            if let Some(n) = c["control"].as_u64() {
                draw(&mut pix, &format!("control_{n}"));
            }
        }
        for c in controls.iter().filter(|c| !lit(c)) {
            if let Some(n) = c["legend"].as_u64() {
                draw(&mut pix, &format!("legend_{n}"));
            }
        }
        // premultiplied, in sRGB's own encoding, as a browser composites them
        let caps = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]).collect();

        // each lit key's shapes
        let mut keys = Vec::new();
        for c in controls.iter().filter(|c| lit(c)) {
            let Some(pos) = c["pos"].as_u64() else { continue };
            let ill: Vec<u64> = c["illumination"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();
            let legend = c["legend"].as_u64();
            let print = legend
                .and_then(|n| tree.node_by_id(&format!("legend_{n}")))
                .and_then(first_fill)
                .unwrap_or([0x9e, 0x9e, 0x9e]);
            keys.push(KeyArt {
                pos: pos as u32,
                under: ill.first().and_then(|n| shape(&format!("illumination_{n}"))),
                cap: ill.get(1).and_then(|n| shape(&format!("illumination_{n}"))),
                glow: ill.get(2).and_then(|n| shape(&format!("illumination_{n}"))),
                legend: legend.and_then(|n| shape(&format!("legend_{n}"))),
                print: [print[0] as f32 / 255.0, print[1] as f32 / 255.0, print[2] as f32 / 255.0],
            });
        }
        Ok(Board { w, h, under, caps, keys })
    }

    /// The places of the keys that light.
    pub fn positions(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys.iter().map(|k| k.pos)
    }

    /// The picture with this light: premultiplied linear RGBA, the SDR white
    /// at 1.0 and the light up to `gain` times it.
    pub fn compose(&self, lights: &[Light], gain: f32) -> Vec<f32> {
        let (w, h) = (self.w as i32, self.h as i32);
        let mut out = vec![0f32; (self.w * self.h * 4) as usize];
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            px[..3].copy_from_slice(&self.under[i * 3..i * 3 + 3]);
            px[3] = 1.0;
        }
        let light = |pos: u32| lights.iter().find(|l| l.pos == pos);
        // lay a colour over the picture through a shape, `alpha` its strength
        let over = |out: &mut [f32], s: &Shape, col: [f32; 3], alpha: f32| {
            if alpha <= 0.0 {
                return;
            }
            for row in 0..s.h as i32 {
                let y = s.y + row;
                if y < 0 || y >= h {
                    continue;
                }
                for col_i in 0..s.w as i32 {
                    let x = s.x + col_i;
                    if x < 0 || x >= w {
                        continue;
                    }
                    let a = s.cover[(row * s.w as i32 + col_i) as usize] * alpha;
                    if a <= 0.0 {
                        continue;
                    }
                    let d = ((y * w + x) * 4) as usize;
                    for c in 0..3 {
                        out[d + c] = out[d + c] * (1.0 - a) + col[c] * a;
                    }
                    out[d + 3] = out[d + 3] * (1.0 - a) + a;
                }
            }
        };
        // the light's colour, encoded as the page's colours are; `gain` above 1 lifts it past white
        let enc = |v: f32| v.max(0.0).powf(1.0 / 2.2);
        let glow_colour = |l: &Light| [enc(lin(l.rgb[0]) * gain), enc(lin(l.rgb[1]) * gain), enc(lin(l.rgb[2]) * gain)];

        // the light each key throws on the deck
        for key in &self.keys {
            if let (Some(l), Some(s)) = (light(key.pos), &key.under) {
                over(&mut out, s, glow_colour(l), l.strength);
            }
        }
        // the caps over it
        for (px, cap) in out.chunks_exact_mut(4).zip(self.caps.chunks_exact(4)) {
            let a = cap[3];
            if a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                px[c] = cap[c] + px[c] * (1.0 - a);
            }
            px[3] = a + px[3] * (1.0 - a);
        }
        // the light on each cap, the glow round each legend, and the legend
        for key in &self.keys {
            let l = light(key.pos);
            let strength = l.map_or(0.0, |l| l.strength);
            if let Some(l) = l {
                if let Some(s) = &key.cap {
                    over(&mut out, s, glow_colour(l), l.strength);
                }
                if let Some(s) = &key.glow {
                    over(&mut out, s, glow_colour(l), l.strength);
                }
            }
            if let Some(s) = &key.legend {
                // printed grey where it is unlit, the key's light as strongly as it is lit
                let g = l.map_or([0.0; 3], glow_colour);
                let col = [
                    key.print[0] * (1.0 - strength) + g[0] * strength,
                    key.print[1] * (1.0 - strength) + g[1] * strength,
                    key.print[2] * (1.0 - strength) + g[2] * strength,
                ];
                let step_back = if l.is_some_and(|l| l.dim) { 0.35 } else { 1.0 };
                over(&mut out, s, col, step_back);
            }
        }
        // mixed in sRGB's encoding, as the web page's CSS mixed it; handed on in linear light
        for px in out.chunks_exact_mut(4) {
            let a = px[3];
            if a > 0.0 {
                for c in 0..3 {
                    px[c] = (px[c] / a).max(0.0).powf(2.2) * a;
                }
            }
        }
        out
    }
}

/// The first fill colour in a piece: the grey a legend is printed in.
fn first_fill(node: &usvg::Node) -> Option<[u8; 3]> {
    match node {
        usvg::Node::Path(p) => match p.fill()?.paint() {
            usvg::Paint::Color(c) => Some([c.red, c.green, c.blue]),
            _ => None,
        },
        usvg::Node::Group(g) => g.children().iter().find_map(first_fill),
        _ => None,
    }
}

/// PQ signal for nits
fn pq(nits: f64) -> f64 {
    let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
    let (c1, c2, c3) = (3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
    let y = (nits.max(0.0) / 10000.0).powf(m1);
    ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
}

/// Linear light to the HDR layer's pixels, quickly: PQ looked up in a table
/// spaced by the fourth root of brightness, which puts most of its entries in
/// the dark, where PQ changes fastest.
pub struct Encoder {
    table: Vec<f32>,
    reference: f32,
}

const STEPS: usize = 8192;

impl Encoder {
    pub fn new(reference: f64) -> Encoder {
        let table = (0..=STEPS).map(|i| pq(10000.0 * (i as f64 / STEPS as f64).powi(4)) as f32).collect();
        Encoder { table, reference: reference as f32 }
    }

    fn signal(&self, nits: f32) -> f32 {
        let u = (nits.max(0.0) / 10000.0).sqrt().sqrt().min(1.0) * STEPS as f32;
        let i = (u as usize).min(STEPS - 1);
        let f = u - i as f32;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }

    /// One pixel: premultiplied linear RGBA (SDR white 1.0, BT.709) to
    /// premultiplied PQ, BT.2020, as the channels of ABGR16161616
    pub fn pixel(&self, px: [f32; 4]) -> [u16; 4] {
        const M: [[f32; 3]; 3] = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
        let a = px[3];
        if a <= 0.0 {
            return [0; 4];
        }
        let c = [px[0] / a, px[1] / a, px[2] / a];
        let mut out = [0u16; 4];
        for (o, row) in out.iter_mut().zip(M) {
            let v = row[0] * c[0] + row[1] * c[1] + row[2] * c[2];
            *o = (self.signal(v * self.reference) * a * 65535.0).round().clamp(0.0, 65535.0) as u16;
        }
        out[3] = (a.min(1.0) * 65535.0).round() as u16;
        out
    }

    /// premultiplied linear RGBA (SDR white 1.0, BT.709) to premultiplied
    /// ABGR16161616, PQ, BT.2020
    pub fn encode(&self, frame: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(frame.len() * 2);
        for px in frame.chunks_exact(4) {
            for v in self.pixel([px[0], px[1], px[2], px[3]]) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> (Tree, Value, Board) {
        let tree = Tree::from_str(crate::z10::SVG, &usvg::Options::default()).unwrap();
        let device: Value = serde_json::from_str(crate::z10::DEVICE).unwrap();
        let b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        (tree, device, b)
    }

    #[test]
    fn the_keyboard_comes_out_whole_and_lit() {
        let t = std::time::Instant::now();
        let (_, _, b) = board();
        let made = t.elapsed();
        assert!(b.keys.len() >= 100, "lit keys: {}", b.keys.len());
        let stat = |f: &dyn Fn(&KeyArt) -> Option<&Shape>| {
            let v: Vec<&Shape> = b.keys.iter().filter_map(|k| f(k)).collect();
            let max = v.iter().flat_map(|s| s.cover.iter()).fold(0f32, |m, c| m.max(*c));
            (v.len(), max)
        };
        eprintln!("shapes: under {:?} cap {:?} glow {:?} legend {:?}", stat(&|k| k.under.as_ref()), stat(&|k| k.cap.as_ref()),
            stat(&|k| k.glow.as_ref()), stat(&|k| k.legend.as_ref()));
        let lights: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 45, 0], strength: 1.0, dim: false }).collect();
        let t = std::time::Instant::now();
        let frame = b.compose(&lights, 1000.0 / 254.0);
        let composed = t.elapsed();
        let enc = Encoder::new(254.0);
        let t = std::time::Instant::now();
        let px = enc.encode(&frame);
        let encoded = t.elapsed();
        assert_eq!(px.len(), (1264 * 506 * 8) as usize);
        eprintln!("made in {made:?}, lit in {composed:?}, encoded in {encoded:?}");
        // the brightest part of the lit keyboard goes beyond the desktop's white
        let brightest = frame.chunks_exact(4).map(|p| p[0] / p[3].max(1e-6)).fold(0f32, f32::max);
        assert!(brightest > 1.2, "the light should go past SDR white: {brightest}");
        if let Some(dir) = std::env::var_os("ECTODECK_HDR_DUMP") {
            std::fs::write(std::path::Path::new(&dir).join("hdr-keyboard-1264x506.raw"), &px).unwrap();
        }
    }

    #[test]
    fn the_table_matches_pq() {
        let enc = Encoder::new(254.0);
        for nits in [0.0f32, 0.05, 0.5, 2.7, 10.0, 100.0, 254.0, 1000.0] {
            let exact = pq(nits as f64) as f32;
            assert!((enc.signal(nits) - exact).abs() < 0.0005, "{nits} nits: {} vs {exact}", enc.signal(nits));
        }
    }
}
