//! The Light lab's stage: a section's drawing of the key, to scale from
//! the light's values as the simulation holds them, with each of its
//! sliders' places round it and the line that joins each slider to the part
//! it changes. Drawn as an SVG and drawn by resvg, since the window's
//! renderer fills a path with a plain colour only; its words are the lab's
//! own text, over it.
//!
//! LED, Keycaps and Wells show one slice through the number row's 2 key,
//! north to south, seen from the side (north on the left): the F-row's well,
//! the strip of chassis between the wells, the main well's wall, the 2 key in
//! its well, and the W key below it. Each frames its own part of the slice,
//! widened to fill its panel, and draws its own parts bright; the LED's
//! shows the switch with the cap's skirt cut away, and the light on the back
//! of the cap's top along it. The display's is a close-up of its dots.
//!
//! Amber is the light. Lengths and heights move the drawing; each share sets
//! how strong its arrow of light is drawn. The slider under the pointer, or
//! in use, has its line and its part drawn white.

use std::fmt::Write;

use ectodeck_draw::hdr::sim::{on_cap_top, Caps, Physics, Switch};
use serde_json::Value;

use super::{ends, get, PARAMS};
use ectodeck_draw::hdr::KeyGeometry;
use crate::text;

// the lab's own colours
const PLOT: &str = "#1b1b1b";
const TILE: &str = "#262626";
const AMBER: &str = "#f0b35a";
const WHITE: &str = "#f5f5f5";
/// a slider's line, and the dot it ends on
const LEAD: &str = "#737373";
const DOT: &str = "#d4d4d4";
/// a measurement: lighter, with ticks at its ends, over a dark casing so it reads over the light
const DIMENSION: &str = "#b4b4b4";
const CASING: &str = "#141414";

/// the plate under the deck, in millimetres
const PLATE: f64 = 1.5;

/// The key the stage draws, in section north to south, in millimetres, as
/// the device's art has it (see KeyGeometry): its foot and its face's inset
/// from it; the deck between its well's north wall and its foot; the gap to
/// the key south of it and that key's foot; north of its well, the chassis
/// to the next well and that well's deck south of its keys; and its legends,
/// each one's middle south of the foot's (north below 0) and how far it
/// runs. Where the art has nothing to say, as a 1u key in a well usually
/// stands (the Z10's 2 key's measurements).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    foot: f64,
    inset: f64,
    deck_n: f64,
    gap_s: f64,
    south_foot: f64,
    strip: f64,
    deck_beyond: f64,
    legends: [Option<(f64, f64)>; 2],
}

impl Default for Geometry {
    fn default() -> Geometry {
        Geometry { foot: 17.39, inset: 1.63, deck_n: 1.62, gap_s: 1.45, south_foot: 17.39, strip: 2.55, deck_beyond: 2.36, legends: [Some((-3.0, 4.1)), Some((4.0, 2.6))] }
    }
}

impl Geometry {
    /// A key as its art has it.
    pub fn of(k: Option<&KeyGeometry>) -> Geometry {
        let mut g = Geometry::default();
        if let Some(k) = k {
            (g.foot, g.inset) = (k.foot as f64, k.inset as f64);
            if let Some(d) = k.deck_north {
                g.deck_n = d as f64;
            }
            if let Some((gap, foot)) = k.south {
                (g.gap_s, g.south_foot) = (gap as f64, foot as f64);
            }
            if let Some((strip, deck)) = k.strip {
                (g.strip, g.deck_beyond) = (strip as f64, deck as f64);
            }
            g.legends = [0, 1].map(|i| k.legends.get(i).map(|&(m, w)| (m as f64, w as f64)));
        }
        g
    }

    /// its legends: all of them, or only the first
    fn legends(&self, all: bool) -> Vec<(f64, f64)> {
        self.legends.iter().take(if all { 2 } else { 1 }).flatten().copied().collect()
    }
}
/// the heights each drawing keeps its scale up to (a cap's top, a well's
/// depth); past them it shrinks to fit
const USUAL_TOP: f64 = 16.0;
const USUAL_BUTTON: f64 = 8.0;
const USUAL_DEPTH: f64 = 3.0;

// the stage: a column of sliders either side of the panel the drawing is in
pub const CTRL_W: f32 = 280.0;
pub const CTRL_H: f32 = 48.0;
/// how much taller a share's slider is, for the words under its track
pub const ENDS_H: f32 = 14.0;
/// how far down a slider its line leaves it, and how far apart sliders stand
pub const ATTACH: f32 = 13.0;
/// how much of its column a choice takes, its name over its options
pub const CHOICE_H: f32 = 74.0;
const PITCH: f64 = 62.0;
const COL: f64 = 20.0;
const GAP: f64 = 28.0;
const MARGIN: f64 = 24.0;

/// Which part of the key a section shows: LED, Keycaps, Legends, Wells, Display.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lens {
    Led,
    Caps,
    Legends,
    Wells,
    Display,
}

impl Lens {
    pub fn of(section: usize) -> Lens {
        [Lens::Led, Lens::Caps, Lens::Legends, Lens::Wells, Lens::Display][section.min(4)]
    }
}

/// A word on the stage, `x` where it starts and `y` its baseline, in the
/// stage's pixels, 11.5px in its own colour.
#[derive(Clone, PartialEq, Debug)]
pub struct Word {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub colour: [u8; 3],
}

/// Where a slider stands: which, on which side, its line's height on the stage.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Place {
    pub i: usize,
    pub left: bool,
    pub y: f32,
}

/// The stage drawn: its size, its drawing, its words, and what some sliders
/// show after their values (Beam's angle, Depth with no walls).
#[derive(Clone, PartialEq)]
pub struct Stage {
    pub w: f32,
    pub h: f32,
    pub svg: String,
    pub words: Vec<Word>,
    pub extras: Vec<(usize, String)>,
}

/// The x of a slider's column on the stage.
pub fn column_x(left: bool, w: f32) -> f32 {
    if left { COL as f32 } else { w - COL as f32 - CTRL_W }
}

/// Where the top of a section's `k`th choice is on the stage, at the top of its left column.
pub fn choice_y(k: usize) -> f32 {
    14.0 + k as f32 * CHOICE_H
}

/// The values the drawing is drawn from, as the simulation holds them (no
/// switch or skirt's foot above the cap's top, no switch wider than fits in
/// a cap, no LED outside its switch): lengths in millimetres, the LED's
/// place in the art's units too; and the key drawn, as the art has it.
#[derive(Clone, Copy, Debug)]
pub struct Values {
    p: Physics,
    /// a black housing, its LED through a slot in its top; what the caps are
    slot: bool,
    caps: Caps,
    led: f64,
    north: f64,
    beam: f64,
    scatter: f64,
    sides: f64,
    spread: f64,
    switch_top: f64,
    switch_width: f64,
    foot: f64,
    top: f64,
    legends: f64,
    skirts: f64,
    tops: f64,
    black: f64,
    milky: f64,
    cap_in: f64,
    cap_out: f64,
    depth: f64,
    deck: f64,
    chassis: f64,
    lcd_lit: f64,
    lcd_unlit: f64,
    lcd_field: f64,
    lcd_dot: f64,
    lcd_glass: f64,
    nits: f64,
    /// the art's units to the millimetre, and the key drawn; where its
    /// switch sits, mm east and north of its middle; whether it has an LED
    /// of its own; and whether it is a button, a moulded block standing on
    /// the chassis, not a cap over a switch in a well
    mm: f64,
    g: Geometry,
    place: (f64, f64),
    lit: bool,
    solid: bool,
}

/// A value as the light has it, or what the keyboard takes when it has none.
fn raw(light: &Value, path: (&str, &str)) -> f64 {
    get(light, path).unwrap_or_else(|| PARAMS.iter().find(|p| p.path == path).map_or(0.0, |p| p.def))
}

impl Values {
    pub fn of(light: &Value, g: Geometry) -> Values {
        let p = Physics::from(light);
        let mm = p.units_per_mm as f64;
        let edge = p.switch_edge() as f64;
        // the LED's place in the art's units, as the simulation holds it
        let led = (p.led as f64).clamp(-edge, edge);
        Values {
            p,
            slot: p.switch == Switch::Slot,
            caps: p.caps,
            led,
            north: led / mm,
            beam: p.beam as f64,
            scatter: p.scatter as f64,
            sides: p.sides as f64,
            spread: p.glow_spread as f64,
            switch_top: p.switch_top as f64,
            switch_width: p.switch_width as f64,
            foot: p.gap as f64,
            top: p.top as f64,
            legends: p.legends as f64,
            skirts: p.skirts as f64,
            tops: p.tops as f64,
            black: p.black as f64,
            milky: p.milky as f64,
            cap_in: p.cap_in as f64,
            cap_out: p.cap_out as f64,
            depth: p.wall as f64,
            deck: p.deck as f64,
            chassis: p.chassis as f64,
            lcd_lit: raw(light, ("display", "lit")),
            lcd_unlit: raw(light, ("display", "unlit")),
            lcd_field: raw(light, ("display", "background")),
            lcd_dot: raw(light, ("display", "dot")),
            lcd_glass: raw(light, ("display", "glass")),
            nits: p.lumens as f64,
            mm,
            g,
            place: (raw(light, ("place", "east")), raw(light, ("place", "north"))),
            lit: true,
            solid: false,
        }
    }

    /// For a key with no LED of its own: none of an LED's light drawn.
    pub fn unlit(mut self) -> Values {
        self.lit = false;
        self
    }

    /// For a button: a solid block on the chassis, with no switch and no LED.
    pub fn button(mut self) -> Values {
        (self.lit, self.solid) = (false, true);
        self
    }

    /// the LED's own light on the back of the cap's top, `x` mm along the
    /// slice from the key's middle (north below 0), for an LED of unit power
    fn on_top(&self, x: f64) -> f64 {
        on_cap_top(&self.p, self.led as f32, (x * self.mm) as f32 + self.led as f32) as f64
    }
}

/// The beam's half angle, out to half its brightness, in degrees.
fn beam_half(beam: f64) -> f64 {
    0.5f64.powf(1.0 / beam.max(0.01)).acos().to_degrees()
}

fn slider(name: &str) -> usize {
    PARAMS.iter().position(|p| p.label == name).expect("one of the lab's sliders")
}

