//! A keyboard as one HDR picture, drawn from its core's art.
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


/// A cap as the art draws it, for choosing it in the Light lab: its
/// control's id, what the core calls its key (and more briefly, where the
/// core says so), its foot (its skirt's outline) in the art's units, and
/// whether it has an LED and a legend of its own. A button with no light
/// of its own, standing in the keys' light, is one too: a moulded block
/// rather than a cap over a switch.
#[derive(Clone, Debug, PartialEq)]
pub struct CapShape {
    pub id: String,
    pub label: String,
    pub short: Option<String>,
    pub foot: [f32; 4],
    pub lit: bool,
    pub legend: bool,
    pub button: bool,
}

/// A key in section, north to south, as the art has it, in millimetres, for
/// the Light lab to draw: its foot and its face's inset from it; the deck
/// between its well's north wall and its foot; the key south of it (the gap
/// to its foot, and that foot); north of its well, the chassis to the next
/// well and that well's deck south of its keys; and its legends, north to
/// south, each one's middle south of the foot's (north below 0) and how far
/// it runs north to south. What the art does not have is none.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyGeometry {
    pub foot: f32,
    pub inset: f32,
    pub deck_north: Option<f32>,
    pub south: Option<(f32, f32)>,
    pub strip: Option<(f32, f32)>,
    pub legends: Vec<(f32, f32)>,
}

/// A control's id, as the core names it (or its number in the keyboard's
/// reports, where a core names none).
pub fn control_id(c: &Value) -> String {
    c["id"].as_str().map(str::to_string).or_else(|| c["pos"].as_u64().map(|p| p.to_string())).unwrap_or_default()
}

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

impl Light {
    /// How much light its LED gives off, 0 to 1: how strongly it is lit, times
    /// its colour's own brightness in linear light, so a key lit black is a
    /// key with its LED off.
    pub fn shine(&self) -> f32 {
        self.strength.clamp(0.0, 1.0) * lin(self.rgb.iter().copied().max().unwrap_or(0))
    }
}

/// A shape on the board: how much of something there is at each pixel, in
/// its own small box.
#[derive(Clone, Debug)]
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
    /// its cap's place among the simulation's caps (as cap_shapes counts them)
    cap: Option<u32>,
    /// the cap's foot (its skirt's outline) and its face, in the art's units,
    /// and the radius of each one's rounded corners
    foot: Option<[f32; 4]>,
    face: [f32; 4],
    round: [f32; 2],
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
#[derive(Clone, Copy, Debug, PartialEq)]
struct Model {
    /// the light each LED gives off at full white, in lumens, and how many
    /// of the art's units make a millimetre: what make the simulation's light
    /// its nits (see gain)
    lumens: f32,
    scale: f32,
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
        let d = &l["display"];
        Model {
            lumens: f(&l["switches"]["lumens"], 1.0).max(0.0),
            scale: f(&l["scale"], 1.0).max(0.01),
            legend_unlit: f(&l["legends"]["unlit"], 0.15).clamp(0.0, 1.0),
            lcd_nits: f(&d["lit"], 324.0).max(0.0),
            lcd_unlit: f(&d["unlit"], 4.1).max(0.0),
            lcd_field: f(&d["background"], 0.8).max(0.0),
            lcd_dot: f(&d["dot"], 0.84).clamp(0.05, 1.0),
            lcd_glass: f(&d["glass"], 1.0).max(0.0),
        }
    }
}

/// A colour as the core writes it, "#rrggbb".
fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').filter(|s| s.len() == 6)?;
    let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

/// The display's light, from the core's light's "display".
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
    /// the caps, and the legends of unlit keys: premultiplied, sRGB-encoded
    caps: Vec<f32>,
    keys: Vec<KeyArt>,
    /// the caps of the keys with no light of their own (the spacebar, the
    /// media keys): their feet and faces in the art's units, and their
    /// corners' radii, which stand in the light's way
    others: Vec<([f32; 4], [f32; 4], [f32; 2])>,
    /// each cap's foot's own shape, where it is no rounded box, in the
    /// simulation's order of the caps
    odd_feet: Vec<Option<Shape>>,
    /// every cap, lit or not, and the controls they are: the lit keys' in
    /// their order, then the others', as the simulation counts its caps
    pub cap_shapes: Vec<CapShape>,
    model: Model,
    /// where the chassis is: a pixel's coverage
    chassis: Vec<f32>,
    /// each well's box in the art's units, left, top, right, bottom (the
    /// wells' clip's pieces, in order; with no wells in the art, the board's),
    /// and the radius its corners are rounded to
    well_boxes: Vec<[f32; 4]>,
    well_round: Vec<f32>,
    /// the art's own units (the SVG's, which the core's lengths are in) to the
    /// board's pixels: pixel = offset + scale x unit
    pub(crate) art_scale: f32,
    pub(crate) art_offset: (f32, f32),
    /// the lock lights, and their light: the window at its brightest and the
    /// light out of its edges, in nits, and its colour (sRGB)
    indicators: Vec<Indicator>,
    indicator_light: (f32, f32, [u8; 3]),
    /// the board with none of the keys' light, in linear light, opaque: the
    /// body under the caps' shadows and the caps over it (it never changes,
    /// so a picture starts from it)
    base: Vec<f32>,
    /// the simulation's latest picture of the keys' light, and its brightest on each part
    sim: Option<Arc<Snapshot>>,
    most: Most,
    /// where each pixel reads the simulation's light, as seen from above,
    /// and each legend's pixels through its cap's top (a key's in its order):
    /// worked out once for the simulation's shapes, which every picture of
    /// one run shares, rather than for every picture drawn
    reads: Vec<Read>,
    legend_reads: Vec<Vec<Read>>,
    /// the pixels on the caps' feet's edges, in order, with the light of what
    /// their other part shows
    edges: Vec<Edge>,
    /// each lit key's milky band, below where the black of its cap's top
    /// comes down its sides (a key's in its order), the other caps' bands
    /// (in theirs), how much of each pixel a band covers, and for how far
    /// down each cap's black comes, by the simulation's order of the caps
    bands: Vec<Option<Shape>>,
    other_bands: Vec<Option<Shape>>,
    band_map: Vec<f32>,
    bands_for: Option<Vec<u32>>,
    /// which cap each pixel is on, by the simulation's order of the caps
    /// (NO_CAP off them all), for the light each cap's own physics gives it
    cap_of: Vec<u16>,
    reads_for: Option<(usize, usize)>,
}

/// How far either side of a cap's foot the light either side of its edge
/// is read: a cell of the simulation's fine grid, clear of the edge.
const EDGE_READ: f32 = 0.5;

/// A pixel on no cap, in the board's map of which cap each pixel is on.
const NO_CAP: u16 = u16::MAX;

/// Which cap each of the board's pixels is on, by the caps' order in `feet`
/// (each its foot in the art's units and its corners' radius): the one it is
/// furthest inside, of those whose foot's box (and a pixel round it, as a
/// cap's band has) it is in; NO_CAP for the rest.
fn cap_map(w: u32, h: u32, ka: f32, (ox, oy): (f32, f32), feet: &[([f32; 4], f32)]) -> Vec<u16> {
    let mut map = vec![NO_CAP; (w * h) as usize];
    let mut depth = vec![f32::MIN; (w * h) as usize];
    for (i, &(f, r)) in feet.iter().enumerate().take(NO_CAP as usize) {
        let (x0, y0) = (((ox + ka * f[0]).floor() as i32 - 1).max(0), ((oy + ka * f[1]).floor() as i32 - 1).max(0));
        let (x1, y1) = (((ox + ka * f[2]).ceil() as i32 + 1).min(w as i32), ((oy + ka * f[3]).ceil() as i32 + 1).min(h as i32));
        for y in y0..y1 {
            for x in x0..x1 {
                let j = (y as u32 * w + x as u32) as usize;
                let (u, v) = ((x as f32 + 0.5 - ox) / ka, (y as f32 + 0.5 - oy) / ka);
                let inside = sim::from_foot(f, r, u, v).0;
                if inside > depth[j] {
                    (depth[j], map[j]) = (inside, i as u16);
                }
            }
        }
    }
    map
}

/// Milky plastic's own colour, sRGB-encoded: the grey the art prints the
/// legends in (the same plastic), a little warm, as milky white is.
const MILKY_WHITE: [f32; 3] = [0x9e as f32 / 255.0, 0x9b as f32 / 255.0, 0x93 as f32 / 255.0];

/// A pixel on the edge of a cap's foot, which the cap covers part way (`j`,
/// the board's index), and the light of its other part, read clear of the
/// edge where the foot is nearest: for a pixel the simulation puts on the
/// skirt (its cells are coarser than the art), the deck's or the chassis's
/// just outside the foot, where the cap leaves it uncovered; for one it puts
/// outside, the skirt's just inside the foot, where the cap covers it.
#[derive(Clone, Copy)]
struct Edge {
    j: u32,
    other: Read,
}

/// Where a pixel reads the simulation's light, as sim::At has it, kept small:
/// the first of its four cells on each grid, whether the others are a step
/// across and down from it (the grid's edge has none), and their shares.
#[derive(Clone, Copy)]
struct Read {
    code: u8,
    steps: u8,
    fine: u32,
    coarse: u32,
    fw: [f32; 4],
    cw: [f32; 4],
}

impl Read {
    const NONE: Read = Read { code: 255, steps: 0, fine: 0, coarse: 0, fw: [0.0; 4], cw: [0.0; 4] };

