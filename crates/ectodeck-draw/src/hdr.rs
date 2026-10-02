//! The keyboard as one HDR picture, drawn from the Z10 core's art.
//!
//! Everything the drawing shows under and on the keys is flattened into one
//! picture: the body and its print, the caps' shadows, and the keys' light
//! on the deck, the chassis, the caps' skirts and through the legends. What
//! never changes is drawn once for the keyboard's size. The keys' light is
//! the light simulation's (see sim.rs): each key's LED followed through the
//! keyboard's shape, what it lights kept for each LED on its own, so it is
//! filled with the keys' colours each time the lighting changes. Light is
//! added in linear light, and the lit legends go brighter than the desktop's white.
//!
//! The picture comes out as the HDR layer wants it: PQ, BT.2020, 16 bits a
//! channel, premultiplied. SDR colours are decoded as KWin decodes the page's
//! own (gamma 2.2 at the reference white), so the keyboard meets the page
//! round it without a seam.

use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};
use serde_json::Value;

#[path = "sim.rs"]
pub mod sim;

use sim::{At, Coloured, Snapshot};

/// the art's units to the millimetre
const UNITS_PER_MM: f32 = 1.38;

/// One key's light, as the page has it: its colour (sRGB) and how strongly it
/// is lit (0 to 1), and whether the key is remapped, which puts the second
/// function at the foot of its cap aside for the new meaning the page prints there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: u32,
    pub rgb: [u8; 3],
    pub strength: f32,
    pub remapped: bool,
}

/// A shape on the board: how much of something there is at each pixel, in
/// its own small box.
struct Shape {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    cover: Vec<f32>,
}

/// A lit key: its place, and its cap, which its light is simulated in.
struct KeyArt {
    pos: u32,
    /// the cap's foot (its skirt's outline) and its face, in the art's units
    foot: Option<[f32; 4]>,
    face: [f32; 4],
    /// the legend's coverage
    legend: Option<Shape>,
    /// the row of the legend's box where its second function begins, if it has
    /// one: a line of its own at the foot of the cap, like the PF1 under 1
    second: Option<u32>,
    /// the grey its legend is printed in, in sRGB's encoding
    print: [f32; 3],
}

/// What the board takes from the core's "light" itself: where each key's LED
/// sits, how bright the light is, how a legend looks with its LED off, and
/// the display's light. How the light falls is the simulation's.
#[derive(Clone, Copy, Debug)]
struct Model {
    /// how far north of the key's centre the LED sits (the art's units)
    led: f32,
    /// the brightest legend, every key lit white at full, in nits: what all
    /// the simulation's light is scaled by
    nits: f32,
    /// how a lit key's legend looks with its LED off, as a share of the grey
    /// the art prints it in: a backlit legend with no light behind it is dark
    legend_unlit: f32,
    /// the display: its lit dots, its unlit dots, the ground between them, in
    /// nits; how much of each cell a dot fills; how strong the glass's sheen is
    lcd_nits: f32,
    lcd_unlit: f32,
    lcd_field: f32,
    lcd_dot: f32,
    lcd_glass: f32,
}

impl Model {
    fn from(l: &Value) -> Model {
        let f = |v: &Value, d: f32| v.as_f64().map_or(d, |v| v as f32);
        Model {
            led: f(&l["led"], 7.0),
            // or, as the core once had it, the legends' own nits
            nits: f(&l["sim"]["nits"], f(&l["legend"]["nits"], 600.0)).max(0.0),
            legend_unlit: f(&l["legend"]["unlit"], 0.15).clamp(0.0, 1.0),
            lcd_nits: f(&l["lcd"]["nits"], 324.0).max(0.0),
            lcd_unlit: f(&l["lcd"]["unlit"], 4.1).max(0.0),
            lcd_field: f(&l["lcd"]["field"], 0.8).max(0.0),
            lcd_dot: f(&l["lcd"]["dot"], 0.84).clamp(0.05, 1.0),
            lcd_glass: f(&l["lcd"]["glass"], 1.0).max(0.0),
        }
    }
}

/// The display's light, from the core's "lcd".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lcd {
    pub lit: f32,
    pub unlit: f32,
    pub field: f32,
    pub dot: f32,
    pub glass: f32,
}

/// The brightest of each part of the light on the keyboard, in nits, and the
/// brightest the lit keys' colour shows cleanly, before the screen clips it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Peaks {
    pub legend: f32,
    pub deck: f32,
    pub skirt: f32,
    pub chassis: f32,
    pub clean: f32,
}

/// The simulation's light at its brightest on each part, every key lit white
/// at full, as the simulation gives it (before it is made its nits): on the
/// legends, the deck, the caps' skirts and the chassis.
#[derive(Clone, Copy, Debug, Default)]
struct Most {
    legend: f32,
    deck: f32,
    skirt: f32,
    chassis: f32,
}

/// How much of a small source's light falls on a surface `h` below it, at `r`
/// along the surface from the point straight below it: 1 there, falling as
/// (h² / (h² + r²))^1.5, as a lamp's light falls across a floor.
fn falloff(r: f32, h: f32) -> f32 {
    let q = h * h / (h * h + r * r);
    q * q.sqrt()
}

/// The keyboard at one size, ready to light.
pub struct Board {
    pub w: u32,
    pub h: u32,
    /// the body and its print over the ground's colour: opaque, sRGB-encoded
    under: Vec<f32>,
    /// the caps' shadows on the body, from the room's light, which the LEDs' light is not under
    shadow: Vec<f32>,
    /// the caps, and the legends of unlit keys: premultiplied, sRGB-encoded
    caps: Vec<f32>,
    keys: Vec<KeyArt>,
    /// the caps of the keys with no light of their own (the spacebar, the
    /// media keys): their feet and faces in the art's units, which stand in the light's way
    others: Vec<([f32; 4], [f32; 4])>,
    model: Model,
    /// where the chassis is: a pixel's coverage
    chassis: Vec<f32>,
    /// each well's box in the art's units, left, top, right, bottom (the
    /// wells' clip's pieces, in order; with no wells in the art, the board's)
    well_boxes: Vec<[f32; 4]>,
    /// the art's own units (the SVG's, which the core's lengths are in) to the
    /// board's pixels: pixel = offset + scale x unit
    pub(crate) art_scale: f32,
    pub(crate) art_offset: (f32, f32),
    /// the lock lights, and their light: the window at its brightest and the
    /// light out of its edges, in nits, and its colour (sRGB)
    indicators: Vec<Indicator>,
    indicator_light: (f32, f32, [u8; 3]),
    /// the simulation's latest picture of the keys' light, and its brightest on each part
    sim: Option<Arc<Snapshot>>,
    most: Most,
}

/// A lock light: which of the keyboard's lights it shows (a kernel light
/// code: 0 Num Lock, 1 Caps Lock, 2 Scroll Lock), its window in the art, how
/// brightly the LED under it lights each pixel of the window (1 over the
/// LED, less to the window's edges), and the light out of the window's edges
/// onto the chassis round it (1 at the edge).
struct Indicator {
    bit: u8,
    window: Shape,
    lit: Vec<f32>,
    spill: Shape,
}

