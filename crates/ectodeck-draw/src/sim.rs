//! The keyboard's light, simulated: each key's LED followed through the
//! keyboard's shape, the art's caps, wells and body stood up to the heights
//! the core gives, every surface matte and giving back its share of the light.
//!
//! An LED gives off its light out of the top of its switch, in a beam straight
//! up, which lights the legend above it, brightest over the LED. In a switch
//! with a black housing (the LED through a slot in its top, in a small clear
//! window) that is all its light, and the housing stands in the way of the light that falls
//! back on it; in a clear one, the housing scatters a share of it instead,
//! glowing with it all over, its top lighting the back of the cap's top
//! evenly and its sides the deck round the switch and the inside of the cap.
//! A cap's plastic gives back its share of the light that falls on it, and,
//! where it is translucent, lets a share through: its skirt glows on its
//! outside with the light inside it, below where the black of its top comes
//! down the sides (pudding caps), and so may its top. Milky plastic carries
//! the light it takes some way through itself before it leaves, so it glows
//! evenly; clear plastic lets it out where it went in. The light straight
//! from the LED and the housing to the legend and through the cap's own
//! skirt, and from the housing to the deck of its own well, is worked out
//! exactly, cell by cell; the rest (the light that has bounced off the deck,
//! the caps' insides and outsides and the wells' walls, and the housing's on
//! what is further off) is traced as photons, more of them the longer the
//! simulation runs, so its picture comes clear as it goes.
//!
//! What each place shows from above is kept for each LED on its own, so the
//! keys can be any colours, lit and dimmed at will, without simulating again:
//! the deck, the chassis, a cap's skirt (its outside, as the slope of its band
//! shows it from above), and the back of a cap's top, which a legend lets its
//! share of through. Everything is in the art's units, whatever size the
//! keyboard is drawn at.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use serde_json::Value;

use super::Board;

/// the direct light's cells, and the bounced light's, in the art's units
const FINE: f32 = 0.5;
const COARSE: f32 = 2.0;
/// how far from its LED each key's direct and bounced light are kept
const DIRECT_REACH: f32 = 40.0;
const BOUNCE_REACH: f32 = 100.0;
/// photons from each LED for a settled picture, and in each step toward it
pub const SETTLED: usize = 200_000;
const STEP: usize = 2_000;
/// a key's maps of bounced light: the deck, the chassis, the skirts, the
/// backs of the caps' tops (seen through their legends and translucent
/// tops), and the tops' outsides, which only a part lower than what stands
/// round it catches (a button on the chassis among the caps): the same
/// places as the tops' backs
const MAPS: usize = 5;
pub const TOP_OUTSIDE: usize = 4;
/// how many photons the bounced light at a place is evened out over, and how
/// far round it at most (coarse cells)
const EVEN: u32 = 128;
const EVEN_REACH: usize = 8;

/// What a switch's housing is, as the core's switches' "kind" says: black,
/// its LED through a slot in its top, in a small clear window ("led-slot",
/// as older switches with LEDs have it); or clear and glowing with the LED's
/// light, its top as most RGB switches have it ("clear-top"), or all over
/// ("clear"), which differ in how the core shares the glow out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Switch {
    Slot,
    ClearTop,
    Clear,
}

impl Switch {
    /// as the core names it; a core with none is lit as before switches had kinds
    pub fn named(name: Option<&str>) -> Switch {
        match name {
            Some("led-slot") => Switch::Slot,
            Some("clear") => Switch::Clear,
            _ => Switch::ClearTop,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Switch::Slot => "led-slot",
            Switch::ClearTop => "clear-top",
            Switch::Clear => "clear",
        }
    }
}

/// What a cap is, as the core's caps' "kind" says: black all over but its
/// legends ("shine-through", as the Z10 came); its top black and part of its
/// sides, below them translucent plastic ("pudding"); or translucent all
/// over ("translucent"). A core with none is read from its values: walls
/// less than opaque are a pudding cap's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Caps {
    ShineThrough,
    Pudding,
    Translucent,
}

impl Caps {
    /// the core's caps, its light's "caps"
    pub fn of(caps: &Value) -> Caps {
        match caps["kind"].as_str() {
            Some("shine-through") => Caps::ShineThrough,
            Some("pudding") => Caps::Pudding,
            Some("translucent") => Caps::Translucent,
            _ if caps["opacity"].as_f64().unwrap_or(1.0) < 1.0 => Caps::Pudding,
            _ => Caps::ShineThrough,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Caps::ShineThrough => "shine-through",
            Caps::Pudding => "pudding",
            Caps::Translucent => "translucent",
        }
    }
}

/// A key's light, as the core's light description has it: all keys' values
/// with the key's own (its entry in "keys", by its control's id) over them,
/// a section's values one by one.
pub fn key_light(light: &Value, own: &Value) -> Value {
    let mut out = light.clone();
    if let Some(o) = out.as_object_mut() {
        o.remove("keys");
    }
    for (a, v) in own.as_object().into_iter().flatten() {
        match (v.as_object(), out[a.as_str()].is_object()) {
            (Some(section), true) => {
                for (b, x) in section {
                    out[a.as_str()][b.as_str()] = x.clone();
                }
            }
            _ => out[a.as_str()] = v.clone(),
        }
    }
    out
}

/// How every key is lit: all keys' physics, and the keys with physics of
/// their own (another cap, a switch moved, a beam of its own), each by its
/// place among the scene's caps.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub all: Physics,
    pub own: Arc<Vec<(u32, Physics)>>,
}

impl Setup {
    /// Every key alike.
    pub fn alike(all: Physics) -> Setup {
        Setup { all, own: Arc::new(Vec::new()) }
    }

    /// As a core's light description has it: all keys' values, and each
    /// key's own ("keys", by its control's id) over them; `ids` the scene's
    /// caps' controls, in its order.
    pub fn from(light: &Value, ids: &[String]) -> Setup {
        let mut own: Vec<(u32, Physics)> = light["keys"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(id, values)| Some((ids.iter().position(|c| c == id)? as u32, Physics::from(&key_light(light, values)))))
            .collect();
        own.sort_by_key(|o| o.0);
        Setup { all: Physics::from(light), own: Arc::new(own) }
    }

    /// A cap's physics: its own, or all keys'.
    pub fn of(&self, cap: u32) -> &Physics {
        self.own.binary_search_by_key(&cap, |o| o.0).map_or(&self.all, |i| &self.own[i].1)
    }

    /// As the simulation runs it (see Physics::simulated).
    pub fn simulated(&self) -> Setup {
        Setup { all: self.all.simulated(), own: Arc::new(self.own.iter().map(|(i, p)| (*i, p.simulated())).collect()) }
    }

    /// Whether the light falls as it did: everything but the brightness.
    pub fn same_paths(&self, o: &Setup) -> bool {
        self.all.same_paths(&o.all) && self.own.len() == o.own.len() && self.own.iter().zip(o.own.iter()).all(|(a, b)| a.0 == b.0 && a.1.same_paths(&b.1))
    }
}

/// The share of the light that falls on a switch's black housing that it gives back.
const HOUSING: f32 = 0.05;
/// How far round a cap milky plastic carries the light it scatters before it
/// leaves, in millimetres: the spread of where it comes out, each way round
/// the cap's foot from where it went in.
pub const MILKY: f32 = 20.0;

/// The keyboard's physical make, from the core's light: its switches, caps
/// and wells, heights and widths in millimetres (heights above the deck),
/// shares from 0 to 1. (The core names them as the Light lab does: the caps'
/// foot and top, the wells' depth.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Physics {
    /// how many of the art's units make a millimetre, as the core's light
    /// says (its "scale"): the art is in its own units, the core's lengths in mm
    pub units_per_mm: f32,
    /// where the LED sits, north of its switch's centre in the art's units
    /// (the core says it in mm; south below 0; held inside the switch); and
    /// where the switch sits under its cap, mm east and north of the cap's
    /// middle (a key's own "place")
    pub led: f32,
    pub place: [f32; 2],
    /// the light each LED gives off at full white, in lumens: what the
    /// simulation's light, followed as shares of it, is made real by
    pub lumens: f32,
    /// the switch's housing: black with the LED through a slot in it, or clear
    pub switch: Switch,
    /// the switch's housing: how high its top is, where the beam leaves it,
    /// and how wide it is, a square round the switch's centre (at most 14
    /// mm, as fits inside a cap)
    pub switch_top: f32,
    pub switch_width: f32,
    /// how far the housing's glow carries from the LED before it falls to about a third
    pub glow_spread: f32,
    /// how tight the beam is (the power of the cosine it falls as), the
    /// share of the LED's light a clear housing scatters instead, glowing with
    /// it, and the share of that glow that leaves through its sides rather than its top
    pub beam: f32,
    pub scatter: f32,
    pub sides: f32,
    /// a cap's skirt's foot and its top, and the share of the light on its
    /// back a legend lets through
    pub gap: f32,
    pub top: f32,
    pub legends: f32,
    /// what the caps are, and so: the shares of the light on a cap's inside
    /// that its skirt and its top (beside the legend) let through, glowing on
    /// their outsides (none for shine-through caps; a translucent cap's top
    /// lets through what its plastic does); how far
    /// down its sides, as a share of the skirt, the black of its top comes,
    /// which lets none through; and how milky the plastic is, 0 clear to 1
    /// milky: the share of the light it takes in that it scatters. Milky
    /// plastic is one body under the black, the top's underside and the skirt
    /// alike: what it scatters, taken in anywhere, it carries round the cap
    /// (MILKY each way), to leave below the black; the rest it lets straight
    /// through, as clear plastic does, out where it went in (from under the
    /// top, into the black).
    pub caps: Caps,
    pub skirts: f32,
    pub tops: f32,
    pub black: f32,
    pub milky: f32,
    /// the shares the caps' insides and outsides give back
    pub cap_in: f32,
    pub cap_out: f32,
    /// how high the chassis round the wells stands, and the shares the deck
    /// and the chassis give back (the wells' walls are its own sides)
    pub wall: f32,
    pub deck: f32,
    pub chassis: f32,
}

impl Physics {
    pub fn from(light: &Value) -> Physics {
        let (sw, cp, wl) = (&light["switches"], &light["caps"], &light["wells"]);
        let f = |v: &Value, d: f32| v.as_f64().map_or(d, |v| v as f32);
        let share = |v: &Value, d: f32| f(v, d).clamp(0.0, 0.98);
        let scale = light["scale"].as_f64().unwrap_or(1.0).max(0.01);
        let top = f(&cp["top"], 14.0).max(0.5);
        // what the caps let through, as their kind has it
        let caps = Caps::of(cp);
        // (the walls' opacity: what the plastic lets through is the rest)
        let plastic = (1.0 - f(&cp["opacity"], 1.0)).clamp(0.0, 0.98);
        let (skirts, tops, black) = match caps {
            Caps::ShineThrough => (0.0, 0.0, 1.0),
            Caps::Pudding => (plastic, 0.0, f(&cp["black"], 0.0).clamp(0.0, 1.0)),
            Caps::Translucent => (plastic, plastic, 0.0),
        };
        Physics {
            units_per_mm: scale as f32,
            // (5.08 mm north, as Cherry's and Kailh's switches have it)
            led: (sw["led"].as_f64().unwrap_or(5.08) * scale) as f32,
            place: [f(&light["place"]["east"], 0.0), f(&light["place"]["north"], 0.0)],
            lumens: f(&sw["lumens"], 1.0).max(0.0),
            switch: Switch::named(sw["kind"].as_str()),
            switch_top: f(&sw["top"], 6.5).clamp(0.05, top - 0.1),
            switch_width: f(&sw["width"], 12.0).clamp(1.0, 14.0),
            glow_spread: f(&sw["glow"], 4.0).clamp(0.2, 200.0),
            beam: f(&sw["beam"], 24.0).clamp(0.0, 400.0),
            scatter: f(&sw["scattered"], 0.7).clamp(0.0, 1.0),
            sides: f(&sw["sides"], 0.25).clamp(0.0, 1.0),
            gap: f(&cp["foot"], 5.0).clamp(0.0, top - 0.1),
            top,
            // (the legends' opacity: what they let through is the rest)
            legends: (1.0 - f(&light["legends"]["opacity"], 0.7)).clamp(0.0, 1.0),
            caps,
            skirts,
            tops,
            black,
            milky: f(&cp["milky"], 0.0).clamp(0.0, 1.0),
            cap_in: share(&cp["inside"], 0.5),
            cap_out: share(&cp["outside"], 0.18),
            wall: f(&wl["depth"], 2.0).max(0.0),
            deck: share(&wl["deck"], 0.22),
            chassis: share(&wl["chassis"], 0.4),
        }
    }

    /// As the simulation runs it: a black housing scatters none of the LED's
    /// light, so all of it leaves in the beam.
    pub fn simulated(&self) -> Physics {
        if self.switch == Switch::Slot { Physics { scatter: 0.0, ..*self } } else { *self }
    }

    /// Whether the light falls as it did: everything but its overall brightness.
    pub fn same_paths(&self, o: &Physics) -> bool {
        Physics { lumens: 0.0, ..*self } == Physics { lumens: 0.0, ..*o }
    }

    /// How far the switch's edge is from its centre, in the art's units:
    /// the furthest an LED can sit from the centre, inside the switch.
    pub fn switch_edge(&self) -> f32 {
        self.switch_width * self.units_per_mm / 2.0
    }

    /// Where an LED sits, `north` of its key's centre in the art's units, as
    /// the simulation holds it: inside its switch, at most at its edge.
    pub fn held_north(&self, north: f32) -> f32 {
        north.clamp(-self.switch_edge(), self.switch_edge())
    }

    /// Its own LED's place, held inside its switch.
    pub fn held_led(&self) -> f32 {
        self.held_north(self.led)
    }

    /// The beam's light from its source, `up` the cosine of the angle off
    /// straight up: (n + 1) / 2pi cos^n, for its share of the LED's.
    fn beam_at(&self, up: f32) -> f32 {
        if up <= 0.0 {
            return 0.0;
        }
        (1.0 - self.scatter) * (self.beam + 1.0) / std::f32::consts::TAU * up.powf(self.beam)
    }
}

