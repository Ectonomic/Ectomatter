//! Home as a desk: each device drawn as its own page draws it, to scale with
//! the others, its name under it. Selecting one opens its page. Arrange (edit
//! mode) shows dots, lets the desk zoom and pan, and moves devices onto the
//! dots; Done fits the desk to the window again. Day to day the desk always
//! fits the window and nothing on it moves. Where each device sits is kept
//! in the window's state ("desk"), in the desk's own units.

use std::time::{Duration, Instant};

use crate::picture::{Decor, Dots, Plate};

/// The desk's units: at 100% a logical pixel; the Z10's 470 mm are 760 of them.
pub const PER_MM: f32 = 760.0 / 470.0;
/// The dots' spacing, in the desk's units; devices sit on them.
pub const SNAP: f32 = 24.0;
/// At the fit, the dots' least margin from the window's edges; and at any
/// view, how near the edge a dot may be.
const EDGE: f32 = 24.0;
const CULL: f32 = 16.0;
/// A dot's radius on the screen, whatever the zoom, and its white's opacity.
const DOT_R: f32 = 1.25;
pub const DOT_ALPHA: f32 = 0.06;
const ZOOM: (f32, f32) = (0.5, 4.0);
/// At the fit: the room round the devices, and under them for their names.
const PAD: f32 = 120.0;
const BELOW: f32 = 56.0;
/// Between a device seen for the first time and the ones before it.
const GAP: f32 = 96.0;
/// Under a device's picture, where its name begins, and the name's height.
pub const NAME_GAP: f32 = 16.0;
const EASE: Duration = Duration::from_millis(220);
const FADE: Duration = Duration::from_millis(180);
/// The highlight under the device the pointer is over: how far it reaches
/// past the picture (to the sides and top, and below, taking in the name),
/// its corners, and its white's opacity.
const PLATE: (f32, f32, f32, f32) = (16.0, 58.0, 16.0, 9.0 / 255.0);

/// A device on the desk: what it is, the page it opens, its size and place
/// in the desk's units, and whether it is plugged in.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub place: String,
    pub w: f32,
    pub h: f32,
    pub x: f32,
    pub y: f32,
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub z: f32,
    pub px: f32,
    pub py: f32,
}

enum Drag {
    /// the desk moved round, from where the press was and the view then
    Pan { at: (f32, f32), from: View },
    /// a device: which, from where, its place then, and whether it has moved yet
    Item { i: usize, at: (f32, f32), from: (f32, f32), moved: bool },
}

pub struct Desk {
    pub items: Vec<Item>,
    pub editing: bool,
    pub view: View,
    /// the desk's room in the window, in logical pixels
    pub size: (f32, f32),
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    drag: Option<Drag>,
    ease: Option<(Instant, View, View)>,
    /// the dots' opacity, 0 to 1 of DOT_ALPHA, and its fading
    pub dots: f32,
    fade: Option<(Instant, f32, f32)>,
    /// when the view last changed, for pictures drawn for its size to wait till it settles
    pub changed: Instant,
    /// where each device Ectodeck has seen sits, as kept between runs: its id
    /// and its place (one not met yet this run keeps its place too)
    kept: Vec<(String, f32, f32)>,
}

impl Default for Desk {
    fn default() -> Self {
        Desk::new()
    }
}

impl Desk {
    pub fn new() -> Desk {
        // (the tests start from a desk nothing was kept for)
        let kept = if cfg!(test) { Vec::new() } else { Desk::saved() };
        Desk { items: Vec::new(), editing: false, view: View { z: 1.0, px: 0.0, py: 0.0 }, size: (0.0, 0.0), hover: None, pressed: None, drag: None, ease: None, dots: 0.0, fade: None, changed: Instant::now(), kept }
    }

    /// Where the window's state keeps each device: its id and its place.
    fn saved() -> Vec<(String, f32, f32)> {
        let v = crate::uistate::get("desk").unwrap_or_default();
        v.as_object().map_or(Vec::new(), |o| {
            o.iter().filter_map(|(id, at)| Some((id.clone(), at[0].as_f64()? as f32, at[1].as_f64()? as f32))).collect()
        })
    }