/// How tall a slider is: a share's the taller, for its ends' words.
pub fn ctrl_h(i: usize) -> f32 {
    CTRL_H + if ends(PARAMS[i].path).is_some() { ENDS_H } else { 0.0 }
}

// ---------------------------------------------------------------- drawing

type Pt = (f64, f64);

struct Svg {
    defs: String,
    body: String,
    /// the measurements, drawn over the frame's edge
    marks: String,
    n: usize,
    words: Vec<Word>,
}

impl Svg {
    fn new() -> Svg {
        Svg { defs: String::new(), body: String::new(), marks: String::new(), n: 0, words: Vec::new() }
    }

    fn gid(&mut self) -> String {
        self.n += 1;
        format!("g{}", self.n)
    }

    /// a path, as faint as `opacity` (its fill's and its stroke's, so it
    /// needs no layer of its own)
    fn path(&mut self, d: &str, fill: &str, stroke: &str, sw: f64, opacity: f64) {
        if !d.is_empty() {
            let _ = write!(self.body, r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.2}" fill-opacity="{opacity:.3}" stroke-opacity="{opacity:.3}" stroke-linejoin="round" stroke-linecap="round"/>"#);
        }
    }

    fn fill(&mut self, d: &str, fill: &str) {
        self.path(d, fill, "none", 1.0, 1.0);
    }

    fn stroke(&mut self, d: &str, colour: &str, sw: f64) {
        self.path(d, "none", colour, sw, 1.0);
    }