/// A cap: its foot (its skirt's outline) and its face, in the art's units,
/// each a box with rounded corners: the foot's radius, then the face's (each
/// no more than half its box's narrower side).
#[derive(Clone, Copy, Debug)]
struct Cap {
    foot: [f32; 4],
    face: [f32; 4],
    round: [f32; 2],
}

/// A rounded box's corners' radius, no more than half its narrower side.
fn radius(b: [f32; 4], r: f32) -> f32 {
    r.clamp(0.0, ((b[2] - b[0]).min(b[3] - b[1]) / 2.0).max(0.0))
}

/// Whether (`x`, `y`) is inside the box `b` with its corners rounded to `r`
/// (on its outline is outside).
pub fn in_rounded(b: [f32; 4], r: f32, x: f32, y: f32) -> bool {
    in_round(b, radius(b, r), x, y)
}

/// in_rounded, for a radius already no more than half the box's narrower side
fn in_round(b: [f32; 4], r: f32, x: f32, y: f32) -> bool {
    if x <= b[0] || x >= b[2] || y <= b[1] || y >= b[3] {
        return false;
    }
    // how far into a corner's square it is across and down (none, off them)
    let dx = (b[0] + r - x).max(x - (b[2] - r)).max(0.0);
    let dy = (b[1] + r - y).max(y - (b[3] - r)).max(0.0);
    dx == 0.0 || dy == 0.0 || dx * dx + dy * dy < r * r
}

/// For a place by a cap's foot (the box `f` with its corners rounded to
/// `r`): how far in from the foot's outline it is (outside it, less than
/// nothing), the outline's outward direction where it is nearest, and that
/// nearest point.
pub fn from_foot(f: [f32; 4], r: f32, x: f32, y: f32) -> (f32, (f32, f32), (f32, f32)) {
    let r = radius(f, r);
    let (cx, cy) = (x.clamp(f[0] + r, f[2] - r), y.clamp(f[1] + r, f[3] - r));
    let in_box = x > f[0] && x < f[2] && y > f[1] && y < f[3];
    if !in_box || (r > 0.0 && cx != x && cy != y) {
        // outside it, or by a corner: out from the box the round goes round
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt();
        if d > 1e-6 {
            let n = (dx / d, dy / d);
            return (r - d, n, (cx + n.0 * r, cy + n.1 * r));
        }
    }
    let sides = [(x - f[0], (-1.0, 0.0), (f[0], y)), (f[2] - x, (1.0, 0.0), (f[2], y)), (y - f[1], (0.0, -1.0), (x, f[1])), (f[3] - y, (0.0, 1.0), (x, f[3]))];
    sides.into_iter().fold(sides[0], |b, s| if s.0 < b.0 { s } else { b })
}

/// How far along the line from `from` (inside the box `b` with its corners
/// rounded to `r`) to `to` it leaves the box: 0 at `from`, 1 at `to`; with
/// `to` inside it too, never.
fn leave_rounded(from: (f32, f32), to: (f32, f32), b: [f32; 4], r: f32) -> f32 {
    if in_rounded(b, r, to.0, to.1) {
        return f32::INFINITY;
    }
    let edge = |from: f32, to: f32, lo: f32, hi: f32| if to > hi { (hi - from) / (to - from) } else if to < lo { (lo - from) / (to - from) } else { f32::INFINITY };
    let t = edge(from.0, to.0, b[0], b[2]).min(edge(from.1, to.1, b[1], b[3])).min(1.0);
    let r = radius(b, r);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let (px, py) = (from.0 + t * dx, from.1 + t * dy);
    let (cx, cy) = (px.clamp(b[0] + r, b[2] - r), py.clamp(b[1] + r, b[3] - r));
    if r <= 0.0 || cx == px || cy == py {
        return t;
    }
    // it leaves the box by a corner: where it leaves the corner's round
    let (ox, oy) = (from.0 - cx, from.1 - cy);
    let (a, h, c) = (dx * dx + dy * dy, ox * dx + oy * dy, ox * ox + oy * oy - r * r);
    let disc = h * h - a * c;
    if a <= 0.0 || disc < 0.0 {
        return t;
    }
    ((-h + disc.sqrt()) / a).min(t)
}

/// What shows from above at a place: nothing (off the keyboard), the deck,
/// the chassis, a cap's skirt (which cap, which side of it: left, right, top,
/// bottom; how far up it, 0 at its foot to 1 at the face), or a cap's top
/// (which cap).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Surface {
    None,
    Deck,
    Chassis,
    Skirt(u32, u8, f32),
    Top(u32),
}

impl Surface {
    /// which of a key's maps of bounced light it is in: the deck, the
    /// chassis, the skirts, the caps' tops
    fn map(&self) -> Option<usize> {
        match self {
            Surface::Deck => Some(0),
            Surface::Chassis => Some(1),
            Surface::Skirt(..) => Some(2),
            Surface::Top(_) => Some(3),
            Surface::None => None,
        }
    }

    /// what a place's surface is, for drawing it: 0 the deck, 1 the chassis,
    /// 2 a skirt, 3 a cap's top, 255 nothing
    fn code(&self) -> u8 {
        self.map().map_or(255, |m| m as u8)
    }
}

/// A grid over the art: `cell` units a cell, `w` by `h` cells from (`x0`, `y0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub x0: f32,
    pub y0: f32,
    pub cell: f32,
    pub w: usize,
    pub h: usize,
}

impl Grid {
    fn middle(&self, i: usize, j: usize) -> (f32, f32) {
        (self.x0 + (i as f32 + 0.5) * self.cell, self.y0 + (j as f32 + 0.5) * self.cell)
    }

    fn at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let (i, j) = (((x - self.x0) / self.cell).floor(), ((y - self.y0) / self.cell).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.w && (j as usize) < self.h).then(|| (i as usize, j as usize))
    }

    /// A place in the art as a place on the grid, a cell's middle at whole numbers.
    pub fn place(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.x0) / self.cell - 0.5, (y - self.y0) / self.cell - 0.5)
    }

    /// The cell nearest a place on the grid.
    pub fn nearest(&self, gx: f32, gy: f32) -> usize {
        let (i, j) = ((gx + 0.5).floor().clamp(0.0, (self.w - 1) as f32) as usize, (gy + 0.5).floor().clamp(0.0, (self.h - 1) as f32) as usize);
        j * self.w + i
    }

    /// The four cells round a place on the grid, and each one's share of
    /// it, counting only the cells `ok` takes: none at all, if it takes none.
    pub fn round(&self, gx: f32, gy: f32, ok: impl Fn(usize) -> bool) -> [(usize, f32); 4] {
        let gx = gx.clamp(0.0, (self.w - 1) as f32);
        let gy = gy.clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (gx.floor() as usize, gy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
        let mut c = [(y0 * self.w + x0, (1.0 - fx) * (1.0 - fy)), (y0 * self.w + x1, fx * (1.0 - fy)), (y1 * self.w + x0, (1.0 - fx) * fy), (y1 * self.w + x1, fx * fy)];
        let mut sum = 0.0;
        for (i, w) in c.iter_mut() {
            if !ok(*i) {
                *w = 0.0;
            }
            sum += *w;
        }
        if sum > 0.0 {
            c.iter_mut().for_each(|(_, w)| *w /= sum);
        }
        c
    }
}

/// Where to read a place's light: what shows there (0 the deck, 1 the
/// chassis, 2 a skirt, 3 a cap's top), and the cells round it to read it
/// from, each with its share: on the fine grid, the cells of that same
/// surface; on the coarse grid, the cells its map covers.
#[derive(Clone, Copy, Debug)]
pub struct At {
    pub code: u8,
    pub fine: [(usize, f32); 4],
    pub coarse: [(usize, f32); 4],
}

/// The keys' light in their colours, as Snapshot::lit adds it up: on the
/// fine grid, and on the coarse grid's maps, a channel each.
pub struct Coloured {
    pub fine: [Vec<f32>; 3],
    pub coarse: [[Vec<f32>; 3]; MAPS],
}

impl Coloured {
    /// the light at a place, its red, green and blue (as Snapshot::white has it)
    pub fn at(&self, a: &At) -> [f32; 3] {
        let maps = &self.coarse[a.code as usize];
        let direct = a.code as usize != TOP_OUTSIDE;
        [0, 1, 2].map(|c| if direct { a.fine.iter().map(|(i, w)| self.fine[c][*i] * w).sum::<f32>() } else { 0.0 } + a.coarse.iter().map(|(i, w)| maps[c][*i] * w).sum::<f32>())
    }
}

/// A box of a grid's cells, `w` x `h` from cell (`x`, `y`), a value in each.
#[derive(Clone, Debug)]
pub struct Boxed {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}

impl Boxed {
    fn round(g: &Grid, (cx, cy): (f32, f32), reach: f32) -> Boxed {
        let x0 = ((cx - reach - g.x0) / g.cell).floor().max(0.0) as usize;
        let y0 = ((cy - reach - g.y0) / g.cell).floor().max(0.0) as usize;
        let x1 = (((cx + reach - g.x0) / g.cell).ceil().max(0.0) as usize).min(g.w);
        let y1 = (((cy + reach - g.y0) / g.cell).ceil().max(0.0) as usize).min(g.h);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        Boxed { x: x0, y: y0, w, h, v: vec![0.0; w * h] }
    }

    /// add `k` of it into a map of the whole grid
    pub fn add_into(&self, map: &mut [f32], gw: usize, k: f32) {
        self.add_into_rows(map, gw, 0, k);
    }

    /// add `k` of it into the rows from `y0` on of a map of the whole grid,
    /// `band` being as many of them as it holds
    pub fn add_into_rows(&self, band: &mut [f32], gw: usize, y0: usize, k: f32) {
        let rows = band.len() / gw;
        for y in self.y.max(y0)..(self.y + self.h).min(y0 + rows) {
            let j = y - self.y;
            let row = &mut band[(y - y0) * gw + self.x..(y - y0) * gw + self.x + self.w];
            for (o, v) in row.iter_mut().zip(&self.v[j * self.w..(j + 1) * self.w]) {
                *o += k * v;
            }
        }
    }
}

/// The keyboard's shape for the simulation, in the art's units.
pub struct Scene {
    /// each well's box, and the radius its corners are rounded to
    wells: Vec<([f32; 4], f32)>,
    caps: Vec<Cap>,
    /// the caps by the cells of a grid over the art
    cells: Vec<Vec<u32>>,
    cg: Grid,
    /// what shows from above at each fine cell (where it is nothing, the body is not)
    fine: Grid,
    surface: Vec<Surface>,
    /// each lit key: its place, and its foot's middle (its cap is the same
    /// in the caps, which start with the lit keys')
    keys: Vec<(u32, (f32, f32))>,
    /// the art's units to the board's pixels: scale, and offset across and down
    px: (f32, f32, f32),
    /// each cap's foot's own shape, where it is no rounded box, in the board's pixels
    odd: Vec<Option<Arc<super::Shape>>>,
}

impl Scene {
    /// The shape of the keyboard `b` draws: its lit keys' caps, the caps of
    /// the keys with no light (the spacebar, the media keys), its wells and
    /// its body.
    pub fn of(b: &Board) -> Scene {
        let (ka, (ox, oy)) = (b.art_scale, b.art_offset);
        let unit = |px: f32, py: f32| ((px - ox) / ka, (py - oy) / ka);
        let wells: Vec<([f32; 4], f32)> = b.well_boxes.iter().zip(&b.well_round).filter(|(w, _)| w[0] < w[2] && w[1] < w[3]).map(|(&w, &r)| (w, radius(w, r))).collect();
        let mut caps = Vec::new();
        let mut keys = Vec::new();
        for k in &b.keys {
            if let Some(f) = k.foot {
                keys.push((k.pos, ((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0)));
                caps.push(Cap { foot: f, face: k.face, round: [radius(f, k.round[0]), radius(k.face, k.round[1])] });
            }
        }
        caps.extend(b.others.iter().map(|&(foot, face, round)| Cap { foot, face, round: [radius(foot, round[0]), radius(face, round[1])] }));

        // the art's extent: the board's
        let (x0, y0) = unit(0.0, 0.0);
        let (x1, y1) = unit(b.w as f32, b.h as f32);
        let fine = Grid { x0, y0, cell: FINE, w: ((x1 - x0) / FINE).ceil() as usize, h: ((y1 - y0) / FINE).ceil() as usize };
        let cell = 13.0;
        let cg = Grid { x0, y0, cell, w: ((x1 - x0) / cell).ceil() as usize + 1, h: ((y1 - y0) / cell).ceil() as usize + 1 };
        let mut cells = vec![Vec::new(); cg.w * cg.h];
        for (i, c) in caps.iter().enumerate() {
            let (Some((cx0, cy0)), Some((cx1, cy1))) = (cg.at(c.foot[0].max(x0), c.foot[1].max(y0)), cg.at(c.foot[2].min(x1 - 0.01), c.foot[3].min(y1 - 0.01))) else { continue };
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    cells[cy * cg.w + cx].push(i as u32);
                }
            }
        }
        let odd = b.odd_feet.iter().map(|o| o.clone().map(Arc::new)).collect();
        let mut scene = Scene { wells, caps, cells, cg, fine, surface: Vec::new(), keys, px: (ka, ox, oy), odd };
        let body = |x: f32, y: f32| {
            let (px, py) = (ox + ka * x, oy + ka * y);
            px >= 0.0 && py >= 0.0 && (px as u32) < b.w && (py as u32) < b.h && b.chassis[(py as u32 * b.w + px as u32) as usize] > 0.5
        };
        scene.surface = (0..fine.w * fine.h)
            .map(|n| {
                let (x, y) = fine.middle(n % fine.w, n / fine.w);
                scene.surface_at(x, y, &body)
            })
            .collect();
        scene
    }

    fn caps_at(&self, x: f32, y: f32) -> &[u32] {
        self.cg.at(x, y).map_or(&[], |(i, j)| &self.cells[j * self.cg.w + i])
    }

    fn surface_at(&self, x: f32, y: f32, body: &dyn Fn(f32, f32) -> bool) -> Surface {
        for &i in self.caps_at(x, y) {
            let c = &self.caps[i as usize];
            let (f, a) = (c.foot, c.face);
            if !in_round(f, c.round[0], x, y) {
                continue;
            }
            // (a foot of its own shape: only where it is)
            if let Some(o) = self.odd.get(i as usize).and_then(|o| o.as_deref()) {
                let (ka, ox, oy) = self.px;
                let (i, j) = ((ox + ka * x).floor() as i32 - o.x, (oy + ka * y).floor() as i32 - o.y);
                if i < 0 || j < 0 || i >= o.w as i32 || j >= o.h as i32 || o.cover[(j as u32 * o.w + i as u32) as usize] < 0.5 {
                    continue;
                }
            }
            if in_round(a, c.round[1], x, y) {
                return Surface::Top(i);
            }
            // on the skirt: the side it is furthest up
            let ups = [
                (0u8, (x - f[0]) / (a[0] - f[0]).max(1e-3)),
                (1, (f[2] - x) / (f[2] - a[2]).max(1e-3)),
                (2, (y - f[1]) / (a[1] - f[1]).max(1e-3)),
                (3, (f[3] - y) / (f[3] - a[3]).max(1e-3)),
            ];
            let (side, up) = ups.iter().copied().filter(|(_, u)| *u < 1.0).fold((0u8, -1.0f32), |b, (s, u)| if u > b.1 { (s, u) } else { b });
            return Surface::Skirt(i, side, up.clamp(0.0, 1.0));
        }
        if self.in_well(x, y) {
            Surface::Deck
        } else if body(x, y) {
            Surface::Chassis
        } else {
            Surface::None
        }
    }

    /// Whether a place is in a well, its corners as round as the art has them.
    fn in_well(&self, x: f32, y: f32) -> bool {
        self.well_of(x, y).is_some()
    }

    fn well_of(&self, x: f32, y: f32) -> Option<usize> {
        self.wells.iter().position(|&(w, r)| in_round(w, r, x, y))
    }

    fn on_body(&self, x: f32, y: f32) -> bool {
        self.fine.at(x, y).is_some_and(|(i, j)| self.surface[j * self.fine.w + i] != Surface::None)
    }

    /// Where a key's switch's centre is: its foot's middle, or as far east
    /// and north of it as the key's own physics has its switch.
    fn switch_at(&self, k: usize, p: &Physics) -> (f32, f32) {
        let (x, y) = self.keys[k].1;
        (x + p.place[0] * p.units_per_mm, y - p.place[1] * p.units_per_mm)
    }

    /// Where a key's LED sits: north of its switch's centre, inside the switch.
    fn led_at(&self, k: usize, p: &Physics) -> (f32, f32) {
        let (x, y) = self.switch_at(k, p);
        (x, y - p.held_led())
    }
}

