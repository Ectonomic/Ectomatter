//! The Lighting lab's legends: under each section's description, a drawing
//! of what its sliders change, to scale from the light's values as the
//! simulation holds them, drawn again whenever one changes.
//!
//! LED, Keycaps and Wells share one slice through the number row's 2 key,
//! north to south, seen from the side (north on the left): the F-row's well,
//! the strip of chassis between the wells, the main well's wall, the 2 key in
//! its well, and the W key below it. Each section frames its own part of the
//! slice and draws its own parts bright; the LED's shows the switch with the
//! cap's skirt cut away. The display's is a close-up of its dots. Labels are
//! the sliders: each its slider's name over its value, in the margin, its
//! leader ending on a dot on what it names.
//!
//! A drawing is an SVG, drawn by resvg, since the window's renderer fills a
//! path with a plain colour only; its words are the lab's own text, over it.

use std::fmt::Write;

use ectodeck_draw::hdr::sim::{on_cap_top, Physics};
use serde_json::Value;

use super::{get, says, PARAMS, UNITS_PER_MM as MM};
use crate::text;

// the lab's own colours
const PLOT: &str = "#1b1b1b";
const DIM: &str = "#d4d4d4";
const AMBER: &str = "#f0b35a";
/// a label's leader, over a dark casing
const LEAD: &str = "#737373";
/// a measurement: lighter, with ticks at its ends
const DIMENSION: &str = "#b4b4b4";
/// under every leader and measurement, so they read over the light
const CASING: &str = "#141414";

// the art's shapes, in millimetres: a 1u cap's foot, its face's inset from
// the foot, the deck between the main well's north wall and the 2 key, the
// gap between rows, the chassis between the F-row's well and the main well,
// the F-row's deck south of F1, and the plate under the deck
const FOOT: f64 = 17.39;
const INSET: f64 = 1.63;
const DECK_N: f64 = 1.62;
const ROW_GAP: f64 = 1.45;
const STRIP: f64 = 2.55;
const F_DECK: f64 = 2.36;
const PLATE: f64 = 1.5;
/// the 2 key's legend: its middle north of the key's, and across
const LEGEND_AT: f64 = 5.25;
const LEGEND_W: f64 = 3.5;
/// the heights each drawing keeps its scale up to (a cap's top, a well's
/// depth); past them it shrinks to fit
const USUAL_TOP: f64 = 16.0;
const USUAL_DEPTH: f64 = 3.0;

/// the margins the labels sit in, and the widest a legend's drawing and its
/// labels spread, centred in a wider legend
const GUTTER: f64 = 156.0;
const WIDEST: f64 = 672.0;
/// a slice's height, and the display's
pub const SLICE_H: f32 = 300.0;
pub const DISPLAY_H: f32 = 232.0;

/// How a word on a legend looks: a slider's name, its value, what follows
/// the value, a slice's ends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Look {
    Name,
    Value,
    Extra,
    Axis,
}

impl Look {
    /// its weight and size, as the lab sets it
    pub fn face(self) -> (u16, f32) {
        match self {
            Look::Value => (500, 12.5),
            Look::Axis => (400, 11.5),
            _ => (400, 12.5),
        }
    }
}

/// A word on a legend, `x` where it starts and `y` its baseline, in the legend's pixels.
#[derive(Clone, PartialEq, Debug)]
pub struct Word {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub look: Look,
}

/// A legend: its size, its drawing, and its words.
#[derive(Clone, PartialEq)]
pub struct Drawing {
    pub w: f32,
    pub h: f32,
    pub svg: String,
    pub words: Vec<Word>,
}

/// Which part of the slice a legend frames.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lens {
    Led,
    Caps,
    Wells,
}

/// The values the legends are drawn from, as the simulation holds them (no
/// switch or skirt's foot above the cap's top, no switch wider than fits in
/// a cap, no LED outside its switch): lengths in millimetres, the LED's
/// place in the art's units too.
#[derive(Clone, Copy, Debug)]
pub struct Values {
    led: f64,
    north: f64,
    nits: f64,
    beam: f64,
    scatter: f64,
    sides: f64,
    spread: f64,
    switch_top: f64,
    switch_width: f64,
    foot: f64,
    top: f64,
    legends: f64,
    cap_in: f64,
    cap_out: f64,
    depth: f64,
    deck: f64,
    walls: f64,
    chassis: f64,
    unlit: f64,
    lcd_lit: f64,
    lcd_unlit: f64,
    lcd_field: f64,
    lcd_dot: f64,
    lcd_glass: f64,
    /// how brightly the 2 key's legend is lit, as a part of the brightest
    /// place on the back of its cap's top (0 to 1)
    lit: f64,
}

/// A value as the light has it, or what the keyboard takes when it has none.
fn raw(light: &Value, path: (&str, &str)) -> f64 {
    get(light, path).unwrap_or_else(|| PARAMS.iter().find(|p| p.path == path).map_or(0.0, |p| p.def))
}

impl Values {
    pub fn of(light: &Value) -> Values {
        let p = Physics::from(light);
        // the LED inside its switch, as the simulation holds it
        let edge = p.switch_edge() as f64;
        let led = raw(light, ("led", "")).clamp(-edge, edge);
        Values {
            led,
            north: led / MM,
            nits: p.nits as f64,
            beam: p.beam as f64,
            scatter: p.scatter as f64,
            sides: p.sides as f64,
            spread: p.glow_spread as f64,
            switch_top: p.switch_top as f64,
            switch_width: p.switch_width as f64,
            foot: p.gap as f64,
            top: p.top as f64,
            legends: p.legends as f64,
            cap_in: p.cap_in as f64,
            cap_out: p.cap_out as f64,
            depth: p.wall as f64,
            deck: p.deck as f64,
            walls: p.walls as f64,
            chassis: p.chassis as f64,
            unlit: raw(light, ("legend", "unlit")),
            lcd_lit: raw(light, ("lcd", "nits")),
            lcd_unlit: raw(light, ("lcd", "unlit")),
            lcd_field: raw(light, ("lcd", "field")),
            lcd_dot: raw(light, ("lcd", "dot")),
            lcd_glass: raw(light, ("lcd", "glass")),
            lit: legend_lit(&p, led as f32),
        }
    }
}