/// The keys' light for one picture: the simulation's, in the keys' colours,
/// made its nits by `gain`; with every key alike, the simulation's white
/// light in their colour, or else each key's own in its colour, added up.
struct KeysLight<'a> {
    snap: &'a Snapshot,
    gain: f32,
    alike: Option<[f32; 3]>,
    coloured: Option<Coloured>,
}

impl KeysLight<'_> {
    fn at(&self, a: &At) -> [f32; 3] {
        match (self.alike, &self.coloured) {
            (Some(c), _) => {
                let v = self.snap.white(a) * self.gain;
                c.map(|c| c * v)
            }
            (None, Some(m)) => m.at(a).map(|v| v * self.gain),
            _ => [0.0; 3],
        }
    }
}

/// sRGB (as the page's colours are) to linear light, as KWin decodes the page: gamma 2.2.
fn lin(v: u8) -> f32 {
    (v as f32 / 255.0).powf(2.2)
}

/// A key's light: its LEDs' colour in linear light, as strongly as the key is
/// lit. An LED gives off as much light as its colour has: white (all three of
/// its dies full) the most, a red about a third of that; so a part's nits are
/// a key lit white at full, and a colour shows its own share of them. Nothing
/// holds it back: a part asked for more than the screen can show in that
/// colour (a saturated red's channel passes the screen's peak well before its
/// luminance does) is sent as asked, and the screen clips it there.
fn key_colour(l: &Light) -> [f32; 3] {
    let c = [lin(l.rgb[0]), lin(l.rgb[1]), lin(l.rgb[2])];
    c.map(|v| v * l.strength.clamp(0.0, 1.0))
}

/// A colour in linear light at a luminance of 1, for a light whose nits are
/// its own whatever its colour (the lock lights).
fn unit_colour(rgb: [u8; 3]) -> [f32; 3] {
    let c = [lin(rgb[0]), lin(rgb[1]), lin(rgb[2])];
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    if y <= 0.0 { [0.0; 3] } else { c.map(|v| v / y) }
}

/// How bright a key's colour goes, in the SDR white's units, before one of
/// its channels passes `ceiling`: past it the screen clips that channel.
fn clean_limit(l: &Light, ceiling: f32) -> f32 {
    let c = [lin(l.rgb[0]), lin(l.rgb[1]), lin(l.rgb[2])];
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let widest = c[0].max(c[1]).max(c[2]);
    if widest <= 0.0 { 0.0 } else { ceiling * y / widest }
}

/// Where a legend's second function begins, as a row of the legend's box: a
/// line of its own, apart from the rest by a gap, starting in the lower part
/// of the cap's face. A word set in two lines in the middle of a cap, like
/// CAPS LOCK, starts its second line about half way down, and keeps it; the
/// second functions start from about three fifths down (the keypad's arrows,
/// the tallest of them, at 58%).
fn second_function(s: &Shape, face: [f32; 4], ka: f32, oy: f32) -> Option<u32> {
    let inked: Vec<bool> = (0..s.h).map(|r| (0..s.w).any(|c| s.cover[(r * s.w + c) as usize] > 0.05)).collect();
    let last = inked.iter().rposition(|v| *v)? as u32;
    // the start of the last line: the first inked row after the last gap
    let start = (0..=last).rev().find(|&r| r == 0 || (inked[r as usize] && !inked[r as usize - 1]))?;
    let gap = (0..start).rev().take_while(|&r| !inked[r as usize]).count();
    if start == 0 || gap == 0 || !inked[..start as usize].iter().any(|v| *v) {
        return None;
    }
    // its top, in the art's units
    let top = (s.y as f32 + start as f32 - oy) / ka;
    (top > face[1] + (face[3] - face[1]) * 0.54).then_some(start)
}

/// How much of each pixel of a key's legend shows: all of it, but a remapped
/// key's second function, which steps aside for its new meaning.
fn shown(key: &KeyArt, l: Option<&Light>) -> Vec<f32> {
    let Some(s) = &key.legend else { return Vec::new() };
    let aside = l.is_some_and(|l| l.remapped).then_some(key.second).flatten();
    (0..s.cover.len()).map(|i| if aside.is_some_and(|r| i as u32 / s.w >= r) { 0.0 } else { 1.0 }).collect()
}

/// A clip path somewhere under `g`, by its id, with the group it clips.
fn find_clip<'a>(g: &'a usvg::Group, id: &str) -> Option<(&'a usvg::Group, &'a usvg::ClipPath)> {
    if let Some(c) = g.clip_path().filter(|c| c.id() == id) {
        return Some((g, c));
    }
    g.children().iter().find_map(|n| match n {
        usvg::Node::Group(g) => find_clip(g, id),
        _ => None,
    })
}