/// The heights and the housing's half width, in the art's units, and the shares.
#[derive(Clone, Copy)]
struct Stood {
    beam_from: f32,
    half: f32,
    spread: f32,
    /// how high the skirt lets light through, up to where the black comes down
    /// it, and how far milky plastic carries the light it scatters
    clear_to: f32,
    diffuse: f32,
    gap: f32,
    top: f32,
    wall: f32,
    p: Physics,
}

impl Stood {
    fn of(p: &Physics) -> Stood {
        let mm = p.units_per_mm;
        let p = p.simulated();
        let (gap, top) = (p.gap * mm, p.top * mm);
        Stood { beam_from: p.switch_top * mm, half: p.switch_width * mm / 2.0, spread: p.glow_spread * mm, clear_to: gap + (1.0 - p.black) * (top - gap), diffuse: MILKY * mm, gap, top, wall: p.wall * mm, p }
    }
}

/// Every cap's physics as the simulation stands it up: each cap's own where
/// the setup gives it one, all keys' otherwise. The wells, the deck and the
/// chassis are every key's alike: all keys'.
struct Stoods {
    all: Stood,
    caps: Vec<Stood>,
}

impl Stoods {
    fn of(scene: &Scene, setup: &Setup) -> Stoods {
        Stoods { all: Stood::of(&setup.all), caps: (0..scene.caps.len() as u32).map(|i| Stood::of(setup.of(i))).collect() }
    }

    fn cap(&self, i: u32) -> &Stood {
        self.caps.get(i as usize).unwrap_or(&self.all)
    }
}

/// How a surface's light is shared out: what it gives back and what it lets
/// through, each its share, together never more than the light it takes.
fn shared(back: f32, through: f32) -> (f32, f32) {
    let all = back + through;
    if all > 1.0 { (back / all, through / all) } else { (back, through) }
}

/// For the share of a matte rectangle's light that reaches a small surface
/// facing it, `h` from it and parallel to it: for a rectangle from the point
/// under the surface out to (x, y), (1/2pi) (X/sqrt(1+X²) atan(Y/sqrt(1+X²))
/// + Y/sqrt(1+Y²) atan(X/sqrt(1+Y²))) with X = x/h, Y = y/h, signed by its
/// side; any rectangle's share is the sum of its four corners' (see parallel).
fn corner(x: f32, y: f32, h: f32) -> f32 {
    let (a, b) = (x / h, y / h);
    let (sa, sb) = ((1.0 + a * a).sqrt(), (1.0 + b * b).sqrt());
    (a / sa * (b / sa).atan() + b / sb * (a / sb).atan()) / std::f32::consts::TAU
}

/// The share of a matte rectangle's light, from (`x0`, `y0`) to (`x1`, `y1`),
/// that reaches a small surface over (0, 0) `h` from it, facing it.
#[cfg(test)]
fn parallel(x0: f32, y0: f32, x1: f32, y1: f32, h: f32) -> f32 {
    corner(x1, y1, h) - corner(x0, y1, h) - corner(x1, y0, h) + corner(x0, y0, h)
}

/// A switch's clear housing, glowing with its LED's scattered light: a box
/// `half` either side of the switch's centre `c`, from the deck up to the
/// switch's top `h`, its top giving off its share of the light and its sides
/// theirs, each as a matte surface does, and each brightest by the LED and
/// fading away from it as the light carries through the plastic (to about a
/// third every glow_spread). Taken a patch at a time, each patch even: its
/// top a grid of `n` by `n` squares, the light to each unit of their areas
/// in `tops`; its sides in strips up its height, each strip's light summed
/// up its height exactly: each strip's foot, its outward normal, its width
/// and the light to each unit of its area. And every patch's light added up
/// in turn, for sending photons from them by it.
struct Housing {
    c: (f32, f32),
    half: f32,
    h: f32,
    n: usize,
    tops: Vec<f32>,
    strips: Vec<((f32, f32), (f32, f32), f32, f32)>,
    cdf: Vec<f32>,
}

impl Housing {
    fn of(s: &Stood, c: (f32, f32), led: (f32, f32)) -> Housing {
        let (half, h) = (s.half, s.beam_from);
        let w = 2.0 * half;
        let fade = |x: f32, y: f32| (-((x - led.0).powi(2) + (y - led.1).powi(2)).sqrt() / s.spread).exp();
        // the top's squares, and the sides' strips, each as bright as it is near the LED
        let n = 8;
        let cell = w / n as f32;
        let mut tops: Vec<f32> = (0..n * n).map(|k| fade(c.0 - half + (k % n) as f32 * cell + cell / 2.0, c.1 - half + (k / n) as f32 * cell + cell / 2.0)).collect();
        let along = 24;
        let width = w / along as f32;
        let mut strips = Vec::new();
        for (side, nrm) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0f32)].into_iter().enumerate() {
            for i in 0..along {
                let a = -half + (i as f32 + 0.5) * width;
                let foot = if side < 2 { (c.0 + nrm.0 * half, c.1 + a) } else { (c.0 + a, c.1 + nrm.1 * half) };
                strips.push((foot, nrm, width, fade(foot.0, foot.1)));
            }
        }
        // each part its share of the scattered light, over its patches as bright as they are
        let top_sum: f32 = tops.iter().map(|t| t * cell * cell).sum();
        let side_sum: f32 = strips.iter().map(|st| st.3 * st.2 * h).sum();
        let (to_top, to_sides) = (s.p.scatter * (1.0 - s.p.sides) / top_sum.max(1e-12), s.p.scatter * s.p.sides / side_sum.max(1e-12));
        tops.iter_mut().for_each(|t| *t *= to_top);
        strips.iter_mut().for_each(|st| st.3 *= to_sides);
        let mut cdf = Vec::with_capacity(tops.len() + strips.len());
        let mut sum = 0.0;
        for t in &tops {
            sum += t * cell * cell;
            cdf.push(sum);
        }
        for st in &strips {
            sum += st.3 * st.2 * h;
            cdf.push(sum);
        }
        Housing { c, half, h, n, tops, strips, cdf }
    }

    /// A place on it to send a photon from, by how much light each patch
    /// gives off, and the way it faces there.
    fn emit(&self, rng: &mut Rng) -> ([f32; 3], [f32; 3]) {
        let total = self.cdf.last().copied().unwrap_or(0.0);
        let u = rng.f() * total;
        let k = self.cdf.partition_point(|c| *c < u).min(self.cdf.len() - 1);
        let tops = self.n * self.n;
        if k < tops {
            let cell = 2.0 * self.half / self.n as f32;
            let (i, j) = (k % self.n, k / self.n);
            ([self.c.0 - self.half + (i as f32 + rng.f()) * cell, self.c.1 - self.half + (j as f32 + rng.f()) * cell, self.h], [0.0, 0.0, 1.0])
        } else {
            let (f, nrm, width, _) = self.strips[k - tops];
            let a = (rng.f() - 0.5) * width;
            let p = if nrm.0 != 0.0 { [f.0, f.1 + a, rng.f() * self.h] } else { [f.0 + a, f.1, rng.f() * self.h] };
            (p, [nrm.0, nrm.1, 0.0])
        }
    }

    /// Its light on the back of its cap's top at `q`: from its top's squares,
    /// each exactly (see corner), and from its sides, each strip's
    /// (m/pi) D w (1/(r² + (H-h)²) - 1/(r² + H²)) / 2, D how far `q` is out in
    /// front of it, r how far across from it, H how high `q` is, m the strip's light.
    fn on_top(&self, q: [f32; 3]) -> f32 {
        let (x, y) = (q[0] - self.c.0, q[1] - self.c.1);
        let hh = (q[2] - self.h).max(1e-3);
        // the corner terms of the squares' grid, each square's share from its four
        let n = self.n;
        let cell = 2.0 * self.half / n as f32;
        let mut f = vec![0f32; (n + 1) * (n + 1)];
        for j in 0..=n {
            for i in 0..=n {
                f[j * (n + 1) + i] = corner(-self.half + i as f32 * cell - x, -self.half + j as f32 * cell - y, hh);
            }
        }
        let mut face = 0.0;
        for j in 0..n {
            for i in 0..n {
                let at = |i: usize, j: usize| f[j * (n + 1) + i];
                face += self.tops[j * n + i] * (at(i + 1, j + 1) - at(i, j + 1) - at(i + 1, j) + at(i, j));
            }
        }
        let big = q[2];
        let small = q[2] - self.h;
        let mut sides = 0.0;
        for &(f, nrm, width, m) in &self.strips {
            let (dx, dy) = (q[0] - f.0, q[1] - f.1);
            let d = dx * nrm.0 + dy * nrm.1;
            if d <= 0.0 {
                continue;
            }
            let r2 = dx * dx + dy * dy;
            sides += m * d * width * 0.5 * (1.0 / (r2 + small * small) - 1.0 / (r2 + big * big));
        }
        face + sides / std::f32::consts::PI
    }

    /// Its light on a floor at `q`, outside its cap's `foot` (the deck of its
    /// well, or the chassis round the well, from inside which nothing else
    /// stands in the way): from each strip of its sides between the floor's
    /// height and as high up as shines out under the skirt at `gap`, Z above
    /// the floor, (m/pi) D w (1/r² - 1/(r² + Z²)) / 2.
    fn on_floor(&self, q: [f32; 3], foot: [f32; 4], round: f32, gap: f32) -> f32 {
        let mut e = 0.0;
        for &(f, nrm, width, m) in &self.strips {
            let (dx, dy) = (q[0] - f.0, q[1] - f.1);
            let d = dx * nrm.0 + dy * nrm.1;
            if d <= 0.0 {
                continue;
            }
            // how far along the line from the strip to `q` it leaves the foot's outline
            let t = leave_rounded(f, (q[0], q[1]), foot, round);
            if !(t < 1.0) {
                continue;
            }
            // the line from a height z leaves the foot at z + t (floor - z),
            // under the skirt below `gap`; only above the floor does it come down on it
            let z = self.h.min((gap - t * q[2]) / (1.0 - t).max(1e-6)) - q[2];
            if z <= 0.0 {
                continue;
            }
            let r2 = dx * dx + dy * dy;
            e += m * d * width * 0.5 * (1.0 / r2 - 1.0 / (r2 + z * z));
        }
        e / std::f32::consts::PI
    }
}

/// The light straight from an LED and its housing on the back of its cap's
/// top, `along` (the art's units) from over the LED toward the foot of the
/// cap (back toward its top below 0), the LED `north` of the key's centre
/// (held inside its switch): what a legend there shows of it, for an LED of
/// unit power, before the light bounced inside the cap. For the Lighting
/// lab's curve.
pub fn on_cap_top(p: &Physics, north: f32, along: f32) -> f32 {
    let north = p.held_north(north);
    let s = Stood::of(p);
    let q = [0.0, -north + along, s.top];
    let from = [0.0, -north, s.beam_from];
    let d = [q[0] - from[0], q[1] - from[1], q[2] - from[2]];
    let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let up = d[2] / r2.sqrt();
    let beam = s.p.beam_at(up) * up / r2;
    p.legends / std::f32::consts::PI * (beam + Housing::of(&s, (0.0, 0.0), (0.0, -north)).on_top(q))
}

/// What a ray strikes first.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Struck {
    Deck,
    Chassis,
    Outside(u32, u8),
    Inside(u32, u8),
    Under(u32),
    Top(u32),
    Wall,
    Housing,
}

const EPS: f32 = 1e-3;