    fn of(a: Option<sim::At>, fine_w: usize, coarse_w: usize) -> Read {
        let Some(a) = a else { return Read::NONE };
        let step = |c: &[(usize, f32); 4], w: usize| ((c[1].0 - c[0].0) as u8) | ((((c[2].0 - c[0].0) / w.max(1)) as u8) << 1);
        Read {
            code: a.code,
            steps: step(&a.fine, fine_w) | (step(&a.coarse, coarse_w) << 2),
            fine: a.fine[0].0 as u32,
            coarse: a.coarse[0].0 as u32,
            fw: a.fine.map(|c| c.1),
            cw: a.coarse.map(|c| c.1),
        }
    }

    fn at(&self, fine_w: usize, coarse_w: usize) -> sim::At {
        let cells = |first: u32, steps: u8, w: usize, ws: [f32; 4]| {
            let (i, dx, dy) = (first as usize, (steps & 1) as usize, ((steps >> 1) & 1) as usize * w);
            [(i, ws[0]), (i + dx, ws[1]), (i + dy, ws[2]), (i + dy + dx, ws[3])]
        };
        sim::At { code: self.code, fine: cells(self.fine, self.steps, fine_w, self.fw), coarse: cells(self.coarse, self.steps >> 2, coarse_w, self.cw) }
    }
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
    /// each cap's, by the simulation's order of them: how much more of its
    /// sides' light its band takes, where the black of its top comes down
    /// them (none where it does not); and how its translucent top glows
    /// beside its legend, for a legend's share of its light
    to_band: Vec<f32>,
    top_glow: Vec<f32>,
    /// and whether its top is lower than another cap's, which only then can
    /// send light down onto it (a button on the chassis among the caps)
    top_lit: Vec<bool>,
}