/// How brightly the 2 key's legend is lit, as a part of the brightest place
/// on the back of its cap's top: the light straight from its LED and the
/// switch's housing, as the lab's curve has it.
fn legend_lit(p: &Physics, led: f32) -> f64 {
    // a place `x` mm along the slice from the key's middle, north below 0
    let at = |x: f64| on_cap_top(p, led, (x * MM) as f32 + led) as f64;
    let most = (0..=160).map(|i| at(-8.0 + 16.0 * i as f64 / 160.0)).fold(0.0, f64::max);
    if most > 0.0 { at(-LEGEND_AT) / most } else { 0.0 }
}

/// The beam's half angle, out to half its brightness, in degrees.
fn beam_half(beam: f64) -> f64 {
    0.5f64.powf(1.0 / beam.max(0.01)).acos().to_degrees()
}

/// A slider's name, and its value as its box and unit read it ("6.5 mm",
/// "25%"), what follows the value apart.
type Named = (&'static str, String, String);

fn named(path: (&str, &str), v: f64) -> Named {
    let i = PARAMS.iter().position(|p| p.path == path).expect("one of the lab's sliders");
    let (value, extra) = says(i, v);
    (PARAMS[i].label, value, extra)
}

// ---------------------------------------------------------------- drawing

type Pt = (f64, f64);

struct Svg {
    w: f64,
    h: f64,
    /// where the drawing and its labels start across the legend
    ox: f64,
    defs: String,
    body: String,
    n: usize,
    words: Vec<Word>,
    /// whether the legend's ground is drawn under it all (a slice draws its own)
    ground: bool,
}

impl Svg {
    fn new(w: f64, h: f64, ox: f64) -> Svg {
        Svg { w, h, ox, defs: String::new(), body: String::new(), n: 0, words: Vec::new(), ground: true }
    }

    fn gid(&mut self) -> String {
        self.n += 1;
        format!("g{}", self.n)
    }

    /// a path, as faint as `opacity` (its fill's and its stroke's, so it
    /// needs no layer of its own)
    fn path(&mut self, d: &str, fill: &str, stroke: &str, sw: f64, opacity: f64) {
        if !d.is_empty() {
            let _ = write!(self.body, r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" fill-opacity="{opacity:.3}" stroke-opacity="{opacity:.3}" stroke-linejoin="round" stroke-linecap="round"/>"#);
        }
    }

    fn fill(&mut self, d: &str, fill: &str) {
        self.path(d, fill, "none", 1.0, 1.0);
    }

    /// a leader or a measurement: a dark casing under it, so it reads over anything
    fn line(&mut self, d: &str, color: &str) {
        self.path(d, "none", CASING, 3.4, 1.0);
        self.path(d, "none", color, 1.0, 1.0);
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
        self.fill(&format!("M{:.2} {y:.2} a{r} {r} 0 1 0 {} 0 a{r} {r} 0 1 0 {} 0", x - r, 2.0 * r, -2.0 * r), fill);
    }

    /// a word, its baseline at `y`, starting at `x` (`anchor` 0), centred on it (0.5) or ending at it (1)
    fn text(&mut self, x: f64, y: f64, s: &str, look: Look, anchor: f64) -> f64 {
        let (weight, size) = look.face();
        let w = text::width(weight, size, s) as f64;
        let x = x - w * anchor;
        self.words.push(Word { x: (self.ox + x) as f32, y: y as f32, text: s.to_string(), look });
        w
    }

    fn done(self) -> Drawing {
        let (w, h) = (self.w, self.h);
        let ground = if self.ground { format!(r#"<rect width="{w}" height="{h}" rx="8" fill="{PLOT}"/>"#) } else { String::new() };
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs>{}</defs>{ground}<g transform="translate({:.1} 0)">{}</g></svg>"#,
            self.defs, self.ox, self.body
        );
        Drawing { w: w as f32, h: h as f32, svg, words: self.words }
    }
}

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

/// A path the light takes: a line through the points, a head at its end.
fn arrow(s: &mut Svg, pts: &[Pt], head: f64) {
    s.path(&poly(pts, false), "none", AMBER, 1.3, 0.9);
    let ((x0, y0), (x1, y1)) = (pts[pts.len() - 2], pts[pts.len() - 1]);
    let a = (y1 - y0).atan2(x1 - x0);
    let l = (x1 - head * (a - 0.42).cos(), y1 - head * (a - 0.42).sin());
    let r = (x1 - head * (a + 0.42).cos(), y1 - head * (a + 0.42).sin());
    s.path(&poly(&[l, (x1, y1), r], true), AMBER, "none", 1.0, 0.9);
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

/// A slider's name over its value, in the margin on `side`, its leader
/// ending on a dot at `at`. For something flat (a surface, a line across)
/// the label sits above it (`flat`), so its leader meets it at an angle, or
/// (`drop`) the leader runs across above everything and drops straight onto
/// it. `under` is a point the leader must pass below (a skirt's foot).
struct Label {
    side: Side,
    at: Pt,
    name: &'static str,
    value: String,
    extra: String,
    flat: bool,
    drop: bool,
    under: Option<Pt>,
}

impl Label {
    fn new(side: Side, at: Pt, (name, value, extra): Named) -> Label {
        Label { side, at, name, value, extra, flat: false, drop: false, under: None }
    }
}

/// The labels on each side in the order of what they name, top to bottom
/// (of two at one height, the further from the margin first), so no two
/// leaders cross; each level with what it names where its neighbours let it.
fn callouts(s: &mut Svg, mut labels: Vec<Label>, w: f64, h: f64) {
    let (top, bottom) = (14.0, h - 14.0);
    let (pitch, rise) = (36.0f64, 40.0);
    for side in [Side::Left, Side::Right] {
        let edge = if side == Side::Left { GUTTER - 8.0 } else { w - GUTTER + 8.0 };
        let key = |l: &Label| (l.at.1 - if l.drop { 2000.0 } else { 0.0 }, -(l.at.0 - edge).abs());
        let mut mine: Vec<Label> = Vec::new();
        let mut rest = Vec::new();
        for l in labels.drain(..) {
            if l.side == side { mine.push(l) } else { rest.push(l) }
        }
        labels = rest;
        if mine.is_empty() {
            continue;
        }
        mine.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
        let want: Vec<f64> = mine
            .iter()
            .map(|l| {
                let mut y = l.at.1 - 5.0;
                if l.drop {
                    y = (y - rise).min(top + 12.0);
                } else if l.flat {
                    y -= rise;
                }
                if let Some((ux, uy)) = l.under {
                    // low enough at the skirt's foot to pass under it
                    let t = (ux - edge) / if l.at.0 == edge { 1e-9 } else { l.at.0 - edge };
                    if 0.0 < t && t < 1.0 {
                        y = y.max((uy + 4.0 - l.at.1 * t) / (1.0 - t) - 4.0);
                    }
                }
                y
            })
            .collect();
        // in order, a pitch apart, inside the box: pushed down from the top,
        // then up from the bottom; closer together only if they can't fit
        let (lo, hi) = (top + 12.0, bottom - 18.0);
        let gap = pitch.min((hi - lo) / (want.len().max(2) - 1) as f64);
        let mut ys: Vec<f64> = Vec::new();
        for y in want {
            let y = y.clamp(lo, hi);
            ys.push(ys.last().map_or(y, |&last: &f64| y.max(last + gap)));
        }
        let n = ys.len();
        for i in (0..n).rev() {
            ys[i] = ys[i].min(hi - gap * (n - 1 - i) as f64);
            if i + 1 < n {
                ys[i] = ys[i].min(ys[i + 1] - gap);
            }
        }
        for (l, y) in mine.iter().zip(ys) {
            let (ax, ay) = l.at;
            let space = text::width(500, 12.5, " ") as f64;
            let start = if side == Side::Left {
                s.text(16.0, y, l.name, Look::Name, 0.0);
                let vw = s.text(16.0, y + 16.0, &l.value, Look::Value, 0.0);
                if !l.extra.is_empty() {
                    s.text(16.0 + vw + space, y + 16.0, &l.extra, Look::Extra, 0.0);
                }
                (GUTTER - 22.0, y + 4.0)
            } else {
                s.text(w - 16.0, y, l.name, Look::Name, 1.0);
                let mut right = w - 16.0;
                if !l.extra.is_empty() {
                    right -= s.text(right, y + 16.0, &l.extra, Look::Extra, 1.0) + space;
                }
                s.text(right, y + 16.0, &l.value, Look::Value, 1.0);
                (w - GUTTER + 22.0, y + 4.0)
            };
            let route = if l.drop { vec![start, (edge, y + 4.0), (ax, y + 4.0), (ax, ay)] } else { vec![start, (edge, y + 4.0), (ax, ay)] };
            s.line(&poly(&route, false), LEAD);
            s.dot(ax, ay, 2.3, DIM);
        }
    }
}

/// The slice, in millimetres: x north to south from the main well's north
/// wall, z up from the deck.
struct Slice {
    v: Values,
    foot0: f64,
    foot1: f64,
    centre: f64,
    /// the W key's foot
    w0: f64,
    led: f64,
    /// the drawing's cap walls and top, thinner where the cap is low
    wall_t: f64,
    top_t: f64,
    hollow: bool,
    /// the switch's housing, and the 2 key's legend, across
    hx0: f64,
    hx1: f64,
    lx0: f64,
    lx1: f64,
}

impl Slice {
    fn of(v: &Values) -> Slice {
        let (foot0, foot1) = (DECK_N, DECK_N + FOOT);
        let centre = (foot0 + foot1) / 2.0;
        let span = v.top - v.foot;
        let mut sl = Slice {
            v: *v,
            foot0,
            foot1,
            centre,
            w0: foot1 + ROW_GAP,
            led: centre - v.north,
            wall_t: (span * 0.25 + 0.2).min(1.15),
            top_t: (span * 0.3).min(1.25),
            hollow: span > 1.6,
            hx0: 0.0,
            hx1: 0.0,
            lx0: centre - LEGEND_AT - LEGEND_W / 2.0,
            lx1: centre - LEGEND_AT + LEGEND_W / 2.0,
        };
        // the switch as wide as asked, as far as it fits in the cap at its
        // top, and its LED inside it as drawn
        let inner = sl.skirt_x(Side::Left, v.switch_top, true);
        let half = if sl.hollow { (v.switch_width / 2.0).min(centre - inner - 0.25) } else { v.switch_width / 2.0 };
        (sl.hx0, sl.hx1) = (centre - half, centre + half);
        sl.led = sl.led.clamp(sl.hx0, sl.hx1);
        sl
    }

    /// where the 2 key's north (left) or south skirt is at height z, its outer or inner face
    fn skirt_x(&self, side: Side, z: f64, inner: bool) -> f64 {
        let v = &self.v;
        let t = ((z - v.foot) / (v.top - v.foot).max(1e-3)).clamp(0.0, 1.0);
        let wall = if inner { self.wall_t } else { 0.0 };
        if side == Side::Left { self.foot0 + INSET * t + wall } else { self.foot1 - INSET * t - wall }
    }

    /// a cap in section: its skirt from its foot up to its face, and its
    /// top; a cap too low to be hollow, a slab
    fn cap(&self, f0: f64, f1: f64) -> Vec<Pt> {
        let v = &self.v;
        let (a0, a1) = (f0 + INSET, f1 - INSET);
        let (fz, tz) = (v.foot, v.top);
        let mut outer = vec![(f0, fz), (a0, tz), (a1, tz), (f1, fz)];
        if !self.hollow {
            return outer;
        }
        let lean = INSET / (tz - fz).max(1e-3);
        let under = tz - self.top_t;
        let t = self.wall_t;
        outer.extend([(f1 - t, fz), (a1 - t + lean * self.top_t, under), (a0 + t - lean * self.top_t, under), (f0 + t, fz)]);
        outer
    }

    /// the space inside the 2 key's cap, as an outline, anticlockwise on the page
    fn interior(&self) -> Vec<Pt> {
        let v = &self.v;
        let under = v.top - self.top_t;
        vec![
            (self.skirt_x(Side::Left, v.foot, true), v.foot - 50.0),
            (self.skirt_x(Side::Right, v.foot, true), v.foot - 50.0),
            (self.skirt_x(Side::Right, under, true), under),
            (self.skirt_x(Side::Left, under, true), under),
        ]
    }

    /// the F-row's deck, its wall, the chassis between the wells, the main
    /// well's wall and its deck, as one outline
    fn body(&self, x_lo: f64, x_hi: f64) -> Vec<Pt> {
        let d = self.v.depth;
        vec![(x_lo, -PLATE), (x_lo, 0.0), (-STRIP, 0.0), (-STRIP, d), (0.0, d), (0.0, 0.0), (x_hi, 0.0), (x_hi, -PLATE)]
    }
}

/// A measurement, in the legend's pixels: a height, or a length across
/// (`more` when it goes on past where it is drawn).
enum Mark {
    Height { x: f64, y0: f64, y1: f64 },
    Across { x0: f64, x1: f64, y: f64, more: bool },
}

/// The slice, framed and lit for one section, `w` by `h` pixels.
pub fn slice(v: &Values, lens: Lens, w: f32, h: f32) -> Drawing {
    let sl = Slice::of(v);
    let (full, h) = (w as f64, h as f64);
    let w = full.min(WIDEST);
    let mut s = Svg::new(full, h, ((full - w) / 2.0).round());
    let (d, hz, tz) = (v.depth, v.switch_top, v.top);
    let ceiling = v.top.max(USUAL_TOP).max(v.switch_top + 0.5) + 2.4;
    let (x_lo, x_hi, z_lo, z_hi) = match lens {
        Lens::Led => (sl.foot0 - 1.0, sl.foot1 + 1.0, -0.9, ceiling),
        Lens::Caps => (sl.foot0 - 2.0, sl.foot1 + 1.3, -0.6, ceiling),
        Lens::Wells => (-STRIP - F_DECK - 0.3, sl.foot0 - 0.3, -0.8, v.depth.max(USUAL_DEPTH) + 1.6),
    };
    let (room_w, room_h) = (w - 2.0 * GUTTER, h - 34.0);
    let k = (room_w / (x_hi - x_lo)).min(room_h / (z_hi - z_lo));
    let left = GUTTER + (room_w - k * (x_hi - x_lo)) / 2.0;
    let floor = h - 22.0 - (room_h - k * (z_hi - z_lo)) / 2.0;
    let px = move |x: f64| left + k * (x - x_lo);
    let pz = move |z: f64| floor - k * (z - z_lo);
    let p = move |x: f64, z: f64| (px(x), pz(z));
    let pp = move |pts: &[Pt]| pts.iter().map(|&(x, z)| p(x, z)).collect::<Vec<Pt>>();
    let on = |part: Lens| part == lens;
    let line = |part: Lens| if on(part) { "#b0b0b0" } else { "#454545" };
    let fill = |part: Lens| if on(part) { "#3a3a3a" } else { "#262626" };
    let glow = if on(Lens::Led) { 1.0 } else { 0.55 };
    // the frame, on whole pixels: its ground under the slice, and the
    // ground round it drawn over the slice at the end, so nothing of the
    // slice shows past it (cheaper to draw than a clip); a glow is cut to it
    s.ground = false;
    let (fx0, fx1, fy0, fy1) = (px(x_lo).round(), px(x_hi).round(), pz(z_hi).round(), pz(z_lo).round());
    s.fill(&format!("M{fx0} {fy0} H{fx1} V{fy1} H{fx0} Z"), PLOT);
    let framed = |pts: &[Pt]| clip_to(pts, &[(x_lo, z_lo), (x_hi, z_lo), (x_hi, z_hi), (x_lo, z_hi)]);

    // the body: the decks, the walls and the chassis between the wells
    s.path(&poly(&pp(&clip(&sl.body(x_lo - 1.0, x_hi + 1.0), x_lo, x_hi)), true), fill(Lens::Wells), line(Lens::Wells), 1.0, 1.0);
    // the housing, glowing with the LED's scattered light: brightest by the LED, a third every glow spread
    let stops: Vec<(f64, &str, f64)> = (0..17)
        .map(|i| {
            let x = sl.hx0 + (sl.hx1 - sl.hx0) * i as f64 / 16.0;
            let a = (-(x - sl.led).abs() / v.spread.max(0.05)).exp();
            (i as f64 / 16.0, AMBER, (0.07 + 0.38 * a) * glow)
        })
        .collect();
    let housing = s.linear(px(sl.hx0), 0.0, px(sl.hx1), 0.0, &stops);
    s.path(&poly(&pp(&clip(&[(sl.hx0, 0.0), (sl.hx0, hz), (sl.hx1, hz), (sl.hx1, 0.0)], x_lo, x_hi)), true), &housing, if on(Lens::Led) { "#b98a48" } else { "#5a4b36" }, 1.0, 1.0);
    // the beam, from the switch's top at the LED to the back of the cap's top,
    // out to half its brightness, inside the cap
    let rise = tz - sl.top_t - hz;
    let wide = beam_half(v.beam).to_radians().min(1.35).tan() * rise.max(0.0);
    if rise > 0.3 {
        let mut cone = vec![(sl.led, hz), (sl.led + wide, hz + rise), (sl.led - wide, hz + rise)];
        if sl.hollow && !on(Lens::Led) {
            cone = clip_to(&cone, &sl.interior());
        }
        let beam = s.radial(px(sl.led), pz(hz), k * rise * 1.1, &[(0.0, AMBER, 0.7 * glow), (1.0, AMBER, 0.06 * glow)]);
        s.fill(&poly(&pp(&framed(&cone)), true), &beam);
    }
    if x_lo < sl.led && sl.led < x_hi && rise > 0.4 {
        s.dot(px(sl.led), pz(hz) + 1.5, 3.4, AMBER);
    }
    // the deck lit out under the skirts, either side of the key
    for (x0, x1) in [(x_lo.max(0.0), sl.foot0 + 0.6), (sl.foot1 - 0.6, (sl.w0 + 0.6).min(x_hi))] {
        if x1 > x0 {
            let g = s.linear(px(x0), 0.0, px(x1), 0.0, &[(0.0, AMBER, 0.15), (0.55, AMBER, 0.6), (1.0, AMBER, 0.1)]);
            s.path(&poly(&pp(&framed(&[(x0, 0.0), (x1, 0.0), (x1, 0.2), (x0, 0.2)])), true), &g, "none", 1.0, if on(Lens::Led) { 0.9 } else { 0.5 });
        }
    }
    // the caps over it: the 2 key, and the W key below it; in the LED's
    // drawing only the 2 key's top, its skirt cut away to show the switch
    let (a0, a1) = (sl.foot0 + INSET, sl.foot1 - INSET);
    let under = tz - sl.top_t;
    if on(Lens::Led) {
        s.path(&poly(&[p(a0, tz), p(a1, tz), p(a1, under), p(a0, under)], true), fill(Lens::Caps), line(Lens::Caps), 1.0, 1.0);
        for x in [a0, a1] {
            let g = s.linear(0.0, pz(under), 0.0, pz(under) + 22.0, &[(0.0, "#454545", 0.8), (1.0, "#454545", 0.0)]);
            s.path(&poly(&[p(x, under), (px(x), pz(under) + 22.0)], false), "none", &g, 1.0, 1.0);
        }
    } else {
        for (f0, f1) in [(sl.foot0, sl.foot1), (sl.w0, sl.w0 + FOOT)] {
            let shape = clip(&sl.cap(f0, f1), x_lo, x_hi);
            if !shape.is_empty() {
                s.path(&poly(&pp(&shape), true), fill(Lens::Caps), line(Lens::Caps), 1.0, 1.0);
            }
        }
    }
    // the 2 key's legend, lit through its top as brightly as the light behind it lets
    let bright = v.lit;
    s.path(&poly(&pp(&framed(&[(sl.lx0, under), (sl.lx1, under), (sl.lx1, tz), (sl.lx0, tz)])), true), AMBER, "none", 1.0, 0.2 + 0.78 * bright);
    let halo = s.radial(px((sl.lx0 + sl.lx1) / 2.0), pz(tz), k * 2.4, &[(0.0, AMBER, 0.5 * bright), (1.0, AMBER, 0.0)]);
    s.fill(&poly(&pp(&framed(&[(sl.lx0 - 2.2, tz), (sl.lx1 + 2.2, tz), (sl.lx1 + 2.2, tz + 1.8), (sl.lx0 - 2.2, tz + 1.8)])), true), &halo);

    let mut labels: Vec<Label> = Vec::new();
    // measurements, drawn over the light
    let mut marks: Vec<Mark> = Vec::new();
    // a height: its line between ticks, the label level with a point on it
    let height = |marks: &mut Vec<Mark>, labels: &mut Vec<Label>, x: f64, (z0, z1): Pt, side: Side, n: Named, at: f64, under: Option<Pt>| {
        marks.push(Mark::Height { x: px(x), y0: pz(z0), y1: pz(z1) });
        labels.push(Label { under, ..Label::new(side, p(x, z0 + (z1 - z0) * at), n) });
    };
    // a length across, its label above it; `more` when it goes on past where it is drawn
    let across = |marks: &mut Vec<Mark>, labels: &mut Vec<Label>, (x0, x1): Pt, z: f64, side: Side, n: Named, more: bool| {
        marks.push(Mark::Across { x0: px(x0), x1: px(x1), y: pz(z), more });
        labels.push(Label { flat: true, ..Label::new(side, ((px(x0) + px(x1)) / 2.0, pz(z)), n) });
    };
    let skirt_s = p(sl.foot1, v.foot);
    match lens {
        Lens::Led => {
            // the switch's centre line, for where the LED sits from it
            s.path(&poly(&[p(sl.centre, 0.15), p(sl.centre, hz + 0.6)], false), "none", "#5c5c5c", 1.0, 1.0);
            let inside = hz > 2.6;
            across(&mut marks, &mut labels, (sl.led, sl.centre), if inside { hz - 0.6 } else { hz + 0.7 }, Side::Left, named(("led", ""), v.led), false);
            labels.push(Label { drop: true, ..Label::new(Side::Left, p((sl.lx0 + sl.lx1) / 2.0 - 0.6, tz), named(("sim", "nits"), v.nits)) });
            if rise > 0.3 {
                let (name, value, _) = named(("sim", "beam"), v.beam);
                labels.push(Label::new(Side::Left, p(sl.led - wide * 0.55, hz + rise * 0.55), (name, value, format!("(±{:.0}°)", beam_half(v.beam)))));
            }
            let far = (sl.hx1 - sl.led).max(sl.led - sl.hx0);
            let toward = if sl.hx1 - sl.led >= sl.led - sl.hx0 { 1.0 } else { -1.0 };
            let reach = v.spread.min(far - 0.3);
            across(&mut marks, &mut labels, (sl.led, sl.led + toward * reach), if inside { (hz * 0.42).max(0.3) } else { hz + 1.4 }, Side::Left, named(("sim", "glow_spread"), v.spread), v.spread > far - 0.3);
            across(&mut marks, &mut labels, (sl.hx0, sl.hx1), if inside { 0.45f64.min(hz * 0.2) } else { hz + 2.1 }, Side::Left, named(("sim", "switch_width"), v.switch_width), false);
            height(&mut marks, &mut labels, sl.hx1 + 0.8, (0.0, hz), Side::Right, named(("sim", "switch_top"), v.switch_top), 0.5, None);
            // the housing's glow, out of its top (where there is room under the cap's top) and its sides
            if rise > 1.6 {
                let up = 2.4f64.min(rise - 0.5);
                for x in [sl.hx0 + 1.1, sl.centre + 2.4, sl.hx1 - 1.0] {
                    if (x - sl.led).abs() > wide * 0.5 + 0.9 {
                        arrow(&mut s, &[p(x, hz + 0.25), p(x + 0.1, hz + up)], 5.5);
                    }
                }
            }
            labels.push(Label::new(Side::Right, p(sl.hx1 - 0.25, hz - 0.25), named(("sim", "scatter"), v.scatter)));
            let out = 1.9f64.min(v.foot.max(0.6) * 0.45);
            arrow(&mut s, &[p(sl.hx1 + 0.15, out), p(sl.foot1 + 0.6, out * 0.25)], 5.5);
            arrow(&mut s, &[p(sl.hx0 - 0.15, out), p(sl.foot0 - 0.6, out * 0.25)], 5.5);
            labels.push(Label::new(Side::Right, p(sl.foot1 + 0.35, out * 0.32), named(("sim", "sides"), v.sides)));
        }
        Lens::Caps => {
            height(&mut marks, &mut labels, sl.foot0 - 1.3, (0.0, v.foot), Side::Left, named(("sim", "skirt_foot"), v.foot), 0.5, None);
            height(&mut marks, &mut labels, sl.foot0 - 0.55, (0.0, tz), Side::Left, named(("sim", "cap_top"), tz), 0.62, None);
            labels.push(Label { drop: true, ..Label::new(Side::Left, p(sl.lx0 + 0.9, tz), named(("sim", "legends"), v.legends)) });
            labels.push(Label { drop: true, ..Label::new(Side::Right, p(sl.lx1 - 0.7, tz), named(("legend", "unlit"), v.unlit)) });
            // light out of the switch's side and under the skirt's foot onto the deck
            let out = 1.6f64.min(v.foot.max(0.6) * 0.4);
            arrow(&mut s, &[p(sl.hx1 + 0.15, out), p(sl.foot1 + 0.95, out * 0.15)], 5.5);
            if sl.hollow && rise > 1.0 {
                // bounced inside the cap: off its inner wall, up to the back of its top
                let mid_z = (hz.max(v.foot) + under) / 2.0;
                let inner = sl.skirt_x(Side::Right, mid_z, true);
                arrow(&mut s, &[p(sl.hx1 - 0.6, hz + 0.2), p(inner, mid_z), p(sl.centre + 2.2, under - 0.35)], 5.5);
                let (name, value, _) = named(("sim", "cap_in"), v.cap_in);
                if rise > 3.0 {
                    let (tx, ty) = (px(sl.centre - 1.2), pz(mid_z));
                    s.text(tx, ty - 2.0, name, Look::Name, 0.5);
                    s.text(tx, ty + 14.0, &value, Look::Value, 0.5);
                    s.line(&poly(&[(tx + 26.0, ty + 4.0), p(inner, mid_z)], false), LEAD);
                    let (dx, dy) = p(inner, mid_z);
                    s.dot(dx, dy, 2.3, DIM);
                } else {
                    labels.push(Label { under: Some(skirt_s), ..Label::new(Side::Right, p(inner, mid_z), (name, value, String::new())) });
                }
            }
            // light from the next key: onto the outside of this one's skirt, and off it
            let bounce_z = v.foot + 1.8f64.min((tz - v.foot) * 0.3);
            let outside = sl.skirt_x(Side::Right, bounce_z, false);
            arrow(&mut s, &[p(x_hi - 0.1, v.foot * 0.55), p(outside + 0.08, bounce_z - 0.05)], 5.5);
            arrow(&mut s, &[p(outside + 0.1, bounce_z + 0.15), p(x_hi - 0.25, bounce_z + 1.6)], 5.5);
            labels.push(Label::new(Side::Right, p(outside, bounce_z), named(("sim", "cap_out"), v.cap_out)));
        }
        Lens::Wells => {
            // light from under the keys: off the main well's deck onto its wall,
            // and, where the wall is low enough, over it onto the chassis
            let edge = x_hi + 0.3;
            let wall_z = 1.1f64.min(d * 0.6);
            if d > 0.25 {
                arrow(&mut s, &[p(edge, 1.4f64.min(d + 0.9)), p(0.85, 0.03), p(0.03, wall_z), p(0.9, wall_z + 1.25)], 5.0);
                arrow(&mut s, &[p(edge, d + 1.25), p(-0.7, d + 0.03), p(-1.25, d + 1.0)], 5.0);
                height(&mut marks, &mut labels, -STRIP - 0.85, (0.0, d), Side::Left, named(("sim", "depth"), d), 0.8, None);
                labels.push(Label::new(Side::Right, p(0.0, wall_z.max(d * 0.5)), named(("sim", "walls"), v.walls)));
            } else {
                arrow(&mut s, &[p(edge, 1.2), p(-0.8, 0.03), p(-1.4, 1.0)], 5.0);
                let (name, value, _) = named(("sim", "depth"), d);
                labels.push(Label { flat: true, ..Label::new(Side::Left, p(-STRIP - 0.85, 0.0), (name, value, "(no walls)".into())) });
            }
            labels.push(Label { flat: true, ..Label::new(Side::Left, p(-1.6, d), named(("sim", "chassis"), v.chassis)) });
            labels.push(Label { flat: true, ..Label::new(Side::Left, p(-STRIP - F_DECK * 0.62, 0.0), named(("sim", "deck"), v.deck)) });
        }
    }

    // the slice goes on past the frame: each end fades into the ground, and its top where the frame cuts it
    for (a, b) in [(px(x_lo) - 1.0, px(x_lo) + 26.0), (px(x_hi) + 1.0, px(x_hi) - 26.0)] {
        let g = s.linear(a, 0.0, b, 0.0, &[(0.0, PLOT, 1.0), (1.0, PLOT, 0.0)]);
        s.fill(&poly(&[(a, 0.0), (b, 0.0), (b, h), (a, h)], true), &g);
    }
    if z_hi < tz {
        let g = s.linear(0.0, pz(z_hi) - 1.0, 0.0, pz(z_hi) + 30.0, &[(0.0, PLOT, 1.0), (1.0, PLOT, 0.0)]);
        s.fill(&poly(&[(px(x_lo), pz(z_hi) - 1.0), (px(x_hi), pz(z_hi) - 1.0), (px(x_hi), pz(z_hi) + 30.0), (px(x_lo), pz(z_hi) + 30.0)], true), &g);
    }
    // and nothing of it past the frame: the legend's ground all round it, to its rounded edge
    let o = s.ox;
    let ground = format!("{} M{fx0} {fy0} H{fx1} V{fy1} H{fx0} Z", rounded(-o, 0.0, full - o, h, 8.0));
    let _ = write!(s.body, r#"<path d="{ground}" fill="{PLOT}" fill-rule="evenodd"/>"#);

    // the measurements, over everything else in the drawing
    let tick = 5.0;
    for m in marks {
        match m {
            Mark::Height { x, y0, y1 } => {
                s.line(&poly(&[(x, y0), (x, y1)], false), DIMENSION);
                for y in [y0, y1] {
                    s.line(&poly(&[(x - tick, y), (x + tick, y)], false), DIMENSION);
                }
            }
            Mark::Across { x0, x1, y, more } => {
                s.line(&poly(&[(x0, y), (x1, y)], false), DIMENSION);
                s.line(&poly(&[(x0, y - tick), (x0, y + tick)], false), DIMENSION);
                if more {
                    let d = if x1 > x0 { 1.0 } else { -1.0 };
                    s.fill(&poly(&[(x1 - d * 6.0, y - 4.0), (x1, y), (x1 - d * 6.0, y + 4.0)], true), DIMENSION);
                } else {
                    s.line(&poly(&[(x1, y - tick), (x1, y + tick)], false), DIMENSION);
                }
            }
        }
    }

    callouts(&mut s, labels, w, h);
    s.text(GUTTER - 18.0, h - 9.0, "North", Look::Axis, 0.0);
    s.text(w - GUTTER + 18.0, h - 9.0, "South", Look::Axis, 1.0);
    s.done()
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

/// The display close up: a few of its dots, lit and unlit, the ground
/// between them, the glass over them, `w` by `h` pixels.
pub fn display(v: &Values, w: f32, h: f32) -> Drawing {
    let (full, h) = (w as f64, h as f64);
    let w = full.min(WIDEST);
    let mut s = Svg::new(full, h, ((full - w) / 2.0).round());
    // "2C" as the display draws it: 5 by 7 dots a character, a column apart
    let two = ["01110", "10001", "00001", "00010", "00100", "01000", "11111"];
    let cee = ["01110", "10001", "10000", "10000", "10000", "10001", "01110"];
    let glyph: Vec<Vec<bool>> = two.iter().zip(cee).map(|(a, b)| format!("{a}0{b}").chars().map(|c| c == '1').collect()).collect();
    let (rows, cols) = (glyph.len() as f64, glyph[0].len() as f64);
    // cells of the ground round the dots, as the display has
    let margin = 1.0;
    let cell = ((w - 2.0 * GUTTER - 24.0) / (cols + 2.0 * margin)).min((h - 44.0) / (rows + 2.0 * margin));
    let (fw, fh) = (cell * (cols + 2.0 * margin), cell * (rows + 2.0 * margin));
    let gx0 = GUTTER + ((w - 2.0 * GUTTER) - fw) / 2.0;
    let gy0 = (h - fh) / 2.0;
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
    let lit_at = (x0 + cell + dot * 0.5, y0 + dot * 0.5);
    let unlit_at = (x0 + cell + dot * 0.5, y0 + cell * 3.0 + dot * 0.5);
    let field_at = (gx0 + fw - cell * 0.5, gy0 + cell * 2.5);
    // the glass's sheen: a soft band across, its edges where it fades to nothing
    let (bx0, bx1) = (x0 + cell * 3.2, x0 + cell * 6.2);
    let lean = cell * 2.6;
    let g = s.linear(bx0, 0.0, bx1, 0.0, &[(0.0, "#ffffff", 0.0), (0.5, "#ffffff", 0.16 * v.lcd_glass), (1.0, "#ffffff", 0.0)]);
    s.fill(&poly(&[(bx0 + lean, y0), (bx1 + lean, y0), (bx1, y0 + cell * rows), (bx0, y0 + cell * rows)], true), &g);
    let glass_at = (x0 + cell * 7.0 + dot * 0.5, y0 + cell + dot * 0.5);
    // one cell outlined, and its dot's width across under the dots
    let (cx0, cy0) = (x0 + cell * 9.0, y0 + cell * 6.0);
    s.path(&poly(&[(cx0, cy0), (cx0 + cell, cy0), (cx0 + cell, cy0 + cell), (cx0, cy0 + cell)], true), "none", "#d0d0d0", 1.0, 1.0);
    let yy = y0 + cell * rows + cell * 0.45;
    s.line(&poly(&[(cx0, yy), (cx0 + dot, yy)], false), DIMENSION);
    for x in [cx0, cx0 + dot] {
        s.line(&poly(&[(x, yy - 4.0), (x, yy + 4.0)], false), DIMENSION);
    }
    let labels = vec![
        Label { flat: true, ..Label::new(Side::Left, lit_at, named(("lcd", "nits"), v.lcd_lit)) },
        Label::new(Side::Left, unlit_at, named(("lcd", "unlit"), v.lcd_unlit)),
        Label { flat: true, ..Label::new(Side::Right, glass_at, named(("lcd", "glass"), v.lcd_glass)) },
        Label::new(Side::Right, field_at, named(("lcd", "field"), v.lcd_field)),
        Label::new(Side::Right, (cx0 + dot / 2.0, yy), named(("lcd", "dot"), v.lcd_dot)),
    ];
    callouts(&mut s, labels, w, h);
    s.done()
}

/// A legend drawn at the window's scale, as the lab shows it.
pub fn image(d: &Drawing, scale: f32) -> slint::Image {
    use resvg::tiny_skia::{Pixmap, Transform};
    let (pw, ph) = (((d.w * scale).round() as u32).max(1), ((d.h * scale).round() as u32).max(1));
    let mut pix = Pixmap::new(pw, ph).expect("a legend's size");
    if let Ok(tree) = resvg::usvg::Tree::from_str(&d.svg, &resvg::usvg::Options::default()) {
        resvg::render(&tree, Transform::from_scale(pw as f32 / d.w, ph as f32 / d.h), &mut pix.as_mut());
    }
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(pw, ph);
    buf.make_mut_bytes().copy_from_slice(pix.data());
    slint::Image::from_rgba8_premultiplied(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_light() -> Value {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10/device.json")).unwrap();
        super::super::simulated(serde_json::from_str::<Value>(&text).unwrap()["light"].clone())
    }

    /// Every legend reads as an SVG, its words inside it, at the lab's
    /// narrowest and at its usual width, and at values past the sliders'
    /// usual ranges.
    #[test]
    fn the_legends_draw() {
        let mut light = core_light();
        for (k, v) in [("cap_top", 25.0), ("skirt_foot", 24.0), ("depth", 12.0), ("switch_top", 30.0), ("switch_width", 20.0), ("beam", 1.0)] {
            for w in [512.0, 672.0, 1100.0] {
                let mut l = light.clone();
                l["sim"][k] = serde_json::json!(v);
                let v = Values::of(&l);
                for d in [slice(&v, Lens::Led, w, SLICE_H), slice(&v, Lens::Caps, w, SLICE_H), slice(&v, Lens::Wells, w, SLICE_H), display(&v, w, DISPLAY_H)] {
                    resvg::usvg::Tree::from_str(&d.svg, &resvg::usvg::Options::default()).expect("a legend reads as an SVG");
                    for word in &d.words {
                        let (weight, size) = word.look.face();
                        let ww = text::width(weight, size, &word.text);
                        assert!(word.x >= 0.0 && word.x + ww <= d.w + 0.5, "{} inside the legend across ({} of {})", word.text, word.x, d.w);
                        assert!(word.y - size >= 0.0 && word.y <= d.h, "{} inside the legend down", word.text);
                    }
                }
            }
        }
        light["sim"]["switch_width"] = serde_json::json!(12.0);
        let v = Values::of(&light);
        assert!(v.lit > 0.0 && v.lit <= 1.0);
    }

    /// The legends' LED stays inside its switch, however far north of centre
    /// the light asks for it, and comes back out as the switch widens.
    #[test]
    fn the_led_stays_inside_its_switch() {
        let mut light = core_light();
        light["led"] = serde_json::json!(12.0);
        light["sim"]["switch_width"] = serde_json::json!(4.0);
        let v = Values::of(&light);
        assert!((v.led - 2.0 * MM).abs() < 1e-6, "held at a 4 mm switch's edge: {}", v.led);
        light["sim"]["switch_width"] = serde_json::json!(14.0);
        assert!((Values::of(&light).led - 7.0 * MM).abs() < 1e-6);
        light["led"] = serde_json::json!(4.5);
        assert_eq!(Values::of(&light).led, 4.5);
    }

    /// The labels name the sliders, with their values as the boxes read them.
    #[test]
    fn the_labels_are_the_sliders() {
        let v = Values::of(&core_light());
        let words: Vec<String> = slice(&v, Lens::Caps, 672.0, SLICE_H).words.into_iter().map(|w| w.text).collect();
        for name in ["Skirt's foot", "Cap's top", "Legends let through", "LED off", "Inside", "Outside", "North", "South"] {
            assert!(words.iter().any(|w| w == name), "{name} on the Keycaps legend: {words:?}");
        }
        let words: Vec<String> = slice(&v, Lens::Led, 672.0, SLICE_H).words.into_iter().map(|w| w.text).collect();
        for name in ["North of centre", "Brightness", "Beam", "Glow spread", "Switch width", "Switch top", "Scattered", "Through its sides"] {
            assert!(words.iter().any(|w| w == name), "{name} on the LED legend: {words:?}");
        }
    }
}