/// Where the ray from `o` along `d` enters the box from `lo` to `hi`, and the
/// face's normal there, toward the ray; none if it misses the box or starts inside it.
fn enter_box(lo: [f32; 3], hi: [f32; 3], o: [f32; 3], d: [f32; 3]) -> Option<(f32, [f32; 3])> {
    let (mut near, mut far, mut axis) = (f32::NEG_INFINITY, f32::INFINITY, 0);
    for a in 0..3 {
        if d[a] == 0.0 {
            if o[a] < lo[a] || o[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo[a] - o[a]) / d[a], (hi[a] - o[a]) / d[a]);
        let (t_in, t_out) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if t_in > near {
            (near, axis) = (t_in, a);
        }
        far = far.min(t_out);
    }
    if near > far || near <= EPS {
        return None;
    }
    let mut n = [0.0; 3];
    n[axis] = -d[axis].signum();
    Some((near, n))
}

impl Scene {
    /// The nearest thing the ray from `o` along `d` strikes within `reach`:
    /// how far, what, and the normal facing back toward the ray.
    fn hit(&self, ss: &Stoods, o: [f32; 3], d: [f32; 3], reach: f32) -> Option<(f32, Struck, [f32; 3])> {
        let wall = ss.all.wall;
        let mut best: Option<(f32, Struck, [f32; 3])> = None;
        let take = |t: f32, k: Struck, n: [f32; 3], best: &mut Option<(f32, Struck, [f32; 3])>| {
            if t > EPS && t < reach && best.map_or(true, |b| t < b.0) {
                *best = Some((t, k, n));
            }
        };
        if d[2] < 0.0 {
            // the deck, in the wells
            let t = -o[2] / d[2];
            let (x, y) = (o[0] + t * d[0], o[1] + t * d[1]);
            if self.in_well(x, y) {
                take(t, Struck::Deck, [0.0, 0.0, 1.0], &mut best);
            }
            // the chassis, round the wells, from above
            if o[2] > wall {
                let t = (wall - o[2]) / d[2];
                let (x, y) = (o[0] + t * d[0], o[1] + t * d[1]);
                if !self.in_well(x, y) && self.on_body(x, y) {
                    take(t, Struck::Chassis, [0.0, 0.0, 1.0], &mut best);
                }
            }
        }
        // the wells' walls, from inside: their flat sides, between the
        // corners, and the corners' rounds
        for &(w, r) in &self.wells {
            for (axis, at, inward) in [(0usize, w[0], 1.0f32), (0, w[2], -1.0), (1, w[1], 1.0), (1, w[3], -1.0)] {
                if d[axis] * inward >= 0.0 {
                    continue;
                }
                let t = (at - o[axis]) / d[axis];
                let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                let (lo, hi) = if axis == 0 { (w[1], w[3]) } else { (w[0], w[2]) };
                let q = p[1 - axis];
                if q > lo + r && q < hi - r && p[2] > 0.0 && p[2] < wall {
                    let mut n = [0.0; 3];
                    n[axis] = inward;
                    take(t, Struck::Wall, n, &mut best);
                }
            }
            // each corner a quarter of a round wall, met from inside on the way out
            let a = d[0] * d[0] + d[1] * d[1];
            if r > 0.0 && a > 1e-12 {
                for (cx, cy, sx, sy) in [(w[0] + r, w[1] + r, -1.0f32, -1.0f32), (w[2] - r, w[1] + r, 1.0, -1.0), (w[0] + r, w[3] - r, -1.0, 1.0), (w[2] - r, w[3] - r, 1.0, 1.0)] {
                    let (ex, ey) = (o[0] - cx, o[1] - cy);
                    let half_b = ex * d[0] + ey * d[1];
                    let disc = half_b * half_b - a * (ex * ex + ey * ey - r * r);
                    if disc < 0.0 {
                        continue;
                    }
                    let t = (-half_b + disc.sqrt()) / a;
                    let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                    let (dx, dy) = (p[0] - cx, p[1] - cy);
                    if dx * sx > 0.0 && dy * sy > 0.0 && p[2] > 0.0 && p[2] < wall {
                        take(t, Struck::Wall, [-dx / r, -dy / r, 0.0], &mut best);
                    }
                }
            }
        }
        // the caps, by the grid's cells along the ray
        let far = best.map_or(reach, |b| b.0).min(reach);
        let mut seen = [u32::MAX; 32];
        let mut ns = 0usize;
        self.along(o, d, far, &mut |i| {
            if seen[..ns].contains(&i) {
                return;
            }
            if ns < seen.len() {
                seen[ns] = i;
                ns += 1;
            }
            let c = &self.caps[i as usize];
            let s = ss.cap(i);
            let (f, r) = (c.foot, c.round[0]);
            let inside = in_round(f, r, o[0], o[1]);
            // the skirt's flat sides, between its rounded corners; where the
            // ray meets a side by a corner, the corner's round instead (a
            // quarter round about its middle), which it can only meet there
            let mut rounds = 0u8;
            for (side, axis, at) in [(0u8, 0usize, f[0]), (1, 0, f[2]), (2, 1, f[1]), (3, 1, f[3])] {
                if d[axis] == 0.0 {
                    continue;
                }
                let t = (at - o[axis]) / d[axis];
                let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                let (lo, hi) = if axis == 0 { (f[1], f[3]) } else { (f[0], f[2]) };
                let q = p[1 - axis];
                if q < lo || q > hi {
                    continue;
                }
                if q >= lo + r && q <= hi - r {
                    if p[2] >= s.gap && p[2] <= s.top {
                        let mut n = [0.0; 3];
                        n[axis] = -d[axis].signum();
                        take(t, if inside { Struck::Inside(i, side) } else { Struck::Outside(i, side) }, n, &mut best);
                    }
                    continue;
                }
                // the corner: left or right (bit 0), top or bottom (bit 1)
                let high = (q > hi - r) as u8;
                let corner = if axis == 0 { (side & 1) | (high << 1) } else { high | ((side & 1) << 1) };
                rounds |= 1 << corner;
            }
            let a = d[0] * d[0] + d[1] * d[1];
            for corner in 0..4u8 {
                if rounds & (1 << corner) == 0 || a <= 0.0 {
                    continue;
                }
                let (sx, sy) = (if corner & 1 == 0 { -1.0f32 } else { 1.0 }, if corner & 2 == 0 { -1.0f32 } else { 1.0 });
                let (cx, cy) = (if sx < 0.0 { f[0] + r } else { f[2] - r }, if sy < 0.0 { f[1] + r } else { f[3] - r });
                let (ox, oy) = (o[0] - cx, o[1] - cy);
                let (h, cc) = (ox * d[0] + oy * d[1], ox * ox + oy * oy - r * r);
                let disc = h * h - a * cc;
                if disc < 0.0 {
                    continue;
                }
                for t in [(-h - disc.sqrt()) / a, (-h + disc.sqrt()) / a] {
                    let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                    let (rx, ry) = (p[0] - cx, p[1] - cy);
                    if rx * sx < 0.0 || ry * sy < 0.0 || p[2] < s.gap || p[2] > s.top {
                        continue;
                    }
                    // facing back toward the ray; the side it is most on, for its band
                    let mut n = [rx / r, ry / r, 0.0];
                    if n[0] * d[0] + n[1] * d[1] > 0.0 {
                        n = [-n[0], -n[1], 0.0];
                    }
                    let side = if rx.abs() > ry.abs() { if sx < 0.0 { 0 } else { 1 } } else if sy < 0.0 { 2 } else { 3 };
                    take(t, if inside { Struck::Inside(i, side) } else { Struck::Outside(i, side) }, n, &mut best);
                }
            }
            // a lit key's switch, its housing black, in the light's way
            if s.p.switch == Switch::Slot && (i as usize) < self.keys.len() {
                let (cx, cy) = self.switch_at(i as usize, &s.p);
                if let Some((t, n)) = enter_box([cx - s.half, cy - s.half, 0.0], [cx + s.half, cy + s.half, s.beam_from], o, d) {
                    take(t, Struck::Housing, n, &mut best);
                }
            }
            if d[2] != 0.0 {
                let t = (s.top - o[2]) / d[2];
                let (x, y) = (o[0] + t * d[0], o[1] + t * d[1]);
                if in_round(f, r, x, y) {
                    let below = o[2] < s.top;
                    take(t, if below { Struck::Under(i) } else { Struck::Top(i) }, [0.0, 0.0, if below { -1.0 } else { 1.0 }], &mut best);
                }
            }
        });
        best
    }

    /// Every cap in the grid's cells the ray passes over, as far as `reach`.
    /// (A lit key's switch stands inside its cap's foot, so the caps' cells hold it too.)
    fn along(&self, o: [f32; 3], d: [f32; 3], reach: f32, f: &mut dyn FnMut(u32)) {
        let g = &self.cg;
        let (fx, fy) = ((o[0] - g.x0) / g.cell, (o[1] - g.y0) / g.cell);
        let (mut cx, mut cy) = (fx.floor() as i64, fy.floor() as i64);
        let (sx, sy) = (if d[0] > 0.0 { 1 } else { -1 }, if d[1] > 0.0 { 1 } else { -1 });
        let next = |c: i64, s: i64, from: f32, dir: f32| if dir == 0.0 { f32::INFINITY } else { ((c + if s > 0 { 1 } else { 0 }) as f32 - from) * g.cell / dir };
        let (mut tx, mut ty) = (next(cx, sx, fx, d[0]), next(cy, sy, fy, d[1]));
        let (dx, dy) = (if d[0] == 0.0 { f32::INFINITY } else { g.cell / d[0].abs() }, if d[1] == 0.0 { f32::INFINITY } else { g.cell / d[1].abs() });
        let mut t = 0.0f32;
        for _ in 0..4096 {
            if cx >= 0 && cy >= 0 && (cx as usize) < g.w && (cy as usize) < g.h {
                for &i in &self.cells[cy as usize * g.w + cx as usize] {
                    f(i);
                }
            } else if (cx < 0 && sx < 0) || (cy < 0 && sy < 0) || (cx >= g.w as i64 && sx > 0) || (cy >= g.h as i64 && sy > 0) {
                return;
            }
            if t > reach {
                return;
            }
            if tx < ty {
                t = tx;
                tx += dx;
                cx += sx;
            } else {
                t = ty;
                ty += dy;
                cy += sy;
            }
        }
    }

    /// Whether nothing stands between the point `a` and the point `b`.
    fn clear(&self, ss: &Stoods, a: [f32; 3], b: [f32; 3]) -> bool {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len <= EPS {
            return true;
        }
        let d = [d[0] / len, d[1] / len, d[2] / len];
        self.hit(ss, a, d, len - 2.0 * EPS).is_none()
    }

    /// Where on the art a skirt's outside at this height shows from above, and
    /// how much of the skirt each unit of its band stands for.
    fn band(&self, ss: &Stoods, cap: u32, side: u8, p: [f32; 3]) -> ((f32, f32), f32) {
        let (c, s) = (&self.caps[cap as usize], ss.cap(cap));
        let up = ((p[2] - s.gap) / (s.top - s.gap)).clamp(0.0, 1.0);
        let (u, v, band) = match side {
            0 => (c.foot[0] + (c.face[0] - c.foot[0]) * up, p[1], c.face[0] - c.foot[0]),
            1 => (c.foot[2] - (c.foot[2] - c.face[2]) * up, p[1], c.foot[2] - c.face[2]),
            2 => (p[0], c.foot[1] + (c.face[1] - c.foot[1]) * up, c.face[1] - c.foot[1]),
            _ => (p[0], c.foot[3] - (c.foot[3] - c.face[3]) * up, c.foot[3] - c.face[3]),
        };
        ((u, v), band.max(0.05) / (s.top - s.gap))
    }

    /// Where light a cap's plastic takes in at `q` (on its skirt's side
    /// `side`, or the underside of its top) comes out of it, and on which side
    /// of the skirt. What it lets straight through comes out where it went
    /// in, through the skirt below the black (from the top, it goes into the
    /// black: it never reaches the skirt); what milky plastic scatters
    /// (`scattered`) it carries round the cap's foot, as far each way as it
    /// carries light, to anywhere up the skirt below the black.
    fn exit(&self, ss: &Stoods, cap: u32, side: Option<u8>, q: [f32; 3], scattered: bool, rng: &mut Rng) -> Option<([f32; 3], u8)> {
        let (f, s) = (self.caps[cap as usize].foot, ss.cap(cap));
        if s.clear_to <= s.gap {
            return None;
        }
        if !scattered {
            return side.filter(|_| q[2] <= s.clear_to).map(|side| (q, side));
        }
        // round the foot from its north-west corner: east along its north
        // side, south down its east, west along its south, north up its west
        let (w, h) = (f[2] - f[0], f[3] - f[1]);
        let side = side.unwrap_or_else(|| {
            let d = [q[0] - f[0], f[2] - q[0], q[1] - f[1], f[3] - q[1]];
            (0..4).fold(0u8, |b, i| if d[i] < d[b as usize] { i as u8 } else { b })
        });
        let went_in = match side {
            2 => (q[0] - f[0]).clamp(0.0, w),
            1 => w + (q[1] - f[1]).clamp(0.0, h),
            3 => w + h + (f[2] - q[0]).clamp(0.0, w),
            _ => 2.0 * w + h + (f[3] - q[1]).clamp(0.0, h),
        };
        let (along, _) = rng.normals();
        let at = (went_in + along * s.diffuse).rem_euclid(2.0 * (w + h));
        let z = s.gap + rng.f() * (s.clear_to - s.gap);
        Some(if at < w {
            ([f[0] + at, f[1], z], 2)
        } else if at < w + h {
            ([f[2], f[1] + at - w, z], 1)
        } else if at < 2.0 * w + h {
            ([f[2] - (at - w - h), f[3], z], 3)
        } else {
            ([f[0], f[3] - (at - 2.0 * w - h), z], 0)
        })
    }

    /// The point on a cap's skirt a place on its band shows, and the skirt's
    /// outward normal there.
    fn skirt_point(&self, ss: &Stoods, cap: u32, side: u8, up: f32, (x, y): (f32, f32)) -> ([f32; 3], [f32; 3]) {
        let (f, s) = (self.caps[cap as usize].foot, ss.cap(cap));
        let z = s.gap + up * (s.top - s.gap);
        match side {
            0 => ([f[0], y, z], [-1.0, 0.0, 0.0]),
            1 => ([f[2], y, z], [1.0, 0.0, 0.0]),
            2 => ([x, f[1], z], [0.0, -1.0, 0.0]),
            _ => ([x, f[3], z], [0.0, 1.0, 0.0]),
        }
    }
}