impl KeysLight<'_> {
    /// A cap's band's and top's share of its light, and whether light comes
    /// down on its top (see to_band, top_glow and top_lit); none off the caps.
    fn cap(&self, c: u16) -> (f32, f32, bool) {
        let c = c as usize;
        (self.to_band.get(c).copied().unwrap_or(0.0), self.top_glow.get(c).copied().unwrap_or(0.0), self.top_lit.get(c).copied().unwrap_or(false))
    }

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
        let shape_of = |n: &usvg::Node| -> Option<Shape> {
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
        let shape = |id: &str| shape_of(node(id)?);
        // a cap's foot where it is no rounded box (a spacebar whose walls step
        // in over the chassis): its shape, which the simulation keeps its cap to
        let odd_foot = |n: &usvg::Node| match n {
            usvg::Node::Path(p) if p.data().segments().count() > 10 => shape_of(n),
            _ => None,
        };
        let children = |id: &str| -> Vec<usvg::Node> {
            match node(id) {
                Some(usvg::Node::Group(g)) => g.children().to_vec(),
                _ => Vec::new(),
            }
        };

        // Each control's pieces in the art, by its number there ("art"): its
        // cap or button control_<art>, its legend legend_<100 + art> (or the
        // one it names, in art numbered otherwise); a key has an LED unless
        // the core says it has none ("lit": false)
        let controls = device["controls"].as_array().ok_or("the device names no controls")?;
        let sliders = |c: &Value| c.get("slider").is_some();
        let lit = |c: &Value| !sliders(c) && c["lit"].as_bool() != Some(false);
        let legend_of = |c: &Value| c["legend"].as_u64().or_else(|| c["art"].as_u64().map(|n| 100 + n));
        let label = |c: &Value| c["label"].as_str().unwrap_or_default().to_string();
        let short = |c: &Value| c["short"].as_str().map(str::to_string);
        // whether one box lies inside another, not all of it
        let inside = |a: [f32; 4], b: [f32; 4]| a[0] >= b[0] && a[1] >= b[1] && a[2] <= b[2] && a[3] <= b[3] && a != b;
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
        let under: Vec<f32> = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();

        // Every cap but the slider knobs (the page moves those): its shadow (its
        // first piece, drawn blurred) apart from the rest, since the shadow is the
        // room's light blocked, and the LED's light comes from under the cap; and
        // the legends of keys with no light behind them. The media buttons are
        // moulded blocks with no shadow of their own: all their pieces are the block.
        let mut shade = Pixmap::new(w, h).ok_or("board too small")?;
        let mut pix = Pixmap::new(w, h).ok_or("board too small")?;
        for c in controls.iter().filter(|c| !sliders(c)) {
            let Some(n) = c["art"].as_u64() else { continue };
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
            if let Some(n) = legend_of(c) {
                draw(&mut pix, &format!("legend_{n}"));
            }
        }
        let shadow: Vec<f32> = shade.data().chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
        // premultiplied, in sRGB's own encoding
        let caps: Vec<f32> = pix.data().chunks_exact(4).flat_map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]).collect();

        // The wells, where the deck is open to the keys' light: a clip in the
        // art, in the art's own units, as the group it clips has them. That
        // group's transform takes the art's units to the drawing's.
        let wells_id = device["light"]["wells"]["art"].as_str().unwrap_or("mask_1");
        let found = find_clip(tree.root(), wells_id);
        let art = found.map_or(Transform::identity(), |(g, _)| g.abs_transform());
        // the art's units to the board's pixels
        let ka = k * art.sx;
        let (ox, oy) = (tx + k * art.tx, ty + k * art.ty);
        // each well on its own: its box in the art's units, from its piece of
        // the clip, and its corners' radius (a rounded box starts its top edge a
        // radius in from its corner, as the art's caps do); with no wells in the
        // art, the board is one
        let (mut well_boxes, mut well_round): (Vec<[f32; 4]>, Vec<f32>) = (Vec::new(), Vec::new());
        if let Some((_, clip)) = found {
            for n in clip.root().children() {
                let usvg::Node::Path(p) = n else { continue };
                let b = p.data().bounds();
                let to_art = clip.transform().pre_concat(p.abs_transform());
                let mut corners = [(b.left(), b.top()), (b.right(), b.top()), (b.left(), b.bottom()), (b.right(), b.bottom())].map(|(x, y)| resvg::tiny_skia::Point::from_xy(x, y));
                to_art.map_points(&mut corners);
                let (xs, ys) = (corners.map(|p| p.x), corners.map(|p| p.y));
                let lo = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
                let hi = |v: [f32; 4]| v.into_iter().fold(-f32::MAX, f32::max);
                well_boxes.push([lo(xs), lo(ys), hi(xs), hi(ys)]);
                let r = match p.data().points().first() {
                    Some(first) if (first.y - b.top()).abs() < 1e-3 => (first.x - b.left()).clamp(0.0, b.width().min(b.height()) / 2.0) * to_art.sx.abs(),
                    _ => 0.0,
                };
                well_round.push(r);
            }
        }
        if well_boxes.is_empty() {
            well_boxes.push([-ox / ka, -oy / ka, (w as f32 - ox) / ka, (h as f32 - oy) / ka]);
            well_round.push(0.0);
        }
        // a piece's box, in the art's units
        let rect = |n: &usvg::Node| {
            let b = n.abs_bounding_box();
            [(b.left() - art.tx) / art.sx, (b.top() - art.ty) / art.sy, (b.right() - art.tx) / art.sx, (b.bottom() - art.ty) / art.sy]
        };
        // a piece's corners' radius, in the art's units: the art draws a cap's
        // foot and face as boxes with rounded corners, each from the start of
        // its top edge, a radius in from its box's corner
        let round = |n: &usvg::Node| -> f32 {
            let usvg::Node::Path(p) = n else { return 0.0 };
            let (b, Some(first)) = (p.data().bounds(), p.data().points().first()) else { return 0.0 };
            if (first.y - b.top()).abs() > 1e-3 {
                return 0.0;
            }
            (first.x - b.left()).clamp(0.0, b.width().min(b.height()) / 2.0) * p.abs_transform().sx / art.sx
        };
        // each lit key: its legend, and its cap's shape, which its light is simulated in
        let mut keys = Vec::new();
        let mut cap_shapes = Vec::new();
        let mut odd_feet = Vec::new();
        for c in controls.iter().filter(|c| lit(c)) {
            let Some(pos) = c["pos"].as_u64() else { continue };
            let legend_n = legend_of(c);
            let print = legend_n.and_then(|n| tree.node_by_id(&format!("legend_{n}"))).and_then(first_fill).unwrap_or([0x9e, 0x9e, 0x9e]);
            let print = [print[0] as f32 / 255.0, print[1] as f32 / 255.0, print[2] as f32 / 255.0];
            let legend = legend_n.and_then(|n| shape(&format!("legend_{n}")));
            let parts = c["art"].as_u64().map(|n| children(&format!("control_{n}"))).unwrap_or_default();
            let (foot, face, rounds) = match (parts.get(1), parts.get(2)) {
                (Some(f), Some(a)) => (Some(rect(f)), rect(a), [round(f), round(a)]),
                _ => (None, [0.0; 4], [0.0; 2]),
            };
            let second = legend.as_ref().and_then(|s| second_function(s, face, ka, oy));
            let cap = foot.map(|_| cap_shapes.len() as u32);
            if let Some(f) = foot {
                cap_shapes.push(CapShape { id: control_id(c), label: label(c), short: short(c), foot: f, lit: true, legend: legend.is_some(), button: false });
                odd_feet.push(parts.get(1).and_then(odd_foot));
            }
            keys.push(KeyArt { pos: pos as u32, cap, foot, face, round: rounds, legend, second, print });
        }
        // The controls with no light of their own, which stand in its way and
        // catch it: the caps of the keys with none (a shadow, then the cap's
        // foot and face), and the buttons, moulded blocks with no shadow (their
        // first piece the block; its face the next piece where that lies
        // inside it, as a button's in its bezel, or else all of its top).
        let mut others = Vec::new();
        for c in controls.iter().filter(|c| !lit(c) && !sliders(c)) {
            let Some(n) = c["art"].as_u64() else { continue };
            let parts = children(&format!("control_{n}"));
            let shadow = parts.len() >= 3 && matches!(&parts[0], usvg::Node::Group(g) if !g.filters().is_empty());
            let (foot, face) = match (shadow, parts.first(), parts.get(1)) {
                (true, ..) => (&parts[1], &parts[2]),
                (false, Some(block), Some(face @ usvg::Node::Path(_))) if inside(rect(face), rect(block)) => (block, face),
                (false, Some(block), _) => (block, block),
                _ => continue,
            };
            others.push((rect(foot), rect(face), [round(foot), round(face)]));
            cap_shapes.push(CapShape { id: control_id(c), label: label(c), short: short(c), foot: rect(foot), lit: false, legend: false, button: !shadow });
            odd_feet.push(odd_foot(foot));
        }
        // The lock lights, from the core's "lock_lights": each a small window
        // with an LED under it (the art's piece the core names), brightest over
        // the LED and dimmer to the window's corners (its diffuser keeps it from
        // falling far), and a little of its light out of the window's edges
        // onto the chassis. Their lengths are in mm, read in the art's units.
        let ll = &device["lock_lights"];
        let num = |v: &Value, d: f32| v.as_f64().map_or(d, |v| v as f32);
        let scale = device["light"]["scale"].as_f64().unwrap_or(1.0).max(0.01);
        let length = |v: &Value, d: f64| (v.as_f64().unwrap_or(d) * scale) as f32;
        let (spread, floor) = (length(&ll["spread"], 1.45).max(0.05), num(&ll["floor"], 0.5).clamp(0.0, 1.0));
        let reach = length(&ll["spill"]["reach"], 0.72).max(0.05);
        let indicators = [("num", 0), ("caps", 1), ("scroll", 2)]
            .into_iter()
            .filter_map(|(lock, bit)| {
                let id = ll[lock].as_str()?;
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
        let rgb = ll["colour"].as_str().and_then(hex).unwrap_or([255; 3]);
        let indicator_light = (num(&ll["brightness"], 300.0).max(0.0), num(&ll["spill"]["brightness"], 6.0).max(0.0), rgb);
        // the board with none of the keys' light: the body and its print (over
        // the ground's colour, sRGB-encoded) in linear light under the caps'
        // shadows (the room's light, which the LEDs' light is not under), the caps over it
        let dec = |v: f32| v.max(0.0).powf(2.2);
        let base: Vec<f32> = (0..(w * h) as usize)
            .flat_map(|j| {
                let (lit, a) = (1.0 - shadow[j], caps[j * 4 + 3]);
                [0, 1, 2].map(|c| {
                    let body = dec(under[j * 3 + c]) * lit;
                    if a > 0.0 { dec(caps[j * 4 + c] / a) * a + body * (1.0 - a) } else { body }
                })
            })
            .collect();
        let feet: Vec<([f32; 4], f32)> = keys.iter().filter_map(|k| k.foot.map(|f| (f, k.round[0]))).chain(others.iter().map(|o| (o.0, o.2[0]))).collect();
        let cap_of = cap_map(w, h, ka, (ox, oy), &feet);
        Ok(Board {
            w,
            h,
            caps,
            keys,
            others,
            odd_feet,
            cap_shapes,
            model,
            chassis,
            well_boxes,
            well_round,
            art_scale: ka,
            art_offset: (ox, oy),
            indicators,
            indicator_light,
            base,
            sim: None,
            most: Most::default(),
            reads: Vec::new(),
            legend_reads: Vec::new(),
            edges: Vec::new(),
            bands: Vec::new(),
            other_bands: Vec::new(),
            band_map: Vec::new(),
            bands_for: None,
            cap_of,
            reads_for: None,
        })
    }

    /// Light the keyboard by another of the core's "light" descriptions (as the
    /// Light lab tunes it): how bright the light is, how a legend looks
    /// with its LED off, the display. How the light falls is the simulation's,
    /// asked of it on its own (see sim::Simulator::set). True if the picture
    /// changes with it; a change in how the light falls shows with the
    /// simulation's next picture.
    pub fn set_light(&mut self, light: &Value) -> bool {
        let model = Model::from(light);
        let changed = model != self.model;
        self.model = model;
        changed
    }

    /// Light the keys by the simulation's latest picture of their light.
    pub fn set_sim(&mut self, snap: Arc<Snapshot>) {
        let shapes = (Arc::as_ptr(&snap.surface) as usize, Arc::as_ptr(&snap.cover) as usize);
        if self.reads_for != Some(shapes) {
            self.read_from(&snap);
            self.reads_for = Some(shapes);
        }
        let blacks: Vec<u32> = (0..self.cap_shapes.len() as u32).map(|c| snap.setup.of(c).black.to_bits()).collect();
        if self.bands_for.as_ref() != Some(&blacks) {
            let black = |c: u32| f32::from_bits(blacks[c as usize]);
            self.bands = self.keys.iter().map(|k| k.foot.zip(k.cap).and_then(|(f, c)| self.band(f, k.face, k.round[0], black(c)))).collect();
            let lit = self.lit_caps();
            self.other_bands = self.others.iter().enumerate().map(|(i, &(f, a, r))| self.band(f, a, r[0], black((lit + i) as u32))).collect();
            // how much of each pixel is band, to keep the skirts' light to them
            let mut map = vec![0f32; (self.w * self.h) as usize];
            for s in self.bands.iter().chain(&self.other_bands).flatten() {
                for (i, c) in s.cover.iter().enumerate() {
                    let (x, y) = (s.x + (i as u32 % s.w) as i32, s.y + (i as u32 / s.w) as i32);
                    if *c > 0.0 && x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
                        let j = (y as u32 * self.w + x as u32) as usize;
                        map[j] = map[j].max(*c);
                    }
                }
            }
            self.band_map = map;
            self.bands_for = Some(blacks);
        }
        self.most = self.measure(&snap);
        self.sim = Some(snap);
    }

    /// Where each pixel reads the simulation's light, and each legend's
    /// pixels, a band of rows (and a share of the keys) to a thread.
    fn read_from(&mut self, s: &Snapshot) {
        let (w, h) = (self.w as usize, self.h as usize);
        let (fw, cw) = (s.fine.w, s.coarse.w);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let mut reads = vec![Read::NONE; w * h];
        let rows = h.div_ceil(threads).max(1);
        let board = &*self;
        std::thread::scope(|sc| {
            for (b, band) in reads.chunks_mut(rows * w).enumerate() {
                sc.spawn(move || {
                    for (i, r) in band.iter_mut().enumerate() {
                        let j = b * rows * w + i;
                        let (u, v) = board.unit(j % w, j / w);
                        *r = Read::of(s.at(u, v, None), fw, cw);
                    }
                });
            }
        });
        let legend_reads: Vec<Vec<Read>> = std::thread::scope(|sc| {
            let per = board.keys.len().div_ceil(threads).max(1);
            let jobs: Vec<_> = board
                .keys
                .chunks(per)
                .map(|keys| {
                    sc.spawn(move || {
                        keys.iter()
                            .map(|key| match &key.legend {
                                None => Vec::new(),
                                Some(l) => l
                                    .cover
                                    .iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        let (x, y) = (l.x + (i as u32 % l.w) as i32, l.y + (i as u32 / l.w) as i32);
                                        if *c <= 0.0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                                            return Read::NONE;
                                        }
                                        let (u, v) = board.unit(x as usize, y as usize);
                                        Read::of(s.at(u, v, Some(3)), fw, cw)
                                    })
                                    .collect(),
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().expect("a share of the legends' reads")).collect()
        });
        // each cap's skirt, a cap a thread's share: its pixels, and the light
        // just outside its foot where each is nearest
        let feet: Vec<([f32; 4], f32)> = board.keys.iter().filter_map(|k| k.foot.map(|f| (f, k.round[0]))).chain(board.others.iter().map(|o| (o.0, o.2[0]))).collect();
        let (ka, (ox, oy)) = (board.art_scale, board.art_offset);
        let read = &reads;
        let near_edges: Vec<(f32, Edge)> = std::thread::scope(|sc| {
            let per = feet.len().div_ceil(threads).max(1);
            let jobs: Vec<_> = feet
                .chunks(per)
                .map(|feet| {
                    sc.spawn(move || {
                        let mut edges = Vec::new();
                        for &(f, r) in feet {
                            // its box, and a pixel round it for the edge's
                            let (x0, y0) = ((ox + ka * f[0] - 1.0).floor().max(0.0) as usize, (oy + ka * f[1] - 1.0).floor().max(0.0) as usize);
                            let (x1, y1) = (((ox + ka * f[2] + 1.0).ceil().max(0.0) as usize).min(w), ((oy + ka * f[3] + 1.0).ceil().max(0.0) as usize).min(h));
                            for y in y0..y1 {
                                for x in x0..x1 {
                                    let j = y * w + x;
                                    let a = board.caps[j * 4 + 3];
                                    let (u, v) = board.unit(x, y);
                                    let (t, n, p) = sim::from_foot(f, r, u, v);
                                    // only by the edge: up to a pixel outside it, as far inside as the cap leaves uncovered
                                    let near = (-t * ka).max(0.0);
                                    if near > 1.0 {
                                        continue;
                                    }
                                    let other = match read[j].code {
                                        2 if a < 1.0 => s.at(p.0 + n.0 * EDGE_READ, p.1 + n.1 * EDGE_READ, None).filter(|a| a.code <= 1),
                                        0 | 1 if a > 0.0 => s.at(p.0 - n.0 * EDGE_READ, p.1 - n.1 * EDGE_READ, Some(2)),
                                        _ => None,
                                    };
                                    if let Some(other) = other {
                                        edges.push((near, Edge { j: j as u32, other: Read::of(Some(other), fw, cw) }));
                                    }
                                }
                            }
                        }
                        edges
                    })
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().expect("a share of the edges' reads")).collect()
        });
        // a pixel between two caps close together goes with the nearer
        let mut near_edges = near_edges;
        near_edges.sort_unstable_by(|a, b| a.1.j.cmp(&b.1.j).then(a.0.total_cmp(&b.0)));
        near_edges.dedup_by_key(|e| e.1.j);
        let edges: Vec<Edge> = near_edges.into_iter().map(|e| e.1).collect();
        self.reads = reads;
        self.legend_reads = legend_reads;
        self.edges = edges;
    }


    /// A cap's band as its art covers it (its foot `f` with its corners'
    /// radius `r`, its face `a`): its skirt from its foot up to where the
    /// black of its top comes `black` of the way down it (the band runs from
    /// the foot to the face as the skirt does from its foot to its top), its
    /// edge shared out over a pixel.
    fn band(&self, f: [f32; 4], a: [f32; 4], r: f32, black: f32) -> Option<Shape> {
        let (w, h) = (self.w as i32, self.h as i32);
        let (ka, (ox, oy)) = (self.art_scale, self.art_offset);
        let (x0, y0) = (((ox + ka * f[0]).floor() as i32 - 1).max(0), ((oy + ka * f[1]).floor() as i32 - 1).max(0));
        let (x1, y1) = (((ox + ka * f[2]).ceil() as i32 + 1).min(w), ((oy + ka * f[3]).ceil() as i32 + 1).min(h));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (bw, bh) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut cover = vec![0f32; (bw * bh) as usize];
        for j in 0..bh as i32 {
            for i in 0..bw as i32 {
                let (x, y) = (x0 + i, y0 + j);
                let cap = self.caps[((y as u32 * self.w + x as u32) * 4 + 3) as usize];
                if cap <= 0.0 {
                    continue;
                }
                // how far in from the foot it is and how far out from the face:
                // the band's edge is (1 - black) of the way from one outline to
                // the other, so its corners round as theirs do
                let (u, v) = self.unit(x as usize, y as usize);
                let in_foot = sim::from_foot(f, r, u, v).0.max(0.0);
                let out_face = (-sim::from_foot(a, r, u, v).0).max(0.0);
                let milky = ((1.0 - black) * (in_foot + out_face) - in_foot) * ka + 0.5;
                cover[(j as u32 * bw + i as u32) as usize] = cap * milky.clamp(0.0, 1.0);
            }
        }
        cover.iter().any(|c| *c > 0.0).then_some(Shape { x: x0, y: y0, w: bw, h: bh, cover })
    }

    /// How many of the simulation's caps are the lit keys' (the others' follow).
    fn lit_caps(&self) -> usize {
        self.keys.iter().filter(|k| k.cap.is_some()).count()
    }

    /// How every key is lit, as a core's light description has it: all
    /// keys' values, and each key's own (its "keys", by the control's id)
    /// over them, for this board's caps.
    pub fn setup(&self, light: &Value) -> sim::Setup {
        let ids: Vec<String> = self.cap_shapes.iter().map(|c| c.id.clone()).collect();
        sim::Setup::from(light, &ids)
    }

    /// Whether the device has wells round its keys.
    pub fn has_wells(&self) -> bool {
        !self.well_boxes.is_empty()
    }

    /// A key in section, north to south, as the art has it (see
    /// KeyGeometry), at `mm` of the art's units to the millimetre; none for a
    /// control that is not a cap.
    pub fn key_geometry(&self, id: &str, mm: f32) -> Option<KeyGeometry> {
        let f = self.cap_shapes.iter().find(|c| c.id == id)?.foot;
        let key = self.keys.iter().find(|k| k.foot == Some(f));
        let face = match key {
            Some(k) => k.face,
            None => self.others.iter().find(|o| o.0 == f)?.1,
        };
        let (mid_x, mid_y) = ((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0);
        let inside = |w: &[f32; 4], x: f32, y: f32| x >= w[0] && x <= w[2] && y >= w[1] && y <= w[3];
        // (a well over its middle; a key anywhere over it, as the rows are staggered)
        let over = |c: &[f32; 4]| c[0] <= mid_x && mid_x <= c[2];
        let beside = |c: &[f32; 4]| c[0] < f[2] && c[2] > f[0];
        // its well; the one north of it, the chassis between, and that well's deck south of its keys
        let well = self.well_boxes.iter().find(|w| inside(w, mid_x, mid_y));
        let deck_north = well.map(|w| (f[1] - w[1]) / mm);
        let strip = well.and_then(|w| {
            let north = self.well_boxes.iter().filter(|n| n[3] <= w[1] + 0.5 && over(n)).max_by(|a, b| a[3].total_cmp(&b[3]))?;
            let last = self.cap_shapes.iter().map(|c| c.foot).filter(|c| beside(c) && inside(north, (c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0)).map(|c| c[3]).reduce(f32::max)?;
            Some(((w[1] - north[3]) / mm, (north[3] - last) / mm))
        });
        // the key south of it
        let south = self.cap_shapes.iter().map(|c| c.foot).filter(|c| c[1] >= f[3] - 0.5 && beside(c)).min_by(|a, b| a[1].total_cmp(&b[1])).map(|c| ((c[1] - f[3]) / mm, (c[3] - c[1]) / mm));
        // its legend, and the second function at the foot of the cap if it has one, each north to south
        let (ka, oy) = (self.art_scale, self.art_offset.1);
        let legends = key
            .and_then(|k| k.legend.as_ref().map(|s| (k, s)))
            .map(|(k, s)| {
                let inked = |r: u32| (0..s.w).any(|c| s.cover[(r * s.w + c) as usize] > 0.25);
                let lines = match k.second {
                    Some(r) => vec![(0, r), (r, s.h)],
                    None => vec![(0, s.h)],
                };
                lines
                    .into_iter()
                    .filter_map(|(a, b)| {
                        let (first, last) = ((a..b).find(|&r| inked(r))?, (a..b).rev().find(|&r| inked(r))?);
                        let (top, bottom) = ((s.y as f32 + first as f32 - oy) / ka, (s.y as f32 + last as f32 + 1.0 - oy) / ka);
                        Some((((top + bottom) / 2.0 - mid_y) / mm, (bottom - top) / mm))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(KeyGeometry { foot: (f[3] - f[1]) / mm, inset: (face[1] - f[1]) / mm, deck_north, south, strip, legends })
    }

    /// How far the simulation the keys are lit by has got: the photons it
    /// has followed from each LED, and whether that is all it follows.
    pub fn sim_progress(&self) -> Option<(usize, bool)> {
        self.sim.as_ref().map(|s| (s.photons, s.settled))
    }

    /// The simulation's picture the keys are lit by.
    pub fn sim(&self) -> Option<&Snapshot> {
        self.sim.as_deref()
    }

    /// The light each LED gives off at full white, in lumens, and the display's lit dots', in nits.
    pub fn led_lumens(&self) -> f32 {
        self.model.lumens
    }
    pub fn lcd_nits(&self) -> f32 {
        self.model.lcd_nits
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
    /// (The keys' legends a share of them to a thread, and the rest of the
    /// keyboard in bands of rows, on as many threads as it is worth: a new
    /// picture comes in while a slider is dragged.)
    fn measure(&self, s: &Snapshot) -> Most {
        let (w, h) = (self.w as usize, self.h as usize);
        let (fw, cw) = (s.fine.w, s.coarse.w);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let legends = |keys: std::ops::Range<usize>| {
            let mut m = Most::default();
            for k in keys {
                let (Some(l), Some(reads)) = (&self.keys[k].legend, self.legend_reads.get(k)) else { continue };
                for (c, r) in l.cover.iter().zip(reads) {
                    if *c >= 0.5 && r.code != 255 {
                        m.legend = m.legend.max(s.white(&r.at(fw, cw)));
                    }
                }
            }
            m
        };
        let rest = |rows: std::ops::Range<usize>| {
            let mut m = Most::default();
            for j in rows.start * w..rows.end * w {
                let (cap, r) = (self.caps[j * 4 + 3], &self.reads[j]);
                match r.code {
                    0 if cap < 0.01 => m.deck = m.deck.max(s.white(&r.at(fw, cw))),
                    1 if cap < 0.01 => m.chassis = m.chassis.max(s.white(&r.at(fw, cw))),
                    2 if cap > 0.99 => m.skirt = m.skirt.max(s.white(&r.at(fw, cw))),
                    _ => {}
                }
            }
            m
        };
        let (per_key, per_row) = (self.keys.len().div_ceil(threads).max(1), h.div_ceil(threads).max(1));
        std::thread::scope(|sc| {
            let parts: Vec<_> = (0..self.keys.len())
                .step_by(per_key)
                .map(|k| sc.spawn(move || legends(k..(k + per_key).min(self.keys.len()))))
                .chain((0..h).step_by(per_row).map(|y| sc.spawn(move || rest(y..(y + per_row).min(h)))))
                .collect();
            parts.into_iter().map(|p| p.join().expect("a share of the measuring")).fold(Most::default(), |a, b| Most {
                legend: a.legend.max(b.legend),
                deck: a.deck.max(b.deck),
                skirt: a.skirt.max(b.skirt),
                chassis: a.chassis.max(b.chassis),
            })
        })
    }

    /// What makes the simulation's light its nits, in the SDR white's units.
    /// The simulation follows each LED's light as shares of all of it, over
    /// the art's units: a place's light is its luminance for an LED giving off
    /// a lumen, in the art's square units. The LEDs' lumens, and the art's
    /// units to the metre, make it cd/m², nits: no more than that, up to Peak
    /// nits (`ceiling` times the white) at the brightest legend.
    fn gain(&self, white: f32, ceiling: f32) -> f32 {
        let per_m = self.model.scale * 1000.0;
        let g = self.model.lumens * per_m * per_m / white.max(1.0);
        if self.most.legend > 0.0 { g.min(ceiling / self.most.legend) } else { g }
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
        let mut out = Vec::new();
        self.compose_into(&mut out, lights, locks, white, ceiling);
        out
    }

    /// The picture with this light, drawn into `out`, which an animation
    /// keeps from one frame to the next rather than making it anew.
    pub fn compose_into(&self, out: &mut Vec<f32>, lights: &[Light], locks: u8, white: f32, ceiling: f32) {
        let (w, h) = (self.w as usize, self.h as usize);
        if out.len() != w * h * 4 {
            out.clear();
            out.resize(w * h * 4, 0.0);
        }
        // the keys' light in their colours, made once for every band
        let keys = self.sim.as_deref().map(|snap| {
            let colour = |pos: u32| lights.iter().find(|l| l.pos == pos).map_or([0.0; 3], key_colour);
            let colours: Vec<[f32; 3]> = snap.keys.iter().map(|&p| colour(p)).collect();
            let alike = colours.first().copied().filter(|c| colours.iter().all(|o| o == c));
            let coloured = alike.is_none().then(|| snap.lit(&colours));
            // Where the black of a cap's top comes down its sides, the light
            // through them keeps to the band below it: the simulation's cells
            // are coarser than the band, so the band takes all of a side's
            // light, as much more as the black leaves it less of the side. A
            // translucent top glows beside its legend with the light on its
            // back, as much as it lets through for a legend's share of it.
            let physics = (0..self.cap_shapes.len() as u32).map(|c| snap.setup.of(c));
            let to_band = physics.clone().map(|p| if p.skirts > 0.0 && p.black > 0.0 && !self.band_map.is_empty() { 1.0 / (1.0 - p.black).max(0.05) } else { 0.0 }).collect();
            let top_glow = physics.clone().map(|p| if p.tops > 0.0 { p.tops / p.legends.max(0.01) } else { 0.0 }).collect();
            let highest = physics.clone().map(|p| p.top).fold(0.0f32, f32::max);
            let top_lit = physics.map(|p| p.top < highest - 0.01).collect();
            KeysLight { snap, gain: self.gain(white, ceiling), alike, coloured, to_band, top_glow, top_lit }
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
    }

    #[allow(clippy::too_many_arguments)]
    fn compose_rows(&self, lights: &[Light], locks: u8, white: f32, ceiling: f32, y0: usize, out: &mut [f32], keys: Option<&KeysLight>) {
        let w = self.w as usize;
        let n = out.len() / 4;
        let y1 = y0 + n / w;
        let dec = |v: f32| v.max(0.0).powf(2.2);
        let (fw, cw) = keys.map_or((0, 0), |k| (k.snap.fine.w, k.snap.coarse.w));
        // The board unlit, and the keys' light where it shows: on the deck
        // and the chassis where no cap stands over them, as much as a cap's
        // edge lets through (the caps' shadows are the room's light, not
        // over it), and on the caps' skirts' outsides.
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            let j = y0 * w + i;
            px[..3].copy_from_slice(&self.base[j * 3..j * 3 + 3]);
            px[3] = 1.0;
            let Some(k) = keys else { continue };
            let (r, a) = (&self.reads[j], self.caps[j * 4 + 3]);
            // (each cap's band and top by its own physics)
            let (to_band, top_glow, top_lit) = k.cap(self.cap_of[j]);
            // a low top: the light coming down on it, as much as the cap covers
            if r.code == 3 && top_lit && a > 0.0 {
                let e = k.at(&sim::At { code: sim::TOP_OUTSIDE as u8, ..r.at(fw, cw) });
                for c in 0..3 {
                    px[c] += e[c] * a;
                }
            }
            let share = match r.code {
                0 | 1 if a < 1.0 => 1.0 - a,
                2 if to_band > 0.0 => self.band_map[j] * to_band,
                2 if a > 0.0 => a,
                3 if a > 0.0 && top_glow > 0.0 => a * top_glow,
                _ => continue,
            };
            let e = k.at(&r.at(fw, cw));
            for c in 0..3 {
                px[c] += e[c] * share;
            }
        }
        // The caps' feet's edges: each pixel there shows the light of its
        // other part too, the deck's where the cap leaves it uncovered, the
        // skirt's where the cap covers it.
        if let Some(k) = keys {
            let from = self.edges.partition_point(|e| (e.j as usize) < y0 * w);
            for e in self.edges[from..].iter().take_while(|e| (e.j as usize) < y1 * w) {
                let j = e.j as usize;
                let a = self.caps[j * 4 + 3];
                let share = match (self.reads[j].code, k.cap(self.cap_of[j]).0) {
                    (2, _) => 1.0 - a,
                    (_, more) if more > 0.0 => self.band_map[j] * more,
                    _ => a,
                };
                let l = k.at(&e.other.at(fw, cw));
                let i = (j - y0 * w) * 4;
                for c in 0..3 {
                    out[i + c] += l[c] * share;
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
        // Pudding caps: each lit key's milky band below the black of its top,
        // in place of the art's dark skirt, looking as the legends do (milky
        // plastic is what they are): its own white dimmed by the core's unlit
        // share, showing less as the light through it outshines it. Each cap
        // by its own physics: a key with a cap of another kind has none.
        if let Some(k) = keys {
            // (a cap with no LED of its own is a lit key's with its LED off)
            let lit = self.keys.iter().zip(&self.bands).filter_map(|(key, s)| Some((s.as_ref()?, key.cap?, light(key.pos).map_or(0.0, Light::shine))));
            let n = self.lit_caps();
            let others = self.other_bands.iter().enumerate().filter_map(|(i, s)| Some((s.as_ref()?, (n + i) as u32, 0.0)));
            for (s, cap, strength) in lit.chain(others) {
                let p = k.snap.setup.of(cap);
                if p.milky <= 0.0 || p.skirts <= 0.0 {
                    continue;
                }
                let white = MILKY_WHITE.map(|c| dec(c) * p.milky * self.model.legend_unlit * (1.0 - strength));
                each(s, &mut |d, _, a| {
                    let cap = &self.caps[(y0 * w + d) * 4..(y0 * w + d) * 4 + 4];
                    let ca = cap[3].max(1e-6);
                    for c in 0..3 {
                        out[d * 4 + c] += a * (white[c] - dec(cap[c] / ca));
                    }
                });
            }
        }
        // Each lit legend: its own look, dark with its LED off (the art's grey
        // dimmed by the core's unlit share), which shows less as the light
        // behind it outshines it; and the light through it, the brightest
        // thing on the keyboard. A remapped key's without its second function.
        for (n, key) in self.keys.iter().enumerate() {
            let Some(s) = &key.legend else { continue };
            let l = light(key.pos);
            let shown = shown(key, l);
            let strength = l.map_or(0.0, Light::shine);
            let print = [0, 1, 2].map(|c| dec(key.print[c]) * self.model.legend_unlit * (1.0 - strength));
            let reads = self.legend_reads.get(n);
            each(s, &mut |d, i, a| {
                let a = a * shown[i];
                if a <= 0.0 {
                    return;
                }
                for c in 0..3 {
                    out[d * 4 + c] = out[d * 4 + c] * (1.0 - a) + print[c] * a;
                }
                if let (Some(k), Some(r)) = (keys, reads.and_then(|r| r.get(i)).filter(|r| r.code != 255)) {
                    let e = k.at(&r.at(fw, cw));
                    for c in 0..3 {
                        out[d * 4 + c] += e[c] * a;
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
/// read at the bits of the light's share of PQ's 10,000 nits, which split
/// each halving of brightness into the same number of even steps, so most of
/// the table is in the dark, where PQ changes fastest. Nothing is worked out
/// for a pixel but where it falls in the table and how far to the next entry.
pub struct Encoder {
    table: Vec<f32>,
    reference: f32,
}

/// The halvings of brightness the table covers below PQ's peak (down to a
/// trillionth of it, far darker than any screen shows), and the bits that
/// split each into even steps (128 of them).
const OCTAVES: u32 = 56;
const STEP_BITS: u32 = 7;
/// a float's bits past the steps', the way from one entry to the next
const SHIFT: u32 = 23 - STEP_BITS;
/// the bits of the table's darkest entry, a float's exponent and the steps' bits
const FIRST: u32 = (127 - OCTAVES) << STEP_BITS;
/// the table's darkest entry, as a share of PQ's peak
const DARKEST: f32 = 1.0 / (1u64 << OCTAVES) as f32;

impl Encoder {
    pub fn new(reference: f64) -> Encoder {
        let entries = OCTAVES << STEP_BITS;
        let mut table: Vec<f32> = (0..=entries).map(|i| pq(10000.0 * f32::from_bits((FIRST + i) << SHIFT) as f64) as f32).collect();
        // what no light at all gives, to read up from in the darkest step
        table.push(pq(0.0) as f32);
        Encoder { table, reference: reference as f32 }
    }

    fn signal(&self, nits: f32) -> f32 {
        let x = nits * (1.0 / 10000.0);
        let last = (OCTAVES << STEP_BITS) as usize;
        if x >= 1.0 {
            return self.table[last];
        }
        if !(x >= DARKEST) {
            // darker than the table goes (or nothing, or not a number)
            let none = self.table[last + 1];
            return none + (self.table[0] - none) * (x.max(0.0) / DARKEST);
        }
        let b = x.to_bits();
        let i = ((b >> SHIFT) - FIRST) as usize;
        let f = (b & ((1 << SHIFT) - 1)) as f32 * (1.0 / (1 << SHIFT) as f32);
        self.table[i] + (self.table[i + 1] - self.table[i]) * f
    }

    /// One pixel: premultiplied linear RGBA (SDR white 1.0, BT.709) to
    /// premultiplied PQ, BT.2020, as the channels of ABGR16161616
    pub fn pixel(&self, px: [f32; 4]) -> [u16; 4] {
        let a = px[3];
        if a <= 0.0 {
            return [0; 4];
        }
        let r = 1.0 / a;
        self.straight([px[0] * r, px[1] * r, px[2] * r], a)
    }

    /// One pixel given as its colour in linear light (SDR white 1.0, BT.709)
    /// and how much of it there is, to premultiplied PQ, BT.2020, as the
    /// channels of ABGR16161616
    pub fn straight(&self, c: [f32; 3], a: f32) -> [u16; 4] {
        const M: [[f32; 3]; 3] = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
        if a <= 0.0 {
            return [0; 4];
        }
        let k = a * 65535.0;
        let mut out = [0u16; 4];
        // rounded by a half added and the rest dropped, as round() does for
        // these (never negative), without the call it makes for each
        for (o, row) in out.iter_mut().zip(M) {
            let v = row[0] * c[0] + row[1] * c[1] + row[2] * c[2];
            *o = (self.signal(v * self.reference) * k + 0.5).clamp(0.0, 65535.0) as u16;
        }
        out[3] = (a.min(1.0) * 65535.0 + 0.5) as u16;
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
    use sim::{Physics, Scene, Setup, Simulator};

    /// The light the tests simulate, whatever the core is tuned to: the
    /// brightest legend at 600 nits, and the core's own defaults for the rest.
    fn light() -> Value {
        serde_json::json!({
            "scale": 1.38,
            // (LEDs as bright as put the brightest legend at 600 nits)
            "switches": { "led": 4.5 / 1.38, "lumens": 0.2693 },
            "legends": { "unlit": 0.15 },
            "wells": { "art": "mask_1" },
            "display": { "lit": 400 }
        })
    }

    /// A key in section comes from the art: the Z10's 2 key, against the
    /// measurements the Light lab's drawing was first made from (its foot,
    /// its face's inset, the deck and the chassis north of it, the W key
    /// south of it), and its two legends where the art prints them: 2@ about
    /// 3 mm north of its middle, PF2 about 4 mm south. A cap with no legend
    /// has none.
    #[test]
    fn a_key_in_section_is_as_the_art_has_it() {
        let (_, _, b) = board();
        let g = b.key_geometry("2", 1.38).expect("the 2 key");
        eprintln!("{g:?}");
        let near = |a: f32, b: f32| (a - b).abs() < 0.06;
        assert!(near(g.foot, 17.39) && near(g.inset, 1.63), "its foot and face");
        assert!(g.deck_north.is_some_and(|d| near(d, 1.62)), "the deck north of it");
        assert!(g.strip.is_some_and(|(s, d)| near(s, 2.55) && near(d, 2.36)), "the chassis north of its well, and the F row's deck");
        assert!(g.south.is_some_and(|(gap, foot)| near(gap, 1.45) && near(foot, 17.39)), "the W key south of it");
        assert_eq!(g.legends.len(), 2, "2@ and PF2");
        assert!((g.legends[0].0 + 3.0).abs() < 0.3 && (g.legends[0].1 - 4.1).abs() < 0.5, "2@: {:?}", g.legends[0]);
        assert!((g.legends[1].0 - 4.0).abs() < 0.3 && (g.legends[1].1 - 2.6).abs() < 0.5, "PF2: {:?}", g.legends[1]);
        let space = b.cap_shapes.iter().find(|c| c.label == "Space").expect("the spacebar");
        assert!(!space.lit && !space.legend);
        assert!(b.key_geometry(&space.id, 1.38).is_some_and(|g| g.legends.is_empty() && g.south.is_none()));
    }

    /// The Z10's board at the size the window first draws it, unlit.
    fn board() -> (Tree, Value, Board) {
        let tree = Tree::from_str(&crate::fixture::svg(), &usvg::Options::default()).unwrap();
        let mut device: Value = crate::jsonc::parse(&crate::fixture::device()).unwrap();
        device["light"] = light();
        let b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        (tree, device, b)
    }

    /// The board lit by a simulation run to `photons` from each LED.
    fn lit_board(photons: usize) -> (Tree, Value, Board) {
        let (tree, device, mut b) = board();
        let scene = Scene::of(&b);
        let t = std::time::Instant::now();
        let snap = Simulator::settle(&scene, &Setup::alike(Physics::from(&device["light"])), photons);
        eprintln!("simulated {photons} photons from each of {} LEDs in {:?}", snap.keys.len(), t.elapsed());
        b.set_sim(Arc::new(snap));
        (tree, device, b)
    }

    /// The deck is lit right up to each cap's foot, and the foot catches its
    /// light, brightest at the foot and less up the skirt: across the middle
    /// of each side of each lit key, from just outside its foot up its skirt,
    /// the light never dips dark at the edge and comes back.
    #[test]
    fn the_light_runs_up_to_each_caps_foot_and_onto_it() {
        let (_t, _d, b) = lit_board(2000);
        let frame = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        let (mut runs, mut worst) = (0, (f32::MAX, 0u32, 0usize));
        for k in &b.keys {
            let Some(f) = k.foot else { continue };
            let (mx, my) = ((ox + ka * (f[0] + f[2]) / 2.0) as i32, (oy + ka * (f[1] + f[3]) / 2.0) as i32);
            let px = |u: f32| (ox + ka * u).floor() as i32;
            let py = |v: f32| (oy + ka * v).floor() as i32;
            // each side's pixels, from one outside the foot to three in
            let sides: [Vec<(i32, i32)>; 4] = [
                (px(f[0]) - 1..px(f[0]) + 4).map(|x| (x, my)).collect(),
                (px(f[2]) - 3..px(f[2]) + 2).rev().map(|x| (x, my)).collect(),
                (py(f[1]) - 1..py(f[1]) + 4).map(|y| (mx, y)).collect(),
                (py(f[3]) - 3..py(f[3]) + 2).rev().map(|y| (mx, y)).collect(),
            ];
            for (side, run) in sides.iter().enumerate() {
                let l: Vec<f32> = run.iter().map(|&p| lum(&frame, &b, p)).collect();
                runs += 1;
                for i in 0..l.len() - 1 {
                    // a pixel as dark as this beside a brighter one further up is a dark line
                    let r = l[i] / l[i + 1].max(1e-6);
                    if r < worst.0 {
                        worst = (r, k.pos, side);
                    }
                }
            }
        }
        eprintln!("{runs} sides: the darkest pixel against the next one up its skirt is {:.2} of it (key {}, side {})", worst.0, worst.1, worst.2);
        assert!(worst.0 > 0.6, "a dark line at the foot of key {}, side {}", worst.1, worst.2);
    }

    /// A cap's foot is rounded as the art draws it: just inside its box's
    /// corner, outside the round, is the deck, lit as the deck beside the
    /// foot is.
    #[test]
    fn a_caps_foot_is_round_at_its_corners() {
        let (_t, _d, b) = lit_board(2000);
        let frame = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        let (mut corners, mut darkest) = (0, f32::MAX);
        for k in &b.keys {
            let (Some(f), r) = (k.foot, k.round[0]) else { continue };
            assert!(r > 1.0, "key {}'s foot is rounded as the art draws it: {r}", k.pos);
            // its top left corner: inside the box, outside the round; and the deck beside the foot
            let (u, v) = (f[0] + 0.15, f[1] + 0.15);
            let (x, y) = ((ox + ka * u) as i32, (oy + ka * v) as i32);
            let j = y as usize * b.w as usize + x as usize;
            assert_eq!(b.reads[j].code, 0, "key {}'s corner is the deck", k.pos);
            let beside = lum(&frame, &b, ((ox + ka * (f[0] - 0.4)) as i32, (oy + ka * (f[1] + 4.0)) as i32));
            darkest = darkest.min(lum(&frame, &b, (x, y)) / beside.max(1e-6));
            corners += 1;
        }
        eprintln!("{corners} corners: the darkest is {darkest:.2} of the deck beside its foot");
        assert!(darkest > 0.4, "a corner is dark: {darkest:.2}");
    }

    /// A cap's translucent skirt glows with the light inside it: far brighter
    /// than an opaque one, and brightest on the side its LED sits.
    #[test]
    fn translucent_skirts_glow() {
        let (_t, device, mut b) = board();
        let scene = Scene::of(&b);
        let k = b.keys.iter().position(|k| k.foot.is_some()).unwrap();
        let (f, a) = (b.keys[k].foot.unwrap(), b.keys[k].face);
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        // the middle of the key's north skirt and of its south skirt, a third of the way up each
        let mx = (ox + ka * (f[0] + f[2]) / 2.0) as i32;
        let north = (mx, (oy + ka * (f[1] + (a[1] - f[1]) * 0.33)) as i32);
        let south = (mx, (oy + ka * (f[3] - (f[3] - a[3]) * 0.33)) as i32);
        let mut lit = |skirts: f32| {
            let mut l = device["light"].clone();
            l["switches"]["kind"] = serde_json::json!("led-slot");
            l["switches"]["beam"] = serde_json::json!(3.0);
            // (pudding caps, black nowhere, as the plastic letting light through has it)
            l["caps"] = serde_json::json!({ "plastic": skirts });
            b.set_sim(Arc::new(Simulator::settle(&scene, &Setup::alike(Physics::from(&l)), 4000)));
            let frame = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
            (b.most.skirt / b.most.legend, lum(&frame, &b, north), lum(&frame, &b, south))
        };
        let (opaque, _, _) = lit(0.0);
        let (translucent, n, s) = lit(0.3);
        eprintln!("the brightest skirt against the brightest legend: opaque {opaque:.3}, translucent {translucent:.3}; north side {n:.3}, south {s:.3}");
        assert!(translucent > 4.0 * opaque, "translucent skirts glow: {translucent:.3} against {opaque:.3}");
        assert!(n > s, "brightest on the LED's side: north {n:.3}, south {s:.3}");
    }

    /// A pudding cap glows only below the black of its top, which stays dark
    /// all round the top, at its corners as along its sides; and its milky
    /// plastic carries the light round the cap, so it glows more evenly than
    /// clear plastic does.
    #[test]
    fn pudding_caps_glow_below_the_black_and_evenly() {
        let (_t, device, mut b) = board();
        let scene = Scene::of(&b);
        let k = b.keys.iter().position(|k| k.foot.is_some()).unwrap();
        let (f, a) = (b.keys[k].foot.unwrap(), b.keys[k].face);
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        let mx = (ox + ka * (f[0] + f[2]) / 2.0) as i32;
        // up the north skirt from its foot (0) to its face (1), and the south's low on it
        let north = |up: f32| (mx, (oy + ka * (f[1] + (a[1] - f[1]) * up)) as i32);
        let south = (mx, (oy + ka * (f[3] - (f[3] - a[3]) * 0.3)) as i32);
        let mut lit = |milky: f32| {
            let mut l = device["light"].clone();
            l["switches"]["kind"] = serde_json::json!("led-slot");
            l["switches"]["beam"] = serde_json::json!(3.0);
            l["caps"] = serde_json::json!({ "kind": "pudding", "plastic": 0.3, "black": 0.4, "milky": milky });
            b.set_sim(Arc::new(Simulator::settle(&scene, &Setup::alike(Physics::from(&l)), 4000)));
            let frame = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
            // the black at each of the top's corners too: on the diagonal from the
            // foot's rounded corner to the face's, as far toward the face as along the sides
            let r = b.keys[k].round[0];
            let arc = |x: f32, y: f32, sx: f32, sy: f32| (x - sx * r * (1.0 - std::f32::consts::FRAC_1_SQRT_2), y - sy * r * (1.0 - std::f32::consts::FRAC_1_SQRT_2));
            let corner = |sx: f32, sy: f32| {
                let pick = |lo: f32, hi: f32, s: f32| if s < 0.0 { lo } else { hi };
                let foot = arc(pick(f[0], f[2], sx), pick(f[1], f[3], sy), sx, sy);
                let face = arc(pick(a[0], a[2], sx), pick(a[1], a[3], sy), sx, sy);
                let (cx, cy) = (foot.0 + (face.0 - foot.0) * 0.85, foot.1 + (face.1 - foot.1) * 0.85);
                lum(&frame, &b, ((ox + ka * cx) as i32, (oy + ka * cy) as i32))
            };
            let corners = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0)];
            let black = corners.iter().cloned().fold(lum(&frame, &b, north(0.85)), f32::max);
            (lum(&frame, &b, north(0.3)), black, lum(&frame, &b, south))
        };
        let (clear_band, clear_black, clear_south) = lit(0.0);
        let (milky_band, milky_black, milky_south) = lit(1.0);
        eprintln!("clear: band {clear_band:.3}, under the black {clear_black:.3}, south {clear_south:.3}; milky: band {milky_band:.3}, under the black {milky_black:.3}, south {milky_south:.3}");
        assert!(clear_band > 4.0 * clear_black && milky_band > 4.0 * milky_black, "the light gets out below the black, which is dark all round the top, its corners too");
        assert!(milky_south / milky_band > clear_south / clear_band, "milky plastic glows more evenly round the cap");
    }

    /// A key with a cap of its own kind is drawn as its own: among pudding
    /// caps, a stock shine-through cap has no milky band, its LED off or lit,
    /// and the keys far from it look as they did.
    #[test]
    fn a_key_s_own_cap_is_drawn_as_its_own() {
        let (_t, device, mut b) = board();
        let scene = Scene::of(&b);
        let ids: Vec<String> = b.cap_shapes.iter().map(|c| c.id.clone()).collect();
        let mut l = device["light"].clone();
        l["caps"] = serde_json::json!({ "kind": "pudding", "plastic": 0.3, "black": 0.4, "milky": 1.0 });
        l["switches"]["kind"] = serde_json::json!("led-slot");
        l["switches"]["beam"] = serde_json::json!(3.0);
        let key = |label: &str| {
            let pos = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == label).unwrap()["pos"].as_u64().unwrap() as u32;
            b.keys.iter().position(|k| k.pos == pos).unwrap()
        };
        let (w, far) = (key("W"), key("Keypad6"));
        let frames = |b: &mut Board, l: &Value| {
            b.set_sim(Arc::new(Simulator::settle(&scene, &Setup::from(l, &ids), 2000)));
            (b.compose(&[], 0, 254.0, 1000.0 / 254.0), b.compose(&white(b), 0, 254.0, 1000.0 / 254.0))
        };
        let (all_off, all_lit) = frames(&mut b, &l);
        let (bands_w, bands_far) = (b.bands[w].take().unwrap(), b.bands[far].take().unwrap());
        l["keys"] = serde_json::json!({ ids[b.keys[w].cap.unwrap() as usize].clone(): { "caps": { "kind": "shine-through" } } });
        let (own_off, own_lit) = frames(&mut b, &l);
        assert!(b.bands[w].as_ref().is_some_and(|s| s.cover.iter().filter(|c| **c > 0.0).count() < bands_w.cover.iter().filter(|c| **c > 0.0).count() / 4), "its band is the black's edge alone");
        // a band's pixels, each one's light in a frame, added up
        let on = |s: &Shape, f: &[f32]| {
            (0..s.cover.len()).filter(|&i| s.cover[i] > 0.5).map(|i| lum(f, &b, (s.x + (i as u32 % s.w) as i32, s.y + (i as u32 / s.w) as i32))).sum::<f32>()
        };
        eprintln!("W's band, its LED off: {:.3} pudding, {:.3} shine-through; lit: {:.3}, {:.3}", on(&bands_w, &all_off), on(&bands_w, &own_off), on(&bands_w, &all_lit), on(&bands_w, &own_lit));
        assert!(on(&bands_w, &own_off) < 0.25 * on(&bands_w, &all_off), "no milky band with its LED off");
        assert!(on(&bands_w, &own_lit) < 0.25 * on(&bands_w, &all_lit), "no light through its skirt lit");
        let (a, o) = (on(&bands_far, &all_lit), on(&bands_far, &own_lit));
        assert!(on(&bands_far, &all_off) == on(&bands_far, &own_off) && (a - o).abs() < 0.01 * a, "a key far from it as it was: {a} against {o}");
    }

    /// The buttons with no light of their own catch the keys' light round
    /// them: G/PC's face and the media keys are brighter with every key lit
    /// than with none, and the chassis by them is as the art has it unlit.
    #[test]
    fn the_buttons_catch_the_keys_light() {
        let (_t, device, mut b) = board();
        let scene = Scene::of(&b);
        let mut l = device["light"].clone();
        l["caps"] = serde_json::json!({ "kind": "pudding", "plastic": 0.3, "black": 0.4, "milky": 1.0 });
        l["switches"]["kind"] = serde_json::json!("led-slot");
        l["switches"]["beam"] = serde_json::json!(3.0);
        let mut faces = Vec::new();
        for label in ["Game mode button", "Media stop", "Next track"] {
            let c = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == label).unwrap();
            l["keys"][control_id(c)] = serde_json::json!({ "caps": { "kind": "shine-through", "foot": 2.0, "top": if label == "Game mode button" { 3.0 } else { 6.0 } } });
            let f = b.cap_shapes.iter().find(|s| s.label == label).unwrap().foot;
            faces.push((label, f));
        }
        b.set_sim(Arc::new(Simulator::settle(&scene, &b.setup(&l), 4000)));
        let (off, lit) = (b.compose(&[], 0, 254.0, 1000.0 / 254.0), b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0));
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        for (label, f) in faces {
            // its middle, clear of its edges
            let (x0, y0, x1, y1) = ((ox + ka * (f[0] + 2.0)) as i32, (oy + ka * (f[1] + 2.0)) as i32, (ox + ka * (f[2] - 2.0)) as i32, (oy + ka * (f[3] - 2.0)) as i32);
            let mean = |frame: &[f32]| {
                let px: Vec<f32> = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|p| lum(frame, &b, p)).collect();
                px.iter().sum::<f32>() / px.len().max(1) as f32
            };
            eprintln!("{label}: {:.4} unlit, {:.4} with every key lit", mean(&off), mean(&lit));
            // (before they stood in the light, the art covered them from it: no light at all)
            assert!(mean(&lit) > mean(&off) * 1.05, "{label} catches the keys' light");
        }
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

    /// For a core's LED output: the lumens that put its brightest legend,
    /// every key lit white at full, at ECTODECK_NITS (1000), with the light of
    /// the core file ECTODECK_CORE_FILE (the Z10's) or, with ECTODECK_TESTS,
    /// the tests' own. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn lumens_for() {
        let tree = Tree::from_str(&crate::fixture::svg(), &usvg::Options::default()).unwrap();
        let text = std::env::var("ECTODECK_CORE_FILE").map(|p| std::fs::read_to_string(p).unwrap()).unwrap_or_else(|_| crate::fixture::device());
        let mut device = crate::jsonc::parse(&text).unwrap();
        if std::env::var_os("ECTODECK_TESTS").is_some() {
            device["light"] = light();
        }
        let mut b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        let snap = Simulator::settle(&Scene::of(&b), &b.setup(&device["light"]), sim::SETTLED);
        b.set_sim(Arc::new(snap));
        let per_m = b.model.scale * 1000.0;
        let nits: f32 = std::env::var("ECTODECK_NITS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000.0);
        eprintln!("lumens for {nits} nits: {:.4}", nits / (b.most.legend * per_m * per_m));
    }

    /// The legends' share dims the legends alone: a tenth of it leaves the
    /// rest of the keyboard's light about as it was.
    #[test]
    fn the_legends_share_dims_the_legends_alone() {
        let (_, device, mut b) = board();
        let scene = Scene::of(&b);
        let brightest = |b: &mut Board, legends: f64| {
            let mut l = device["light"].clone();
            l["legends"]["opacity"] = serde_json::json!(1.0 - legends);
            b.set_sim(Arc::new(Simulator::settle(&scene, &Setup::alike(Physics::from(&l)), 4000)));
            let lit = b.compose(&white(b), 0, 254.0, 4000.0 / 254.0);
            let most = |code: u8| (0..b.w as usize * b.h as usize).filter(|&j| b.reads[j].code == code && b.caps[j * 4 + 3] < 0.01).map(|j| lum(&lit, b, ((j % b.w as usize) as i32, (j / b.w as usize) as i32))).fold(0f32, f32::max);
            (b.peaks(&white(b), 254.0, 4000.0 / 254.0).legend, most(0))
        };
        let (legend, deck) = brightest(&mut b, 0.3);
        let (legend_dim, deck_dim) = brightest(&mut b, 0.03);
        eprintln!("legends at 30%: brightest legend {legend:.0} nits, deck {deck:.4}; at 3%: {legend_dim:.0} nits, deck {deck_dim:.4}");
        assert!(legend_dim < legend * 0.2, "the legends dim with their share");
        assert!((deck_dim - deck).abs() < deck * 0.15, "the deck stays about as it was");
    }

    /// The brightest legend, every key lit white at full, is as bright as
    /// the LEDs' lumens make it (the tests' 0.2693 lumens, 600 nits); a legend is
    /// brightest over its LED; the deck by a key's skirt is brightest on its
    /// LED's side; the chassis catches the light out of the wells; and
    /// nothing lights the page round the keyboard.
    #[test]
    fn the_light_falls_as_the_simulation_has_it() {
        let (_, device, b) = lit_board(20_000);
        let px = |u: f32, v: f32| ((b.art_offset.0 + b.art_scale * u) as i32, (b.art_offset.1 + b.art_scale * v) as i32);
        let lit = b.compose(&white(&b), 0, 254.0, 1000.0 / 254.0);
        let dark = b.compose(&[], 0, 254.0, 1000.0 / 254.0);
        let add = |p: (i32, i32)| (lum(&lit, &b, p) - lum(&dark, &b, p)) * 254.0;
        // the brightest legend at the LEDs' strength, times the share the legends let through
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
        assert!((brightest - 600.0).abs() < 600.0 * 0.03, "the brightest legend is as bright as the LEDs' lumens make it: {brightest}");
        // W: its legend brighter by its LED (north) than further off
        let c = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == "W").unwrap();
        let key = b.keys.iter().find(|k| k.pos as u64 == c["pos"].as_u64().unwrap()).unwrap();
        let (f, north) = (key.foot.unwrap(), Physics::from(&device["light"]).held_led());
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

    /// A key lit black is a key with its LED off: its legend shows the grey
    /// it is printed in, as dim as LED off has it, as an unlit key's does.
    #[test]
    fn a_key_lit_black_looks_as_its_led_off() {
        let (_, device, b) = lit_board(4_000);
        let w = device["controls"].as_array().unwrap().iter().find(|c| c["label"] == "W").unwrap()["pos"].as_u64().unwrap() as u32;
        let key = b.keys.iter().find(|k| k.pos == w).unwrap();
        let s = key.legend.as_ref().unwrap();
        let legend = |pic: &[f32]| (0..s.cover.len()).filter(|&i| s.cover[i] > 0.5).map(|i| {
            let (x, y) = ((s.x + (i as u32 % s.w) as i32) as usize, (s.y + (i as u32 / s.w) as i32) as usize);
            pic[(y * b.w as usize + x) * 4 + 1]
        }).fold(0.0f32, f32::max);
        let off = b.compose(&[Light { pos: w, rgb: [255, 255, 255], strength: 0.0, remapped: false }], 0, 254.0, 1000.0 / 254.0);
        let black = b.compose(&[Light { pos: w, rgb: [0, 0, 0], strength: 1.0, remapped: false }], 0, 254.0, 1000.0 / 254.0);
        let lit = b.compose(&[Light { pos: w, rgb: [255, 255, 255], strength: 1.0, remapped: false }], 0, 254.0, 1000.0 / 254.0);
        eprintln!("W's legend: its LED off {:.4}, lit black {:.4}, lit white {:.4}", legend(&off), legend(&black), legend(&lit));
        assert!(legend(&off) > 0.0, "a legend with its LED off shows its print");
        assert!((legend(&black) - legend(&off)).abs() < 1e-6, "lit black, it looks as with its LED off");
        assert!(legend(&lit) > legend(&off) * 5.0, "lit, its light outshines the print");
    }

    /// Each key's light is its own: one key lit alone lights its own legend
    /// and the deck round it; two lit together give each one's light added
    /// up; and a red gives off its own share of white's light.
    #[test]
    fn each_key_s_light_is_its_own() {
        let (_, device, mut b) = lit_board(4_000);
        // (the legends' LED-off print taken off, LED off at 0%, so what shows
        // on a legend is its light alone)
        b.model.legend_unlit = 0.0;
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
        // W's own legend is lit; Q's, beside it, not by much. Against W lit black.
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
        let nits = device["lock_lights"]["brightness"].as_f64().unwrap() as f32;
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
    /// ECTODECK_SIM (as JSON over the tests' own, a section's values one by
    /// one, as a key's own are: {"switches": {"beam": 3}}), how
    /// bright each part is at its brightest and typically, and how evenly
    /// the wide legends are lit. Run with --ignored.
    #[test]
    #[ignore]
    fn calibrate() {
        let (tree, mut device, _) = board();
        if let Ok(j) = std::env::var("ECTODECK_SIM") {
            device["light"] = sim::key_light(&device["light"], &serde_json::from_str(&j).unwrap());
        }
        let mut b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        let photons = std::env::var("ECTODECK_SIM_PHOTONS").ok().and_then(|v| v.parse().ok()).unwrap_or(40_000);
        let snap = Simulator::settle(&Scene::of(&b), &Setup::alike(Physics::from(&device["light"])), photons);
        b.set_sim(Arc::new(snap));
        let nits = device["light"]["brightness"].as_f64().unwrap_or(600.0) as f32;
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

    /// The table gives PQ to well within one of the screen's 65,536 steps,
    /// from no light to past PQ's peak, through every halving between.
    #[test]
    fn the_table_matches_pq() {
        let enc = Encoder::new(254.0);
        let step = 1.0 / 65535.0;
        let mut worst = (0f64, 0f32);
        let mut check = |nits: f32| {
            // past PQ's peak the screen shows its peak
            let off = (enc.signal(nits) as f64 - pq((nits as f64).min(10000.0))).abs();
            if off > worst.0 {
                worst = (off, nits);
            }
        };
        for nits in [0.0f32, 1e-12, 1e-9, 0.05, 0.5, 2.7, 10.0, 100.0, 254.0, 1000.0, 9999.9, 10000.0, 20000.0] {
            check(nits);
        }
        // a million places, evenly through each halving from 1e-10 nits up
        for i in 0..1_000_000 {
            check(1e-10 * 2f32.powf(i as f32 / 1_000_000.0 * 46.6));
        }
        assert!(worst.0 < 0.1 * step, "{} nits is {:.3} steps off", worst.1, worst.0 / step);
    }
}