    /// a measurement: a dark casing under it, so it reads over anything
    fn mark(&mut self, d: &str, colour: &str) {
        let _ = write!(self.marks, r#"<path d="{d}" fill="none" stroke="{CASING}" stroke-width="3.4" stroke-linecap="round"/><path d="{d}" fill="none" stroke="{colour}" stroke-width="1" stroke-linecap="round"/>"#);
    }

    fn gradient(&mut self, kind: &str, at: String, stops: &[(f64, &str, f64)]) -> String {
        let g = self.gid();
        let mut st = String::new();
        for (o, c, a) in stops {
            let _ = write!(st, r#"<stop offset="{o:.3}" stop-color="{c}" stop-opacity="{a:.3}"/>"#);
        }
        let _ = write!(self.defs, r#"<{kind}Gradient id="{g}" gradientUnits="userSpaceOnUse" {at}>{st}</{kind}Gradient>"#);
        format!("url(#{g})")
    }

    fn linear(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stops: &[(f64, &str, f64)]) -> String {
        self.gradient("linear", format!(r#"x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}""#), stops)
    }

    fn radial(&mut self, cx: f64, cy: f64, r: f64, stops: &[(f64, &str, f64)]) -> String {
        self.gradient("radial", format!(r#"cx="{cx:.1}" cy="{cy:.1}" r="{:.1}""#, r.max(0.1)), stops)
    }

    fn dot(&mut self, x: f64, y: f64, r: f64, fill: &str) {
        self.fill(&format!("M{:.2} {y:.2} a{r:.2} {r:.2} 0 1 0 {:.2} 0 a{r:.2} {r:.2} 0 1 0 {:.2} 0", x - r, 2.0 * r, -2.0 * r), fill);
    }

    /// a word, its baseline at `y`, starting at `x` (`anchor` 0), or ending at it (1)
    fn text(&mut self, x: f64, y: f64, s: &str, colour: [u8; 3], anchor: f64) {
        let x = x - text::width(400, 11.5, s) as f64 * anchor;
        self.words.push(Word { x: x as f32, y: y as f32, text: s.to_string(), colour });
    }
}

const QUIET_WORD: [u8; 3] = [0x9a, 0x9a, 0x9a];
const FAINT_WORD: [u8; 3] = [0x6f, 0x6f, 0x6f];
const WHITE_WORD: [u8; 3] = [0xf5, 0xf5, 0xf5];

/// A closed outline cut by one straight edge (Sutherland and Hodgman).
fn cut(pts: &[Pt], inside: impl Fn(Pt) -> bool, at: impl Fn(Pt, Pt) -> Pt) -> Vec<Pt> {
    let mut out = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[(i + pts.len() - 1) % pts.len()], pts[i]);
        if inside(b) {
            if !inside(a) {
                out.push(at(a, b));
            }
            out.push(b);
        } else if inside(a) {
            out.push(at(a, b));
        }
    }
    out
}

/// A closed outline cut to lo <= x <= hi.
fn clip(pts: &[Pt], lo: f64, hi: f64) -> Vec<Pt> {
    let at_x = |x: f64| move |a: Pt, b: Pt| (x, a.1 + (b.1 - a.1) * (x - a.0) / (if b.0 == a.0 { 1e-9 } else { b.0 - a.0 }));
    let pts = cut(pts, |p| p.0 >= lo, at_x(lo));
    if pts.is_empty() { pts } else { cut(&pts, |p| p.0 <= hi, at_x(hi)) }
}

/// A closed outline cut to a convex outline, its points anticlockwise.
fn clip_to(pts: &[Pt], hull: &[Pt]) -> Vec<Pt> {
    let mut pts = pts.to_vec();
    for i in 0..hull.len() {
        let ((ax, ay), (bx, by)) = (hull[i], hull[(i + 1) % hull.len()]);
        let side = move |p: Pt| (bx - ax) * (p.1 - ay) - (by - ay) * (p.0 - ax);
        let at = move |p: Pt, q: Pt| {
            let (d1, d2) = (side(p), side(q));
            let t = d1 / if d1 == d2 { 1e-9 } else { d1 - d2 };
            (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
        };
        pts = cut(&pts, |p| side(p) >= 0.0, at);
        if pts.is_empty() {
            break;
        }
    }
    pts
}

/// A path through the points; nothing for a shape the frame cut away.
fn poly(pts: &[Pt], close: bool) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let _ = write!(d, "{}{x:.1} {y:.1}", if i == 0 { "M" } else { " L" });
    }
    if close {
        d += " Z";
    }
    d
}

/// A rectangle with rounded corners, as a path.
fn rounded(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> String {
    format!(
        "M{:.1} {y0:.1} H{:.1} A{r} {r} 0 0 1 {x1:.1} {:.1} V{:.1} A{r} {r} 0 0 1 {:.1} {y1:.1} H{:.1} A{r} {r} 0 0 1 {x0:.1} {:.1} V{:.1} A{r} {r} 0 0 1 {:.1} {y0:.1} Z",
        x0 + r, x1 - r, y0 + r, y1 - r, x1 - r, x0 + r, y1 - r, y0 + r, x0 + r
    )
}

/// A path the light takes, as strong as the share of it that goes that way.
fn ray(s: &mut Svg, pts: &[Pt], share: f64, grow: f64, white: bool) {
    let sw = (0.7 + 2.0 * share) * grow.min(1.25);
    let (colour, op) = if white { (WHITE, 1.0) } else { (AMBER, 0.35 + 0.6 * share) };
    s.path(&poly(pts, false), "none", colour, sw, op);
    let ((x0, y0), (x1, y1)) = (pts[pts.len() - 2], pts[pts.len() - 1]);
    let a = (y1 - y0).atan2(x1 - x0);
    let head = (4.5 + 2.5 * share) * grow.min(1.4);
    let l = (x1 - head * (a - 0.42).cos(), y1 - head * (a - 0.42).sin());
    let r = (x1 - head * (a + 0.42).cos(), y1 - head * (a + 0.42).sin());
    s.path(&poly(&[l, (x1, y1), r], true), colour, "none", 1.0, op);
}

/// A slider's place on the drawing: its anchor, and how its line comes in.
/// For something flat (a surface, a line across) its slider stands above it
/// (`flat`), so its line meets it at an angle, or (`drop`) the line runs
/// across above everything and drops straight onto it; with an `elbow` it
/// comes in level at that height, then straight up or down to the anchor.
struct Label {
    i: usize,
    left: bool,
    at: Pt,
    flat: bool,
    drop: bool,
    elbow: Option<f64>,
}

impl Label {
    fn new(left: bool, at: Pt, name: &str) -> Label {
        Label { i: slider(name), left, at, flat: false, drop: false, elbow: None }
    }
}

/// The slice, in millimetres: x north to south from the main well's north
/// wall, z up from the deck.
struct Slice {
    v: Values,
    foot0: f64,
    foot1: f64,
    /// the cap's middle, and the switch's centre, north of it as far as it sits
    mid: f64,
    centre: f64,
    /// the W key's foot
    w0: f64,
    led: f64,
    /// the drawing's cap walls and top, thinner where the cap is low
    wall_t: f64,
    top_t: f64,
    hollow: bool,
    /// the switch's housing across
    hx0: f64,
    hx1: f64,
}

impl Slice {
    fn of(v: &Values) -> Slice {
        let (foot0, foot1) = (v.g.deck_n, v.g.deck_n + v.g.foot);
        let mid = (foot0 + foot1) / 2.0;
        let centre = mid - v.place.1;
        let span = v.top - v.foot;
        let mut sl = Slice {
            v: *v,
            foot0,
            foot1,
            mid,
            centre,
            w0: foot1 + v.g.gap_s,
            led: centre - v.north,
            wall_t: (span * 0.25 + 0.2).min(1.15),
            top_t: (span * 0.3).min(1.25),
            hollow: !v.solid && span > 1.6,
            hx0: 0.0,
            hx1: 0.0,
        };
        // the switch as wide as asked, as far as it fits in the cap at its
        // top, and its LED inside it as drawn
        let (inner, inner_s) = (sl.skirt_x(true, v.switch_top, true), sl.skirt_x(false, v.switch_top, true));
        let half = if sl.hollow { (v.switch_width / 2.0).min(centre - inner - 0.25).min(inner_s - centre - 0.25) } else { v.switch_width / 2.0 };
        (sl.hx0, sl.hx1) = (centre - half, centre + half);
        sl.led = sl.led.clamp(sl.hx0, sl.hx1);
        sl
    }

    /// where the 2 key's north or south skirt is at height z, its outer or inner face
    fn skirt_x(&self, north: bool, z: f64, inner: bool) -> f64 {
        let v = &self.v;
        let t = ((z - v.foot) / (v.top - v.foot).max(1e-3)).clamp(0.0, 1.0);
        let wall = if inner { self.wall_t } else { 0.0 };
        let inset = self.v.g.inset;
        if north { self.foot0 + inset * t + wall } else { self.foot1 - inset * t - wall }
    }

    /// a cap in section: its skirt from its foot up to its face, and its
    /// top; a cap too low to be hollow, a slab
    fn cap(&self, f0: f64, f1: f64) -> Vec<Pt> {
        let v = &self.v;
        let (a0, a1) = (f0 + v.g.inset, f1 - v.g.inset);
        let (fz, tz) = (v.foot, v.top);
        let mut outer = vec![(f0, fz), (a0, tz), (a1, tz), (f1, fz)];
        if !self.hollow {
            return outer;
        }
        let lean = v.g.inset / (tz - fz).max(1e-3);
        let under = tz - self.top_t;
        let t = self.wall_t;
        outer.extend([(f1 - t, fz), (a1 - t + lean * self.top_t, under), (a0 + t - lean * self.top_t, under), (f0 + t, fz)]);
        outer
    }

    /// the space inside the 2 key's cap, as an outline, anticlockwise on the
    /// page: up the inside of each skirt from its foot to the top, and
    /// straight down below the feet
    fn interior(&self) -> Vec<Pt> {
        let v = &self.v;
        let under = v.top - self.top_t;
        let (n_foot, s_foot) = (self.skirt_x(true, v.foot, true), self.skirt_x(false, v.foot, true));
        vec![
            (n_foot, v.foot - 50.0),
            (s_foot, v.foot - 50.0),
            (s_foot, v.foot),
            (self.skirt_x(false, under, true), under),
            (self.skirt_x(true, under, true), under),
            (n_foot, v.foot),
        ]
    }

    /// the F-row's deck, its wall, the chassis between the wells, the main
    /// well's wall and its deck, as one outline; under a button, the chassis
    fn body(&self, x_lo: f64, x_hi: f64) -> Vec<Pt> {
        let (d, strip) = (self.v.depth, self.v.g.strip);
        if self.v.solid {
            return vec![(x_lo, -PLATE), (x_lo, d), (x_hi, d), (x_hi, -PLATE)];
        }
        vec![(x_lo, -PLATE), (x_lo, 0.0), (-strip, 0.0), (-strip, d), (0.0, d), (0.0, 0.0), (x_hi, 0.0), (x_hi, -PLATE)]
    }
}

/// The part of the slice a section shows, in millimetres, widened about its
/// middle to the panel's shape (`aspect`, width over height) so the drawing fills it.
fn frame_of(v: &Values, sl: &Slice, lens: Lens, aspect: f64) -> (f64, f64, f64, f64) {
    // (a button, low on the chassis, to a scale of its own)
    let top = if v.solid { v.top.max(USUAL_BUTTON) + 2.4 } else { v.top.max(USUAL_TOP).max(v.switch_top + 0.5) + 2.4 + if lens == Lens::Led { 1.6 } else { 0.0 } };
    let (mut x_lo, mut x_hi, z_lo, z_hi) = match lens {
        Lens::Led => (sl.foot0 - 1.0, sl.foot1 + 1.0, -0.9, top),
        Lens::Caps => (sl.foot0 - 2.0, sl.foot1 + 1.3, -0.6, top),
        _ => (-v.g.strip - v.g.deck_beyond - 0.3, sl.foot0 - 0.3, -0.8, v.depth.max(USUAL_DEPTH) + 1.6),
    };
    let want = (z_hi - z_lo) * aspect;
    if want > x_hi - x_lo {
        let grow = (want - (x_hi - x_lo)) / 2.0;
        (x_lo, x_hi) = (x_lo - grow, x_hi + grow);
    }
    (x_lo, x_hi, z_lo, z_hi)
}

/// A measurement, in the panel's pixels: a height, or a length across, and the slider it measures.
enum Mark {
    Height { x: f64, y0: f64, y1: f64, i: usize },
    Across { x0: f64, x1: f64, y: f64, i: usize },
}

/// A section's drawing of the slice, `pw` by `ph`: what it draws, each
/// slider's place on it, and the frame it is cut to, in its pixels.
fn slice(v: &Values, lens: Lens, pw: f64, ph: f64, hot: Option<usize>) -> (Svg, Vec<Label>, (f64, f64, f64, f64)) {
    let sl = Slice::of(v);
    let mut s = Svg::new();
    let (d, hz, tz) = (v.depth, v.switch_top, v.top);
    let (room_w, room_h) = (pw - 2.0 * MARGIN, ph - 40.0);
    let (x_lo, x_hi, z_lo, z_hi) = frame_of(v, &sl, lens, room_w / room_h);
    let k = (room_w / (x_hi - x_lo)).min(room_h / (z_hi - z_lo));
    let left = MARGIN + (room_w - k * (x_hi - x_lo)) / 2.0;
    let floor = ph - 28.0 - (room_h - k * (z_hi - z_lo)) / 2.0;
    let px = move |x: f64| left + k * (x - x_lo);
    let pz = move |z: f64| floor - k * (z - z_lo);
    let p = move |x: f64, z: f64| (px(x), pz(z));
    let pp = move |pts: &[Pt]| pts.iter().map(|&(x, z)| p(x, z)).collect::<Vec<Pt>>();
    let grow = (k / 14.0).min(1.5);
    let on = |part: Lens| part == lens;
    let line = |part: Lens| if on(part) { "#b0b0b0" } else { "#454545" };
    let fill = |part: Lens| if on(part) { "#3a3a3a" } else { "#262626" };
    let glow = if !v.lit { 0.0 } else if on(Lens::Led) { 1.0 } else { 0.55 };
    let white = |name: &str| hot.is_some_and(|i| PARAMS[i].label == name);
    let frame = (px(x_lo).round(), pz(z_hi).round(), px(x_hi).round(), pz(z_lo).round());

    // the body: the decks, the walls and the chassis between the wells
    s.path(&poly(&pp(&clip(&sl.body(x_lo - 1.0, x_hi + 1.0), x_lo, x_hi)), true), fill(Lens::Wells), line(Lens::Wells), 1.0, 1.0);
    let housing_box = clip(&[(sl.hx0, 0.0), (sl.hx0, hz), (sl.hx1, hz), (sl.hx1, 0.0)], x_lo, x_hi);
    if v.solid {
        // a button has no switch to draw
    } else if v.slot {
        // the housing, black, and the LED's slot at its top, glowing round the LED
        s.path(&poly(&pp(&housing_box), true), "#141414", if on(Lens::Led) { "#6a6a6a" } else { "#3a3a3a" }, 1.0, 1.0);
        let (wx0, wx1, wz) = (sl.led - 1.3, sl.led + 1.3, (hz - 1.1).max(hz * 0.6));
        let window = s.linear(0.0, pz(hz), 0.0, pz(wz), &[(0.0, AMBER, 0.55 * glow), (1.0, AMBER, 0.12 * glow)]);
        s.path(&poly(&pp(&clip(&[(wx0, wz), (wx0, hz), (wx1, hz), (wx1, wz)], x_lo, x_hi)), true), &window, "none", 1.0, 1.0);
    } else {
        // the housing, glowing with the LED's scattered light: brightest by the LED, a third every glow spread
        let stops: Vec<(f64, &str, f64)> = (0..17)
            .map(|i| {
                let x = sl.hx0 + (sl.hx1 - sl.hx0) * i as f64 / 16.0;
                (i as f64 / 16.0, AMBER, (0.07 + 0.38 * (-(x - sl.led).abs() / v.spread.max(0.05)).exp()) * glow)
            })
            .collect();
        let housing = s.linear(px(sl.hx0), 0.0, px(sl.hx1), 0.0, &stops);
        s.path(&poly(&pp(&housing_box), true), &housing, if on(Lens::Led) { "#b98a48" } else { "#5a4b36" }, 1.0, 1.0);
    }
    // the share of the LED's light the housing scatters: none from a black one
    let scatter = if v.slot { 0.0 } else { v.scatter };
    // the beam, from the switch's top at the LED to the back of the cap's top,
    // out to half its brightness, inside the cap; as strong as the light the switch doesn't scatter
    let rise = tz - sl.top_t - hz;
    let wide = beam_half(v.beam).to_radians().min(1.35).tan() * rise.max(0.0);
    if rise > 0.3 && v.lit {
        let mut cone = vec![(sl.led, hz), (sl.led + wide, hz + rise), (sl.led - wide, hz + rise)];
        if sl.hollow && !on(Lens::Led) {
            cone = clip_to(&cone, &sl.interior());
        }
        let beam = s.radial(px(sl.led), pz(hz), k * rise * 1.1, &[(0.0, AMBER, (0.25 + 0.6 * (1.0 - scatter)) * glow), (1.0, AMBER, 0.06 * glow)]);
        s.fill(&poly(&pp(&cone), true), &beam);
        if white("Beam") {
            s.stroke(&poly(&pp(&cone), true), WHITE, 1.3);
        }
    }
    if v.lit && x_lo < sl.led && sl.led < x_hi && rise > 0.4 {
        s.dot(px(sl.led), pz(hz) + 1.5, 3.4 * grow, if white("North of centre") { WHITE } else { AMBER });
    }
    // the deck lit out under the skirts, either side of the key
    for (x0, x1) in [(x_lo.max(0.0), sl.foot0 + 0.6), (sl.foot1 - 0.6, (sl.w0 + 0.6).min(x_hi))] {
        if x1 > x0 && v.lit {
            let g = s.linear(px(x0), 0.0, px(x1), 0.0, &[(0.0, AMBER, 0.15), (0.55, AMBER, 0.6), (1.0, AMBER, 0.1)]);
            s.path(&poly(&[p(x0, 0.0), p(x1, 0.0), p(x1, 0.2), p(x0, 0.2)], true), &g, "none", 1.0, if on(Lens::Led) { 0.9 } else { 0.5 });
        }
    }
    // the caps: in the LED's drawing only the 2 key's top, its skirt cut away to show the switch
    let (a0, a1) = (sl.foot0 + v.g.inset, sl.foot1 - v.g.inset);
    let under = tz - sl.top_t;
    if on(Lens::Led) {
        s.path(&poly(&[p(a0, tz), p(a1, tz), p(a1, under), p(a0, under)], true), fill(Lens::Caps), line(Lens::Caps), 1.0, 1.0);
        for x in [a0, a1] {
            let g = s.linear(0.0, pz(under), 0.0, pz(under) + 34.0, &[(0.0, "#454545", 0.8), (1.0, "#454545", 0.0)]);
            s.path(&poly(&[p(x, under), (px(x), pz(under) + 34.0)], false), "none", &g, 1.0, 1.0);
        }
    } else {
        // (a button alone on the chassis)
        for (f0, f1) in [(sl.foot0, sl.foot1), (sl.w0, sl.w0 + v.g.south_foot)].into_iter().take(if v.solid { 1 } else { 2 }) {
            let shape = clip(&sl.cap(f0, f1), x_lo, x_hi);
            if !shape.is_empty() {
                s.path(&poly(&pp(&shape), true), fill(Lens::Caps), if f0 == sl.foot0 { line(Lens::Caps) } else { "#3a3a3a" }, 1.0, 1.0);
            }
        }
    }
    // the key's legends where the art prints them, each as brightly as the
    // light behind it lets (in the LED's drawing all of them, the second
    // function too; in the caps' the first)
    let face: Vec<f64> = (0..=200).map(|i| a0 + (a1 - a0) * i as f64 / 200.0).collect();
    let light: Vec<f64> = face.iter().map(|&x| v.on_top(x - sl.centre)).collect();
    let most = light.iter().cloned().fold(0.0, f64::max).max(1e-12);
    for (at, wide) in v.g.legends(on(Lens::Led)) {
        let mid = sl.mid + at;
        let bright = v.on_top(mid - sl.centre) / most;
        let (lx0, lx1) = (mid - wide / 2.0, mid + wide / 2.0);
        s.path(&poly(&[p(lx0, tz), p(lx1, tz), p(lx1, under), p(lx0, under)], true), AMBER, "none", 1.0, 0.2 + 0.78 * bright);
        if lens == Lens::Caps {
            // the light the legend lets through, as strong as its share
            let halo = s.radial(px(mid), pz(tz), k * 2.6, &[(0.0, AMBER, (0.15 + 0.7 * v.legends) * bright), (1.0, AMBER, 0.0)]);
            s.fill(&poly(&[p(lx0 - 2.4, tz), p(lx1 + 2.4, tz), p(lx1 + 2.4, tz + 2.0), p(lx0 - 2.4, tz + 2.0)], true), &halo);
        }
    }

    let mut labels: Vec<Label> = Vec::new();
    let mut marks: Vec<Mark> = Vec::new();
    match lens {
        Lens::Led => {
            // the light on the back of the cap's top, along it, drawn over it
            // with its scale: the LED's own light, before what bounces inside
            // the cap; its top as bright as LED output makes it
            let base = pz(tz) - 12.0;
            let tall = 74.0f64.min(pz(tz) - pz(z_hi) - 40.0);
            let pts: Vec<Pt> = face.iter().zip(&light).map(|(&x, &l)| (px(x), base - tall * l / most)).collect();
            let (x0, x1) = (pts[0].0, pts[pts.len() - 1].0);
            s.stroke(&format!("M{x0:.1} {:.1} L{x1:.1} {:.1}", base - tall, base - tall), if white("LED output") { WHITE } else { "#333333" }, 1.0);
            s.stroke(&format!("M{x0:.1} {base:.1} L{x1:.1} {base:.1}"), "#3a3a3a", 1.0);
            let mut area = poly(&pts, false);
            let _ = write!(area, " L{x1:.1} {base:.1} L{x0:.1} {base:.1} Z");
            s.path(&area, AMBER, "none", 1.0, 0.1);
            s.stroke(&poly(&pts, false), AMBER, 1.6);
            let (n, u) = super::reads(&PARAMS[slider("LED output")], v.nits);
            s.text(x0 - 8.0, base - tall + 4.0, &format!("{n} {u}"), if white("LED output") { WHITE_WORD } else { QUIET_WORD }, 1.0);
            s.text(x0 - 8.0, base + 4.0, "0", QUIET_WORD, 1.0);
            let peak = pts.iter().zip(&light).fold((0.0, x0), |a, (q, &l)| if l > a.0 { (l, q.0) } else { a }).1;
            // (on the right, clear of Brightness's line down to the top's end)
            let (end, anchor) = if peak < (x0 + x1) / 2.0 { (x1 - 12.0, 1.0) } else { (x0, 0.0) };
            s.text(end, base - tall - 22.0, "Light on the cap's top", QUIET_WORD, anchor);
            match v.g.legends(true).len() {
                0 => {}
                1 => s.text(end, base - tall - 7.0, "over its legend", FAINT_WORD, anchor),
                _ => s.text(end, base - tall - 7.0, "over its two legends", FAINT_WORD, anchor),
            }
            labels.push(Label { drop: true, ..Label::new(false, (x1, base - tall), "LED output") });
            // the switch's centre line, and the LED north of it
            s.stroke(&poly(&[p(sl.centre, 0.15), p(sl.centre, hz + 0.6)], false), "#5c5c5c", 1.0);
            let y_nc = if hz > 2.6 { hz - 0.6 } else { hz + 0.7 };
            marks.push(Mark::Across { x0: px(sl.led), x1: px(sl.centre), y: pz(y_nc), i: slider("North of centre") });
            labels.push(Label::new(true, p(sl.led, hz), "North of centre"));
            // Beam on the cone's edge; with no room for a beam, where it would meet the cap's top
            let beam_at = if rise > 0.3 { p(sl.led - wide * 0.62, hz + rise * 0.62) } else { p(sl.led, under) };
            labels.push(Label::new(true, beam_at, "Beam"));
            let width_z = if hz > 2.6 { 0.45 } else { hz + 2.1 };
            marks.push(Mark::Across { x0: px(sl.hx0), x1: px(sl.hx1), y: pz(width_z), i: slider("Switch width") });
            labels.push(Label { flat: true, ..Label::new(true, p(sl.hx0, width_z), "Switch width") });
            marks.push(Mark::Height { x: px(sl.hx1 + 1.9), y0: pz(0.0), y1: pz(hz), i: slider("Switch top") });
            labels.push(Label::new(false, p(sl.hx1 + 1.9, hz * 0.5), "Switch top"));
            // a clear housing's glow (a black one has none)
            if !v.slot {
            // the housing's glow falling away from the LED: to a third every glow spread
            let far = (sl.hx1 - sl.led).max(sl.led - sl.hx0);
            let toward = if sl.hx1 - sl.led >= sl.led - sl.hx0 { 1.0 } else { -1.0 };
            let (zb, amp, spread) = (hz * 0.2, hz * 0.34, v.spread.max(0.05));
            let curve: Vec<Pt> = (0..=60).map(|i| {
                let dd = far * i as f64 / 60.0;
                p(sl.led + toward * dd, zb + amp * (-dd / spread).exp())
            }).collect();
            s.stroke(&poly(&[p(sl.led, zb), p(sl.led + toward * far, zb)], false), "#6b5532", 1.0);
            s.path(&poly(&curve, false), "none", if white("Glow spread") { WHITE } else { AMBER }, 1.2, 0.95);
            let at = spread.min(far);
            labels.push(Label::new(true, p(sl.led + toward * at, zb + amp * (-at / spread).exp()), "Glow spread"));
            // the glow out of the housing's top and, short, out of its sides, each as strong as its share
            let (top_share, side_share) = (v.scatter * (1.0 - v.sides), v.scatter * v.sides);
            let mut ups = Vec::new();
            if rise > 1.6 {
                let up = 2.4f64.min(rise * 0.4);
                for x in [sl.hx0 + 1.1, sl.centre + 2.4, sl.hx1 - 1.0] {
                    if (x - sl.led).abs() > wide * 0.5 + 0.9 {
                        ray(&mut s, &[p(x, hz + 0.25), p(x + 0.1, hz + up)], top_share, grow, white("Scattered"));
                        ups.push((x, up));
                    }
                }
            }
            let scattered = match ups.get(ups.len() / 2) {
                Some(&(x, up)) => p(x + 0.05, hz + 0.25 + (up - 0.25) * 0.45),
                None => p(sl.hx1 - 0.25, hz - 0.25),
            };
            labels.push(Label::new(false, scattered, "Scattered"));
            let out = 1.9f64.min(v.foot.max(0.6) * 0.45);
            ray(&mut s, &[p(sl.hx1 + 0.15, out), p(sl.hx1 + 1.5, out - 0.55)], side_share, grow, white("Through its sides"));
            ray(&mut s, &[p(sl.hx0 - 0.15, out), p(sl.hx0 - 1.5, out - 0.55)], side_share, grow, white("Through its sides"));
            labels.push(Label::new(false, p(sl.hx1 + 0.85, out - 0.27), "Through its sides"));
            }
        }
        Lens::Caps => {
            marks.push(Mark::Height { x: px(sl.foot0 - 1.3), y0: pz(0.0), y1: pz(v.foot), i: slider("Skirt's foot") });
            labels.push(Label::new(true, p(sl.foot0 - 1.3, v.foot * 0.5), "Skirt's foot"));
            marks.push(Mark::Height { x: px(sl.foot0 - 0.55), y0: pz(0.0), y1: pz(tz), i: slider("Cap's top") });
            labels.push(Label::new(true, p(sl.foot0 - 0.55, tz * 0.62), "Cap's top"));
            if let Some(&(at, wide)) = v.g.legends(false).first() {
                let mid = sl.mid + at;
                labels.push(Label { drop: true, ..Label::new(true, p(mid - wide / 2.0 + 0.9, tz), "Legend opacity") });
                labels.push(Label { flat: true, ..Label::new(false, p(mid + wide / 2.0 - 0.4, tz), "LED off") });
            }
            // light out of a clear switch's side and under the skirt's foot onto the deck
            if !v.slot && !v.solid {
                let out = 1.6f64.min(v.foot.max(0.6) * 0.4);
                ray(&mut s, &[p(sl.hx1 + 0.15, out), p(sl.foot1 + 0.95, out * 0.15)], v.scatter * v.sides, grow, false);
            }
            // the skirts: black from the top down Black down the sides, and below
            // it their translucent plastic, as white as it is milky, glowing with
            // the light it lets through; and the top beside its legends
            let clear_to = v.foot + (1.0 - v.black) * (tz - v.foot);
            if sl.hollow {
                let op = |share: f64| 0.12 + 0.7 * share.clamp(0.0, 1.0);
                for north in [true, false] {
                    let band = [p(sl.skirt_x(north, v.foot, false), v.foot), p(sl.skirt_x(north, clear_to, false), clear_to), p(sl.skirt_x(north, clear_to, true), clear_to), p(sl.skirt_x(north, v.foot, true), v.foot)];
                    let milky = if white("Haze") { WHITE } else { "#d8d2c6" };
                    s.path(&poly(&band, true), milky, "none", 1.0, if v.skirts > 0.0 { 0.08 + 0.55 * v.milky } else { 0.0 });
                    let lit = if white("Wall opacity") { WHITE } else { AMBER };
                    s.path(&poly(&[band[0], band[1]], false), "none", lit, 2.4 * grow.min(1.2), if v.skirts > 0.0 { op(v.skirts) } else { 0.0 });
                }
                if v.tops > 0.0 {
                    let lit = if white("Wall opacity") { WHITE } else { AMBER };
                    s.path(&poly(&[p(a0 + 0.3, tz), p(a1 - 0.3, tz)], false), "none", lit, 2.4 * grow.min(1.2), op(v.tops));
                }
            }
            // light through the north skirt below its black, from inside the cap to its outside
            // (a shine-through cap's plastic shows only through its legends; a translucent one has no black)
            let (shine, translucent) = (v.caps == Caps::ShineThrough, v.caps == Caps::Translucent);
            if !shine {
                let side_z = v.foot + (clear_to - v.foot) * 0.7;
                let (wall_in, wall_out) = (sl.skirt_x(true, side_z, true), sl.skirt_x(true, side_z, false));
                ray(&mut s, &[p(wall_in + 1.1, side_z + 0.35), p(wall_out - 1.2, side_z - 0.25)], v.skirts, grow, white("Wall opacity"));
                labels.push(Label::new(true, p(wall_out, side_z), "Wall opacity"));
                // Haze on the plastic low on the north skirt
                let milky_z = v.foot + (clear_to - v.foot) * 0.25;
                labels.push(Label::new(true, p(sl.skirt_x(true, milky_z, false), milky_z), "Haze"));
            }
            if !shine && !translucent {
                // Black down the sides measured beside the south skirt
                let bx = (sl.foot1 + 1.0).min(x_hi - 0.4);
                marks.push(Mark::Height { x: px(bx), y0: pz(clear_to), y1: pz(tz), i: slider("Black down the sides") });
                labels.push(Label::new(false, p(bx, (clear_to + tz) / 2.0), "Black down the sides"));
            }
            // light up through the top, beside the legends
            let top_x = a1 - 1.6;
            if sl.hollow {
                ray(&mut s, &[p(top_x, under - 0.6), p(top_x + 0.15, tz + 1.4)], v.tops, grow, white("Wall opacity"));
            }
            if sl.hollow && rise > 1.0 {
                // bounced inside the cap: off its inner wall, up to the back of its top
                let mid_z = (hz.max(v.foot) + under) / 2.0;
                let inner = sl.skirt_x(false, mid_z, true);
                ray(&mut s, &[p(sl.hx1 - 0.6, hz + 0.2), p(inner, mid_z), p(sl.centre + 2.2, under - 0.35)], v.cap_in, grow, white("Inside reflection"));
            }
            // Inside's line comes in under the skirt's foot, as the light does, to the wall just above it
            // (a button is solid: nothing inside it)
            if !v.solid {
                let low = v.foot + 0.7f64.min((tz - v.foot) * 0.5);
                labels.push(Label { elbow: Some(pz(v.foot * 0.45)), ..Label::new(false, p(sl.skirt_x(false, low, true), low), "Inside reflection") });
            }
            // light from the next key: onto the outside of this one's skirt, and off it;
            // onto a button from above the chassis it stands on, and onto its top too
            let bounce_z = v.foot + 1.8f64.min((tz - v.foot) * 0.3);
            let outside = sl.skirt_x(false, bounce_z, false);
            let from_z = if v.solid { bounce_z + 1.2 } else { v.foot * 0.55 };
            ray(&mut s, &[p((x_hi - 0.1).min(outside + 2.4), from_z), p(outside + 0.08, bounce_z - 0.05)], 0.5, grow, false);
            ray(&mut s, &[p(outside + 0.1, bounce_z + 0.15), p((x_hi - 0.25).min(outside + 2.2), bounce_z + 1.6)], v.cap_out, grow, white("Outside reflection"));
            labels.push(Label::new(false, p(outside, bounce_z), "Outside reflection"));
            if v.solid {
                let top_x = (a0 + a1) / 2.0;
                ray(&mut s, &[p(top_x + 2.2, tz + 2.0), p(top_x + 0.1, tz + 0.05)], 0.5, grow, false);
                ray(&mut s, &[p(top_x - 0.1, tz + 0.1), p(top_x - 1.6, tz + 1.7)], v.cap_out, grow, white("Outside reflection"));
            }
        }
        _ => {
            // light from under the keys: off the main well's deck onto its wall
            // (the chassis's own side, giving back what it does), and, where the
            // wall is low enough, over it onto the chassis
            let edge = (x_hi + 0.3).min(sl.foot0 + 1.6);
            let wall_z = 1.1f64.min(d * 0.6);
            if d > 0.25 {
                ray(&mut s, &[p(edge, 1.4f64.min(d + 0.9)), p(0.85, 0.03), p(0.03, wall_z), p(0.9, wall_z + 1.25)], v.chassis, grow, white("Chassis reflection"));
                ray(&mut s, &[p(edge, d + 1.25), p(-0.7, d + 0.03), p(-1.25, d + 1.0)], v.chassis, grow, white("Chassis reflection"));
                marks.push(Mark::Height { x: px(-v.g.strip - 0.85), y0: pz(0.0), y1: pz(d), i: slider("Depth") });
                labels.push(Label::new(true, p(-v.g.strip - 0.85, d * 0.8), "Depth"));
            } else {
                ray(&mut s, &[p(edge, 1.2), p(-0.8, 0.03), p(-1.4, 1.0)], v.chassis, grow, white("Chassis reflection"));
                labels.push(Label { flat: true, ..Label::new(true, p(-v.g.strip - 0.85, 0.0), "Depth") });
            }
            let dx = -v.g.strip - v.g.deck_beyond * 0.62;
            ray(&mut s, &[p(dx - 1.3, 1.1), p(dx + 0.3, 0.03), p(dx + 1.0, 0.9)], v.deck, grow, white("Deck reflection"));
            labels.push(Label { flat: true, ..Label::new(true, p(-1.6, d), "Chassis reflection") });
            labels.push(Label { flat: true, ..Label::new(true, p(dx, 0.0), "Deck reflection") });
        }
    }
    // the slice goes on past the frame: each end fades into the ground, and its top where the frame cuts it
    for (a, b) in [(px(x_lo) - 1.0, px(x_lo) + 40.0), (px(x_hi) + 1.0, px(x_hi) - 40.0)] {
        let g = s.linear(a, 0.0, b, 0.0, &[(0.0, PLOT, 1.0), (1.0, PLOT, 0.0)]);
        s.fill(&poly(&[(a, 0.0), (b, 0.0), (b, ph), (a, ph)], true), &g);
    }
    if z_hi < tz {
        let g = s.linear(0.0, pz(z_hi) - 1.0, 0.0, pz(z_hi) + 30.0, &[(0.0, PLOT, 1.0), (1.0, PLOT, 0.0)]);
        s.fill(&poly(&[(px(x_lo), pz(z_hi) - 1.0), (px(x_hi), pz(z_hi) - 1.0), (px(x_hi), pz(z_hi) + 30.0), (px(x_lo), pz(z_hi) + 30.0)], true), &g);
    }
    // the measurements, over the frame's edge
    let tick = 5.0f64;
    for m in marks {
        match m {
            Mark::Height { x, y0, y1, i } => {
                let c = if Some(i) == hot { WHITE } else { DIMENSION };
                s.mark(&poly(&[(x, y0), (x, y1)], false), c);
                for y in [y0, y1] {
                    s.mark(&poly(&[(x - tick, y), (x + tick, y)], false), c);
                }
            }
            Mark::Across { x0, x1, y, i } => {
                let c = if Some(i) == hot { WHITE } else { DIMENSION };
                s.mark(&poly(&[(x0, y), (x1, y)], false), c);
                for x in [x0, x1] {
                    s.mark(&poly(&[(x, y - tick), (x, y + tick)], false), c);
                }
            }
        }
    }
    (s, labels, frame)
}

/// A colour `t` of the way from `a` to `b`.
fn mix(a: &str, b: &str, t: f64) -> String {
    let ch = |s: &str, i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0) as f64;
    let t = t.clamp(0.0, 1.0);
    let mut out = String::from("#");
    for i in [1, 3, 5] {
        let _ = write!(out, "{:02x}", (ch(a, i) + (ch(b, i) - ch(a, i)) * t).round() as u8);
    }
    out
}

/// The display close up, `pw` by `ph`: a few of its dots, lit and unlit,
/// the ground between them, the glass over them, and each slider's place.
fn display(v: &Values, pw: f64, ph: f64, hot: Option<usize>) -> (Svg, Vec<Label>) {
    let mut s = Svg::new();
    // "2C" as the display draws it: 5 by 7 dots a character, a column apart
    let two = ["01110", "10001", "00001", "00010", "00100", "01000", "11111"];
    let cee = ["01110", "10001", "10000", "10000", "10000", "10001", "01110"];
    let glyph: Vec<Vec<bool>> = two.iter().zip(cee).map(|(a, b)| format!("{a}0{b}").chars().map(|c| c == '1').collect()).collect();
    let (rows, cols) = (glyph.len() as f64, glyph[0].len() as f64);
    // a cell of the ground round the dots, as the display has
    let margin = 1.0;
    let cell = ((pw - 2.0 * MARGIN) / (cols + 2.0 * margin)).min((ph - 60.0) / (rows + 2.0 * margin));
    let (fw, fh) = (cell * (cols + 2.0 * margin), cell * (rows + 2.0 * margin));
    let (gx0, gy0) = ((pw - fw) / 2.0, (ph - 10.0 - fh) / 2.0);
    let (x0, y0) = (gx0 + cell * margin, gy0 + cell * margin);
    // each dot a square from its cell's corner, as the display draws it, `dot` of the cell across
    let dot = cell * v.lcd_dot.clamp(0.05, 1.0);
    let lit = v.lcd_lit.max(1e-3);
    // each part as bright as its nits beside the lit dots', in one scale, none stacked on another
    let seen = |n: f64| 0.95 * (n / lit).clamp(0.0, 1.0).powf(0.42);
    s.fill(&poly(&[(gx0, gy0), (gx0 + fw, gy0), (gx0 + fw, gy0 + fh), (gx0, gy0 + fh)], true), &mix(PLOT, AMBER, seen(v.lcd_field)));
    for (r, row) in glyph.iter().enumerate() {
        for (c, &on) in row.iter().enumerate() {
            let (cx, cy) = (x0 + cell * c as f64, y0 + cell * r as f64);
            s.fill(&poly(&[(cx, cy), (cx + dot, cy), (cx + dot, cy + dot), (cx, cy + dot)], true), &mix(PLOT, AMBER, seen(if on { lit } else { v.lcd_unlit })));
        }
    }
    // the glass's sheen across the whole display: a soft band, leaning, its
    // gradient across it, its edges fading to nothing
    let (bx0, bx1, lean) = (x0 + cell * 3.2, x0 + cell * 6.2, cell * 3.0);
    let n = fh.hypot(lean);
    let (ux, uy) = (fh / n, lean / n);
    let half = (bx1 - bx0) * fh / n / 2.0;
    let (cx, cy) = ((bx0 + bx1) / 2.0 + lean / 2.0, gy0 + fh / 2.0);
    let g = s.linear(cx - ux * half, cy - uy * half, cx + ux * half, cy + uy * half, &[(0.0, "#ffffff", 0.0), (0.5, "#ffffff", 0.16 * v.lcd_glass), (1.0, "#ffffff", 0.0)]);
    s.fill(&poly(&[(bx0 + lean, gy0), (bx1 + lean, gy0), (bx1, gy0 + fh), (bx0, gy0 + fh)], true), &g);
    // one cell outlined, and its dot's width across under the dots
    let (cx0, cy0) = (x0 + cell * 9.0, y0 + cell * 6.0);
    s.stroke(&poly(&[(cx0, cy0), (cx0 + cell, cy0), (cx0 + cell, cy0 + cell), (cx0, cy0 + cell)], true), "#d0d0d0", 1.0);
    let yy = y0 + cell * rows + cell * 0.45;
    let c = if hot == Some(slider("Dot size")) { WHITE } else { DIMENSION };
    s.mark(&poly(&[(cx0, yy), (cx0 + dot, yy)], false), c);
    for x in [cx0, cx0 + dot] {
        s.mark(&poly(&[(x, yy - 4.0), (x, yy + 4.0)], false), c);
    }
    // Glass sheen's place: on the sheen where it crosses the ground above the dots
    let band = (bx0 + bx1) / 2.0 + lean * (1.0 - (cell * 0.5) / fh);
    let labels = vec![
        Label { flat: true, ..Label::new(true, (x0 + cell + dot * 0.5, y0 + dot * 0.5), "Lit dots") },
        Label::new(true, (x0 + dot * 0.5, y0 + cell * 3.0 + dot * 0.5), "Unlit dots"),
        Label { flat: true, ..Label::new(false, (band, gy0 + cell * 0.5), "Glass sheen") },
        Label::new(false, (gx0 + fw - cell * 0.5, gy0 + cell * 2.5), "Background"),
        Label::new(false, (cx0 + dot / 2.0, yy), "Dot size"),
    ];
    (s, labels)
}

// ---------------------------------------------------------------- the stage

/// The panel the drawing is in, on a stage `w` by `h`: x, y, width, height.
fn panel(w: f64, h: f64) -> (f64, f64, f64, f64) {
    let px0 = COL + CTRL_W as f64 + GAP;
    (px0, 14.0, (w - 2.0 * px0).max(120.0), h - 28.0)
}

fn drawn(v: &Values, lens: Lens, w: f64, h: f64, hot: Option<usize>, set: Option<&Legends>) -> (Svg, Vec<Label>, Option<(f64, f64, f64, f64)>) {
    let (px0, py0, pw, ph) = panel(w, h);
    let (mut s, mut labels, frame) = match lens {
        Lens::Display => {
            let (s, l) = display(v, pw, ph, hot);
            (s, l, None)
        }
        Lens::Legends => {
            let (s, l) = set.map_or_else(|| (Svg::new(), Vec::new()), |set| legends(set, pw, ph, hot));
            (s, l, None)
        }
        _ => {
            let (s, l, f) = slice(v, lens, pw, ph, hot);
            (s, l, Some(f))
        }
    };
    for l in &mut labels {
        l.at = (l.at.0 + px0, l.at.1 + py0);
        l.elbow = l.elbow.map(|e| e + py0);
    }
    for word in &mut s.words {
        word.x += px0 as f32;
        word.y += py0 as f32;
    }
    (s, labels, frame.map(|(a, b, c, d)| (a + px0, b + py0, c + px0, d + py0)))
}

/// Where each slider of a section stands on a stage `w` by `h`, from these
/// values: on its side, level with what it names where its neighbours let
/// it, in the order of what they name, so no two lines cross; on the left,
/// under the section's `choices`. A section's sliders are placed once, as
/// it opens, and stay put while its values change; only the drawing and the
/// ends of their lines follow.
pub fn places(v: &Values, lens: Lens, w: f32, h: f32, choices: usize, right_top: f32, shown: Option<&[usize]>, set: Option<&Legends>) -> Vec<Place> {
    let (w, h) = (w as f64, h as f64);
    let (_, mut labels, _) = drawn(v, lens, w, h, None, set);
    // only the sliders shown take a place
    if let Some(shown) = shown {
        labels.retain(|l| shown.contains(&l.i));
    }
    let (px0, _, pw, _) = panel(w, h);
    let rise = 46.0;
    let mut out = Vec::new();
    for left in [true, false] {
        let edge = if left { px0 + 14.0 } else { px0 + pw - 14.0 };
        let level = |l: &Label| l.elbow.unwrap_or(l.at.1);
        let mut mine: Vec<&Label> = labels.iter().filter(|l| l.left == left).collect();
        mine.sort_by(|a, b| {
            let ka = (level(a) - if a.drop { 2000.0 } else { 0.0 }, -(a.at.0 - edge).abs());
            let kb = (level(b) - if b.drop { 2000.0 } else { 0.0 }, -(b.at.0 - edge).abs());
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let under = if left { choices as f64 * CHOICE_H as f64 } else { right_top as f64 };
        // how far each slider stands above the next: its own height and the
        // gap between, all the less where the column is short of room
        let pitch: Vec<f64> = mine.iter().map(|l| ctrl_h(l.i) as f64 + PITCH - CTRL_H as f64).collect();
        let last = mine.last().map_or(CTRL_H, |l| ctrl_h(l.i)) as f64;
        let (lo, hi) = (14.0 + ATTACH as f64 + under, h - 14.0 - (last - ATTACH as f64));
        let hi = hi.max(lo);
        let want: Vec<f64> = mine.iter().map(|l| if l.drop { lo } else if l.flat { level(l) - rise } else { level(l) }).collect();
        let need: f64 = pitch.iter().rev().skip(1).sum();
        let room = if need > 0.0 { ((hi - lo) / need).min(1.0) } else { 1.0 };
        let mut ys: Vec<f64> = Vec::new();
        for (n, y) in want.into_iter().enumerate() {
            let y = y.clamp(lo, hi);
            ys.push(ys.last().map_or(y, |&last: &f64| y.max(last + pitch[n - 1] * room)));
        }
        let n = ys.len();
        for i in (0..n).rev() {
            ys[i] = ys[i].min(hi - pitch[i..n - 1].iter().sum::<f64>() * room);
            if i + 1 < n {
                ys[i] = ys[i].min(ys[i + 1] - pitch[i] * room);
            }
        }
        out.extend(mine.iter().zip(ys).map(|(l, y)| Place { i: l.i, left, y: y as f32 }));
    }
    out
}

/// The stage, `w` by `h`: the tile, the drawing in its panel, and the line
/// from each slider, standing at its place, to its part; `hot` the slider
/// under the pointer or in use, drawn white.
pub fn stage(v: &Values, lens: Lens, w: f32, h: f32, places: &[Place], hot: Option<usize>, set: Option<&Legends>) -> Stage {
    let (wf, hf) = (w as f64, h as f64);
    let (mut s, labels, frame) = drawn(v, lens, wf, hf, hot, set);
    let (px0, py0, pw, ph) = panel(wf, hf);
    let mut svg = String::new();
    let _ = write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wf}" height="{hf}" viewBox="0 0 {wf} {hf}"><defs>{}</defs>"#, s.defs);
    let _ = write!(svg, r#"<path d="{}" fill="{PLOT}"/>"#, rounded(px0, py0, px0 + pw, py0 + ph, 8.0));
    let _ = write!(svg, r#"<g transform="translate({px0} {py0})">{}</g>"#, s.body);
    // nothing of the slice past its frame: the panel's ground over it all round the frame,
    // then the tile's round the panel (cheaper to draw than a clip)
    if let Some((fx0, fy0, fx1, fy1)) = frame {
        let _ = write!(svg, r#"<path d="{} M{fx0} {fy0} H{fx1} V{fy1} H{fx0} Z" fill="{PLOT}" fill-rule="evenodd"/>"#, rounded(px0, py0, px0 + pw, py0 + ph, 8.0));
    }
    let _ = write!(svg, r#"<path d="{} {}" fill="{TILE}" fill-rule="evenodd"/>"#, rounded(0.0, 0.0, wf, hf, 14.0), rounded(px0, py0, px0 + pw, py0 + ph, 8.0));
    let _ = write!(svg, r#"<g transform="translate({px0} {py0})">{}</g>"#, s.marks);
    // each slider's line, from where it stands to the dot on its part
    let mut extras = Vec::new();
    for l in &labels {
        let Some(place) = places.iter().find(|q| q.i == l.i) else { continue };
        let ay = place.y as f64;
        let x = column_x(l.left, w) as f64;
        let start = if l.left { (x + CTRL_W as f64 + 8.0, ay) } else { (x - 8.0, ay) };
        let edge = if l.left { px0 + 14.0 } else { px0 + pw - 14.0 };
        let (ax, by) = l.at;
        let route = if l.drop || l.elbow.is_some() { vec![start, (edge, ay), (ax, ay), (ax, by)] } else { vec![start, (edge, ay), (ax, by)] };
        let is_hot = hot == Some(l.i);
        let _ = write!(svg, r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linejoin="round"/>"#, poly(&route, false), if is_hot { WHITE } else { LEAD }, if is_hot { 1.5 } else { 1.0 });
        let r = if is_hot { 3.0 } else { 2.3 };
        let _ = write!(svg, r#"<path d="M{:.2} {by:.2} a{r} {r} 0 1 0 {} 0 a{r} {r} 0 1 0 {} 0" fill="{}"/>"#, ax - r, 2.0 * r, -2.0 * r, if is_hot { WHITE } else { DOT });
    }
    if lens == Lens::Led && v.switch_top > 0.0 {
        extras.push((slider("Beam"), format!("±{:.0}°", beam_half(v.beam))));
    }
    if lens == Lens::Wells && v.depth <= 0.25 {
        extras.push((slider("Depth"), "no walls".to_string()));
    }
    svg += "</svg>";
    // (the slices are seen from the side, north to south; the legends from above)
    if !matches!(lens, Lens::Display | Lens::Legends) {
        s.words.push(Word { x: (px0 + 16.0) as f32, y: (py0 + ph - 12.0) as f32, text: "North".into(), colour: QUIET_WORD });
        let south = text::width(400, 11.5, "South") as f64;
        s.words.push(Word { x: (px0 + pw - 16.0 - south) as f32, y: (py0 + ph - 12.0) as f32, text: "South".into(), colour: QUIET_WORD });
    }
    Stage { w, h, svg, words: s.words, extras }
}

// ---------------------------------------------------------------- the legends

/// A key drawn from above with its print: its foot across and north to
/// south, and its top from the foot's top left corner (mm), and what is
/// printed on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Printed {
    pub foot: (f64, f64),
    pub face: [f64; 4],
    pub print: ectodeck_draw::print::Print,
}

/// The legends' drawing: how they are set, how the core's art prints them,
/// and the key it is of, large, with keys of other kinds beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Legends {
    pub style: ectodeck_draw::print::Style,
    pub own: ectodeck_draw::print::Own,
    pub key: Printed,
    pub others: Vec<Printed>,
}

/// A path as SVG's path data.
fn svg_path(p: &resvg::tiny_skia::Path) -> String {
    use resvg::tiny_skia::PathSegment;
    let mut d = String::new();
    for seg in p.segments() {
        let _ = match seg {
            PathSegment::MoveTo(a) => write!(d, "M{:.2} {:.2}", a.x, a.y),
            PathSegment::LineTo(a) => write!(d, "L{:.2} {:.2}", a.x, a.y),
            PathSegment::QuadTo(a, b) => write!(d, "Q{:.2} {:.2} {:.2} {:.2}", a.x, a.y, b.x, b.y),
            PathSegment::CubicTo(a, b, c) => write!(d, "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", a.x, a.y, b.x, b.y, c.x, c.y),
            PathSegment::Close => write!(d, "Z"),
        };
    }
    d
}

/// A key's print set as the section says, on its top at (`x`, `y`) `k`
/// pixels to the millimetre: its inks drawn in the light's amber (a glow
/// under them), and its lines.
fn print_on(s: &mut Svg, set: &Legends, key: &Printed, x: f64, y: f64, k: f64) -> Vec<ectodeck_draw::print::Line> {
    let face = [(x + key.face[0] * k) as f32, (y + key.face[1] * k) as f32, (x + key.face[2] * k) as f32, (y + key.face[3] * k) as f32];
    let db = ectodeck_draw::print::faces();
    let laid = ectodeck_draw::print::set(&db, &key.print, &set.style, &set.own, face, k as f32);
    let blur = s.gid();
    let _ = write!(s.defs, r#"<filter id="{blur}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{:.2}"/></filter>"#, (k * 0.35).max(0.8));
    for glow in [true, false] {
        let _ = write!(s.body, r#"<g{}>"#, if glow { format!(r#" filter="url(#{blur})" opacity="0.55""#) } else { String::new() });
        for ink in &laid.inks {
            let t = ink.to_art;
            let paint = match ink.stroke {
                None => format!(r#"fill="{AMBER}" fill-opacity="{:.2}""#, ink.opacity),
                Some(w) => format!(r#"fill="none" stroke="{AMBER}" stroke-opacity="{:.2}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round""#, ink.opacity),
            };
            let _ = write!(s.body, r#"<path d="{}" transform="matrix({} {} {} {} {} {})" {paint}/>"#, svg_path(&ink.path), t.sx, t.ky, t.kx, t.sy, t.tx, t.ty);
        }
        s.body += "</g>";
    }
    laid.lines
}

/// A key's cap from above: its foot and its top inside it.
fn cap_from_above(s: &mut Svg, key: &Printed, x: f64, y: f64, k: f64, big: bool) {
    let (w, d) = (key.foot.0 * k, key.foot.1 * k);
    let r = (1.6 * k).min(10.0);
    s.path(&rounded(x, y, x + w, y + d, r), "#262626", if big { "#5c5c5c" } else { "#454545" }, 1.0, 1.0);
    let [a0, b0, a1, b1] = key.face;
    s.path(&rounded(x + a0 * k, y + b0 * k, x + a1 * k, y + b1 * k, (r - 2.0).max(1.0)), "#2b2b2b", "#3a3a3a", 1.0, 1.0);
}

/// The legends' drawing, `pw` by `ph`: the key it is of from above, large,
/// its print set as the section sets it, and its letters' and words'
/// heights and its inset measured on it; under it, keys of other kinds set
/// alike, to one scale.
fn legends(set: &Legends, pw: f64, ph: f64, hot: Option<usize>) -> (Svg, Vec<Label>) {
    use ectodeck_draw::print::{Place as At, Role};
    let mut s = Svg::new();
    let white = |name: &str| hot.is_some_and(|i| PARAMS[i].label == name);
    let key = &set.key;
    let k = ((pw - 140.0) / key.foot.0).min((ph * if set.others.is_empty() { 0.8 } else { 0.52 }) / key.foot.1).min(26.0);
    let (x, y) = ((pw - key.foot.0 * k) / 2.0, if set.others.is_empty() { (ph - key.foot.1 * k) / 2.0 } else { 34.0 });
    cap_from_above(&mut s, key, x, y, k, true);
    let lines = print_on(&mut s, set, key, x, y, k);
    let mut labels = Vec::new();
    // each size on its line's capitals, beside the key's top
    let face_right = x + key.face[2] * k;
    let sx = x + key.foot.0 * k + 14.0;
    let letters = lines.iter().find(|l| l.letter && l.role != Role::Second).or_else(|| lines.iter().find(|l| l.letter));
    let words = lines.iter().find(|l| !l.letter);
    for (line, name) in [(letters, "Letter size"), (words, "Word size")] {
        let Some(l) = line else { continue };
        let (top, base) = (l.top as f64, l.base as f64);
        let c = if white(name) { WHITE } else { DIMENSION };
        s.mark(&poly(&[(sx, top), (sx, base)], false), c);
        for yy in [top, base] {
            s.mark(&poly(&[(sx - 5.0, yy), (sx + 5.0, yy)], false), c);
        }
        let _ = write!(s.marks, r##"<path d="M{face_right:.1} {top:.1} H{sx:.1} M{face_right:.1} {base:.1} H{sx:.1}" stroke="#5a5a5a" stroke-width="1" stroke-dasharray="2 3"/>"##);
        labels.push(Label::new(false, (sx, (top + base) / 2.0), name));
    }
    // Inset: down from the top's edge to the first line's capitals, and in
    // from its left at the top left (in the middle it has no say)
    // (nor, as the keyboard prints them, on a key it prints in the middle)
    let inset_says = matches!(set.style.place, At::TopLeft | At::Top) || (set.style.place == At::Printed && !key.print.centred);
    if let (Some(first), true) = (lines.first(), inset_says) {
        let face_top = y + key.face[1] * k;
        let top = first.top as f64;
        let ix = face_right - 16.0;
        let c = if white("Inset") { WHITE } else { DIMENSION };
        s.mark(&poly(&[(ix, face_top), (ix, top)], false), c);
        for yy in [face_top, top] {
            s.mark(&poly(&[(ix - 5.0, yy), (ix + 5.0, yy)], false), c);
        }
        if set.style.place == At::TopLeft {
            let face_left = x + key.face[0] * k;
            let iy = first.base as f64 + 0.6 * (first.base - first.top) as f64;
            s.mark(&poly(&[(face_left, iy), (first.left as f64, iy)], false), c);
            for xx in [face_left, first.left as f64] {
                s.mark(&poly(&[(xx, iy - 5.0), (xx, iy + 5.0)], false), c);
            }
        }
        labels.push(Label::new(false, (ix, (face_top + top) / 2.0), "Inset"));
    }
    // the others, a row of them under it, to one scale
    if !set.others.is_empty() {
        let room = pw - 48.0;
        let total: f64 = set.others.iter().map(|o| o.foot.0).sum::<f64>() + 4.0 * set.others.len().saturating_sub(1) as f64;
        let ks = (room / total).min((ph * 0.24) / set.others.iter().map(|o| o.foot.1).fold(1.0, f64::max)).min(k * 0.62);
        let mut ox = (pw - total * ks) / 2.0;
        let oy = ph - 26.0 - set.others.iter().map(|o| o.foot.1).fold(0.0, f64::max) * ks;
        for o in &set.others {
            cap_from_above(&mut s, o, ox, oy, ks, false);
            print_on(&mut s, set, o, ox, oy, ks);
            ox += (o.foot.0 + 4.0) * ks;
        }
    }
    (s, labels)
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The typefaces' list, `w` by `h`: the core's own at the top, then this
/// computer's from `first`, `rows` of them, each its name set in itself and
/// a few legends beside it in amber; `current` the one the legends are set
/// in (its place on the list, the core's own 0).
#[allow(clippy::too_many_arguments)]
pub fn face_list(core: &(String, u16), others: &[(String, Vec<u16>)], first: usize, rows: usize, current: usize, total: usize, w: f32, h: f32) -> Stage {
    let (wf, hf) = (w as f64, h as f64);
    let mut svg = String::new();
    let _ = write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wf}" height="{hf}" viewBox="0 0 {wf} {hf}">"#);
    let _ = write!(svg, r##"<path d="{}" fill="#2c2c2c" stroke="#3a3a3a" stroke-width="1"/>"##, rounded(0.5, 0.5, wf - 0.5, hf - 0.5, 12.0));
    let row = |svg: &mut String, y: f64, family: &str, weight: u16, on: bool| {
        if on {
            let _ = write!(svg, r##"<path d="{}" fill="#3a3a3a"/>"##, rounded(8.0, y, wf - 8.0, y + 34.0, 7.0));
        }
        let _ = write!(svg, r##"<text x="20" y="{:.1}" font-family="'{}'" font-weight="{weight}" font-size="15" fill="{}">{}</text>"##, y + 22.5, xml(family), if on { "#ffffff" } else { "#d4d4d4" }, xml(family));
        let _ = write!(svg, r##"<text x="{:.1}" y="{:.1}" font-family="'{}'" font-weight="{weight}" font-size="13" fill="{AMBER}" text-anchor="end" opacity="0.9">ESC  Q  2 @  ENTER</text>"##, wf - 20.0, y + 22.0, xml(family));
    };
    // (the box to find one by name above them, the page's own)
    let mut words = vec![Word { x: 20.0, y: 58.0, text: "The core's own".into(), colour: QUIET_WORD }];
    row(&mut svg, 66.0, &core.0, core.1, current == 0);
    words.push(Word { x: 20.0, y: 124.0, text: if others.len() == total { format!("On this computer, {total}") } else { format!("On this computer, {} found", others.len()) }, colour: QUIET_WORD });
    for (i, (family, weights)) in others.iter().skip(first).take(rows).enumerate() {
        let weight = weights.iter().copied().min_by_key(|w| (*w as i32 - 600).abs()).unwrap_or(400);
        row(&mut svg, 132.0 + i as f64 * 34.0, family, weight, current == first + i + 1);
    }
    svg += "</svg>";
    Stage { w, h, svg, words, extras: Vec::new() }
}

/// A stage's pixels with its words set in the typefaces they name (the
/// typefaces' list).
pub fn pixels_set(st: &Stage, scale: f32) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    use resvg::tiny_skia::{Pixmap, Transform};
    let (pw, ph) = (((st.w * scale).round() as u32).max(1), ((st.h * scale).round() as u32).max(1));
    let mut pix = Pixmap::new(pw, ph).expect("the stage's size");
    let opt = resvg::usvg::Options { fontdb: ectodeck_draw::print::faces(), ..Default::default() };
    if let Ok(tree) = resvg::usvg::Tree::from_str(&st.svg, &opt) {
        resvg::render(&tree, Transform::from_scale(pw as f32 / st.w, ph as f32 / st.h), &mut pix.as_mut());
    }
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(pw, ph);
    buf.make_mut_bytes().copy_from_slice(pix.data());
    buf
}

/// Where the switch pad has the key: its cap's middle on the pad, and the
/// pad's pixels to the millimetre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadMap {
    pub x: f32,
    pub y: f32,
    pub k: f32,
}

/// One key from above, for moving its switch: its cap's foot and face as
/// big as the art has them (`size`, mm across and north to south), a mark
/// at its middle, its switch's housing where it sits and its LED north of
/// the housing's centre, north up; the housing white while it is moved.
pub fn from_above(v: &Values, size: (f64, f64), w: f32, h: f32, hot: bool) -> (Stage, PadMap) {
    let (wf, hf) = (w as f64, h as f64);
    let (fw, fd) = (size.0.max(1.0), size.1.max(1.0));
    // room round the key, and above it for the word that says which way north is
    let (side, above, below) = (16.0, 24.0, 12.0);
    let k = ((wf - 2.0 * side) / fw).min((hf - above - below) / fd);
    let (cx, cy) = (wf / 2.0, above + (hf - above - below) / 2.0);
    let (x0, y0, x1, y1) = (cx - fw * k / 2.0, cy - fd * k / 2.0, cx + fw * k / 2.0, cy + fd * k / 2.0);
    let mut svg = String::new();
    let _ = write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wf}" height="{hf}" viewBox="0 0 {wf} {hf}">"#);
    let _ = write!(svg, r#"<path d="{}" fill="{PLOT}"/>"#, rounded(0.0, 0.0, wf, hf, 8.0));
    // the cap: its foot, and its face inside it
    let r = (1.6 * k).min(6.0);
    let _ = write!(svg, r##"<path d="{}" fill="#262626" stroke="#5c5c5c" stroke-width="1"/>"##, rounded(x0, y0, x1, y1, r));
    let i = v.g.inset * k;
    let _ = write!(svg, r##"<path d="{}" fill="none" stroke="#3a3a3a" stroke-width="1"/>"##, rounded(x0 + i, y0 + i, x1 - i, y1 - i, (r - 1.0).max(1.0)));
    // its middle
    let _ = write!(svg, r##"<path d="M{:.1} {cy:.1} H{:.1} M{cx:.1} {:.1} V{:.1}" stroke="#6f6f6f" stroke-width="1"/>"##, cx - 5.0, cx + 5.0, cy - 5.0, cy + 5.0);
    // the switch where it sits, a line to it from the middle once it has moved, and its LED
    let (sx, sy) = (cx + v.place.0 * k, cy - v.place.1 * k);
    if v.place.0.abs() + v.place.1.abs() > 0.05 {
        let _ = write!(svg, r#"<path d="M{cx:.1} {cy:.1} L{sx:.1} {sy:.1}" stroke="{DIMENSION}" stroke-width="1" stroke-dasharray="3 3"/>"#);
    }
    let half = v.switch_width / 2.0 * k;
    let _ = write!(svg, r##"<path d="{}" fill="#1f1f1f" fill-opacity="0.9" stroke="{}" stroke-width="{}"/>"##, rounded(sx - half, sy - half, sx + half, sy + half, (1.2 * k).min(4.0)), if hot { WHITE } else { "#a3a3a3" }, if hot { 1.5 } else { 1.2 });
    let _ = write!(svg, r#"<circle cx="{sx:.1}" cy="{:.1}" r="3.2" fill="{AMBER}"/>"#, sy - v.north * k);
    svg += "</svg>";
    let words = vec![Word { x: 12.0, y: 16.0, text: "North".into(), colour: QUIET_WORD }];
    (Stage { w, h, svg, words, extras: Vec::new() }, PadMap { x: cx as f32, y: cy as f32, k: k as f32 })
}

/// A stage's pixels at the window's scale, as the lab shows it.
pub fn pixels(st: &Stage, scale: f32) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    use resvg::tiny_skia::{Pixmap, Transform};
    let (pw, ph) = (((st.w * scale).round() as u32).max(1), ((st.h * scale).round() as u32).max(1));
    let mut pix = Pixmap::new(pw, ph).expect("the stage's size");
    if let Ok(tree) = resvg::usvg::Tree::from_str(&st.svg, &resvg::usvg::Options::default()) {
        resvg::render(&tree, Transform::from_scale(pw as f32 / st.w, ph as f32 / st.h), &mut pix.as_mut());
    }
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(pw, ph);
    buf.make_mut_bytes().copy_from_slice(pix.data());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_light() -> Value {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10/device.json")).unwrap();
        ectodeck_draw::jsonc::parse(&text).unwrap()["light"].clone()
    }

    /// Every section's stage reads as an SVG with a line to each of its
    /// sliders, at the lab's smallest and usual sizes and at values past the
    /// sliders' usual ranges, its words inside it.
    #[test]
    fn the_stages_draw() {
        let light = core_light();
        for ((a, b), x) in [(("caps", "top"), 25.0), (("caps", "foot"), 24.0), (("wells", "depth"), 12.0), (("wells", "depth"), 0.0), (("switches", "top"), 30.0), (("switches", "width"), 1.0), (("switches", "beam"), 1.0)] {
            for (w, h) in [(1032.0, 452.0), (1232.0, 572.0), (1800.0, 900.0)] {
                let mut l = light.clone();
                l[a][b] = serde_json::json!(x);
                let v = Values::of(&l, Geometry::default());
                for section in [0, 1, 3, 4] {
                    let lens = Lens::of(section);
                    let places = places(&v, lens, w, h, super::super::SECTIONS[section].3.len(), 0.0, None, None);
                    let ids = &super::super::sliders_of(section, &l);
                    assert_eq!(places.len(), ids.len(), "{lens:?}: a place for each of its sliders");
                    assert!(ids.iter().all(|i| places.iter().any(|p| p.i == *i)), "{lens:?}: its own sliders");
                    for p in &places {
                        assert!(p.y - ATTACH >= 0.0 && p.y - ATTACH + ctrl_h(p.i) <= h, "{lens:?}: slider {} inside the stage at {}", p.i, p.y);
                    }
                    let st = stage(&v, lens, w, h, &places, Some(ids[0]), None);
                    resvg::usvg::Tree::from_str(&st.svg, &resvg::usvg::Options::default()).expect("a stage reads as an SVG");
                    for word in &st.words {
                        assert!(word.x >= 0.0 && word.x + text::width(400, 11.5, &word.text) <= w + 0.5 && word.y <= h, "{} inside the stage", word.text);
                    }
                }
            }
        }
    }

    /// A section's sliders stand where it opened them however its values move.
    #[test]
    fn the_sliders_stand_still() {
        let light = core_light();
        let v = Values::of(&light, Geometry::default());
        let places = places(&v, Lens::Led, 1232.0, 572.0, 2, 0.0, None, None);
        let mut moved = light.clone();
        moved["switches"]["top"] = serde_json::json!(9.5);
        let st = stage(&Values::of(&moved, Geometry::default()), Lens::Led, 1232.0, 572.0, &places, None, None);
        let lines = st.svg.matches(r##"stroke="#737373""##).count();
        assert_eq!(lines, places.len(), "a line from each slider, from where it stood");
    }
}