/// A small fast random number source, one a thread.
struct Rng(u64);

impl Rng {
    /// two independent normal deviates (Box and Muller)
    fn normals(&mut self) -> (f32, f32) {
        let (u, v) = (self.f().max(1e-9), self.f());
        let r = (-2.0 * u.ln()).sqrt();
        let a = std::f32::consts::TAU * v;
        (r * a.cos(), r * a.sin())
    }

    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// straight up, falling as the cosine to the power `n`
    fn beam(&mut self, n: f32) -> [f32; 3] {
        let z = self.f().max(1e-9).powf(1.0 / (n + 1.0));
        let a = std::f32::consts::TAU * self.f();
        let r = (1.0 - z * z).max(0.0).sqrt();
        [r * a.cos(), r * a.sin(), z]
    }

    fn diffuse(&mut self, n: [f32; 3]) -> [f32; 3] {
        let (u, a) = (self.f(), std::f32::consts::TAU * self.f());
        let (r, z) = (u.sqrt(), (1.0 - u).max(0.0).sqrt());
        let t = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let b = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
        let bl = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
        let b = [b[0] / bl, b[1] / bl, b[2] / bl];
        let c = [n[1] * b[2] - n[2] * b[1], n[2] * b[0] - n[0] * b[2], n[0] * b[1] - n[1] * b[0]];
        let (x, y) = (r * a.cos(), r * a.sin());
        [x * b[0] + y * c[0] + z * n[0], x * b[1] + y * c[1] + z * n[1], x * b[2] + y * c[2] + z * n[2]]
    }
}

/// The simulation's picture so far: for each lit key (in the order the
/// scene has them, which is the board's), what its LED shows at each place
/// from above, for an LED of unit power: straight from the LED, on the fine
/// grid; bounced, on the coarse grid, a map each for the open deck and
/// chassis, the skirts and the caps' tops; and every key's added up, for
/// keys all lit alike. Each place gives the light seen there.
pub struct Snapshot {
    /// how every key was lit, as the simulation ran it
    pub setup: Setup,
    /// photons sent from each LED so far, and whether that is all
    pub photons: usize,
    pub settled: bool,
    pub fine: Grid,
    pub coarse: Grid,
    /// what shows at each fine cell (Surface::code): 0 the deck, 1 the chassis, 2 a skirt, 3 a cap's top, 255 nothing
    pub surface: Arc<Vec<u8>>,
    /// how much of each coarse cell each map's surfaces cover
    pub cover: Arc<[Vec<f32>; MAPS]>,
    pub keys: Vec<u32>,
    pub direct: Arc<Vec<Boxed>>,
    pub bounced: Vec<[Boxed; MAPS]>,
    pub direct_all: Arc<Vec<f32>>,
    pub bounced_all: [Vec<f32>; MAPS],
}

impl Snapshot {
    /// Where to read the light at a place in the art: as what shows there
    /// from above, or as `code` says; nothing off the keyboard.
    pub fn at(&self, u: f32, v: f32, code: Option<u8>) -> Option<At> {
        let (fx, fy) = self.fine.place(u, v);
        let code = code.unwrap_or_else(|| self.surface[self.fine.nearest(fx, fy)]);
        if code as usize >= MAPS {
            return None;
        }
        // (the outside of a cap's top is where its top is)
        let cells = if code as usize == TOP_OUTSIDE { 3 } else { code };
        let fine = self.fine.round(fx, fy, |i| self.surface[i] == cells);
        let (cx, cy) = self.coarse.place(u, v);
        let cover = &self.cover[code as usize];
        let coarse = self.coarse.round(cx, cy, |i| cover[i] > 0.0);
        Some(At { code, fine, coarse })
    }

    /// The light at a place with every key lit white at full. (The light
    /// straight from an LED onto a cap's top is on its back: its outside has none.)
    pub fn white(&self, a: &At) -> f32 {
        let map = &self.bounced_all[a.code as usize];
        let direct = if a.code as usize == TOP_OUTSIDE { 0.0 } else { a.fine.iter().map(|(i, w)| self.direct_all[*i] * w).sum::<f32>() };
        direct + a.coarse.iter().map(|(i, w)| map[*i] * w).sum::<f32>()
    }

    /// The light the keys show, each in its own colour (linear, in the
    /// keys' order), added up.
    pub fn lit(&self, colours: &[[f32; 3]]) -> Coloured {
        Coloured { fine: self.spread(&self.fine, colours, &|k| &self.direct[k]), coarse: std::array::from_fn(|m| self.spread(&self.coarse, colours, &|k| &self.bounced[k][m])) }
    }

    /// Each key's box on a grid in its colour, added up: a map a channel,
    /// a band of rows a thread.
    fn spread<'a>(&'a self, g: &Grid, colours: &[[f32; 3]], boxed: &(dyn Fn(usize) -> &'a Boxed + Sync)) -> [Vec<f32>; 3] {
        let mut maps: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0f32; g.w * g.h]);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let rows = g.h.div_ceil(threads).max(1);
        let [r, gr, b] = &mut maps;
        std::thread::scope(|sc| {
            for (n, ((r, gr), b)) in r.chunks_mut(rows * g.w).zip(gr.chunks_mut(rows * g.w)).zip(b.chunks_mut(rows * g.w)).enumerate() {
                sc.spawn(move || {
                    let y0 = n * rows;
                    for (k, c) in colours.iter().enumerate() {
                        let bx = boxed(k);
                        if bx.y >= y0 + r.len() / g.w || bx.y + bx.h <= y0 {
                            continue;
                        }
                        for (map, c) in [(&mut *r, c[0]), (&mut *gr, c[1]), (&mut *b, c[2])] {
                            if c > 0.0 {
                                bx.add_into_rows(map, g.w, y0, c);
                            }
                        }
                    }
                });
            }
        });
        maps
    }
}

/// The simulation, run where it can: started on a keyboard's shape and its
/// make, it works in the background, with a first picture as soon as it has
/// the light straight from the LEDs and a first few photons' bounced light
/// (a tenth of a second or so), and clearer ones as photons come in,
/// until it has SETTLED photons from each LED; asked for another make (or
/// the LEDs moved), it starts again. Its brightness is the board's to apply,
/// so a change of that alone asks nothing of it.
pub struct Simulator {
    shared: Arc<Shared>,
    /// what it was last asked to simulate; none once told to rest
    asked: Mutex<Option<Setup>>,
}

struct Shared {
    ask: Mutex<Option<Setup>>,
    /// stop the run under way, and start none
    rest: AtomicBool,
    wake: Condvar,
    latest: Mutex<Option<Arc<Snapshot>>>,
    published: Condvar,
    version: AtomicU64,
    stop: AtomicBool,
    /// told each time there is a new picture
    notify: Box<dyn Fn() + Send + Sync>,
}

impl Simulator {
    /// Start simulating: `notify` is told, from the simulation's thread, each
    /// time there is a new picture.
    pub fn new(scene: Scene, setup: Setup, notify: Box<dyn Fn() + Send + Sync>) -> Simulator {
        let shared = Arc::new(Shared { ask: Mutex::new(Some(setup.clone())), rest: AtomicBool::new(false), wake: Condvar::new(), latest: Mutex::new(None), published: Condvar::new(), version: AtomicU64::new(0), stop: AtomicBool::new(false), notify });
        let s = shared.clone();
        std::thread::Builder::new().name("light simulation".into()).spawn(move || work(scene, s)).expect("the simulation's thread");
        Simulator { shared, asked: Mutex::new(Some(setup)) }
    }

    /// Simulate another make, or the LEDs moved, from the start; but not for
    /// a change of brightness alone.
    pub fn set(&self, setup: &Setup) {
        let mut asked = self.asked.lock().unwrap();
        if asked.as_ref().is_some_and(|a| a.same_paths(setup)) {
            return;
        }
        *asked = Some(setup.clone());
        self.shared.rest.store(false, Ordering::SeqCst);
        *self.shared.ask.lock().unwrap() = Some(setup.clone());
        self.shared.wake.notify_all();
    }

    /// Stop simulating, and start nothing new: the picture wanted is kept already.
    pub fn rest(&self) {
        *self.asked.lock().unwrap() = None;
        *self.shared.ask.lock().unwrap() = None;
        self.shared.rest.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }

    /// The picture so far, and a count that changes each time there is a new one.
    pub fn latest(&self) -> Option<Arc<Snapshot>> {
        self.shared.latest.lock().unwrap().clone()
    }

    pub fn version(&self) -> u64 {
        self.shared.version.load(Ordering::SeqCst)
    }

    /// Wait for a first picture, at most `most`: it comes in a tenth of a second or so.
    pub fn wait(&self, most: std::time::Duration) -> Option<Arc<Snapshot>> {
        let latest = self.shared.latest.lock().unwrap();
        let (latest, _) = self.shared.published.wait_timeout_while(latest, most, |l| l.is_none()).unwrap();
        latest.clone()
    }

    /// Simulate to the end, here and now (for a snapshot of the keyboard), `photons` from each LED.
    pub fn settle(scene: &Scene, setup: &Setup, photons: usize) -> Snapshot {
        let mut run = Run::new(scene, setup);
        while run.photons < photons {
            run.trace(scene, (photons - run.photons).min(STEP * 16));
        }
        run.snapshot(true)
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }
}

fn work(scene: Scene, shared: Arc<Shared>) {
    loop {
        let setup = {
            let mut ask = shared.ask.lock().unwrap();
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(a) = ask.take() {
                    break a;
                }
                ask = shared.wake.wait(ask).unwrap();
            }
        };
        let mut run = Run::new(&scene, &setup);
        let publish = |run: &Run, settled: bool| {
            *shared.latest.lock().unwrap() = Some(Arc::new(run.snapshot(settled)));
            shared.version.fetch_add(1, Ordering::SeqCst);
            shared.published.notify_all();
            (shared.notify)();
        };
        // the bounced light, a step at a time, a picture each time the photons
        // double: the first with the first step's, so the bounced light never
        // goes missing from a picture while the light straight from the LEDs
        // shows. A newer ask waits for that first picture, so a slider dragged
        // shows on the keyboard as it goes rather than only once let go.
        let mut next = STEP;
        while run.photons < SETTLED {
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            if shared.rest.swap(false, Ordering::SeqCst) || (next > STEP && shared.ask.lock().unwrap().is_some()) {
                break;
            }
            run.trace(&scene, STEP);
            if run.photons >= next || run.photons >= SETTLED {
                publish(&run, run.photons >= SETTLED);
                next *= 2;
            }
        }
    }
}

/// One run of the simulation: the light straight from every LED, and the
/// bounced light gathered so far.
struct Run {
    ss: Stoods,
    setup: Setup,
    leds: Vec<(f32, f32)>,
    fine: Grid,
    coarse: Grid,
    direct: Arc<Vec<Boxed>>,
    direct_all: Arc<Vec<f32>>,
    surface: Arc<Vec<u8>>,
    cover: Arc<[Vec<f32>; MAPS]>,
    /// what each coarse cell of each map is of, which its light is evened
    /// out among: the deck by its well, the chassis all one, a skirt by its
    /// cap and side, a cap's top by its cap; u32::MAX none
    class: Arc<[Vec<u32>; MAPS]>,
    keys: Vec<u32>,
    /// each key's bounced light, its maps, added up as the photons land
    gathered: Vec<Gathered>,
    photons: usize,
    seed: u64,
}

/// A key's bounced light as the photons land: its maps, and how many photons each cell has had.
struct Gathered {
    maps: [Boxed; MAPS],
    counts: [Vec<u32>; MAPS],
}

impl Run {
    fn new(scene: &Scene, setup: &Setup) -> Run {
        let setup = setup.simulated();
        let ss = Stoods::of(scene, &setup);
        let fine = scene.fine;
        let coarse = Grid { x0: fine.x0, y0: fine.y0, cell: COARSE, w: ((fine.w as f32 * FINE) / COARSE).ceil() as usize, h: ((fine.h as f32 * FINE) / COARSE).ceil() as usize };
        let leds: Vec<(f32, f32)> = (0..scene.keys.len()).map(|k| scene.led_at(k, &ss.cap(k as u32).p)).collect();
        // the light straight from each LED, cell by cell
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let per = leds.len().div_ceil(threads).max(1);
        let mut direct: Vec<Boxed> = leds.iter().map(|&l| Boxed::round(&fine, l, DIRECT_REACH)).collect();
        std::thread::scope(|sc| {
            for (n, chunk) in direct.chunks_mut(per).enumerate() {
                let (leds, ss) = (&leds, &ss);
                sc.spawn(move || {
                    for (i, b) in chunk.iter_mut().enumerate() {
                        let k = n * per + i;
                        straight(scene, ss, k, leds[k], b);
                    }
                });
            }
        });
        let mut all = vec![0f32; fine.w * fine.h];
        for b in &direct {
            b.add_into(&mut all, fine.w, 1.0);
        }
        let gathered = leds
            .iter()
            .map(|&l| {
                let maps: [Boxed; MAPS] = std::array::from_fn(|_| Boxed::round(&coarse, l, BOUNCE_REACH));
                let counts = std::array::from_fn(|m| vec![0u32; maps[m].v.len()]);
                Gathered { maps, counts }
            })
            .collect();
        // how much of each coarse cell each map's surfaces cover, which the
        // light gathered in it is spread over, and what the cell is of
        let mut cover: [Vec<f32>; MAPS] = std::array::from_fn(|_| vec![0f32; coarse.w * coarse.h]);
        let mut class: [Vec<u32>; MAPS] = std::array::from_fn(|_| vec![u32::MAX; coarse.w * coarse.h]);
        let share = (FINE / COARSE).powi(2);
        let per = (COARSE / FINE).round() as usize;
        for cj in 0..coarse.h {
            for ci in 0..coarse.w {
                let mut of: Vec<(usize, u32)> = Vec::with_capacity(per * per);
                for fj in cj * per..((cj + 1) * per).min(fine.h) {
                    for fi in ci * per..((ci + 1) * per).min(fine.w) {
                        let sf = scene.surface[fj * fine.w + fi];
                        let (x, y) = fine.middle(fi, fj);
                        let id = match sf {
                            Surface::Deck => scene.well_of(x, y).map_or(0, |w| w as u32),
                            Surface::Chassis => 0,
                            Surface::Skirt(c, side, _) => c * 4 + side as u32,
                            Surface::Top(c) => c,
                            Surface::None => continue,
                        };
                        let m = sf.map().unwrap();
                        cover[m][cj * coarse.w + ci] += share;
                        of.push((m, id));
                    }
                }
                for (m, class) in class.iter_mut().enumerate() {
                    let ids: Vec<u32> = of.iter().filter(|o| o.0 == m).map(|o| o.1).collect();
                    if let Some(&most) = ids.iter().max_by_key(|a| ids.iter().filter(|b| b == a).count()) {
                        class[cj * coarse.w + ci] = most;
                    }
                }
            }
        }
        // (the tops' outsides are where the tops are)
        (cover[TOP_OUTSIDE], class[TOP_OUTSIDE]) = (cover[3].clone(), class[3].clone());
        Run {
            ss,
            setup,
            leds,
            fine,
            coarse,
            direct: Arc::new(direct),
            direct_all: Arc::new(all),
            surface: Arc::new(scene.surface.iter().map(|s| s.code()).collect()),
            cover: Arc::new(cover),
            class: Arc::new(class),
            keys: scene.keys.iter().map(|k| k.0).collect(),
            gathered,
            photons: 0,
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Send `n` more photons from each LED, and gather where their bounced light lands.
    fn trace(&mut self, scene: &Scene, n: usize) {
        let (coarse, seed) = (self.coarse, self.seed);
        let (ss, leds) = (&self.ss, &self.leds);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let per = self.gathered.len().div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for (c, chunk) in self.gathered.chunks_mut(per).enumerate() {
                sc.spawn(move || {
                    for (i, g) in chunk.iter_mut().enumerate() {
                        let k = c * per + i;
                        let mut rng = Rng(seed ^ ((k as u64 + 1).wrapping_mul(0x2545_F491_4F6C_DD1D)).rotate_left(17) | 1);
                        bounce(scene, ss, k, leds[k], g, &coarse, n, &mut rng, false);
                    }
                });
            }
        });
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.photons += n;
    }