    /// Whether a place is kept for a device, met this run or not.
    pub fn keeps(&self, id: &str) -> bool {
        self.kept.iter().any(|(k, _, _)| k == id)
    }

    /// Each device's place kept: those on the desk where they are now, and
    /// every other where it was kept.
    fn save(&mut self) {
        for i in &self.items {
            match self.kept.iter_mut().find(|(k, _, _)| *k == i.id) {
                Some(k) => (k.1, k.2) = (i.x, i.y),
                None => self.kept.push((i.id.clone(), i.x, i.y)),
            }
        }
        let o: serde_json::Map<String, serde_json::Value> = self.kept.iter().map(|(id, x, y)| (id.clone(), serde_json::json!([x, y]))).collect();
        crate::uistate::set("desk", serde_json::Value::Object(o));
    }

    /// A device Ectodeck knows: put on the desk where it was kept, or, seen
    /// for the first time, beside the others (a keyboard, `left`, to their
    /// left; anything else to their right); or brought up to date.
    pub fn meet(&mut self, id: &str, name: &str, place: &str, (w, h): (f32, f32), connected: bool, left: bool) {
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            let resized = (it.w, it.h) != (w, h);
            (it.name, it.w, it.h, it.connected) = (name.to_string(), w, h, connected);
            if resized && !self.editing {
                self.fit();
            }
            return;
        }
        let (x, y) = match self.kept.iter().find(|(s, _, _)| s == id) {
            Some(&(_, x, y)) => (x, y),
            // (seen for the first time: beside the others, its middle level with
            // theirs, on the dots)
            None if self.items.is_empty() => (0.0, 0.0),
            None => {
                let leftmost = self.items.iter().map(|i| i.x).fold(f32::INFINITY, f32::min);
                let right = self.items.iter().map(|i| i.x + i.w).fold(f32::NEG_INFINITY, f32::max);
                let top = self.items.iter().map(|i| i.y).fold(f32::INFINITY, f32::min);
                let bottom = self.items.iter().map(|i| i.y + i.h).fold(f32::NEG_INFINITY, f32::max);
                let x = if left { snap(leftmost - GAP - w) } else { snap(right + GAP) };
                (x, snap((top + bottom) / 2.0 - h / 2.0))
            }
        };
        self.items.push(Item { id: id.into(), name: name.into(), place: place.into(), w, h, x, y, connected });
        self.save();
        if !self.editing {
            self.fit();
        }
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        if (w, h) == self.size || w <= 0.0 || h <= 0.0 {
            return;
        }
        self.size = (w, h);
        if !self.editing {
            self.fit();
        }
    }

    /// The dots' spacing in the desk's units at zoom `z`: every other one
    /// where they would come closer than 12 pixels on the screen.
    fn step(z: f32) -> f32 {
        if SNAP * z < 12.0 { 2.0 * SNAP } else { SNAP }
    }

    /// Every device as large as the window allows, in the middle, the dots
    /// lined up the same distance from each edge.
    pub fn fitted(&self) -> View {
        let (w, h) = self.size;
        if self.items.is_empty() || w <= 0.0 || h <= 0.0 {
            return View { z: 1.0, px: w / 2.0, py: h / 2.0 };
        }
        let x0 = self.items.iter().map(|i| i.x).fold(f32::INFINITY, f32::min);
        let y0 = self.items.iter().map(|i| i.y).fold(f32::INFINITY, f32::min);
        let x1 = self.items.iter().map(|i| i.x + i.w).fold(f32::NEG_INFINITY, f32::max);
        let y1 = self.items.iter().map(|i| i.y + i.h).fold(f32::NEG_INFINITY, f32::max);
        let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let z = ((w - 2.0 * PAD) / bw).min((h - 2.0 * PAD - BELOW) / bh).clamp(ZOOM.0, ZOOM.1);
        let (px, py) = ((w - bw * z) / 2.0 - x0 * z, (h - bh * z - BELOW) / 2.0 - y0 * z);
        let s = Desk::step(z) * z;
        let mx = (w - ((w - 2.0 * EDGE) / s).floor() * s) / 2.0;
        let my = (h - ((h - 2.0 * EDGE) / s).floor() * s) / 2.0;
        View { z, px: mx + ((px - mx) / s).round() * s, py: my + ((py - my) / s).round() * s }
    }

    pub fn fit(&mut self) {
        self.set_view(self.fitted());
    }

    fn set_view(&mut self, v: View) {
        if v != self.view {
            self.view = v;
            self.changed = Instant::now();
        }
    }

    /// The view zoomed to `z` about a point of the desk (in its logical pixels).
    pub fn zoomed(&self, z: f32, (sx, sy): (f32, f32)) -> View {
        let z = z.clamp(ZOOM.0, ZOOM.1);
        let v = self.view;
        View { z, px: sx - (sx - v.px) * z / v.z, py: sy - (sy - v.py) * z / v.z }
    }

    /// Zoom by a step of the controls (-1 out, 1 in), to 100% (0), or to the fit (2), eased.
    pub fn zoom_step(&mut self, how: i32) {
        if !self.editing {
            return;
        }
        let mid = (self.size.0 / 2.0, self.size.1 / 2.0);
        let to = match how {
            -1 => self.zoomed(self.view.z / 1.25, mid),
            1 => self.zoomed(self.view.z * 1.25, mid),
            0 => self.zoomed(1.0, mid),
            _ => self.fitted(),
        };
        self.ease_to(to);
    }

    /// The wheel, in edit mode: zoom where the pointer is, at once.
    pub fn wheel(&mut self, at: (f32, f32), dy: f32) -> bool {
        if !self.editing {
            return false;
        }
        self.ease = None;
        let v = self.zoomed(self.view.z * (dy * 0.01).exp(), at);
        self.set_view(v);
        true
    }

    fn ease_to(&mut self, to: View) {
        self.ease = Some((Instant::now(), self.view, to));
    }

    /// Edit mode on or off: the dots fade in or out; out, the desk eases back to the fit.
    pub fn set_editing(&mut self, on: bool) {
        if on == self.editing {
            return;
        }
        self.editing = on;
        self.drag = None;
        self.pressed = None;
        self.fade = Some((Instant::now(), self.dots, if on { 1.0 } else { 0.0 }));
        if !on {
            self.ease_to(self.fitted());
        }
    }

    pub fn animating(&self) -> bool {
        self.ease.is_some() || self.fade.is_some()
    }

    /// The easing and the fading, moved on to `now`; true while either goes on.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some((t0, from, to)) = self.ease {
            let k = (now.duration_since(t0).as_secs_f32() / EASE.as_secs_f32()).min(1.0);
            let e = 1.0 - (1.0 - k).powi(3);
            self.set_view(View { z: from.z + (to.z - from.z) * e, px: from.px + (to.px - from.px) * e, py: from.py + (to.py - from.py) * e });
            if k >= 1.0 {
                self.ease = None;
            }
        }
        if let Some((t0, from, to)) = self.fade {
            let k = (now.duration_since(t0).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            self.dots = from + (to - from) * k;
            if k >= 1.0 {
                self.fade = None;
            }
        }
        self.animating()
    }

    /// A device's picture on the desk, in its logical pixels: pressed, a pixel
    /// lower; carried in edit mode, three higher.
    pub fn screen(&self, i: usize) -> [f32; 4] {
        let (it, v) = (&self.items[i], self.view);
        let nudge = match &self.drag {
            Some(Drag::Item { i: d, moved: true, .. }) if *d == i => -3.0,
            _ if self.pressed == Some(i) && !self.editing => 1.0,
            _ => 0.0,
        };
        [v.px + it.x * v.z, v.py + it.y * v.z + nudge, it.w * v.z, it.h * v.z]
    }

    /// A press on device `i`, at a point of the desk: in edit mode its drag
    /// carries it. (Outside edit mode a press on a device only shows; let go
    /// where it was, it's a click.)
    pub fn press(&mut self, i: Option<usize>, at: (f32, f32)) {
        let i = i.filter(|i| *i < self.items.len());
        self.pressed = i;
        self.drag = i.map(|i| Drag::Item { i, at, from: (self.items[i].x, self.items[i].y), moved: false });
    }

    /// The middle button pressed on the desk, on a device or not: in edit
    /// mode its drag moves around the desk.
    pub fn pan_from(&mut self, at: (f32, f32)) {
        if self.editing {
            self.pressed = None;
            self.drag = Some(Drag::Pan { at, from: self.view });
        }
    }

    /// The pointer moved, pressed; true if anything on the desk moved.
    pub fn moved(&mut self, to: (f32, f32)) -> bool {
        let editing = self.editing;
        let z = self.view.z;
        match &mut self.drag {
            Some(Drag::Pan { at, from }) => {
                let v = View { px: from.px + to.0 - at.0, py: from.py + to.1 - at.1, ..*from };
                self.ease = None;
                self.set_view(v);
                true
            }
            Some(Drag::Item { i, at, from, moved }) => {
                let (dx, dy) = (to.0 - at.0, to.1 - at.1);
                if !*moved && dx.abs() + dy.abs() < 5.0 {
                    return false;
                }
                *moved = true;
                if !editing {
                    // (outside edit mode a drag moves nothing, and isn't a click)
                    self.pressed = None;
                    return true;
                }
                let (i, x, y) = (*i, snap(from.0 + dx / z), snap(from.1 + dy / z));
                let it = &mut self.items[i];
                (it.x, it.y) = (x, y);
                // (lifted as it is carried, so the desk changes on its first move)
                true
            }
            None => false,
        }
    }

    /// The press let go: the device clicked, if it was a click (outside edit mode).
    pub fn release(&mut self) -> Option<usize> {
        let clicked = match self.drag.take() {
            Some(Drag::Item { i, moved: false, .. }) if !self.editing => Some(i),
            Some(Drag::Item { moved: true, .. }) if self.editing => {
                self.save();
                None
            }
            _ => None,
        };
        self.pressed = None;
        clicked
    }

    pub fn panning(&self) -> bool {
        matches!(self.drag, Some(Drag::Pan { .. }))
    }

    pub fn carrying(&self) -> bool {
        matches!(self.drag, Some(Drag::Item { moved: true, .. })) && self.editing
    }

    pub fn zoom_text(&self) -> String {
        format!("{}%", (self.view.z * 100.0).round())
    }

    /// What the desk lays on the window's ground, in the buffer's pixels: the
    /// dots while they show (none under a device's name, `names`), and the
    /// highlight under the device the pointer is over, outside edit mode.
    /// `at` is where the desk is in the window, in logical pixels.
    pub fn decor(&self, at: (f32, f32), scale: f32, names: &[[f32; 4]]) -> crate::picture::Decor {
        let (w, h) = self.size;
        let v = self.view;
        let s = |p: f32| p * scale;
        let dots = (self.dots > 0.0 && !self.items.is_empty()).then(|| Dots {
            ox: s(at.0 + v.px),
            oy: s(at.1 + v.py),
            step: s(Desk::step(v.z) * v.z),
            r: s(DOT_R),
            alpha: DOT_ALPHA * self.dots,
            bounds: [s(at.0 + CULL), s(at.1 + CULL), s(at.0 + w - CULL), s(at.1 + h - CULL)],
            holes: names.iter().map(|n| [s(at.0 + n[0] - 8.0), s(at.1 + n[1] - 6.0), s(at.0 + n[2] + 8.0), s(at.1 + n[3] + 6.0)]).collect(),
        });
        let plates = match self.hover.filter(|_| !self.editing) {
            Some(i) if i < self.items.len() => {
                let [x, y, iw, ih] = self.screen(i);
                let (side, below, radius, alpha) = PLATE;
                vec![Plate { rect: [s(at.0 + x - side), s(at.1 + y - side), s(at.0 + x + iw + side), s(at.1 + y + ih + below)], radius: s(radius), alpha }]
            }
            _ => Vec::new(),
        };
        Decor { dots, plates }
    }
}