impl Board {
    /// Draw what never changes, and find each key's cap, for a board `w` x
    /// `h` pixels: the drawing fitted into it, keeping its proportions. The
    /// keys' light comes from the simulation (see set_sim); till then the
    /// keys are dark.
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
            let outer = match n {
                usvg::Node::Group(g) => g.abs_transform().pre_concat(g.transform().invert()?),
                _ => n.abs_transform(),
            };
            let bb = n.abs_layer_bounding_box()?;
            Some(onto.pre_concat(outer).pre_translate(bb.x(), bb.y()))
        };
        let board = Transform::from_row(k, 0.0, 0.0, k, tx, ty);
        let draw_node = |pix: &mut Pixmap, n: &usvg::Node| {
            if let Some(t) = placed(n, board) {
                resvg::render_node(n, t, &mut pix.as_mut());
            }
        };
        // draw one piece onto a pixmap the size of the board
        let draw = |pix: &mut Pixmap, id: &str| {
            if let Some(n) = node(id) {
                draw_node(pix, n);
            }
        };
        // cut one piece's coverage, in its own box
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
        let children = |id: &str| -> Vec<usvg::Node> {
            match node(id) {
                Some(usvg::Node::Group(g)) => g.children().to_vec(),
                _ => Vec::new(),
            }
        };

        let controls = device["controls"].as_array().ok_or("the device names no controls")?;
        let lit = |c: &Value| c["illumination"].is_array();
        let sliders = |c: &Value| c.get("slider").is_some();
        let model = Model::from(&device["light"]);

        // under the light: the body and its print, over the ground
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        pix.fill(resvg::tiny_skia::Color::from_rgba8(base[0], base[1], base[2], 255));
        draw(&mut pix, "body");
        draw(&mut pix, "legend-body");
        // the chassis itself, which light spilling out of the wells falls on (not the page round it)
        let mut body = Pixmap::new(w, h).ok_or("board too small")?;
        draw(&mut body, "body");
        let chassis: Vec<f32> = body.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
        let under = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();

        // Every cap but the slider knobs (the page moves those): its shadow (its
        // first piece, drawn blurred) apart from the rest, since the shadow is the
        // room's light blocked, and the LED's light comes from under the cap; and
        // the legends of keys with no light behind them. The media buttons are
        // moulded blocks with no shadow of their own: all their pieces are the block.
        let mut shade = Pixmap::new(w, h).ok_or("board too small")?;
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        for c in controls.iter().filter(|c| !sliders(c)) {
            let Some(n) = c["control"].as_u64() else { continue };
            let id = format!("control_{n}");
            let parts = children(&id);
            let shadow = parts.len() >= 3 && matches!(&parts[0], usvg::Node::Group(g) if !g.filters().is_empty());
            if shadow {
                draw_node(&mut shade, &parts[0]);
                for p in &parts[1..] {
                    draw_node(&mut pix, p);
                }
            } else {
                draw(&mut pix, &id);
            }
        }
        for c in controls.iter().filter(|c| !lit(c)) {
            if let Some(n) = c["legend"].as_u64() {
                draw(&mut pix, &format!("legend_{n}"));
            }
        }
        let shadow: Vec<f32> = shade.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
        // premultiplied, in sRGB's own encoding
        let caps: Vec<f32> = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]).collect();

        // The wells, where the deck is open to the keys' light: a clip in the
        // art, in the art's own units, as the group it clips has them. That
        // group's transform takes the art's units to the drawing's; the core's
        // lengths are in the art's units.
        let wells_id = device["light"]["wells"].as_str().unwrap_or("mask_1");
        let found = find_clip(tree.root(), wells_id);
        let art = found.map_or(Transform::identity(), |(g, _)| g.abs_transform());
        // the art's units to the board's pixels
        let ka = k * art.sx;
        let (ox, oy) = (tx + k * art.tx, ty + k * art.ty);
        // each well on its own: its box in the art's units, from its piece of
        // the clip; with no wells in the art, the board is one
        let mut well_boxes: Vec<[f32; 4]> = Vec::new();
        if let Some((_, clip)) = found {
            for n in clip.root().children() {
                let usvg::Node::Path(p) = n else { continue };
                let b = p.data().bounds();
                let mut corners = [(b.left(), b.top()), (b.right(), b.top()), (b.left(), b.bottom()), (b.right(), b.bottom())].map(|(x, y)| resvg::tiny_skia::Point::from_xy(x, y));
                clip.transform().pre_concat(p.abs_transform()).map_points(&mut corners);
                let (xs, ys) = (corners.map(|p| p.x), corners.map(|p| p.y));
                let lo = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
                let hi = |v: [f32; 4]| v.into_iter().fold(-f32::MAX, f32::max);
                well_boxes.push([lo(xs), lo(ys), hi(xs), hi(ys)]);
            }
        }
        if well_boxes.is_empty() {
            well_boxes.push([-ox / ka, -oy / ka, (w as f32 - ox) / ka, (h as f32 - oy) / ka]);
        }
        // a piece's box, in the art's units
        let rect = |n: &usvg::Node| {
            let b = n.abs_bounding_box();
            [(b.left() - art.tx) / art.sx, (b.top() - art.ty) / art.sy, (b.right() - art.tx) / art.sx, (b.bottom() - art.ty) / art.sy]
        };
        // each lit key: its legend, and its cap's shape, which its light is simulated in
        let mut keys = Vec::new();
        for c in controls.iter().filter(|c| lit(c)) {
            let Some(pos) = c["pos"].as_u64() else { continue };
            let legend_n = c["legend"].as_u64();
            let print = legend_n.and_then(|n| tree.node_by_id(&format!("legend_{n}"))).and_then(first_fill).unwrap_or([0x9e, 0x9e, 0x9e]);
            let print = [print[0] as f32 / 255.0, print[1] as f32 / 255.0, print[2] as f32 / 255.0];
            let legend = legend_n.and_then(|n| shape(&format!("legend_{n}")));
            let parts = c["control"].as_u64().map(|n| children(&format!("control_{n}"))).unwrap_or_default();
            let (foot, face) = match (parts.get(1).map(rect), parts.get(2).map(rect)) {
                (Some(f), Some(a)) => (Some(f), a),
                _ => (None, [0.0; 4]),
            };
            let second = legend.as_ref().and_then(|s| second_function(s, face, ka, oy));
            keys.push(KeyArt { pos: pos as u32, foot, face, legend, second, print });
        }
        // the caps of the keys with no light of their own, which stand in its way
        let mut others = Vec::new();
        for c in controls.iter().filter(|c| !lit(c) && !sliders(c)) {
            let Some(n) = c["control"].as_u64() else { continue };
            let parts = children(&format!("control_{n}"));
            if parts.len() >= 3 && matches!(&parts[0], usvg::Node::Group(g) if !g.filters().is_empty()) {
                others.push((rect(&parts[1]), rect(&parts[2])));
            }
        }
        // The lock lights, from the core's "indicators": each a small window
        // with an LED under it, brightest over the LED and dimmer to the
        // window's corners (its diffuser keeps it from falling far), and a
        // little of its light out of the window's edges onto the chassis.
        let ind = &device["indicators"];
        let num = |v: &Value, d: f32| v.as_f64().map_or(d, |v| v as f32);
        let (spread, floor) = (num(&ind["spread"], 2.0).max(0.05), num(&ind["floor"], 0.5).clamp(0.0, 1.0));
        let reach = num(&ind["spill"]["reach"], 1.0).max(0.05);
        let indicators = ind["lights"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| {
                let bit = match l["lock"].as_str()? {
                    "num" => 0,
                    "caps" => 1,
                    "scroll" => 2,
                    _ => return None,
                };
                let id = l["art"].as_str()?;
                let window = shape(id)?;
                // the window's edges and middle, in the board's pixels
                let b = node(id)?.abs_bounding_box();
                let (l0, t0, r0, b0) = (tx + k * b.left(), ty + k * b.top(), tx + k * b.right(), ty + k * b.bottom());
                let (cx, cy) = ((l0 + r0) / 2.0, (t0 + b0) / 2.0);
                let lit = (0..window.cover.len())
                    .map(|i| {
                        let (x, y) = (window.x as f32 + (i as u32 % window.w) as f32 + 0.5, window.y as f32 + (i as u32 / window.w) as f32 + 0.5);
                        let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / ka;
                        floor + (1.0 - floor) * falloff(r, spread)
                    })
                    .collect();
                // out of its edges onto the chassis, falling away from the window
                let m = reach * 6.0 * ka;
                let (sx, sy) = ((l0 - m).floor().max(0.0) as i32, (t0 - m).floor().max(0.0) as i32);
                let sw = ((r0 + m).ceil().min(w as f32) as i32 - sx).max(0) as u32;
                let sh = ((b0 + m).ceil().min(h as f32) as i32 - sy).max(0) as u32;
                let mut cover = vec![0f32; (sw * sh) as usize];
                for j in 0..sh {
                    for i in 0..sw {
                        let (x, y) = (sx + i as i32, sy + j as i32);
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let (dx, dy) = ((l0 - px).max(px - r0).max(0.0), (t0 - py).max(py - b0).max(0.0));
                        let d = (dx * dx + dy * dy).sqrt() / ka;
                        let inside = if x >= window.x && y >= window.y && x < window.x + window.w as i32 && y < window.y + window.h as i32 {
                            window.cover[((y - window.y) as u32 * window.w + (x - window.x) as u32) as usize]
                        } else {
                            0.0
                        };
                        cover[(j * sw + i) as usize] = chassis[(y as u32 * w + x as u32) as usize] * (1.0 - inside) * (-d / reach).exp();
                    }
                }
                Some(Indicator { bit, window, lit, spill: Shape { x: sx, y: sy, w: sw, h: sh, cover } })
            })
            .collect();
        let rgb = ind["rgb"].as_array().map(|c| [0, 1, 2].map(|i| c.get(i).and_then(|v| v.as_u64()).unwrap_or(255).min(255) as u8)).unwrap_or([255; 3]);
        let indicator_light = (num(&ind["nits"], 300.0).max(0.0), num(&ind["spill"]["nits"], 6.0).max(0.0), rgb);
        Ok(Board {
            w,
            h,
            under,
            shadow,
            caps,
            keys,
            others,
            model,
            chassis,
            well_boxes,
            art_scale: ka,
            art_offset: (ox, oy),
            indicators,
            indicator_light,
            sim: None,
            most: Most::default(),
        })
    }

    /// Light the keyboard by another of the core's "light" descriptions (as the
    /// Lighting lab tunes it): how bright the light is, how a legend looks
    /// with its LED off, the display. How the light falls is the simulation's,
    /// asked of it on its own (see sim::Simulator::set).
    pub fn set_light(&mut self, light: &Value) {
        self.model = Model::from(light);
    }

    /// Light the keys by the simulation's latest picture of their light.
    pub fn set_sim(&mut self, snap: Arc<Snapshot>) {
        self.most = self.measure(&snap);
        self.sim = Some(snap);
    }

    /// How far the simulation the keys are lit by has got: the photons it
    /// has followed from each LED, and whether that is all it follows.
    pub fn sim_progress(&self) -> Option<(usize, bool)> {
        self.sim.as_ref().map(|s| (s.photons, s.settled))
    }

    /// The brightest legend, every key lit white at full, in nits, and the display's lit dots'.
    pub fn legend_nits(&self) -> f32 {
        self.model.nits
    }
    pub fn lcd_nits(&self) -> f32 {
        self.model.lcd_nits
    }

    /// How far north of its key's centre each LED sits, in the art's units.
    pub fn led_north(&self) -> f32 {
        self.model.led
    }

    /// The display as the core has it lit: its lit dots, unlit dots and the
    /// ground between them in nits, how much of each cell a dot fills, and
    /// how strong the glass's sheen over it is (1 as the art draws it).
    pub fn lcd(&self) -> Lcd {
        let m = &self.model;
        Lcd { lit: m.lcd_nits, unlit: m.lcd_unlit, field: m.lcd_field, dot: m.lcd_dot, glass: m.lcd_glass }
    }

    /// A pixel's middle in the art's units.
    fn unit(&self, x: usize, y: usize) -> (f32, f32) {
        ((x as f32 + 0.5 - self.art_offset.0) / self.art_scale, (y as f32 + 0.5 - self.art_offset.1) / self.art_scale)
    }

    /// The simulation's light at its brightest on each part, every key lit
    /// white at full: on the legends where they are solid, and on the deck,
    /// the chassis and the skirts where no cap's edge covers them part way.
    fn measure(&self, s: &Snapshot) -> Most {
        let w = self.w as usize;
        let mut m = Most::default();
        for key in &self.keys {
            let Some(l) = &key.legend else { continue };
            for (i, c) in l.cover.iter().enumerate() {
                let (x, y) = (l.x + (i as u32 % l.w) as i32, l.y + (i as u32 / l.w) as i32);
                if *c < 0.5 || x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
                    continue;
                }
                let (u, v) = self.unit(x as usize, y as usize);
                if let Some(a) = s.at(u, v, Some(3)) {
                    m.legend = m.legend.max(s.white(&a));
                }
            }
        }
        for j in 0..w * self.h as usize {
            let cap = self.caps[j * 4 + 3];
            let (u, v) = self.unit(j % w, j / w);
            let Some(a) = s.at(u, v, None) else { continue };
            match a.code {
                0 if cap < 0.01 => m.deck = m.deck.max(s.white(&a)),
                1 if cap < 0.01 => m.chassis = m.chassis.max(s.white(&a)),
                2 if cap > 0.99 => m.skirt = m.skirt.max(s.white(&a)),
                _ => {}
            }
        }
        m
    }

    /// What makes the simulation's light its nits, in the SDR white's units:
    /// the brightest legend, every key lit white at full, at the core's nits,
    /// up to Peak nits (`ceiling` times the white).
    fn gain(&self, white: f32, ceiling: f32) -> f32 {
        let top = (self.model.nits / white.max(1.0)).min(ceiling);
        if self.most.legend > 0.0 { top / self.most.legend } else { 0.0 }
    }

    /// The brightest light on the keyboard with these lights, in nits, with
    /// the SDR white at `white` nits and Peak nits at `ceiling` times it: the
    /// legends', the deck's, the skirts', the chassis's (as if every key were
    /// lit as the brightest is), and the brightest the lit keys' colour shows
    /// cleanly, before the screen clips one of its channels.
    pub fn peaks(&self, lights: &[Light], white: f32, ceiling: f32) -> Peaks {
        let lit: Vec<&Light> = lights.iter().filter(|l| self.keys.iter().any(|k| k.pos == l.pos)).collect();
        // the brightest key's light: how strongly it is lit, by how much light its colour has
        let strongest = lit.iter().map(|l| { let c = key_colour(l); 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }).fold(0.0f32, f32::max) * white;
        let clean = lit.iter().map(|l| clean_limit(l, ceiling) * l.strength.clamp(0.0, 1.0)).fold(0.0f32, f32::max);
        let g = self.gain(white, ceiling) * strongest;
        let m = self.most;
        Peaks { legend: g * m.legend, deck: g * m.deck, skirt: g * m.skirt, chassis: g * m.chassis, clean: clean * white }
    }

    /// The places of the keys that light.
    pub fn positions(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys.iter().map(|k| k.pos)
    }

    /// The picture with this light: premultiplied linear RGBA, the SDR white
    /// at 1.0 (`white` nits). The keys' light is the simulation's, in their
    /// colours, as strongly as each is lit: through the legends, on the
    /// deck, the chassis and the caps' skirts; the brightest legend, every
    /// key lit white at full, at the core's nits, in luminance, up to Peak
    /// nits (`ceiling` times the white), and the rest as bright as the
    /// simulation has it beside that. Nothing else holds it back: a colour
    /// asked for brighter than the screen can show it goes as asked, and the
    /// screen clips it. The caps' shadows are the room's light blocked, so
    /// the LEDs' light is not under them.
    pub fn compose(&self, lights: &[Light], locks: u8, white: f32, ceiling: f32) -> Vec<f32> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut out = vec![0f32; w * h * 4];
        // the keys' light in their colours, made once for every band
        let keys = self.sim.as_deref().map(|snap| {
            let colour = |pos: u32| lights.iter().find(|l| l.pos == pos).map_or([0.0; 3], key_colour);
            let colours: Vec<[f32; 3]> = snap.keys.iter().map(|&p| colour(p)).collect();
            let alike = colours.first().copied().filter(|c| colours.iter().all(|o| o == c));
            let coloured = alike.is_none().then(|| snap.lit(&colours));
            KeysLight { snap, gain: self.gain(white, ceiling), alike, coloured }
        });
        let keys = keys.as_ref();
        // in bands of rows, one a thread: every step below is the pixel's own
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let rows = h.div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for (b, band) in out.chunks_mut(rows * w * 4).enumerate() {
                let y0 = b * rows;
                sc.spawn(move || self.compose_rows(lights, locks, white, ceiling, y0, band, keys));
            }
        });
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn compose_rows(&self, lights: &[Light], locks: u8, white: f32, ceiling: f32, y0: usize, out: &mut [f32], keys: Option<&KeysLight>) {
        let w = self.w as usize;
        let n = out.len() / 4;
        let y1 = y0 + n / w;
        let dec = |v: f32| v.max(0.0).powf(2.2);
        // The body and its print, in linear light, opaque, under the caps'
        // shadows; and the keys' light on the deck and the chassis, where no
        // cap stands over them, which the caps' shadows are not on.
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            let j = y0 * w + i;
            let lit = 1.0 - self.shadow[j];
            for c in 0..3 {
                px[c] = dec(self.under[j * 3 + c]) * lit;
            }
            px[3] = 1.0;
            if let Some(k) = keys.filter(|_| self.caps[j * 4 + 3] < 1.0) {
                let (u, v) = self.unit(j % w, j / w);
                if let Some(a) = k.snap.at(u, v, None).filter(|a| a.code <= 1) {
                    let e = k.at(&a);
                    for c in 0..3 {
                        px[c] += e[c];
                    }
                }
            }
        }
        // the caps over it, and the keys' light on their skirts' outsides
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            let j = y0 * w + i;
            let cap = &self.caps[j * 4..j * 4 + 4];
            let a = cap[3];
            if a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                px[c] = dec(cap[c] / a) * a + px[c] * (1.0 - a);
            }
            if let Some(k) = keys {
                let (u, v) = self.unit(j % w, j / w);
                if let Some(at) = k.snap.at(u, v, None).filter(|at| at.code == 2) {
                    let e = k.at(&at);
                    for c in 0..3 {
                        px[c] += e[c] * a;
                    }
                }
            }
        }
        let light = |pos: u32| lights.iter().find(|l| l.pos == pos);
        // each pixel of a shape in this band: its place here, its index in the shape, and its value
        let each = |s: &Shape, f: &mut dyn FnMut(usize, usize, f32)| {
            for row in 0..s.h as i32 {
                let y = s.y + row;
                if y < y0 as i32 || y >= y1 as i32 {
                    continue;
                }
                for col in 0..s.w as i32 {
                    let x = s.x + col;
                    if x < 0 || x >= w as i32 {
                        continue;
                    }
                    let i = (row * s.w as i32 + col) as usize;
                    let a = s.cover[i];
                    if a > 0.0 {
                        f((y as usize - y0) * w + x as usize, i, a);
                    }
                }
            }
        };
        // Each lit legend: its own look, dark with its LED off (the art's grey
        // dimmed by the core's unlit share), which shows less as the light
        // behind it outshines it; and the light through it, the brightest
        // thing on the keyboard. A remapped key's without its second function.
        for key in &self.keys {
            let Some(s) = &key.legend else { continue };
            let l = light(key.pos);
            let shown = shown(key, l);
            let strength = l.map_or(0.0, |l| l.strength.clamp(0.0, 1.0));
            let print = [0, 1, 2].map(|c| dec(key.print[c]) * self.model.legend_unlit * (1.0 - strength));
            each(s, &mut |d, i, a| {
                let a = a * shown[i];
                if a <= 0.0 {
                    return;
                }
                for c in 0..3 {
                    out[d * 4 + c] = out[d * 4 + c] * (1.0 - a) + print[c] * a;
                }
                if let Some(k) = keys {
                    let j = y0 * w + d;
                    let (u, v) = self.unit(j % w, j / w);
                    if let Some(at) = k.snap.at(u, v, Some(3)) {
                        let e = k.at(&at);
                        for c in 0..3 {
                            out[d * 4 + c] += e[c] * a;
                        }
                    }
                }
            });
        }
        // the lock lights that are on, as the keyboard's own are: each window
        // lit by its LED, and a little of its light out of its edges
        let (nits, spill, rgb) = self.indicator_light;
        let lamp = unit_colour(rgb);
        let top = |n: f32| (n / white.max(1.0)).min(ceiling);
        for ind in &self.indicators {
            if locks & (1 << ind.bit) == 0 {
                continue;
            }
            for (s, k, lit) in [(&ind.window, top(nits), Some(&ind.lit)), (&ind.spill, top(spill), None)] {
                if k <= 0.0 {
                    continue;
                }
                each(s, &mut |d, i, a| {
                    let a = a * k * lit.map_or(1.0, |l| l[i]);
                    for c in 0..3 {
                        out[d * 4 + c] += lamp[c] * a;
                    }
                });
            }
        }
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
    use sim::{Physics, Scene, Simulator};

    /// The light the tests simulate, whatever the core is tuned to: the
    /// brightest legend at 600 nits, and the core's own defaults for the rest.
    fn light() -> Value {
        serde_json::json!({
            "led": 4.5,
            "sim": { "nits": 600 },
            "legend": { "unlit": 0.15 },
            "lcd": { "nits": 400 },
            "wells": "mask_1"
        })
    }

    /// The Z10's board at the size the window first draws it, unlit.
    fn board() -> (Tree, Value, Board) {
        let tree = Tree::from_str(crate::z10::SVG, &usvg::Options::default()).unwrap();
        let mut device: Value = serde_json::from_str(crate::z10::DEVICE).unwrap();
        device["light"] = light();
        let b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        (tree, device, b)
    }

    /// The board lit by a simulation run to `photons` from each LED.
    fn lit_board(photons: usize) -> (Tree, Value, Board) {
        let (tree, device, mut b) = board();
        let scene = Scene::of(&b);
        let t = std::time::Instant::now();
        let snap = Simulator::settle(&scene, Physics::from(&device["light"]), b.led_north(), photons);
        eprintln!("simulated {photons} photons from each of {} LEDs in {:?}", snap.keys.len(), t.elapsed());
        b.set_sim(Arc::new(snap));
        (tree, device, b)
    }

    fn white(b: &Board) -> Vec<Light> {
        b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 255, 255], strength: 1.0, remapped: false }).collect()
    }

    fn lum(f: &[f32], b: &Board, (x, y): (i32, i32)) -> f32 {
        let i = ((y as u32 * b.w + x as u32) * 4) as usize;
        0.2126 * f[i] + 0.7152 * f[i + 1] + 0.0722 * f[i + 2]
    }

    #[test]
    fn the_keyboard_comes_out_whole_and_lit() {
        let t = std::time::Instant::now();
        let (_, _, b) = lit_board(4_000);
        let made = t.elapsed();
        assert!(b.keys.len() >= 100, "lit keys: {}", b.keys.len());
        let lights: Vec<Light> = b.keys.iter().map(|k| Light { pos: k.pos, rgb: [255, 45, 0], strength: 1.0, remapped: false }).collect();
        let t = std::time::Instant::now();
        let frame = b.compose(&lights, 0, 254.0, 1000.0 / 254.0);
        let composed = t.elapsed();
        // every key its own colour, the slower way
        let rainbow: Vec<Light> = b.keys.iter().enumerate().map(|(i, k)| Light { pos: k.pos, rgb: [(i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 53 % 256) as u8], strength: 1.0, remapped: false }).collect();
        let t = std::time::Instant::now();
        let _ = b.compose(&rainbow, 0, 254.0, 1000.0 / 254.0);
        let each = t.elapsed();
        let enc = Encoder::new(254.0);
        let t = std::time::Instant::now();
        let px = enc.encode(&frame);
        let encoded = t.elapsed();
        assert_eq!(px.len(), (1264 * 506 * 8) as usize);
        eprintln!("made and simulated in {made:?}, lit in {composed:?} (every key its own colour {each:?}), encoded in {encoded:?}");
        // the brightest part of the lit keyboard goes beyond the desktop's white
        let brightest = frame.chunks_exact(4).map(|p| p[0] / p[3].max(1e-6)).fold(0f32, f32::max);
        assert!(brightest > 1.2, "the light should go past SDR white: {brightest}");
        if let Some(dir) = std::env::var_os("ECTODECK_HDR_DUMP") {
            std::fs::write(std::path::Path::new(&dir).join("hdr-keyboard-1264x506.raw"), &px).unwrap();
        }
    }

    /// The brightest legend is the core's nits, every key lit white at full;
    /// a legend is brightest over its LED; the deck by a key's skirt is
    /// brightest on its LED's side; the chassis catches the light out of the
    /// wells; and nothing lights the page round the keyboard.
    #[test]
    fn the_light_falls_as_the_simulation_has_it() {
        let (_, device, b) = lit_board(20_000);
        let px = |u: f32, v: f32| ((b.art_offset.0 + b.art_scale * u) as i32, (b.art_offset.1 + b.art_scale * v) as i32);
        let lit = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
        let dark = b.compose(&[], 0, 254.0, 1000.0 / 254.0);
        let add = |p: (i32, i32)| (lum(&lit, &b, p) - lum(&dark, &b, p)) * 254.0;
        // the brightest legend at the core's nits
        let mut brightest = 0f32;
        for key in &b.keys {
            let Some(l) = &key.legend else { continue };
            for (i, c) in l.cover.iter().enumerate() {
                if *c > 0.99 {
                    brightest = brightest.max(add((l.x + (i as u32 % l.w) as i32, l.y + (i as u32 / l.w) as i32)));
                }
            }
        }
        eprintln!("the brightest legend: {brightest:.1} nits");
        assert!((brightest - 600.0).abs() < 600.0 * 0.03, "the brightest legend is the core's nits: {brightest}");
        // W: its legend brighter by its LED (north) than further off
        let c = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == "W").unwrap();
        let key = b.keys.iter().find(|k| k.pos as u64 == c["pos"].as_u64().unwrap()).unwrap();
        let (f, north) = (key.foot.unwrap(), b.led_north());
        let (cx, cy) = ((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0);
        let l = key.legend.as_ref().unwrap();
        let by = |far: bool| {
            let mut best = (0f32, f32::MAX);
            for (i, cov) in l.cover.iter().enumerate() {
                if *cov > 0.99 {
                    let (x, y) = (l.x + (i as u32 % l.w) as i32, l.y + (i as u32 / l.w) as i32);
                    let (_, v) = b.unit(x as usize, y as usize);
                    let d = (v - (cy - north)).abs();
                    if (!far && d < best.1) || (far && (best.1 == f32::MAX || d > best.1)) {
                        best = (add((x, y)), d);
                    }
                }
            }
            best
        };
        let (near, far) = (by(false), by(true));
        eprintln!("W's legend: {:.0} nits {:.1} units from its LED, {:.0} nits {:.1} away", near.0, near.1, far.0, far.1);
        assert!(near.0 > far.0 * 1.3, "a legend is brightest by its LED");
        // the deck just outside W's skirt, W lit alone: brighter on its LED's side than its far side
        let alone = b.compose(&[Light { pos: key.pos, rgb: [255, 255, 255], strength: 1.0, remapped: false }], 0, 254.0, 1000.0 / 254.0);
        let by_w = |p: (i32, i32)| (lum(&alone, &b, p) - lum(&dark, &b, p)) * 254.0;
        let (n, s) = (by_w(px(cx, f[1] - 0.7)), by_w(px(cx, f[3] + 0.7)));
        eprintln!("the deck by W's skirt, W lit alone: north {n:.2} nits, south {s:.2}");
        assert!(n > s * 1.5 && n > 0.05, "the deck is brighter on the LED's side");
        // the chassis above the Escape key's well, which starts at 59.26 in the art
        let esc = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == "Escape").unwrap();
        let ek = b.keys.iter().find(|k| k.pos as u64 == esc["pos"].as_u64().unwrap()).unwrap();
        let ex = (ek.foot.unwrap()[0] + ek.foot.unwrap()[2]) / 2.0;
        let (by_well, further, off) = (add(px(ex, 58.0)), add(px(ex, 45.0)), add(px(ex, -1.5)));
        eprintln!("the chassis above Escape: by its well {by_well:.3} nits, further off {further:.3}, off the keyboard {off:.4}");
        assert!(by_well > further, "the chassis is lit most by the wells");
        assert!(off.abs() < 1e-3, "nothing lights the page round the keyboard");
        let p = b.peaks(&white(&b), 254.0, 1000.0 / 254.0);
        eprintln!("brightest: legend {:.0} nits, deck {:.1}, skirts {:.1}, chassis {:.2}", p.legend, p.deck, p.skirt, p.chassis);
        assert!((p.legend - 600.0).abs() < 1.0 && p.deck > 0.0 && p.chassis > 0.0 && p.skirt > 0.0);
    }

    /// Each key's light is its own: one key lit alone lights its own legend
    /// and the deck round it; two lit together give each one's light added
    /// up; and a red gives off its own share of white's light.
    #[test]
    fn each_key_s_light_is_its_own() {
        let (_, device, b) = lit_board(4_000);
        let pos = |label: &str| device["controls"].as_array().unwrap().iter().find(|c| c["label"] == label).unwrap()["pos"].as_u64().unwrap() as u32;
        let one = |p: u32, rgb: [u8; 3]| Light { pos: p, rgb, strength: 1.0, remapped: false };
        let dark = b.compose(&[], 0, 254.0, 1000.0 / 254.0);
        let (w, e) = (pos("W"), pos("E"));
        let a = b.compose(&[one(w, [255, 255, 255])], 0, 254.0, 1000.0 / 254.0);
        let c = b.compose(&[one(e, [255, 255, 255])], 0, 254.0, 1000.0 / 254.0);
        let both = b.compose(&[one(w, [255, 255, 255]), one(e, [255, 255, 255])], 0, 254.0, 1000.0 / 254.0);
        let mut worst = 0f32;
        for i in (0..dark.len()).step_by(4) {
            for ch in 0..3 {
                let sum = a[i + ch] + c[i + ch] - dark[i + ch];
                worst = worst.max((both[i + ch] - sum).abs());
            }
        }
        eprintln!("two keys lit together against each alone, added: {:.2e} at worst", worst);
        assert!(worst < 1e-4, "light adds up");
        // W's own legend is lit; Q's, beside it, not by much. Against W lit
        // black at full, whose legend's LED-off print has gone as a lit one's does.
        let black = b.compose(&[one(w, [0, 0, 0])], 0, 254.0, 1000.0 / 254.0);
        let legend_most = |f: &[f32], p: u32| {
            let key = b.keys.iter().find(|k| k.pos == p).unwrap();
            let l = key.legend.as_ref().unwrap();
            let at = |i: usize| (l.x + (i as u32 % l.w) as i32, l.y + (i as u32 / l.w) as i32);
            (0..l.cover.len()).filter(|i| l.cover[*i] > 0.99).map(|i| lum(f, &b, at(i)) - lum(&black, &b, at(i))).fold(0f32, f32::max) * 254.0
        };
        let (own, next) = (legend_most(&a, w), legend_most(&a, pos("Q")));
        eprintln!("W lit alone: its legend {own:.0} nits, Q's beside it {next:.2}");
        assert!(own > 100.0 && next < own * 0.02, "a key's legend is lit by its own LED");
        // a red: its share of white's light, a fifth of it (sRGB red's luminance in linear light, gamma 2.2)
        let red = b.compose(&[one(w, [255, 0, 0])], 0, 254.0, 1000.0 / 254.0);
        let (rw, rr) = (legend_most(&a, w), legend_most(&red, w));
        eprintln!("W's legend in white {rw:.0} nits, in red {rr:.0}");
        assert!((rr / rw - 0.2126).abs() < 0.01);
    }

    /// A remapped key's second function steps aside, and only that: the
    /// legends with a line of their own at the foot of the cap have one (the
    /// PF row, the keypad's second functions), and a word set in two lines in
    /// the middle of a cap keeps both.
    #[test]
    fn second_functions_are_the_lines_at_the_foot_of_a_cap() {
        let (_, device, b) = board();
        let label = |pos: u32| device["controls"].as_array().unwrap().iter().find(|c| c["pos"].as_u64() == Some(pos as u64)).and_then(|c| c["label"].as_str()).unwrap_or("?").to_string();
        let with: Vec<String> = b.keys.iter().filter(|k| k.second.is_some()).map(|k| label(k.pos)).collect();
        eprintln!("{} keys with a second function: {}", with.len(), with.join(", "));
        for l in ["1", "5", "Keypad7", "Keypad8", "Keypad2", "F1"] {
            assert!(with.iter().any(|w| w == l), "{l} has a second function");
        }
        for l in ["Caps Lock", "W", "E1", "Fn", "Tab"] {
            assert!(!with.iter().any(|w| w == l), "{l} has none");
        }
    }

    /// A lock light shines when its lock is on, at the core's nits, and only then.
    #[test]
    fn lock_lights_follow_the_keyboard_s() {
        let (_, device, b) = board();
        assert_eq!(b.indicators.len(), 3, "Num Lock, Caps Lock and Scroll Lock");
        let peak = |f: &[f32], s: &Shape| {
            let mut most = 0f32;
            for j in 0..s.h {
                for i in 0..s.w {
                    if s.cover[(j * s.w + i) as usize] > 0.99 {
                        let k = (((s.y + j as i32) as u32 * b.w + (s.x + i as i32) as u32) * 4) as usize;
                        most = most.max(0.2126 * f[k] + 0.7152 * f[k + 1] + 0.0722 * f[k + 2]);
                    }
                }
            }
            most * 254.0
        };
        let off = b.compose(&[], 0, 254.0, 1000.0 / 254.0);
        let caps = b.compose(&[], 0b010, 254.0, 1000.0 / 254.0);
        let nits = device["indicators"]["nits"].as_f64().unwrap() as f32;
        for ind in &b.indicators {
            let s = &ind.window;
            let (dark, lit) = (peak(&off, s), peak(&caps, s));
            let corner = ind.lit.iter().zip(&s.cover).filter(|(_, c)| **c > 0.99).map(|(l, _)| *l).fold(1.0f32, f32::min);
            eprintln!("light {}: {dark:.1} nits off, {lit:.1} with Caps Lock on; its corners {:.0}% of its middle; a dot is {:.2} art units", ind.bit, corner * 100.0, 1.0 / b.art_scale);
            if ind.bit == 1 {
                assert!((lit - dark) / nits > 0.98 && lit - dark < nits + 0.5, "Caps Lock's light is at its nits over the LED");
                assert!(corner < 0.95 && corner > 0.3, "and dimmer to its corners");
                // a little of it on the chassis just past the window's edge, none further off
                let sp = &ind.spill;
                let edge = (0..sp.cover.len()).map(|i| sp.cover[i]).fold(0.0f32, f32::max);
                assert!(edge > 0.5, "the spill starts at the window's edge");
            } else {
                assert!((lit - dark).abs() < 1e-3, "the others stay off");
            }
        }
    }

    /// For tuning the simulation's defaults by hand: the light of
    /// ECTODECK_SIM (the core's "sim", as JSON over the tests' own), how
    /// bright each part is at its brightest and typically, and how evenly
    /// the wide legends are lit. Run with --ignored.
    #[test]
    #[ignore]
    fn calibrate() {
        let (tree, mut device, _) = board();
        if let Ok(j) = std::env::var("ECTODECK_SIM") {
            let more: Value = serde_json::from_str(&j).unwrap();
            for (k, v) in more.as_object().unwrap() {
                if k == "led" {
                    device["light"]["led"] = v.clone();
                } else {
                    device["light"]["sim"][k] = v.clone();
                }
            }
        }
        let mut b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        let photons = std::env::var("ECTODECK_SIM_PHOTONS").ok().and_then(|v| v.parse().ok()).unwrap_or(40_000);
        let snap = Simulator::settle(&Scene::of(&b), Physics::from(&device["light"]), b.led_north(), photons);
        b.set_sim(Arc::new(snap));
        let nits = device["light"]["sim"]["nits"].as_f64().unwrap_or(600.0) as f32;
        let lit = b.compose(&white(&b), 0, 254.0, 4000.0 / 254.0);
        // every key lit black at full: no light, and the legends' LED-off print gone as a lit key's is
        let black: Vec<Light> = white(&b).into_iter().map(|l| Light { rgb: [0, 0, 0], ..l }).collect();
        let dark = b.compose(&black, 0, 254.0, 4000.0 / 254.0);
        let w = b.w as usize;
        let add = |j: usize| (lum(&lit, &b, ((j % w) as i32, (j / w) as i32)) - lum(&dark, &b, ((j % w) as i32, (j / w) as i32))) * 254.0;
        let s = b.sim.as_deref().unwrap();
        let (mut deck, mut chassis, mut skirt) = (Vec::new(), Vec::new(), Vec::new());
        for j in 0..w * b.h as usize {
            let (u, v) = b.unit(j % w, j / w);
            let Some(a) = s.at(u, v, None) else { continue };
            let cap = b.caps[j * 4 + 3];
            match a.code {
                0 if cap < 0.01 => deck.push(add(j)),
                1 if cap < 0.01 => chassis.push(add(j)),
                2 if cap > 0.99 => skirt.push(add(j)),
                _ => {}
            }
        }
        let stat = |v: &mut Vec<f32>| {
            v.sort_by(|a, b| a.total_cmp(b));
            let q = |f: f32| v.get(((v.len() as f32 - 1.0) * f) as usize).copied().unwrap_or(0.0);
            format!("most {:6.1}  99% {:6.1}  90% {:6.1}  half {:6.2}", q(1.0), q(0.99), q(0.9), q(0.5))
        };
        let p = b.peaks(&white(&b), 254.0, 4000.0 / 254.0);
        eprintln!("at a {nits:.0}-nit legend: legend {:.0}", p.legend);
        eprintln!("  deck     {}", stat(&mut deck));
        eprintln!("  chassis  {}", stat(&mut chassis));
        eprintln!("  skirts   {}", stat(&mut skirt));
        // every legend's solid parts together, and each part's share of their bright tenth
        let mut legends = Vec::new();
        for key in &b.keys {
            let Some(l) = &key.legend else { continue };
            for (i, c) in l.cover.iter().enumerate() {
                if *c > 0.99 {
                    legends.push(add(((l.y + (i as u32 / l.w) as i32) as usize) * w + (l.x + (i as u32 % l.w) as i32) as usize));
                }
            }
        }
        eprintln!("  legends  {}", stat(&mut legends));
        let q = |v: &Vec<f32>, f: f32| v.get(((v.len() as f32 - 1.0) * f) as usize).copied().unwrap_or(0.0);
        let top = q(&legends, 0.9).max(1e-6);
        // the second functions, at the foot of their caps, against the legends over them
        let mut seconds: Vec<f32> = b
            .keys
            .iter()
            .filter_map(|k| {
                let (l, r) = (k.legend.as_ref()?, k.second?);
                let most = |f: &dyn Fn(u32) -> bool| (0..l.cover.len()).filter(|i| l.cover[*i] > 0.5 && f(*i as u32 / l.w)).map(|i| add(((l.y + (i as u32 / l.w) as i32) as usize) * w + (l.x + (i as u32 % l.w) as i32) as usize)).fold(0f32, f32::max);
                Some(most(&|row| row >= r) / most(&|row| row < r).max(1e-6))
            })
            .collect();
        seconds.sort_by(|a, b| a.total_cmp(b));
        eprintln!("  SECONDS {:.1}% of the legends over them, half of them", q(&seconds, 0.5) * 100.0);
        eprintln!("  RATIOS deck90 {:.2}% deck50 {:.2}% chassis99 {:.2}% chassis90 {:.2}% chassis75 {:.2}% skirt99 {:.2}% skirt90 {:.2}% legend10 {:.1}% legend50 {:.1}%",
            q(&deck, 0.9) / top * 100.0, q(&deck, 0.5) / top * 100.0, q(&chassis, 0.99) / top * 100.0, q(&chassis, 0.9) / top * 100.0, q(&chassis, 0.75) / top * 100.0,
            q(&skirt, 0.99) / top * 100.0, q(&skirt, 0.9) / top * 100.0, q(&legends, 0.1) / top * 100.0, q(&legends, 0.5) / top * 100.0);
        // the wide legends, and the keys' legends as a whole: their dimmest solid part against their brightest
        let label = |pos: u32| device["controls"].as_array().unwrap().iter().find(|c| c["pos"].as_u64() == Some(pos as u64)).and_then(|c| c["label"].as_str()).unwrap_or("?").to_string();
        let mut spans = Vec::new();
        for key in &b.keys {
            let Some(l) = &key.legend else { continue };
            let v: Vec<f32> = (0..l.cover.len()).filter(|i| l.cover[*i] > 0.99).map(|i| add(((l.y + (i as u32 / l.w) as i32) as usize) * w + (l.x + (i as u32 % l.w) as i32) as usize)).collect();
            if v.is_empty() {
                continue;
            }
            let (lo, hi) = (v.iter().copied().fold(f32::MAX, f32::min), v.iter().copied().fold(0f32, f32::max));
            spans.push((label(key.pos), lo, hi));
        }
        for name in ["Backspace", "Enter", "Caps Lock", "Print Screen", "1", "W", "Keypad7"] {
            if let Some((n, lo, hi)) = spans.iter().find(|s| s.0 == name) {
                eprintln!("  legend {n:13} dimmest {lo:6.1}  brightest {hi:6.1}");
            }
        }
        let mut his: Vec<f32> = spans.iter().map(|s| s.2).collect();
        his.sort_by(|a, b| a.total_cmp(b));
        eprintln!("  legends' brightest, half of them under {:.0}; the dimmest key's {:.0}", his[his.len() / 2], his[0]);
        spans.sort_by(|a, b| a.2.total_cmp(&b.2));
        eprintln!("  the dimmest keys: {}", spans.iter().take(6).map(|s| format!("{} {:.0}", s.0, s.2)).collect::<Vec<_>>().join(", "));
        // ECTODECK_SIM_CLASSES: each pixel's part, for comparing pictures by
        // it: 1 deck, 2 chassis, 3 skirt, 4 a legend's solid part, 0 the rest
        if let Ok(path) = std::env::var("ECTODECK_SIM_CLASSES") {
            let mut m = vec![0u8; w * b.h as usize];
            for (j, c) in m.iter_mut().enumerate() {
                let (u, v) = b.unit(j % w, j / w);
                let Some(a) = s.at(u, v, None) else { continue };
                let cap = b.caps[j * 4 + 3];
                *c = match a.code {
                    0 if cap < 0.01 => 1,
                    1 if cap < 0.01 => 2,
                    2 if cap > 0.99 => 3,
                    _ => 0,
                };
            }
            for key in &b.keys {
                let Some(l) = &key.legend else { continue };
                for (i, c) in l.cover.iter().enumerate() {
                    if *c > 0.99 {
                        m[((l.y + (i as u32 / l.w) as i32) as usize) * w + (l.x + (i as u32 % l.w) as i32) as usize] = 4;
                    }
                }
            }
            std::fs::write(path, m).unwrap();
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