    fn snapshot(&self, settled: bool) -> Snapshot {
        let share = if self.photons > 0 { 1.0 / self.photons as f32 } else { 0.0 };
        // each key's bounced light, each place's evened out over enough photons round it, a thread a share of the keys
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let per = self.gathered.len().div_ceil(threads).max(1);
        let mut bounced: Vec<[Boxed; MAPS]> = Vec::with_capacity(self.gathered.len());
        std::thread::scope(|sc| {
            let jobs: Vec<_> = self
                .gathered
                .chunks(per)
                .map(|chunk| sc.spawn(move || chunk.iter().map(|g| std::array::from_fn(|m| even(&g.maps[m], &g.counts[m], &self.class[m], &self.cover[m], self.coarse.w, share))).collect::<Vec<[Boxed; MAPS]>>()))
                .collect();
            for j in jobs {
                bounced.extend(j.join().unwrap_or_else(|e| std::panic::resume_unwind(e)));
            }
        });
        let n = self.coarse.w * self.coarse.h;
        let mut all: [Vec<f32>; MAPS] = std::array::from_fn(|_| vec![0f32; n]);
        for maps in &bounced {
            for (m, a) in all.iter_mut().enumerate() {
                maps[m].add_into(a, self.coarse.w, 1.0);
            }
        }
        Snapshot {
            setup: self.setup.clone(),
            photons: self.photons,
            settled,
            fine: self.fine,
            coarse: self.coarse,
            surface: self.surface.clone(),
            cover: self.cover.clone(),
            keys: self.keys.clone(),
            direct: self.direct.clone(),
            bounced,
            direct_all: self.direct_all.clone(),
            bounced_all: all,
        }
    }
}

/// A key's bounced light on one map, each place's the light of the
/// photons round it over the area they landed on, as far round as holds
/// EVEN of them (at most EVEN_REACH cells), among the cells of the same
/// surface only (the same well's deck, the same cap's skirt's side): where
/// photons are many it keeps its detail, and where they are few it is
/// smooth rather than speckled. Each photon `share` of its LED's light.
fn even(b: &Boxed, counts: &[u32], class: &[u32], cover: &[f32], cw: usize, share: f32) -> Boxed {
    let mut out = Boxed { x: b.x, y: b.y, w: b.w, h: b.h, v: vec![0.0; b.v.len()] };
    if b.w == 0 || b.h == 0 {
        return out;
    }
    let at = |i: usize, j: usize| (b.y + j) * cw + b.x + i;
    // the surfaces in the box, and where each is
    let mut boxes: Vec<(u32, [usize; 4])> = Vec::new();
    for j in 0..b.h {
        for i in 0..b.w {
            let g = at(i, j);
            if class[g] == u32::MAX || cover[g] <= 0.0 {
                continue;
            }
            match boxes.iter_mut().find(|c| c.0 == class[g]) {
                Some(c) => c.1 = [c.1[0].min(i), c.1[1].min(j), c.1[2].max(i + 1), c.1[3].max(j + 1)],
                None => boxes.push((class[g], [i, j, i + 1, j + 1])),
            }
        }
    }
    for (id, [x0, y0, x1, y1]) in boxes {
        let (w, h) = (x1 - x0, y1 - y0);
        // running sums over the surface's box: light, photons, area
        let mut sums = vec![[0f64; 3]; (w + 1) * (h + 1)];
        for j in 0..h {
            for i in 0..w {
                let (bi, bj) = (x0 + i, y0 + j);
                let g = at(bi, bj);
                let cell = if class[g] == id { [b.v[bj * b.w + bi] as f64, counts[bj * b.w + bi] as f64, cover[g] as f64] } else { [0.0; 3] };
                let s = |i: usize, j: usize| sums[j * (w + 1) + i];
                let (up, left, both) = (s(i + 1, j), s(i, j + 1), s(i, j));
                sums[(j + 1) * (w + 1) + i + 1] = [0, 1, 2].map(|c| cell[c] + up[c] + left[c] - both[c]);
            }
        }
        let within = |i: usize, j: usize, r: usize| {
            let (a0, b0) = (i.saturating_sub(r), j.saturating_sub(r));
            let (a1, b1) = ((i + r + 1).min(w), (j + r + 1).min(h));
            let s = |i: usize, j: usize| sums[j * (w + 1) + i];
            let (p, q, u, v) = (s(a1, b1), s(a0, b1), s(a1, b0), s(a0, b0));
            [0, 1, 2].map(|c| p[c] - q[c] - u[c] + v[c])
        };
        for j in 0..h {
            for i in 0..w {
                let (bi, bj) = (x0 + i, y0 + j);
                let g = at(bi, bj);
                if class[g] != id || cover[g] <= 0.0 {
                    continue;
                }
                // the least reach that holds enough photons
                let (mut lo, mut hi) = (0, EVEN_REACH);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if within(i, j, mid)[1] >= EVEN as f64 { hi = mid } else { lo = mid + 1 }
                }
                let [light, _, area] = within(i, j, lo);
                out.v[bj * b.w + bi] = if area > 0.0 { (light / area) as f32 * share } else { 0.0 };
            }
        }
    }
    out
}

/// The light straight from key `k`'s LED at each cell of its box: on the
/// back of its own cap's top (its beam, and its housing's glow), through its
/// own cap's skirt where the plastic lets it straight through (its beam, on
/// the skirt's inside), on the deck of its own well and the chassis round
/// the wells (its housing's glow, out under the skirt), and, from a switch
/// whose top is below the skirt's foot, on the outsides of other caps'
/// skirts (its beam, where nothing stands in the way); each as the surface
/// there gives it back or lets it through.
fn straight(scene: &Scene, ss: &Stoods, k: usize, (lx, ly): (f32, f32), b: &mut Boxed) {
    let s = ss.cap(k as u32);
    let (p, body) = (&s.p, &ss.all.p);
    let pi = std::f32::consts::PI;
    let high = [lx, ly, s.beam_from];
    let housing = Housing::of(s, scene.switch_at(k, p), (lx, ly));
    let (foot, round) = (scene.caps[k].foot, scene.caps[k].round[0]);
    let middle = scene.keys[k].1;
    let own_well = scene.well_of(middle.0, middle.1);
    let beam = |q: [f32; 3], n: [f32; 3]| -> f32 {
        let d = [q[0] - high[0], q[1] - high[1], q[2] - high[2]];
        let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if r2 <= 1e-6 {
            return 0.0;
        }
        let r = r2.sqrt();
        let cos_q = -(d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) / r;
        if cos_q <= 0.0 {
            return 0.0;
        }
        p.beam_at(d[2] / r) * cos_q / r2
    };
    for j in 0..b.h {
        for i in 0..b.w {
            let (ci, cj) = (b.x + i, b.y + j);
            let (x, y) = scene.fine.middle(ci, cj);
            let v = match scene.surface[cj * scene.fine.w + ci] {
                Surface::Deck if own_well.is_some() && scene.well_of(x, y) == own_well => body.deck / pi * housing.on_floor([x, y, 0.0], foot, round, s.gap),
                Surface::Chassis => body.chassis / pi * housing.on_floor([x, y, ss.all.wall], foot, round, s.gap),
                Surface::Top(c) if c as usize == k => {
                    let q = [x, y, s.top];
                    p.legends / pi * (beam(q, [0.0, 0.0, -1.0]) + housing.on_top(q))
                }
                // (what milky plastic scatters it carries round the cap: those photons are followed instead)
                Surface::Skirt(c, side, up) if c as usize == k && p.skirts > 0.0 && p.milky < 1.0 => {
                    let (q, n) = scene.skirt_point(ss, c, side, up, (x, y));
                    let through = shared(p.cap_in, p.skirts).1 * (1.0 - p.milky);
                    if q[2] <= s.clear_to { through / pi * beam(q, [-n[0], -n[1], -n[2]]) } else { 0.0 }
                }
                Surface::Skirt(c, side, up) if c as usize != k && s.beam_from < s.gap => {
                    let (q, n) = scene.skirt_point(ss, c, side, up, (x, y));
                    let off = [q[0] + n[0] * 0.01, q[1] + n[1] * 0.01, q[2]];
                    if scene.clear(ss, off, high) { ss.cap(c).p.cap_out / pi * beam(q, n) } else { 0.0 }
                }
                _ => 0.0,
            };
            b.v[j * b.w + i] = v;
        }
    }
}