/// A place in the desk's units, onto the nearest dot.
pub fn snap(v: f32) -> f32 {
    (v / SNAP).round() * SNAP
}

/// A device's size on the desk, in its units, from its whole drawing (its
/// viewBox) at `per_mm` of the drawing's units to the millimeter.
pub fn size_of(svg: &str, per_mm: f32) -> (f32, f32) {
    let vb = view_box(svg).unwrap_or([0.0, 0.0, 649.0, 260.0]);
    (vb[2] / per_mm * PER_MM, vb[3] / per_mm * PER_MM)
}

fn view_box(svg: &str) -> Option<[f32; 4]> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let vb = root.split("viewBox=\"").nth(1)?.split('"').next()?;
    let n: Vec<f32> = vb.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse().ok()).collect();
    (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
}

/// What the empty desk says: the devices Ectodeck works with, by the cores it has.
pub fn works_with() -> String {
    let mut names: Vec<String> = Vec::new();
    if let Ok(dirs) = std::fs::read_dir(crate::cores::dir()) {
        let mut dirs: Vec<_> = dirs.flatten().map(|d| d.path()).collect();
        dirs.sort();
        for d in dirs {
            let Ok(t) = std::fs::read_to_string(d.join("core.json")) else { continue };
            let Ok(c) = ectodeck_draw::jsonc::parse(&t) else { continue };
            match c["devices"].as_array() {
                Some(a) => names.extend(a.iter().filter_map(|n| n.as_str()).map(|n| format!("the {n}"))),
                None => names.extend(c["name"].as_str().map(|n| format!("the {n}"))),
            }
        }
    }
    let list = match names.len() {
        0 => return "Ectodeck has no cores to work with devices yet.".into(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        n => format!("{}, and {}", names[..n - 1].join(", "), names[n - 1]),
    };
    let one = if names.len() == 1 { "it" } else { "one" };
    format!("Ectodeck works with {list}. Plug {one} in and it shows up here.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desk() -> Desk {
        let mut d = Desk::new();
        d.size = (1380.0, 992.0);
        d.items = vec![
            Item { id: "z10".into(), name: "EVGA Z10 RGB".into(), place: "z10".into(), w: 760.5, h: 304.6, x: 0.0, y: 0.0, connected: true },
            Item { id: "mouse".into(), name: "SteelSeries Sensei 310".into(), place: "mouse".into(), w: 118.3, h: 202.3, x: 864.0, y: 72.0, connected: true },
        ];
        d.fit();
        d
    }

    /// Each device comes back where it was kept, whichever is met first: the
    /// one met first, kept again, doesn't lose the other's place.
    #[test]
    fn each_device_keeps_its_place_whichever_is_met_first() {
        for kb_first in [true, false] {
            let mut d = Desk::new();
            d.size = (1380.0, 992.0);
            d.kept = vec![("z10".into(), -864.0, 0.0), ("mouse".into(), 0.0, 0.0)];
            let kb = |d: &mut Desk| d.meet("z10", "EVGA Z10 RGB", "z10", (760.5, 304.6), true, true);
            let mouse = |d: &mut Desk| d.meet("mouse", "SteelSeries Sensei 310", "mouse", (118.3, 202.3), true, false);
            if kb_first {
                kb(&mut d);
                mouse(&mut d);
            } else {
                mouse(&mut d);
                kb(&mut d);
            }
            let at = |id: &str| d.items.iter().find(|i| i.id == id).map(|i| (i.x, i.y)).unwrap();
            assert_eq!((at("z10"), at("mouse")), ((-864.0, 0.0), (0.0, 0.0)), "keyboard first {kb_first}");
            assert!(d.keeps("z10") && d.keeps("mouse"));
        }
    }

    /// Met in either order, a keyboard sits left of a mouse, level with it.
    #[test]
    fn a_keyboard_goes_left_of_a_mouse() {
        for kb_first in [true, false] {
            let mut d = Desk::new();
            d.size = (1380.0, 992.0);
            let kb = |d: &mut Desk| d.meet("kb-test", "A keyboard", "z10", (760.0, 304.0), true, true);
            let mouse = |d: &mut Desk| d.meet("mouse-test", "A mouse", "mouse", (118.0, 202.0), true, false);
            if kb_first {
                kb(&mut d);
                mouse(&mut d);
            } else {
                mouse(&mut d);
                kb(&mut d);
            }
            let (k, m) = (d.items.iter().find(|i| i.id == "kb-test").unwrap(), d.items.iter().find(|i| i.id == "mouse-test").unwrap());
            assert!(k.x + k.w < m.x, "keyboard first {kb_first}: the keyboard ({}) left of the mouse ({})", k.x, m.x);
            assert!(((k.y + k.h / 2.0) - (m.y + m.h / 2.0)).abs() <= SNAP, "level");
        }
    }

    /// At the fit the devices fill the window as far as its room allows, and
    /// the dots come out the same distance from each edge, all whole.
    #[test]
    fn the_fit_lines_the_dots_up_evenly() {
        let d = desk();
        let v = d.view;
        assert!((v.z - 1.16).abs() < 0.01, "{}", v.z);
        let s = SNAP * v.z;
        let first = |o: f32| o - ((o - EDGE) / s).floor() * s;
        let (l, t) = (first(v.px), first(v.py));
        let r = d.size.0 - (l + ((d.size.0 - EDGE - l) / s).floor() * s);
        let b = d.size.1 - (t + ((d.size.1 - EDGE - t) / s).floor() * s);
        assert!((l - r).abs() < 0.01 && (t - b).abs() < 0.01, "left {l} right {r} top {t} bottom {b}");
        assert!(l >= EDGE && t >= EDGE);
    }

    /// Outside edit mode nothing moves: no zoom, no pan, no device; a press
    /// and let go without moving is a click.
    #[test]
    fn day_to_day_nothing_moves() {
        let mut d = desk();
        let v = d.view;
        assert!(!d.wheel((400.0, 300.0), 120.0));
        d.press(None, (40.0, 900.0));
        d.moved((140.0, 860.0));
        assert_eq!(d.release(), None);
        d.pan_from((40.0, 900.0));
        d.moved((140.0, 860.0));
        assert_eq!(d.release(), None);
        d.press(Some(1), (1200.0, 500.0));
        d.moved((1260.0, 540.0));
        assert_eq!(d.release(), None, "a drag isn't a click");
        assert_eq!((d.items[1].x, d.items[1].y), (864.0, 72.0));
        assert_eq!(d.view, v);
        d.press(Some(1), (1200.0, 500.0));
        assert_eq!(d.release(), Some(1), "a click opens the device");
    }

    /// In edit mode the wheel zooms about the pointer, a drag with the middle
    /// button pans (on a device too; the left button on the empty desk
    /// doesn't), a device lands on the dots, and Done eases back to the fit.
    #[test]
    fn arranging_zooms_pans_and_snaps() {
        let mut d = desk();
        let fit = d.view;
        d.set_editing(true);
        let at = (400.0, 300.0);
        let before = ((at.0 - d.view.px) / d.view.z, (at.1 - d.view.py) / d.view.z);
        assert!(d.wheel(at, 45.0));
        let after = ((at.0 - d.view.px) / d.view.z, (at.1 - d.view.py) / d.view.z);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3, "the point under the pointer stays");
        assert!(d.view.z > fit.z);
        let v = d.view;
        d.press(None, (40.0, 900.0));
        d.moved((140.0, 860.0));
        d.release();
        assert_eq!(d.view, v, "the left button on the empty desk doesn't pan");
        d.pan_from((40.0, 900.0));
        d.moved((140.0, 860.0));
        d.release();
        assert!((d.view.px - v.px - 100.0).abs() < 1e-3 && (d.view.py - v.py + 40.0).abs() < 1e-3, "panned");
        let (v, at) = (d.view, (d.items[1].x, d.items[1].y));
        d.pan_from((1000.0, 500.0));
        d.moved((1030.0, 520.0));
        d.release();
        assert!(d.view != v && (d.items[1].x, d.items[1].y) == at, "panned from over a device, which stays put");
        d.press(Some(1), (1000.0, 500.0));
        d.moved((930.0, 561.0));
        assert!(d.carrying());
        d.release();
        let it = &d.items[1];
        assert!(it.x % SNAP == 0.0 && it.y % SNAP == 0.0, "on the dots: {} {}", it.x, it.y);
        assert_ne!((it.x, it.y), (864.0, 72.0));
        d.set_editing(false);
        d.tick(Instant::now() + EASE + FADE);
        assert_eq!(d.view, d.fitted(), "back to the fit");
        assert_eq!(d.dots, 0.0, "the dots put away");
    }
}