/// `n` photons from key `k`'s LED at (`lx`, `ly`) and its housing, each
/// followed, its light gathered where it lands on what shows from above:
/// the deck and chassis (the first map), the skirts' outsides (the second,
/// at the place on the band the height shows at), the backs of the caps'
/// tops (the third); each as that surface gives it back, or, landing on a
/// skirt's inside, as much as it lets through to its outside, for the area
/// of a cell. Where a photon first lands is gathered only where straight()
/// has not worked out its light exactly already. A photon goes on off what
/// it lands on, or through it, each by its share.
/// (`first`, for checking the two against each other: gather those too,
/// and follow each photon no further.)
#[allow(clippy::too_many_arguments)]
fn bounce(scene: &Scene, ss: &Stoods, k: usize, (lx, ly): (f32, f32), into: &mut Gathered, g: &Grid, n: usize, rng: &mut Rng, first: bool) {
    let s = ss.cap(k as u32);
    let (p, body) = (&s.p, &ss.all.p);
    let pi = std::f32::consts::PI;
    let per_area = 1.0 / (g.cell * g.cell);
    let middle = scene.keys[k].1;
    let own_well = scene.well_of(middle.0, middle.1);
    let housing = Housing::of(s, scene.switch_at(k, p), (lx, ly));
    let mut gather = |m: usize, (x, y): (f32, f32), v: f32| {
        let b = &mut into.maps[m];
        if let Some((i, j)) = g.at(x, y) {
            if i >= b.x && j >= b.y && i < b.x + b.w && j < b.y + b.h {
                let n = (j - b.y) * b.w + (i - b.x);
                b.v[n] += v;
                into.counts[m][n] += 1;
            }
        }
    };
    let visible = |q: [f32; 3], what: Surface| scene.fine.at(q[0], q[1]).is_some_and(|(i, j)| scene.surface[j * scene.fine.w + i] == what);
    for _ in 0..n {
        // from the housing, a place on it by how much light it gives off there, or in the beam
        let glow = rng.f() < p.scatter;
        let (mut o, mut d) = if glow {
            let (q, nrm) = housing.emit(rng);
            ([q[0] + nrm[0] * EPS * 4.0, q[1] + nrm[1] * EPS * 4.0, q[2] + nrm[2] * EPS * 4.0], rng.diffuse(nrm))
        } else {
            ([lx, ly, s.beam_from], rng.beam(p.beam))
        };
        for bounce in 0..48 {
            let Some((t, what, nrm)) = scene.hit(ss, o, d, 1e4) else { break };
            let q = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
            // the cap struck, by its own physics; the body, by all keys'
            let (sc, pc) = match what {
                Struck::Outside(c, _) | Struck::Inside(c, _) | Struck::Under(c) | Struck::Top(c) => (ss.cap(c), &ss.cap(c).p),
                _ => (&ss.all, &ss.all.p),
            };
            // worked out exactly already: the beam's first landing; the
            // housing's on the back of its own cap's top, and on the deck of
            // its own well and the chassis near enough for straight()
            let near = (q[0] - lx).abs() < DIRECT_REACH - 1.0 && (q[1] - ly).abs() < DIRECT_REACH - 1.0;
            let exact = !first
                && bounce == 0
                && match what {
                    _ if !glow => true,
                    Struck::Under(c) => c as usize == k,
                    Struck::Deck => near && scene.well_of(q[0], q[1]) == own_well,
                    Struck::Chassis => near,
                    _ => false,
                };
            // where light the cap's plastic takes in from inside the cap would
            // come out of it: milky plastic scatters a photon as often as it
            // is milky, and carries it round the cap; the rest it lets
            // straight through (from under the top, into the black). (Drawn
            // however milky it is, so a little more milky changes only the
            // photons it scatters, not where every other one goes.)
            let scattered = pc.skirts > 0.0 && matches!(what, Struck::Inside(..) | Struck::Under(_)) && rng.f() < pc.milky;
            let out = match what {
                Struck::Inside(c, side) if pc.skirts > 0.0 => scene.exit(ss, c, Some(side), q, scattered, rng),
                Struck::Under(c) if pc.skirts > 0.0 && scattered => scene.exit(ss, c, None, q, true, rng),
                _ => None,
            };
            // what it gives back and lets through, and where it shows
            let (rho, through) = match what {
                Struck::Deck => {
                    if !exact && visible(q, Surface::Deck) {
                        gather(0, (q[0], q[1]), body.deck / pi * per_area);
                    }
                    (body.deck, 0.0)
                }
                Struck::Chassis => {
                    if !exact && visible(q, Surface::Chassis) {
                        gather(1, (q[0], q[1]), body.chassis / pi * per_area);
                    }
                    (body.chassis, 0.0)
                }
                Struck::Outside(c, side) => {
                    // (the black comes down the sides on the outside: nothing gets in under it)
                    let (back, through) = shared(pc.cap_out, if q[2] <= sc.clear_to { pc.skirts } else { 0.0 });
                    if !exact {
                        let (at, stands) = scene.band(ss, c, side, q);
                        gather(2, at, back / pi * per_area * stands);
                    }
                    (back, through)
                }
                Struck::Inside(c, _) => {
                    // the light it lets through glows on the skirt's outside, where it
                    // comes out (straight through, worked out exactly for the beam's
                    // first landing)
                    let (back, through) = shared(pc.cap_in, pc.skirts);
                    if let (false, true, Some((e, side))) = (exact && !scattered, through > 0.0, out) {
                        let (at, stands) = scene.band(ss, c, side, e);
                        gather(2, at, through / pi * per_area * stands);
                    }
                    (back, through)
                }
                Struck::Under(c) => {
                    if !exact {
                        gather(3, (q[0], q[1]), pc.legends / pi * per_area);
                    }
                    // milky plastic under the black top takes its share in, and carries
                    // what it scatters to the skirt
                    let carried = if out.is_some() { pc.skirts } else { 0.0 };
                    let all = pc.cap_in + pc.tops + carried;
                    let k = if all > 1.0 { 1.0 / all } else { 1.0 };
                    if let (false, true, Some((e, side))) = (exact, carried > 0.0, out) {
                        let (at, stands) = scene.band(ss, c, side, e);
                        gather(2, at, carried * k / pi * per_area * stands);
                    }
                    (pc.cap_in * k, (pc.tops + carried) * k)
                }
                Struck::Top(_) => {
                    // the light coming down on a top, as its outside gives it back
                    let (back, through) = shared(pc.cap_out, pc.tops);
                    if !exact {
                        gather(TOP_OUTSIDE, (q[0], q[1]), back / pi * per_area);
                    }
                    (back, through)
                }
                Struck::Wall => (body.chassis, 0.0),
                Struck::Housing => (HOUSING, 0.0),
            };
            if first {
                break;
            }
            let r = rng.f();
            if r < through {
                // through it: out of its other side; or out of the skirt where the
                // cap's plastic lets it out (under the black, it is lost)
                let carried = match what {
                    Struck::Inside(..) => true,
                    Struck::Under(_) => r >= through * pc.tops / (pc.tops + if out.is_some() { pc.skirts } else { 0.0 }).max(1e-9),
                    _ => false,
                };
                let (q, n) = if carried {
                    let Some((e, side)) = out else { break };
                    let n = match side {
                        0 => [1.0, 0.0, 0.0],
                        1 => [-1.0, 0.0, 0.0],
                        2 => [0.0, 1.0, 0.0],
                        _ => [0.0, -1.0, 0.0],
                    };
                    (e, n)
                } else {
                    (q, nrm)
                };
                o = [q[0] - n[0] * EPS * 4.0, q[1] - n[1] * EPS * 4.0, q[2] - n[2] * EPS * 4.0];
                d = rng.diffuse([-n[0], -n[1], -n[2]]);
            } else if r < through + rho {
                o = [q[0] + nrm[0] * EPS * 4.0, q[1] + nrm[1] * EPS * 4.0, q[2] + nrm[2] * EPS * 4.0];
                d = rng.diffuse(nrm);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resvg::usvg::{self, Tree};

    fn scene() -> (Board, Scene, Physics) {
        let tree = Tree::from_str(&crate::fixture::svg(), &usvg::Options::default()).unwrap();
        let mut device: Value = crate::jsonc::parse(&crate::fixture::device()).unwrap();
        device["light"] = serde_json::json!({ "scale": 1.38, "switches": { "led": 4.5 / 1.38 }, "wells": { "art": "mask_1" } });
        let b = Board::new(&tree, &device, [0x20, 0x20, 0x20], 1264, 506).unwrap();
        let scene = Scene::of(&b);
        (b, scene, Physics::from(&device["light"]))
    }

    /// A black housing scatters none of its LED's light: the beam holds it
    /// all, whatever the core's scatter says; and it stands in the way of
    /// light falling back on it, where a clear one lets it by.
    #[test]
    fn a_black_housing_sends_all_its_light_up_and_stops_what_falls_on_it() {
        let (_b, scene, p) = scene();
        let slot = Physics { switch: Switch::Slot, scatter: 0.7, ..p };
        let s = Stood::of(&slot);
        let ss = Stoods::of(&scene, &Setup::alike(slot));
        let n = 200_000;
        let total = (0..n).map(|i| s.p.beam_at((i as f32 + 0.5) / n as f32) as f64).sum::<f64>() / n as f64 * std::f64::consts::TAU;
        assert!((total - 1.0).abs() < 1e-3, "the beam holds {total:.4} of the LED's light");
        // straight down onto the top of the first lit key's switch, from under its cap
        let (cx, cy) = scene.keys[0].1;
        let o = [cx + 0.3, cy - 0.2, s.beam_from + 1.0];
        let hit = scene.hit(&ss, o, [0.0, 0.0, -1.0], 1e4).map(|h| (h.0, h.1));
        assert!(matches!(hit, Some((t, Struck::Housing)) if (t - 1.0).abs() < 1e-3), "a black housing is struck: {hit:?}");
        let clear = Stoods::of(&scene, &Setup::alike(Physics { switch: Switch::ClearTop, ..p }));
        let hit = scene.hit(&clear, o, [0.0, 0.0, -1.0], 1e4).map(|h| h.1);
        assert!(!matches!(hit, Some(Struck::Housing)), "a clear housing lets the light by: {hit:?}");
    }

    /// A change waits for the run under way to show its first picture, so a
    /// slider dragged in the lab shows on the keyboard as it goes; then the
    /// change is simulated.
    #[test]
    fn a_change_waits_for_the_first_picture() {
        let (_b, scene, p) = scene();
        // each picture held as it is published, until it has been looked at
        let (published, pictures) = std::sync::mpsc::channel::<()>();
        let (looked, looked_at) = std::sync::mpsc::channel::<()>();
        let looked_at = Mutex::new(looked_at);
        let s = Simulator::new(scene, Setup::alike(p), Box::new(move || {
            let _ = published.send(());
            let _ = looked_at.lock().unwrap().recv();
        }));
        let next = || {
            pictures.recv_timeout(std::time::Duration::from_secs(20)).expect("a picture");
            let l = s.latest().expect("the picture published");
            looked.send(()).unwrap();
            l.setup.all
        };
        // the first run under way, working out its light straight from the LEDs
        while s.shared.ask.lock().unwrap().is_some() {
            std::thread::yield_now();
        }
        let q = Physics { beam: p.beam * 2.0, ..p };
        s.set(&Setup::alike(q));
        assert!(next().same_paths(&p.simulated()), "the first run showed its picture before the change");
        while !next().same_paths(&q.simulated()) {}
    }

    /// The beam holds the LED's light but the share the switch scatters, however tight it is.
    #[test]
    fn the_beam_holds_its_share() {
        for (beam, scatter) in [(0.0, 0.4), (25.0, 0.4), (120.0, 0.1)] {
            let p = Physics { beam, scatter, ..Physics::from(&Value::Null) };
            let n = 200_000;
            let total = (0..n).map(|i| p.beam_at((i as f32 + 0.5) / n as f32) as f64).sum::<f64>() / n as f64 * std::f64::consts::TAU;
            eprintln!("beam {beam}, scattered {scatter}: the beam holds {total:.4}");
            assert!((total - (1.0 - scatter) as f64).abs() < 1e-3);
        }
    }

    /// The spacebar's walls step in where it runs over the chassis, as the
    /// art draws its foot: beside the step it is the chassis, not its wall;
    /// above the step, and inside it, its wall.
    #[test]
    fn the_spacebar_s_walls_step_in_over_the_chassis() {
        let (b, scene, _) = scene();
        let space = b.cap_shapes.iter().position(|c| c.label == "Space").unwrap() as u32;
        let f = scene.caps[space as usize].foot;
        let at = |x: f32, y: f32| {
            let (i, j) = scene.fine.at(x, y).unwrap();
            scene.surface[j * scene.fine.w + i]
        };
        assert!(scene.odd[space as usize].is_some(), "its foot is its own shape");
        assert_eq!(at(f[0] + 0.4, f[3] - 3.0), Surface::Chassis, "beside the step, over the chassis");
        assert!(matches!(at(f[0] + 0.4, f[1] + 10.0), Surface::Skirt(c, ..) if c == space), "its wall above the step");
        assert!(matches!(at(f[0] + 1.3, f[3] - 3.0), Surface::Skirt(c, ..) if c == space), "its shorter wall below it");
        assert!((0..scene.caps.len()).filter(|&c| c != space as usize).all(|c| scene.odd[c].is_none()), "every other cap a rounded box");
    }

    /// The wells are as round at their corners as the art draws them: a
    /// place in a corner's square but outside its round is the chassis, not
    /// the deck, and a ray across the deck meets the corner's round wall.
    #[test]
    fn the_wells_corners_are_round() {
        let (_b, scene, _) = scene();
        let &(w, r) = scene.wells.iter().find(|(w, _)| (w[0] - 52.262).abs() < 0.1 && (w[3] - 225.739).abs() < 0.1).expect("the main well");
        assert!((r - 3.518).abs() < 0.01, "its corners' radius: {r}");
        let (x, y) = (w[0] + 0.5, w[3] - 0.5);
        assert!(!scene.in_well(x, y) && scene.in_well(w[0] + r, w[3] - r) && scene.in_well(w[0] + 0.5, w[3] - r - 1.0), "in the corner's square, out of its round");
        let (fx, fy) = scene.fine.at(x, y).unwrap();
        assert_eq!(scene.surface[fy * scene.fine.w + fx], Surface::Chassis);
        // a ray along the deck toward the corner, a millimetre up, meets its round wall
        let ss = Stoods::of(&scene, &Setup::alike(Physics::from(&serde_json::json!({ "scale": 1.38 }))));
        let (cx, cy) = (w[0] + r, w[3] - r);
        let from = [cx + 5.0, cy - 5.0, 1.0];
        let d = [-std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 0.0];
        let hit = scene.hit(&ss, from, d, 1e4).map(|h| (h.0, h.1));
        let want = 5.0 * std::f32::consts::SQRT_2 + r;
        assert!(matches!(hit, Some((t, Struck::Wall)) if (t - want).abs() < 0.05), "the corner's round: {hit:?}, {want}");
    }

    /// An LED stays inside its switch: it sits as far north of centre as
    /// asked up to the switch's edge and no further, in the light on its
    /// cap's top too.
    #[test]
    fn the_led_stays_inside_its_switch() {
        let p = Physics { switch_width: 12.0, ..Physics::from(&Value::Null) };
        let edge = 6.0 * p.units_per_mm;
        assert_eq!(p.held_north(3.0), 3.0);
        assert!((p.held_north(20.0) - edge).abs() < 1e-5 && (p.held_north(-20.0) + edge).abs() < 1e-5);
        assert_eq!(on_cap_top(&p, 20.0, 1.5), on_cap_top(&p, edge, 1.5));
        // a switch wider than fits in a cap is held to 14 mm, and its edge with it
        let wide = Physics::from(&serde_json::json!({ "switches": { "width": 30.0 } }));
        assert!((wide.switch_edge() - 7.0 * p.units_per_mm).abs() < 1e-5);
    }

    /// The exact share of a rectangle's light, against the rectangle cut
    /// into small patches, each one's light added up.
    #[test]
    fn a_rectangle_s_share_is_exact() {
        for (x0, y0, x1, y1, h) in [(-5.0f32, -5.0, 5.0, 5.0, 3.0f32), (2.0, -1.0, 9.0, 4.0, 2.5), (-8.0, 3.0, -1.0, 6.0, 10.0)] {
            let n = 800;
            let (dx, dy) = ((x1 - x0) / n as f32, (y1 - y0) / n as f32);
            let mut sum = 0f64;
            for i in 0..n {
                for j in 0..n {
                    let (x, y) = (x0 + (i as f32 + 0.5) * dx, y0 + (j as f32 + 0.5) * dy);
                    let r2 = (x * x + y * y + h * h) as f64;
                    sum += (h as f64).powi(2) / (std::f64::consts::PI * r2 * r2) * (dx * dy) as f64;
                }
            }
            let exact = parallel(x0, y0, x1, y1, h) as f64;
            eprintln!("a {:.0} x {:.0} rectangle {h} away: {exact:.5} exactly, {sum:.5} in patches", x1 - x0, y1 - y0);
            assert!((exact - sum).abs() < 1e-4 + sum * 1e-3);
        }
    }

    /// The light worked out exactly (the beam's and the housing's glow on
    /// the back of its cap's top, the glow on the deck of its well) and the
    /// same light traced as photons, each followed to where it first lands,
    /// agree: for a key in the main block, over the coarse cells wholly
    /// of each surface.
    #[test]
    fn exact_and_traced_light_agree() {
        let (_, scene, p) = scene();
        let k = 30;
        for scatter in [1.0f32, 0.4] {
            let ss = Stoods::of(&scene, &Setup::alike(Physics { scatter, ..p }));
            let fine = scene.fine;
            let coarse = Grid { x0: fine.x0, y0: fine.y0, cell: COARSE, w: ((fine.w as f32 * FINE) / COARSE).ceil() as usize, h: ((fine.h as f32 * FINE) / COARSE).ceil() as usize };
            let led = scene.led_at(k, &ss.all.p);
            let mut exact = Boxed::round(&fine, led, DIRECT_REACH);
            straight(&scene, &ss, k, led, &mut exact);
            let n = 4_000_000;
            let maps: [Boxed; MAPS] = std::array::from_fn(|_| Boxed::round(&coarse, led, BOUNCE_REACH));
            let mut traced = Gathered { counts: std::array::from_fn(|m| vec![0; maps[m].v.len()]), maps };
            bounce(&scene, &ss, k, led, &mut traced, &coarse, n, &mut Rng(0x1234_5678_9abc_def1), true);
            let traced = traced.maps;
            let own_well = scene.well_of(scene.keys[k].1 .0, scene.keys[k].1 .1);
            let per = (COARSE / FINE) as usize;
            // the deck: all the light landing on it near the key, exact and traced
            let (mut e_flux, mut t_flux) = (0f64, 0f64);
            let t = &traced[0];
            for cj in 0..t.h {
                for ci in 0..t.w {
                    let (gi, gj) = (t.x + ci, t.y + cj);
                    let (x, y) = coarse.middle(gi, gj);
                    if (x - led.0).abs() > 30.0 || (y - led.1).abs() > 30.0 {
                        continue;
                    }
                    // the cells of its well's deck (and caps), none of the chassis past its walls
                    let (mut deck, mut e) = (false, 0f64);
                    let mut mixed = false;
                    for f in 0..per * per {
                        let (ii, jj) = (gi * per + f % per, gj * per + f / per);
                        let (fx, fy) = fine.middle(ii, jj);
                        match scene.surface[jj * fine.w + ii] {
                            Surface::Deck if scene.well_of(fx, fy) == own_well => {
                                deck = true;
                                e += (exact.v[(jj - exact.y) * exact.w + (ii - exact.x)] * FINE * FINE) as f64;
                            }
                            Surface::Deck | Surface::Chassis | Surface::None => mixed = true,
                            _ => {}
                        }
                    }
                    if deck && !mixed {
                        e_flux += e;
                        t_flux += (t.v[cj * t.w + ci] * COARSE * COARSE) as f64 / n as f64;
                    }
                }
            }
            eprintln!("scattered {scatter}: on the deck round it, {e_flux:.4e} exactly, {t_flux:.4e} traced ({:+.2}%)", (t_flux / e_flux - 1.0) * 100.0);
            assert!(e_flux > 0.0 && (t_flux / e_flux - 1.0).abs() < 0.02, "the deck: {t_flux} against {e_flux}");
            for (m, what) in [(3usize, "the back of its cap's top")] {
                let (mut e_sum, mut t_sum) = (0f64, 0f64);
                let t = &traced[m];
                for cj in 0..t.h {
                    for ci in 0..t.w {
                        let (gi, gj) = (t.x + ci, t.y + cj);
                        let mut e = 0f64;
                        let whole = (0..per * per).all(|f| {
                            let (ii, jj) = (gi * per + f % per, gj * per + f / per);
                            let (x, y) = fine.middle(ii, jj);
                            let ok = ii >= exact.x && jj >= exact.y && ii < exact.x + exact.w && jj < exact.y + exact.h && match scene.surface[jj * fine.w + ii] {
                                Surface::Top(c) => m == 3 && c as usize == k,
                                Surface::Deck => m == 0 && scene.well_of(x, y) == own_well,
                                _ => false,
                            };
                            if ok {
                                e += exact.v[(jj - exact.y) * exact.w + (ii - exact.x)] as f64;
                            }
                            ok
                        });
                        if whole {
                            e_sum += e / (per * per) as f64;
                            t_sum += t.v[cj * t.w + ci] as f64 / n as f64;
                        }
                    }
                }
                eprintln!("scattered {scatter}: on {what}, {e_sum:.4e} exactly, {t_sum:.4e} traced ({:+.2}%)", (t_sum / e_sum - 1.0) * 100.0);
                assert!(e_sum > 0.0 && (t_sum / e_sum - 1.0).abs() < 0.02, "{what}: {t_sum} against {e_sum}");
            }
        }
    }

    /// A well with no LED in it (the media keys') gets none of the LEDs'
    /// light straight, only a little bounced off the caps round it, every other key lit.
    #[test]
    fn wells_without_leds_stay_dark() {
        let (_, scene, p) = scene();
        let snap = Simulator::settle(&scene, &Setup::alike(p), 20_000);
        let leds: Vec<(f32, f32)> = (0..scene.keys.len()).map(|k| scene.led_at(k, &p)).collect();
        let lit_wells: Vec<usize> = leds.iter().filter_map(|&(x, y)| scene.well_of(x, y)).collect();
        let deck = |n: usize| {
            let (i, j) = (n % snap.fine.w, n / snap.fine.w);
            let (x, y) = snap.fine.middle(i, j);
            let a = snap.at(x, y, None)?;
            (scene.surface[n] == Surface::Deck).then(|| (scene.well_of(x, y), snap.white(&a)))
        };
        let all: Vec<(Option<usize>, f32)> = (0..snap.fine.w * snap.fine.h).filter_map(deck).collect();
        let brightest = all.iter().map(|d| d.1).fold(0f32, f32::max);
        let dark = all.iter().filter(|d| d.0.is_some_and(|w| !lit_wells.contains(&w))).map(|d| d.1).fold(0f32, f32::max);
        let unlit = (0..scene.wells.len()).filter(|w| !lit_wells.contains(w)).count();
        eprintln!("{unlit} wells with no LED: their deck at most {:.3}% of the brightest deck", dark / brightest * 100.0);
        let straight = (0..snap.fine.w * snap.fine.h)
            .filter(|&n| {
                let (x, y) = snap.fine.middle(n % snap.fine.w, n / snap.fine.w);
                scene.surface[n] == Surface::Deck && scene.well_of(x, y).is_some_and(|w| !lit_wells.contains(&w))
            })
            .map(|n| snap.direct_all[n])
            .fold(0f32, f32::max);
        assert!(unlit >= 1 && straight == 0.0 && dark < brightest * 0.06);
    }

    /// The keys' light as the core's light description has it, with one key's
    /// own values ("keys", by its control's id): `own` its values, of the
    /// key `k` in the simulation's order of the caps.
    fn own_setup(b: &Board, k: usize, own: Value) -> (Setup, Setup) {
        let ids: Vec<String> = b.cap_shapes.iter().map(|c| c.id.clone()).collect();
        let mut light = serde_json::json!({ "scale": 1.38, "switches": { "kind": "led-slot", "led": 4.5 / 1.38, "beam": 3.0 }, "wells": { "art": "mask_1" } });
        let all = Setup::from(&light, &ids);
        light["keys"] = serde_json::json!({ ids[k].clone(): own });
        (all, Setup::from(&light, &ids))
    }

    /// A key's own values are its alone: it takes every other value from all
    /// keys'; its cap of another kind lights its own skirt; and the keys far
    /// from it are lit as before.
    #[test]
    fn a_key_s_own_values_are_its_alone() {
        let (b, scene, _) = scene();
        let k = 30;
        let (all, own) = own_setup(&b, k, serde_json::json!({ "caps": { "kind": "translucent", "opacity": 0.5 } }));
        let p = *own.of(k as u32);
        assert_eq!((p.caps, p.skirts, p.tops, p.black), (Caps::Translucent, 0.5, 0.5, 0.0));
        assert_eq!(Physics { caps: Caps::ShineThrough, skirts: 0.0, tops: 0.0, black: 1.0, ..p }, all.all, "the rest is all keys'");
        assert!(own.all == all.all && (0..b.cap_shapes.len() as u32).filter(|&c| c != k as u32).all(|c| own.of(c) == &all.all), "the other keys are as all keys are");
        let (a, o) = (Simulator::settle(&scene, &all, 4000), Simulator::settle(&scene, &own, 4000));
        // its skirt, seen from above
        let skirt = |snap: &Snapshot| {
            let cells = (0..snap.fine.w * snap.fine.h).filter(|&n| matches!(scene.surface[n], Surface::Skirt(c, ..) if c as usize == k));
            cells.map(|n| snap.fine.middle(n % snap.fine.w, n / snap.fine.w)).filter_map(|(x, y)| snap.at(x, y, None)).map(|at| snap.white(&at)).sum::<f32>()
        };
        eprintln!("its skirt: {:.4} shine-through, {:.4} translucent", skirt(&a), skirt(&o));
        assert!(skirt(&o) > 3.0 * skirt(&a), "its translucent skirt glows");
        // the keys far from it: the same light straight from their LEDs, and much the same bounced
        let (kx, ky) = scene.keys[k].1;
        let far: Vec<usize> = (0..scene.keys.len()).filter(|&j| ((scene.keys[j].1 .0 - kx).powi(2) + (scene.keys[j].1 .1 - ky).powi(2)).sqrt() > 2.5 * BOUNCE_REACH).collect();
        assert!(far.len() > 20, "keys far from it: {}", far.len());
        let total = |x: &Boxed| x.v.iter().map(|v| *v as f64).sum::<f64>();
        for &j in &far {
            assert!(a.direct[j].v == o.direct[j].v, "key {j}'s light straight from its LED");
            for m in 0..MAPS {
                let (ta, to) = (total(&a.bounced[j][m]), total(&o.bounced[j][m]));
                assert!((ta - to).abs() <= 0.01 * ta.max(1e-12), "key {j}'s bounced light ({m}): {ta} against {to}");
            }
        }
    }

    /// A button with no light of its own (G/PC, the media keys) stands in
    /// the keys' light on the chassis: a ray across the chassis strikes its
    /// side, and the light coming down on its top, lower than the caps round
    /// it, is gathered there; a cap's top, as high as any, catches none.
    #[test]
    fn a_button_stands_in_the_light_and_catches_it() {
        let (b, scene, _) = scene();
        let ids: Vec<String> = b.cap_shapes.iter().map(|c| c.id.clone()).collect();
        let at = |label: &str| b.cap_shapes.iter().position(|c| c.label == label).unwrap_or_else(|| panic!("{label} stands in the light"));
        let (gpc, stop) = (at("Game mode button"), at("Media stop"));
        assert!(gpc >= scene.keys.len() && stop >= scene.keys.len(), "they have no light of their own");
        let mut light = serde_json::json!({ "scale": 1.38, "switches": { "kind": "led-slot", "led": 4.5 / 1.38, "beam": 3.0 }, "caps": { "kind": "pudding", "opacity": 0.7, "black": 0.4, "milky": 1.0 }, "wells": { "art": "mask_1" } });
        for id in [&ids[gpc], &ids[stop]] {
            light["keys"][id.as_str()] = serde_json::json!({ "caps": { "kind": "shine-through", "foot": 2.0, "top": 4.0 } });
        }
        let setup = Setup::from(&light, &ids);
        // a ray along the chassis, a millimetre up, into the media key's west side
        let ss = Stoods::of(&scene, &setup);
        let f = scene.caps[stop].foot;
        let (y, z) = ((f[1] + f[3]) / 2.0, 3.0);
        let hit = scene.hit(&ss, [f[0] - 5.0, y, z], [1.0, 0.0, 0.0], 1e4).map(|h| (h.0, h.1));
        assert!(matches!(hit, Some((t, Struck::Outside(c, 0))) if c as usize == stop && (t - 5.0).abs() < 1e-3), "it stands in the way: {hit:?}");
        let snap = Simulator::settle(&scene, &setup, 20_000);
        let top = |c: usize| {
            let f = scene.caps[c].foot;
            snap.at((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0, Some(TOP_OUTSIDE as u8)).map_or(0.0, |a| snap.white(&a))
        };
        let escape = (0..scene.keys.len()).find(|&k| b.cap_shapes[k].label == "Escape").unwrap();
        eprintln!("light coming down on G/PC's top {:.3e}, Escape's {:.3e}", top(gpc), top(escape));
        assert!(top(gpc) > 0.0, "G/PC's top catches the light coming down on it");
        assert_eq!(top(escape), 0.0, "a cap as high as any catches none on its top");
    }

    /// A key's switch moved under its cap takes its LED with it: the LED is
    /// as far east and north of where it was, and its light on the back of
    /// the cap's top goes the same way; every other key's stays where it was.
    #[test]
    fn a_key_s_switch_moved_takes_its_led_with_it() {
        let (b, scene, _) = scene();
        let k = 30;
        let (east, north) = (-2.0f32, 3.0f32);
        let (all, own) = own_setup(&b, k, serde_json::json!({ "place": { "east": east, "north": north } }));
        let (a, o) = (Run::new(&scene, &all), Run::new(&scene, &own));
        let mm = all.all.units_per_mm;
        let moved = (o.leds[k].0 - a.leds[k].0, a.leds[k].1 - o.leds[k].1);
        assert!((moved.0 - east * mm).abs() < 1e-4 && (moved.1 - north * mm).abs() < 1e-4, "its LED moved {moved:?}");
        assert!((0..scene.keys.len()).filter(|&j| j != k).all(|j| a.leds[j] == o.leds[j]), "the other keys' LEDs stay");
        // the light straight on the back of its cap's top: where it is, on the average
        let middle = |r: &Run| {
            let d = &r.direct[k];
            let (mut sum, mut sx, mut sy) = (0f64, 0f64, 0f64);
            for j in 0..d.h {
                for i in 0..d.w {
                    let (ci, cj) = (d.x + i, d.y + j);
                    if matches!(scene.surface[cj * scene.fine.w + ci], Surface::Top(c) if c as usize == k) {
                        let (x, y) = scene.fine.middle(ci, cj);
                        let v = d.v[j * d.w + i] as f64;
                        (sum, sx, sy) = (sum + v, sx + v * x as f64, sy + v * y as f64);
                    }
                }
            }
            (sx / sum, sy / sum)
        };
        let (ma, mo) = (middle(&a), middle(&o));
        let went = ((mo.0 - ma.0) as f32, (ma.1 - mo.1) as f32);
        eprintln!("its LED moved {:.2} east, {:.2} north; the light on its top {:.2} east, {:.2} north", moved.0, moved.1, went.0, went.1);
        assert!(went.0 < 0.3 * east * mm && went.1 > 0.3 * north * mm, "the light on its top goes with it: {went:?}");
    }
}


