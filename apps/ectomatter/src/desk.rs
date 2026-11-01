//! Home as a desk: each device drawn as its own page draws it, to scale with
//! the others, its name under it. Selecting one opens its page. Arrange (edit
//! mode) shows dots, lets the desk zoom and pan (a drag on the empty desk),
//! and moves devices onto the dots; pads are added at their makers' sizes or
//! drawn, one at a time, once asked for. Done fits the desk to the window
//! again. Day to day the desk always fits the window and nothing on it
//! moves. Where each device sits is kept in the window's state ("desk"), in
//! the desk's own units.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::picture::{Decor, Dots, PadLaid, Photo, Plate, RingLaid};

/// The desk's units: at 100% a logical pixel; the Z10's 470 mm are 760 of them.
pub const PER_MM: f32 = 760.0 / 470.0;
/// Every other dot's spacing, in the desk's units: where a device is put the
/// first time it's met, and a pad is added (in Arrange, what's carried moves
/// a dot at a time, `on_dots`).
pub const SNAP: f32 = 24.0;
/// At the fit, the dots' least margin from the window's edges; and at any
/// view, how near the edge a dot may be: as near as keeps it whole, lit and
/// grown by a ring, with its soft edge, so the dots run to the edges with no
/// gutter wider than their spacing (Grant, 2026-10-31).
const EDGE: f32 = 24.0;
const CULL: f32 = DOT_R + RING_GROW + 1.0;
/// A dot's radius on the screen, whatever the zoom, and its white's opacity.
/// The dots lie at half SNAP, at rest too; in edit mode every other one each
/// way shows brighter.
const DOT_R: f32 = 1.25;
pub const DOT_ALPHA: f32 = 0.06;
const SNAP_LIFT: f32 = 0.7;
/// A key's press on Home: a ring runs out across the dots from it, lighting
/// the ones it passes in the key's light. Where it starts and how fast it runs
/// (logical pixels, and a second), how wide its band is, how long it lasts,
/// how much a dot it lights grows (logical pixels) and brightens, and the most
/// that run at once (fast typing).
const RING_START: f32 = 40.0;
const RING_SPEED: f32 = 520.0;
const RING_WIDTH: f32 = 26.0;
const RING_LIFE: Duration = Duration::from_millis(1500);
const RING_GROW: f32 = 1.5;
const RING_LIT: f32 = 0.94;
const RINGS_MOST: usize = 12;
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
    /// what it is, as its core says ("keyboard", "mouse"): a mousepad added
    /// goes under a mouse
    pub kind: String,
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
    /// the empty desk pressed, in edit mode: dragged, it moves round the desk
    /// as Pan does; let go where it was, it's a click, which lets the chosen
    /// pad go
    Ground { at: (f32, f32), from: View, moved: bool },
    /// a device: which, from where, its place then, and whether it has moved yet
    Item { i: usize, at: (f32, f32), from: (f32, f32), moved: bool },
    /// a pad drawn, in edit mode, once Draw your own was chosen: from the dot
    /// it began at to the dot the pointer is nearest now, in the desk's units
    Draw { from: (f32, f32), to: (f32, f32) },
    /// a pad carried, in edit mode: which, from where, its place then,
    /// whether it has moved yet, and whether it was chosen before the press
    Pad { i: usize, at: (f32, f32), from: (f32, f32), moved: bool, was: bool },
    /// the chosen pad sized by its edges, in edit mode: which, from where, its
    /// box then, and which of its edges move (left, top, right, bottom)
    Size { i: usize, at: (f32, f32), from: [f32; 4], edges: [bool; 4] },
}

/// A pad drawn on the desk, under the devices, as a desk mat or a mousepad
/// is: its place and size in the desk's units, its color, and a picture of
/// the person's own on it, filling it, if it has one.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [u8; 3],
    pub picture: Option<String>,
}

/// A pad's color when it's drawn: a shade lighter than the desk, as a dark mat is.
pub const PAD_COLOR: [u8; 3] = [0x25, 0x25, 0x2b];
/// A pad's corners' radius, in the desk's units (about 5 mm).
pub const PAD_RADIUS: f32 = 8.0;
/// A pad's least size each way, in the desk's units: two dots apart.
const PAD_LEAST: f32 = 2.0 * SNAP;
/// The longest side a pad may be given, in millimeters (100 inches).
const PAD_MOST_MM: f32 = 2540.0;

/// Pads as their makers size them, in millimeters, each with what a menu
/// calls it and the kind it is: mousepads as SteelSeries' QcK comes (Razer's
/// Gigantus V2 Large and Logitech's G640 the same, or 10 mm wider), and desk
/// mats as most come (SteelSeries' QcK XXL and Logitech's G840 the middle one).
pub const SIZES: [(&str, &str, f32, f32); 6] = [
    ("Mousepads", "Small", 250.0, 210.0),
    ("Mousepads", "Medium", 320.0, 270.0),
    ("Mousepads", "Large", 450.0, 400.0),
    ("Desk mats", "Extended", 800.0, 300.0),
    ("Desk mats", "Standard", 900.0, 400.0),
    ("Desk mats", "Large", 1200.0, 600.0),
];

/// A length in millimeters as inches, to a tenth, as the menus show sizes.
pub fn inches(mm: f32) -> String {
    let s = format!("{:.1}", mm / 25.4);
    s.strip_suffix(".0").map_or(s.clone(), String::from)
}

/// A pad's size in words: its width and its depth, in inches.
pub fn size_words(w_mm: f32, h_mm: f32) -> String {
    format!("{} × {} in", inches(w_mm), inches(h_mm))
}

/// Which of SIZES a size is, to the millimeter.
pub fn standard(w_mm: f32, h_mm: f32) -> Option<usize> {
    SIZES.iter().position(|s| (s.2 - w_mm).abs() < 1.0 && (s.3 - h_mm).abs() < 1.0)
}

/// A length as it was typed, in millimeters: inches (a bare number, or
/// with in or "), or millimeters or centimeters with their own words.
pub fn typed_mm(t: &str) -> Option<f32> {
    let t = t.trim().to_lowercase();
    let (n, per) = if let Some(n) = t.strip_suffix("mm") {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("cm") {
        (n, 10.0)
    } else {
        (t.trim_end_matches(['"', '″']).trim_end_matches("inches").trim_end_matches("in"), 25.4)
    };
    let v: f32 = n.trim().parse().ok()?;
    (v.is_finite() && v > 0.0).then_some(v * per)
}
/// The longest side a pad's picture is kept at, in its pixels: enough for a
/// desk mat across a large screen.
const PICTURE_SIDE: f32 = 2048.0;
/// The largest picture file a pad takes.
const PICTURE_BYTES: u64 = 64 << 20;

/// The pads as the window's state keeps them.
pub fn pads_of(v: &Value) -> Vec<Pad> {
    v.as_array().map_or(Vec::new(), |a| {
        a.iter()
            .filter_map(|p| {
                let c = p["color"].as_str().and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok()).unwrap_or(0x25252b);
                let (w, h) = (p["w"].as_f64()? as f32, p["h"].as_f64()? as f32);
                (w > 0.0 && h > 0.0).then(|| Pad { x: p["x"].as_f64().unwrap_or(0.0) as f32, y: p["y"].as_f64().unwrap_or(0.0) as f32, w, h, color: [(c >> 16) as u8, (c >> 8) as u8, c as u8], picture: p["picture"].as_str().filter(|s| !s.is_empty()).map(String::from) })
            })
            .collect()
    })
}

/// The pads as the window's state keeps them: each one's place and size in
/// the desk's units, its color as a hex code, and its picture's file.
pub fn pads_value(pads: &[Pad]) -> Value {
    Value::Array(pads.iter().map(|p| json!({ "x": p.x, "y": p.y, "w": p.w, "h": p.h, "color": format!("#{:02X}{:02X}{:02X}", p.color[0], p.color[1], p.color[2]), "picture": p.picture })).collect())
}

/// Where Ectomatter keeps the pictures put on pads.
fn pictures_dir() -> PathBuf {
    crate::data_dir().join("desk")
}

/// Whether this run is an offscreen check (`--snapshot`).
pub fn offscreen() -> bool {
    std::env::args().any(|a| a == "--snapshot")
}

/// Whether this run is an offscreen check, or a test, which keep nothing.
pub fn keeps_nothing() -> bool {
    cfg!(test) || offscreen()
}

/// A picture chosen for a pad, read and kept in Ectomatter's own data (so the
/// pad keeps it if the file is moved or deleted): the copy's path, and the
/// picture. One already kept is kept once. An offscreen check keeps nothing
/// and uses the file where it is.
pub fn import_picture(from: &Path) -> Result<(PathBuf, Photo), String> {
    let meta = std::fs::metadata(from).map_err(|_| "That file can't be read.".to_string())?;
    if meta.len() > PICTURE_BYTES {
        return Err("That picture is larger than 64 MB.".into());
    }
    let photo = photo_of(from)?;
    if keeps_nothing() {
        return Ok((from.to_path_buf(), photo));
    }
    let bytes = std::fs::read(from).map_err(|_| "That file can't be read.".to_string())?;
    // (named by what is in it, so the same picture chosen twice is kept once)
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
    let ext = from.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_else(|| "png".into());
    let dir = pictures_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let to = dir.join(format!("pad-{hash:016x}.{ext}"));
    if !to.exists() {
        std::fs::write(&to, &bytes).map_err(|e| e.to_string())?;
    }
    Ok((to, photo))
}

/// A pad's picture, read from its file: a PNG, JPEG, GIF (its first frame),
/// WebP or SVG picture, at its own size, or smaller to fit PICTURE_SIDE (an
/// SVG picture, which has no pixels of its own, at that).
pub fn photo_of(path: &Path) -> Result<Photo, String> {
    use resvg::usvg;
    let refused = || "That file isn't a picture Ectomatter can show. Choose a PNG, JPEG, WebP, GIF or SVG picture.".to_string();
    let href = path.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let opt = usvg::Options::default();
    // (its own size first, as the picture's file says it)
    let probe = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image href="{href}"/></svg>"#);
    let tree = usvg::Tree::from_str(&probe, &opt).map_err(|_| refused())?;
    fn first(g: &usvg::Group) -> Option<usvg::Size> {
        g.children().iter().find_map(|n| match n {
            usvg::Node::Image(i) => Some(i.size()),
            usvg::Node::Group(g) => first(g),
            _ => None,
        })
    }
    let size = first(tree.root()).ok_or_else(refused)?;
    let (iw, ih) = (size.width(), size.height());
    let vector = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    let k = PICTURE_SIDE / iw.max(ih);
    let k = if vector { k } else { k.min(1.0) };
    let (w, h) = ((iw * k).round().max(1.0) as u32, (ih * k).round().max(1.0) as u32);
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><image href="{href}" width="{w}" height="{h}" preserveAspectRatio="none"/></svg>"#);
    let tree = usvg::Tree::from_str(&svg, &opt).map_err(|_| refused())?;
    let mut pm = resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(refused)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    let px: Vec<[u8; 4]> = pm.data().chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    Ok(Photo::new(w as usize, h as usize, px))
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
    /// the ease into Arrange running: the dots held where it ends, lined up
    entering: bool,
    /// the dots' opacity, 0 to 1 of DOT_ALPHA, and its fading
    pub dots: f32,
    fade: Option<(Instant, f32, f32)>,
    /// when the view last changed, for pictures drawn for its size to wait till it settles
    pub changed: Instant,
    /// where each device Ectomatter has seen sits, as kept between runs: its id
    /// and its place (one not met yet this run keeps its place too)
    kept: Vec<(String, f32, f32)>,
    /// the pads on the desk, kept between runs, the one chosen in edit mode,
    /// and the one the pointer is over
    pub pads: Vec<Pad>,
    pub pad_chosen: Option<usize>,
    pub pad_hover: Option<usize>,
    /// Draw your own chosen, in edit mode: the next drag across the desk
    /// draws a pad, and then it's off, as a drawing tool goes back to the
    /// pointer once it has drawn
    pub draw_tool: bool,
    /// the ripples running, and the moment they're drawn at
    rings: Vec<Ring>,
    now: Instant,
}

/// A press's ripple: where it started, in the desk's logical pixels, the
/// light it runs in (sRGB), and when.
#[derive(Clone, Debug)]
struct Ring {
    x: f32,
    y: f32,
    color: [u8; 3],
    t0: Instant,
}

/// The light a ripple starting at `at` runs in, from the lights on the desk
/// as the devices show them now (where each sits, in the desk's logical
/// pixels, and its color, none where it's dark): the lit one nearest it, so a
/// pressed key's own where it has one, and the keys' round it where it has
/// none (the space bar); white with none lit.
pub fn ripple_light(at: (f32, f32), lights: &[((f32, f32), Option<[u8; 3]>)]) -> [u8; 3] {
    lights
        .iter()
        .filter_map(|(p, c)| c.filter(|c| c.iter().any(|v| *v > 24)).map(|c| ((p.0 - at.0).powi(2) + (p.1 - at.1).powi(2), c)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or([255; 3], |(_, c)| c)
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
        let pads = if cfg!(test) { Vec::new() } else { pads_of(&crate::uistate::get("desk_pads").unwrap_or_default()) };
        Desk { items: Vec::new(), editing: false, view: View { z: 1.0, px: 0.0, py: 0.0 }, size: (0.0, 0.0), hover: None, pressed: None, drag: None, ease: None, entering: false, dots: 0.0, fade: None, changed: Instant::now(), kept, pads, pad_chosen: None, pad_hover: None, draw_tool: false, rings: Vec::new(), now: Instant::now() }
    }

    /// A ripple from a point of the desk (its logical pixels), in a light's
    /// color, `age` into its run (none, for a press now).
    pub fn ripple(&mut self, at: (f32, f32), color: [u8; 3], age: Duration) {
        if self.rings.len() >= RINGS_MOST {
            self.rings.remove(0);
        }
        let now = Instant::now();
        self.now = now;
        self.rings.push(Ring { x: at.0, y: at.1, color, t0: now.checked_sub(age).unwrap_or(now) });
    }

    /// Whether a ripple is running.
    pub fn rippling(&self) -> bool {
        !self.rings.is_empty()
    }

    /// The pads kept as they are now.
    pub fn save_pads(&self) {
        crate::uistate::set("desk_pads", pads_value(&self.pads));
    }

    /// A picture no pad shows any more taken out of Ectomatter's data (only one
    /// Ectomatter kept there itself).
    pub fn forget_picture(&self, picture: Option<String>) {
        let Some(path) = picture else { return };
        if keeps_nothing() || self.pads.iter().any(|p| p.picture.as_deref() == Some(path.as_str())) {
            return;
        }
        let path = PathBuf::from(path);
        if path.parent() == Some(pictures_dir().as_path()) && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("pad-")) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// The chosen pad's color, kept once it's settled (the picker keeps it as it closes).
    pub fn set_pad_color(&mut self, rgb: [u8; 3]) {
        if let Some(p) = self.pad_chosen.and_then(|i| self.pads.get_mut(i)) {
            p.color = rgb;
        }
    }

    /// A picture put on a pad (the one at `i`, if it's still there and in the
    /// same place as `was`), or taken off it; kept.
    pub fn set_pad_picture(&mut self, i: usize, was: [f32; 4], picture: Option<String>) -> bool {
        let Some(p) = self.pads.get_mut(i).filter(|p| [p.x, p.y, p.w, p.h] == was) else { return false };
        let old = std::mem::replace(&mut p.picture, picture);
        self.save_pads();
        self.forget_picture(old);
        true
    }

    /// A point on the desk's room in the window (its logical pixels) in the
    /// desk's own units.
    fn on_desk(&self, at: (f32, f32)) -> (f32, f32) {
        ((at.0 - self.view.px) / self.view.z, (at.1 - self.view.py) / self.view.z)
    }

    /// The pad at a point of the desk's room in the window, the top one.
    pub fn pad_at(&self, at: (f32, f32)) -> Option<usize> {
        let (x, y) = self.on_desk(at);
        self.pads.iter().rposition(|p| x >= p.x && x < p.x + p.w && y >= p.y && y < p.y + p.h)
    }

    /// A pad on the desk's room, in its logical pixels: x, y, width, height.
    pub fn pad_screen(&self, p: &Pad) -> [f32; 4] {
        let v = self.view;
        [v.px + p.x * v.z, v.py + p.y * v.z, p.w * v.z, p.h * v.z]
    }

    /// The chosen pad's box, in the desk's units, for a picture asked for it.
    pub fn chosen_box(&self) -> Option<(usize, [f32; 4])> {
        let i = self.pad_chosen?;
        let p = self.pads.get(i)?;
        Some((i, [p.x, p.y, p.w, p.h]))
    }

    /// The pad being drawn, as it is now, in the desk's units.
    pub fn drawing(&self) -> Option<Pad> {
        match self.drag {
            Some(Drag::Draw { from, to }) => {
                let (x0, y0, x1, y1) = (from.0.min(to.0), from.1.min(to.1), from.0.max(to.0), from.1.max(to.1));
                Some(Pad { x: x0, y: y0, w: x1 - x0, h: y1 - y0, color: PAD_COLOR, picture: None })
            }
            _ => None,
        }
    }

    /// Where the window's state keeps each device: its id and its place.
    fn saved() -> Vec<(String, f32, f32)> {
        let v = crate::uistate::get("desk").unwrap_or_default();
        v.as_object().map_or(Vec::new(), |o| {
            o.iter().filter_map(|(id, at)| Some((id.clone(), at[0].as_f64()? as f32, at[1].as_f64()? as f32))).collect()
        })
    }

    /// The devices as they sit on the desk, left to right by their middles
    /// (and top to bottom where two line up): the order every list of them
    /// in Ectomatter follows.
    pub fn order(&self) -> Vec<String> {
        let mut at: Vec<(f32, f32, &str)> = self.items.iter().map(|i| (i.x + i.w / 2.0, i.y + i.h / 2.0, i.id.as_str())).collect();
        at.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        at.into_iter().map(|(_, _, id)| id.to_string()).collect()
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

    /// A device Ectomatter knows: put on the desk where it was kept, or, seen
    /// for the first time, beside the others (a keyboard, `left`, to their
    /// left; anything else to their right); or brought up to date.
    pub fn meet(&mut self, id: &str, name: &str, place: &str, kind: &str, (w, h): (f32, f32), connected: bool, left: bool) {
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            let resized = (it.w, it.h) != (w, h);
            (it.name, it.kind, it.w, it.h, it.connected) = (name.to_string(), kind.to_string(), w, h, connected);
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
        self.items.push(Item { id: id.into(), name: name.into(), place: place.into(), kind: kind.into(), w, h, x, y, connected });
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
        if self.editing {
            // (the dots lined up with the window as it is now, the short way)
            self.ease_to(self.aligned(self.view));
        } else {
            self.fit();
        }
    }

    /// The dots' spacing in the desk's units at zoom `z`: every other one
    /// where they would come closer than 12 pixels on the screen.
    fn step(z: f32) -> f32 {
        if SNAP * z < 12.0 { 2.0 * SNAP } else { SNAP }
    }

    /// Every device as large as the window allows, in the middle, the dots
    /// lined up the same distance from each edge, as Arrange shows them.
    pub fn fitted(&self) -> View {
        self.aligned(self.fitted_free())
    }

    /// Every device as large as the window allows, in the middle, exactly:
    /// day to day, with no dots to line up, the desk follows the window
    /// smoothly as it is sized, never a dot at a time.
    pub fn fitted_free(&self) -> View {
        let (w, h) = self.size;
        if self.items.is_empty() || w <= 0.0 || h <= 0.0 {
            return View { z: 1.0, px: w / 2.0, py: h / 2.0 };
        }
        // (the devices and the pads under them, all of it)
        let boxes: Vec<[f32; 4]> = self.items.iter().map(|i| [i.x, i.y, i.x + i.w, i.y + i.h]).chain(self.pads.iter().map(|p| [p.x, p.y, p.x + p.w, p.y + p.h])).collect();
        let x0 = boxes.iter().map(|b| b[0]).fold(f32::INFINITY, f32::min);
        let y0 = boxes.iter().map(|b| b[1]).fold(f32::INFINITY, f32::min);
        let x1 = boxes.iter().map(|b| b[2]).fold(f32::NEG_INFINITY, f32::max);
        let y1 = boxes.iter().map(|b| b[3]).fold(f32::NEG_INFINITY, f32::max);
        let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let z = ((w - 2.0 * PAD) / bw).min((h - 2.0 * PAD - BELOW) / bh).clamp(ZOOM.0, ZOOM.1);
        let (px, py) = ((w - bw * z) / 2.0 - x0 * z, (h - bh * z - BELOW) / 2.0 - y0 * z);
        View { z, px, py }
    }

    /// A view moved the least it takes for the dots to line up the same
    /// distance from each edge of the window (under half a dot each way).
    fn aligned(&self, v: View) -> View {
        let (w, h) = self.size;
        if w <= 0.0 || h <= 0.0 {
            return v;
        }
        let s = Desk::step(v.z) * v.z;
        let mx = (w - ((w - 2.0 * EDGE) / s).floor() * s) / 2.0;
        let my = (h - ((h - 2.0 * EDGE) / s).floor() * s) / 2.0;
        View { z: v.z, px: mx + ((v.px - mx) / s).round() * s, py: my + ((v.py - my) / s).round() * s }
    }

    /// The fit: day to day exactly, in Arrange with the dots lined up.
    pub fn fit(&mut self) {
        self.set_view(if self.editing { self.fitted() } else { self.fitted_free() });
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
        self.entering = false;
        let v = self.zoomed(self.view.z * (dy * 0.01).exp(), at);
        self.set_view(v);
        true
    }

    fn ease_to(&mut self, to: View) {
        self.ease = Some((Instant::now(), self.view, to));
        self.entering = false;
    }

    /// Edit mode on or off: the dots fade in or out; out, the desk eases back to the fit.
    pub fn set_editing(&mut self, on: bool) {
        if on == self.editing {
            return;
        }
        self.editing = on;
        self.drag = None;
        self.pressed = None;
        self.pad_chosen = None;
        self.pad_hover = None;
        self.draw_tool = false;
        self.fade = Some((Instant::now(), self.dots, if on { 1.0 } else { 0.0 }));
        // (in, the dots lined up as they come, the short way; out, the exact fit)
        self.ease_to(if on { self.aligned(self.view) } else { self.fitted_free() });
        self.entering = on;
    }

    pub fn animating(&self) -> bool {
        self.ease.is_some() || self.fade.is_some()
    }

    /// The easing, the fading and the ripples, moved on to `now`; true while
    /// any goes on.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        self.rings.retain(|r| now.saturating_duration_since(r.t0) < RING_LIFE);
        if let Some((t0, from, to)) = self.ease {
            let k = (now.duration_since(t0).as_secs_f32() / EASE.as_secs_f32()).min(1.0);
            let e = 1.0 - (1.0 - k).powi(3);
            self.set_view(View { z: from.z + (to.z - from.z) * e, px: from.px + (to.px - from.px) * e, py: from.py + (to.py - from.py) * e });
            if k >= 1.0 {
                self.ease = None;
                self.entering = false;
            }
        }
        if let Some((t0, from, to)) = self.fade {
            let k = (now.duration_since(t0).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
            self.dots = from + (to - from) * k;
            if k >= 1.0 {
                self.fade = None;
            }
        }
        self.animating() || self.rippling()
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
        // in edit mode, a press on a device lets the chosen pad go; one on the
        // empty desk: with Draw your own chosen, a pad is drawn from the
        // nearest dot; else on a pad, it's carried; off one, the desk moves
        if i.is_some() && self.editing {
            self.pad_chosen = None;
        }
        if i.is_none() && self.editing {
            let (x, y) = self.on_desk(at);
            self.drag = Some(match self.pad_at(at) {
                _ if self.draw_tool => Drag::Draw { from: (snap(x), snap(y)), to: (snap(x), snap(y)) },
                Some(k) => Drag::Pad { i: k, at, from: (self.pads[k].x, self.pads[k].y), moved: false, was: self.pad_chosen == Some(k) },
                None => Drag::Ground { at, from: self.view, moved: false },
            });
        }
    }

    /// A press on the chosen pad's edges (left, top, right, bottom: those
    /// that move), in edit mode: its drag sizes it.
    pub fn press_edges(&mut self, edges: [bool; 4], at: (f32, f32)) {
        let Some((i, b)) = self.chosen_box().filter(|_| self.editing && edges.iter().any(|e| *e)) else { return };
        self.pressed = None;
        self.drag = Some(Drag::Size { i, at, from: b, edges });
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
                self.entering = false;
                self.set_view(v);
                true
            }
            Some(Drag::Ground { at, from, moved }) => {
                if !*moved && (to.0 - at.0).abs() + (to.1 - at.1).abs() < 5.0 {
                    return false;
                }
                *moved = true;
                let v = View { px: from.px + to.0 - at.0, py: from.py + to.1 - at.1, ..*from };
                self.ease = None;
                self.entering = false;
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
                // (a dot at a time, as the dots show at this zoom)
                let (i, x, y) = (*i, on_dots(from.0 + dx / z, z), on_dots(from.1 + dy / z, z));
                let it = &mut self.items[i];
                (it.x, it.y) = (x, y);
                // (lifted as it is carried, so the desk changes on its first move)
                true
            }
            Some(Drag::Draw { to: end, .. }) => {
                let v = self.view;
                let (x, y) = (on_dots((to.0 - v.px) / v.z, v.z), on_dots((to.1 - v.py) / v.z, v.z));
                let changed = *end != (x, y);
                *end = (x, y);
                changed
            }
            Some(Drag::Pad { i, at, from, moved, .. }) => {
                let (dx, dy) = (to.0 - at.0, to.1 - at.1);
                if !*moved && dx.abs() + dy.abs() < 5.0 {
                    return false;
                }
                *moved = true;
                let (i, x, y) = (*i, on_dots(from.0 + dx / z, z), on_dots(from.1 + dy / z, z));
                if let Some(p) = self.pads.get_mut(i) {
                    (p.x, p.y) = (x, y);
                }
                self.pad_chosen = Some(i);
                true
            }
            Some(Drag::Size { i, at, from, edges }) => {
                let (dx, dy) = ((to.0 - at.0) / z, (to.1 - at.1) / z);
                let [x, y, w, h] = *from;
                // each edge that moves, onto the dots, the pad kept two dots across
                let x0 = if edges[0] { on_dots(x + dx, z).min(x + w - PAD_LEAST) } else { x };
                let y0 = if edges[1] { on_dots(y + dy, z).min(y + h - PAD_LEAST) } else { y };
                let x1 = if edges[2] { on_dots(x + w + dx, z).max(x + PAD_LEAST) } else { x + w };
                let y1 = if edges[3] { on_dots(y + h + dy, z).max(y + PAD_LEAST) } else { y + h };
                let Some(p) = self.pads.get_mut(*i) else { return false };
                let changed = [p.x, p.y, p.w, p.h] != [x0, y0, x1 - x0, y1 - y0];
                (p.x, p.y, p.w, p.h) = (x0, y0, x1 - x0, y1 - y0);
                changed
            }
            None => false,
        }
    }

    /// The press let go: the device clicked, if it was a click (outside edit mode).
    pub fn release(&mut self) -> Option<usize> {
        let drawn = self.drawing();
        let clicked = match self.drag.take() {
            Some(Drag::Item { i, moved: false, .. }) if !self.editing => Some(i),
            Some(Drag::Item { moved: true, .. }) if self.editing => {
                self.save();
                None
            }
            // a pad drawn at least two dots each way, chosen, and drawing off
            // (less, nothing drawn, and Draw your own still on)
            Some(Drag::Draw { .. }) => {
                if let Some(p) = drawn.filter(|p| p.w >= PAD_LEAST && p.h >= PAD_LEAST) {
                    self.pads.push(p);
                    self.pad_chosen = Some(self.pads.len() - 1);
                    self.draw_tool = false;
                    self.save_pads();
                }
                None
            }
            // a click on the empty desk lets the chosen pad go; a drag moved round it
            Some(Drag::Ground { moved: false, .. }) => {
                self.pad_chosen = None;
                None
            }
            // a pad carried, chosen; one clicked, chosen, or let go if it was
            Some(Drag::Pad { i, moved, was, .. }) => {
                self.pad_chosen = if !moved && was { None } else { Some(i) };
                if moved {
                    self.save_pads();
                }
                None
            }
            Some(Drag::Size { .. }) => {
                self.save_pads();
                None
            }
            _ => None,
        };
        self.pressed = None;
        clicked
    }

    /// The chosen pad taken off the desk.
    pub fn delete_pad(&mut self) {
        if let Some(i) = self.pad_chosen.take().filter(|i| *i < self.pads.len()) {
            let gone = self.pads.remove(i);
            self.pad_hover = None;
            self.save_pads();
            self.forget_picture(gone.picture);
        }
    }


    pub fn panning(&self) -> bool {
        matches!(self.drag, Some(Drag::Pan { .. }) | Some(Drag::Ground { moved: true, .. }))
    }

    /// A pad added at one of SIZES (by its place there), at its real size,
    /// chosen: a mousepad under the mouse, a desk mat under every device
    /// together, on the dots; with no device where it goes, in the middle of
    /// the view. The view eases to the fit if the pad isn't all in it.
    pub fn add_pad(&mut self, k: usize) {
        let Some(&(kind, _, w_mm, h_mm)) = SIZES.get(k) else { return };
        let (w, h) = (w_mm * PER_MM, h_mm * PER_MM);
        let under: Vec<&Item> = self.items.iter().filter(|i| kind != "Mousepads" || i.kind == "mouse").collect();
        let (cx, cy) = if under.is_empty() {
            self.on_desk((self.size.0 / 2.0, self.size.1 / 2.0))
        } else {
            let x0 = under.iter().map(|i| i.x).fold(f32::INFINITY, f32::min);
            let y0 = under.iter().map(|i| i.y).fold(f32::INFINITY, f32::min);
            let x1 = under.iter().map(|i| i.x + i.w).fold(f32::NEG_INFINITY, f32::max);
            let y1 = under.iter().map(|i| i.y + i.h).fold(f32::NEG_INFINITY, f32::max);
            ((x0 + x1) / 2.0, (y0 + y1) / 2.0)
        };
        self.pads.push(Pad { x: snap(cx - w / 2.0), y: snap(cy - h / 2.0), w, h, color: PAD_COLOR, picture: None });
        self.pad_chosen = Some(self.pads.len() - 1);
        self.draw_tool = false;
        self.save_pads();
        self.show_chosen();
    }

    /// The view eased to the fit when the chosen pad isn't all in it.
    fn show_chosen(&mut self) {
        let Some(p) = self.pad_chosen.and_then(|i| self.pads.get(i)) else { return };
        let [px, py, pw, ph] = self.pad_screen(p);
        if px < 0.0 || py < 0.0 || px + pw > self.size.0 || py + ph > self.size.1 {
            let to = self.fitted();
            self.ease_to(to);
        }
    }

    /// The chosen pad's size, in millimeters.
    pub fn pad_size_mm(&self) -> Option<(f32, f32)> {
        let p = self.pads.get(self.pad_chosen?)?;
        Some((p.w / PER_MM, p.h / PER_MM))
    }

    /// The chosen pad given a size, in millimeters (either side left as it is
    /// where it's none), its middle where it was; kept.
    pub fn set_pad_size(&mut self, w_mm: Option<f32>, h_mm: Option<f32>) {
        let Some(p) = self.pad_chosen.and_then(|i| self.pads.get_mut(i)) else { return };
        let side = |mm: Option<f32>, now: f32| mm.map_or(now, |mm| (mm * PER_MM).clamp(PAD_LEAST, PAD_MOST_MM * PER_MM));
        let (w, h) = (side(w_mm, p.w), side(h_mm, p.h));
        let (cx, cy) = (p.x + p.w / 2.0, p.y + p.h / 2.0);
        (p.x, p.y, p.w, p.h) = (cx - w / 2.0, cy - h / 2.0, w, h);
        self.save_pads();
        self.show_chosen();
    }

    pub fn carrying(&self) -> bool {
        matches!(self.drag, Some(Drag::Item { moved: true, .. }) | Some(Drag::Pad { moved: true, .. })) && self.editing
    }

    /// Whether a device's name (its box on the desk's room, as x0, y0, x1, y1)
    /// lies over a pad it could be lost on: one with a picture, or one light
    /// enough that the name's gray loses its contrast there.
    pub fn name_backed(&self, [x0, y0, x1, y1]: [f32; 4]) -> bool {
        self.pads.iter().any(|p| {
            let [px, py, pw, ph] = self.pad_screen(p);
            let over = x1 > px && x0 < px + pw && y1 > py && y0 < py + ph;
            let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
            let light = 0.2126 * lin(p.color[0]) + 0.7152 * lin(p.color[1]) + 0.0722 * lin(p.color[2]) > 0.1;
            over && (p.picture.is_some() || light)
        })
    }

    /// The pads on the desk's room, in its logical pixels, for their rings
    /// and their presses: each one's box, whether it's chosen, whether the
    /// pointer is over it, and whether it's the one being drawn.
    pub fn pads_shown(&self) -> Vec<([f32; 4], bool, bool, bool)> {
        let mut out: Vec<([f32; 4], bool, bool, bool)> = self.pads.iter().enumerate().map(|(i, p)| (self.pad_screen(p), self.pad_chosen == Some(i), self.pad_hover == Some(i), false)).collect();
        if let Some(p) = self.drawing() {
            out.push((self.pad_screen(&p), true, false, true));
        }
        out
    }

    pub fn zoom_text(&self) -> String {
        format!("{}%", (self.view.z * 100.0).round())
    }

    /// What the desk lays on the window's ground, in the buffer's pixels: the
    /// pads, each with its picture as `pictures` has it read, the dots while
    /// they show (none under a device's name, `names`), and the highlight
    /// under the device the pointer is over, outside edit mode. `at` is where
    /// the desk is in the window, in logical pixels; `corner`, the radius of
    /// the window's corner at the desk's bottom right (0 when it's square), where
    /// no dot goes that the corner's curve would cut.
    pub fn decor(&self, at: (f32, f32), scale: f32, corner: f32, names: &[[f32; 4]], pictures: &dyn Fn(&str) -> Option<Arc<Photo>>) -> crate::picture::Decor {
        let (w, h) = self.size;
        let v = self.view;
        let s = |p: f32| p * scale;
        let room = [s(at.0), s(at.1), s(at.0 + w), s(at.1 + h)];
        let pads = if self.items.is_empty() { Vec::new() } else { self.pads.iter().cloned().chain(self.drawing()).collect() };
        let pads = pads
            .iter()
            .map(|p| {
                let [x, y, pw, ph] = self.pad_screen(p);
                PadLaid { rect: [s(at.0 + x), s(at.1 + y), s(at.0 + x + pw), s(at.1 + y + ph)], radius: s(PAD_RADIUS * v.z), color: p.color, picture: p.picture.as_deref().and_then(pictures), bounds: room }
            })
            .collect();
        // the dots, at rest and arranged, and the ripples running through them
        // (at rest, spread evenly, the same margin all the way round; in
        // Arrange, where devices snap, held where the ease into it ends, then
        // moving with the desk)
        let pitch = Desk::step(v.z) * v.z / 2.0;
        let (ox, oy, step, inset) = if !self.editing {
            let ((ox, sx), (oy, sy)) = (even(w, pitch), even(h, pitch));
            (ox, oy, [sx, sy], ox - pitch / 2.0)
        } else {
            let a = if self.entering { self.aligned(v) } else { v };
            (a.px, a.py, [pitch; 2], CULL)
        };
        let dots = (!self.items.is_empty()).then(|| Dots {
            ox: s(at.0 + ox),
            oy: s(at.1 + oy),
            step: step.map(s),
            r: s(DOT_R),
            alpha: DOT_ALPHA,
            snap_alpha: DOT_ALPHA * (1.0 + SNAP_LIFT * self.dots),
            over_pads: self.dots > 0.0,
            bounds: [s(at.0 + inset), s(at.1 + inset), s(at.0 + w - inset), s(at.1 + h - inset)],
            holes: names
                .iter()
                .map(|n| [s(at.0 + n[0] - 8.0), s(at.1 + n[1] - 6.0), s(at.0 + n[2] + 8.0), s(at.1 + n[3] + 6.0)])
                .chain((corner > 0.0).then(|| [s(at.0 + w - corner), s(at.1 + h - corner), s(at.0 + w), s(at.1 + h)]))
                .collect(),
            grow: s(RING_GROW),
            lit: RING_LIT,
            rings: self
                .rings
                .iter()
                .map(|r| {
                    let t = self.now.saturating_duration_since(r.t0).as_secs_f32();
                    RingLaid {
                        x: s(at.0 + r.x),
                        y: s(at.1 + r.y),
                        radius: s(RING_START + RING_SPEED * t),
                        width: s(RING_WIDTH),
                        strength: (1.0 - t / RING_LIFE.as_secs_f32()).max(0.0).powf(1.4),
                        color: r.color.map(|c| c as f32 / 255.0),
                    }
                })
                .collect(),
        });
        let plates = match self.hover.filter(|_| !self.editing) {
            Some(i) if i < self.items.len() => {
                let [x, y, iw, ih] = self.screen(i);
                let (side, below, radius, alpha) = PLATE;
                vec![Plate { rect: [s(at.0 + x - side), s(at.1 + y - side), s(at.0 + x + iw + side), s(at.1 + y + ih + below)], radius: s(radius), alpha }]
            }
            _ => Vec::new(),
        };
        Decor { pads, dots, plates }
    }
}

/// Dots across a length `len`, about `pitch` apart, as they lie at rest: as
/// far from each end as from each other, as if the next lay on the edge
/// (Grant, 2026-11-01: "the gap at the edges to match the gap between
/// dots"), the spacing made to fit by under half a dot over the length.
/// Where the first lies, and the spacing (the same).
fn even(len: f32, pitch: f32) -> (f32, f32) {
    let s = len / (len / pitch).round().max(2.0);
    (s, s)
}

/// A place in the desk's units, onto the nearest dot.
pub fn snap(v: f32) -> f32 {
    (v / SNAP).round() * SNAP
}

/// `v` on the nearest of the dots as they show at zoom `z`: what's carried
/// or drawn in Arrange moves a dot row or column at a time (Grant,
/// 2026-10-26), half SNAP, or SNAP where the dots show every other one.
pub fn on_dots(v: f32, z: f32) -> f32 {
    let d = Desk::step(z) / 2.0;
    (v / d).round() * d
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

/// What the empty desk says: the devices Ectomatter works with, by the cores it has.
pub fn works_with() -> String {
    let mut names: Vec<String> = Vec::new();
    for p in crate::packages::of(crate::packages::Kind::Core) {
        let c = &p.manifest.spec;
        match &c.devices {
            Some(a) => names.extend(a.iter().map(|n| format!("the {n}"))),
            None => names.push(format!("the {}", c.name)),
        }
    }
    let list = match names.len() {
        0 => return "Ectomatter has no cores to work with devices yet.".into(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        n => format!("{}, and {}", names[..n - 1].join(", "), names[n - 1]),
    };
    let one = if names.len() == 1 { "it" } else { "one" };
    format!("Ectomatter works with {list}. Plug {one} in and it shows up here.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desk() -> Desk {
        let mut d = Desk::new();
        d.size = (1380.0, 992.0);
        d.items = vec![
            Item { id: "z10".into(), name: "EVGA Z10 RGB".into(), place: "z10".into(), kind: "keyboard".into(), w: 760.5, h: 304.6, x: 0.0, y: 0.0, connected: true },
            Item { id: "mouse".into(), name: "SteelSeries Sensei 310".into(), place: "mouse".into(), kind: "mouse".into(), w: 118.3, h: 202.3, x: 864.0, y: 72.0, connected: true },
        ];
        d.fit();
        d
    }

    /// Sized a pixel at a time, day to day, the desk follows the window
    /// smoothly: the keyboard never moves more than a pixel or so a step,
    /// where lining the dots up made it jump a dot at a time.
    #[test]
    fn sizing_the_window_moves_the_desk_smoothly() {
        let mut d = desk();
        let mut last = d.screen(0);
        let mut worst = 0f32;
        for w in 1380..1500 {
            d.resize(w as f32, 992.0);
            let now = d.screen(0);
            worst = worst.max((now[0] - last[0]).abs()).max((now[1] - last[1]).abs());
            last = now;
        }
        assert!(worst < 1.5, "the keyboard jumped {worst} pixels in one step");
        // in Arrange the dots line up, the desk eased there the short way
        d.set_editing(true);
        d.tick(Instant::now() + EASE + FADE);
        assert_eq!(d.view, d.fitted(), "lined up with the dots");
        assert!((d.view.px - d.fitted_free().px).abs() <= SNAP * d.view.z / 2.0 + 1e-3);
    }

    /// Every list of the devices follows the desk, left to right: the mouse
    /// put left of the keyboard comes first.
    #[test]
    fn the_devices_are_listed_as_they_sit() {
        let mut d = desk();
        assert_eq!(d.order(), ["z10", "mouse"]);
        d.items[1].x = -400.0;
        assert_eq!(d.order(), ["mouse", "z10"]);
    }

    /// Each device comes back where it was kept, whichever is met first: the
    /// one met first, kept again, doesn't lose the other's place.
    #[test]
    fn each_device_keeps_its_place_whichever_is_met_first() {
        for kb_first in [true, false] {
            let mut d = Desk::new();
            d.size = (1380.0, 992.0);
            d.kept = vec![("z10".into(), -864.0, 0.0), ("mouse".into(), 0.0, 0.0)];
            let kb = |d: &mut Desk| d.meet("z10", "EVGA Z10 RGB", "z10", "keyboard", (760.5, 304.6), true, true);
            let mouse = |d: &mut Desk| d.meet("mouse", "SteelSeries Sensei 310", "mouse", "mouse", (118.3, 202.3), true, false);
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
            let kb = |d: &mut Desk| d.meet("kb-test", "A keyboard", "z10", "keyboard", (760.0, 304.0), true, true);
            let mouse = |d: &mut Desk| d.meet("mouse-test", "A mouse", "mouse", "mouse", (118.0, 202.0), true, false);
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
        let v = d.fitted();
        assert!((v.z - 1.16).abs() < 0.01, "{}", v.z);
        let s = SNAP * v.z;
        let first = |o: f32| o - ((o - EDGE) / s).floor() * s;
        let (l, t) = (first(v.px), first(v.py));
        let r = d.size.0 - (l + ((d.size.0 - EDGE - l) / s).floor() * s);
        let b = d.size.1 - (t + ((d.size.1 - EDGE - t) / s).floor() * s);
        assert!((l - r).abs() < 0.01 && (t - b).abs() < 0.01, "left {l} right {r} top {t} bottom {b}");
        assert!(l >= EDGE && t >= EDGE);
    }

    /// In edit mode a drag on the empty desk draws a pad on the dots, chosen
    /// once drawn; a drag on it carries it; one too small to be a pad is a
    /// click that lets the chosen one go; outside edit mode nothing is drawn.
    #[test]
    fn a_pad_is_drawn_once_draw_your_own_is_chosen() {
        let mut d = desk();
        d.press(None, (20.0, 600.0));
        d.moved((420.0, 900.0));
        d.release();
        assert!(d.pads.is_empty(), "outside edit mode");
        d.set_editing(true);
        let z = d.view.z;
        // (a drag on the empty desk moves round it, and draws nothing)
        let v = d.view;
        d.press(None, (20.0, 600.0));
        d.moved((420.0, 900.0));
        assert!(d.drawing().is_none() && d.panning());
        d.release();
        assert!(d.pads.is_empty() && d.view != v, "moved round the desk, nothing drawn");
        d.view = v;
        // Draw your own: a click draws nothing and leaves it on; a drag draws
        // one, chosen, and puts it away
        d.draw_tool = true;
        d.press(None, (20.0, 600.0));
        d.release();
        assert!(d.pads.is_empty() && d.draw_tool, "a click draws nothing");
        d.press(None, (20.0, 600.0));
        d.moved((420.0, 900.0));
        assert!(d.drawing().is_some_and(|p| p.w > 0.0));
        d.release();
        assert_eq!((d.pads.len(), d.pad_chosen, d.draw_tool), (1, Some(0), false));
        let p = d.pads[0].clone();
        assert!([p.x, p.y, p.w, p.h].iter().all(|v| (v / SNAP).fract() == 0.0), "on the dots: {p:?}");
        assert!((p.w - 400.0 / z).abs() <= SNAP, "as wide as drawn");
        // carried by a drag on it
        let [sx, sy, ..] = d.pad_screen(&p);
        d.press(None, (sx + 10.0, sy + 10.0));
        d.moved((sx + 10.0 + 3.0 * SNAP * z, sy + 10.0));
        d.release();
        assert_eq!((d.pads.len(), d.pads[0].x), (1, p.x + 3.0 * SNAP));
        assert_eq!(d.pad_chosen, Some(0), "still chosen, carried");
        // chosen again, it lets go; a third time it's chosen
        let [sx, sy, ..] = d.pad_screen(&d.pads[0].clone());
        d.press(None, (sx + 10.0, sy + 10.0));
        d.release();
        assert_eq!(d.pad_chosen, None, "chosen again, let go");
        d.press(None, (sx + 10.0, sy + 10.0));
        d.release();
        assert_eq!(d.pad_chosen, Some(0));
        // a click on the empty desk lets it go, and so does a press on a device
        d.press(None, (1300.0, 60.0));
        d.release();
        assert_eq!(d.pad_chosen, None);
        d.pad_chosen = Some(0);
        d.press(Some(0), (100.0, 100.0));
        d.release();
        assert_eq!(d.pad_chosen, None, "a device pressed");
        d.pad_chosen = Some(0);
        d.delete_pad();
        assert!(d.pads.is_empty() && d.pad_chosen.is_none());
    }

    /// A pad at a maker's size is that size on the desk, to the millimeter:
    /// a mousepad under the mouse, a desk mat under every device together,
    /// chosen; sized again from the menu or typed, its middle stays put, and
    /// it's never less than two dots across.
    #[test]
    fn a_pad_goes_on_at_its_makers_size_where_it_belongs() {
        let mut d = desk();
        d.set_editing(true);
        let mid = |p: &Pad| (p.x + p.w / 2.0, p.y + p.h / 2.0);
        let large_mousepad = SIZES.iter().position(|s| s.0 == "Mousepads" && s.1 == "Large").unwrap();
        d.add_pad(large_mousepad);
        let (p, m) = (d.pads[0].clone(), d.items[1].clone());
        assert_eq!(d.pad_chosen, Some(0));
        assert!((p.w / PER_MM - 450.0).abs() < 0.01 && (p.h / PER_MM - 400.0).abs() < 0.01, "{p:?}");
        let (cx, cy) = mid(&p);
        assert!((cx - (m.x + m.w / 2.0)).abs() <= SNAP / 2.0 && (cy - (m.y + m.h / 2.0)).abs() <= SNAP / 2.0, "under the mouse");
        assert!((p.x / SNAP).fract() == 0.0 && (p.y / SNAP).fract() == 0.0, "on the dots");
        let standard_mat = SIZES.iter().position(|s| s.1 == "Standard").unwrap();
        d.add_pad(standard_mat);
        let p = d.pads[1].clone();
        let (x0, x1) = (d.items[0].x, d.items[1].x + d.items[1].w);
        let (y0, y1) = (d.items.iter().map(|i| i.y).fold(f32::INFINITY, f32::min), d.items.iter().map(|i| i.y + i.h).fold(f32::NEG_INFINITY, f32::max));
        assert!((mid(&p).0 - (x0 + x1) / 2.0).abs() <= SNAP / 2.0 && (mid(&p).1 - (y0 + y1) / 2.0).abs() <= SNAP / 2.0, "under both");
        assert_eq!(standard(p.w / PER_MM, p.h / PER_MM), Some(standard_mat));
        // sized again: a maker's size, then a width typed, the middle where it was
        let was = mid(&d.pads[1]);
        d.set_pad_size(Some(SIZES[0].2), Some(SIZES[0].3));
        assert_eq!(d.pad_size_mm().map(|(w, h)| (w.round(), h.round())), Some((250.0, 210.0)));
        d.set_pad_size(typed_mm("30"), None);
        let (w, h) = d.pad_size_mm().unwrap();
        assert!((w - 762.0).abs() < 0.01 && (h - 210.0).abs() < 0.01);
        let now = mid(&d.pads[1]);
        assert!((now.0 - was.0).abs() < 1e-3 && (now.1 - was.1).abs() < 1e-3, "its middle stays");
        d.set_pad_size(Some(1.0), None);
        assert_eq!(d.pads[1].w, PAD_LEAST, "never less than two dots");
        assert!(d.draw_tool == false);
    }

    /// Sizes in the menus' words: inches to a tenth, and what's typed read
    /// as inches unless it says millimeters or centimeters.
    #[test]
    fn sizes_read_in_inches() {
        assert_eq!(size_words(900.0, 400.0), "35.4 × 15.7 in");
        assert_eq!(inches(762.0), "30");
        let words: Vec<String> = SIZES.iter().map(|s| size_words(s.2, s.3)).collect();
        assert_eq!(words, ["9.8 × 8.3 in", "12.6 × 10.6 in", "17.7 × 15.7 in", "31.5 × 11.8 in", "35.4 × 15.7 in", "47.2 × 23.6 in"]);
        assert_eq!(typed_mm("10"), Some(254.0));
        assert_eq!(typed_mm(" 12.5 in "), Some(317.5));
        assert_eq!(typed_mm("10\""), Some(254.0));
        assert_eq!(typed_mm("900 mm"), Some(900.0));
        assert_eq!(typed_mm("45cm"), Some(450.0));
        assert_eq!((typed_mm("wide"), typed_mm("0"), typed_mm("-3")), (None, None, None));
        assert_eq!(standard(450.2, 399.9), Some(2));
        assert_eq!(standard(451.5, 400.0), None);
    }

    /// The chosen pad is sized by its edges, onto the dots, never less than
    /// two dots across; outside edit mode its edges size nothing.
    #[test]
    fn a_pad_is_sized_by_its_edges() {
        let mut d = desk();
        d.pads.push(Pad { x: 96.0, y: 96.0, w: 240.0, h: 120.0, color: PAD_COLOR, picture: None });
        d.pad_chosen = Some(0);
        d.press_edges([false, false, true, false], (0.0, 0.0));
        assert!(!d.moved((500.0, 0.0)), "outside edit mode");
        d.release();
        d.set_editing(true);
        d.pad_chosen = Some(0);
        let z = d.view.z;
        // the right edge out three dots
        d.press_edges([false, false, true, false], (0.0, 0.0));
        assert!(d.moved((3.0 * SNAP * z + 2.0, 0.0)));
        d.release();
        assert_eq!([d.pads[0].x, d.pads[0].w], [96.0, 240.0 + 3.0 * SNAP]);
        // the top left corner in, as far as it goes: two dots across, two down
        d.press_edges([true, true, false, false], (0.0, 0.0));
        d.moved((5000.0, 5000.0));
        d.release();
        let p = &d.pads[0];
        assert_eq!([p.x + p.w, p.y + p.h, p.w, p.h], [96.0 + 240.0 + 3.0 * SNAP, 216.0, PAD_LEAST, PAD_LEAST]);
        assert_eq!(d.pad_chosen, Some(0), "still chosen");
    }

    /// The pads are kept as the window's state keeps them, and read back the
    /// same; one with no size is no pad.
    #[test]
    fn the_pads_are_kept_and_read_back() {
        let pads = vec![
            Pad { x: -48.0, y: 24.0, w: 960.0, h: 432.0, color: [0x12, 0x34, 0x56], picture: Some("/data/pad-1.png".into()) },
            Pad { x: 864.0, y: 48.0, w: 240.0, h: 288.0, color: PAD_COLOR, picture: None },
        ];
        let v = pads_value(&pads);
        assert_eq!(v[0]["color"], "#123456");
        assert_eq!(pads_of(&v), pads);
        assert_eq!(pads_of(&json!([{ "x": 0, "y": 0, "w": 0, "h": 48 }])), Vec::new());
    }

    /// The desk fits its pads in as well as its devices; the pads lie on the
    /// ground, within the desk's room, the one being drawn with them.
    #[test]
    fn the_desk_fits_its_pads_in() {
        let mut d = desk();
        let before = d.fitted();
        d.pads.push(Pad { x: -480.0, y: -48.0, w: 2400.0, h: 600.0, color: PAD_COLOR, picture: None });
        let after = d.fitted();
        assert!(after.z < before.z, "zoomed out to show it whole");
        d.fit();
        let [x, y, w, h] = d.pad_screen(&d.pads[0]);
        assert!(x >= 0.0 && y >= 0.0 && x + w <= d.size.0 && y + h <= d.size.1, "on the window: {x} {y} {w} {h}");
        let decor = d.decor((72.0, 36.0), 2.0, 0.0, &[], &|_| None);
        assert_eq!(decor.pads.len(), 1);
        assert_eq!(decor.pads[0].bounds, [144.0, 72.0, 144.0 + 2.0 * d.size.0, 72.0 + 2.0 * d.size.1]);
        assert!((decor.pads[0].rect[0] - 2.0 * (72.0 + x)).abs() < 1e-3);
        d.set_editing(true);
        d.draw_tool = true;
        d.press(None, (60.0, 60.0));
        d.moved((400.0, 300.0));
        assert_eq!(d.decor((0.0, 0.0), 1.0, 0.0, &[], &|_| None).pads.len(), 2, "the one being drawn too");
        assert_eq!(d.pads_shown().iter().filter(|p| p.3).count(), 1);
    }

    /// A picture file is read for a pad at its own size, its colors as they
    /// are; a file that's no picture is refused in words.
    #[test]
    fn a_pad_reads_its_picture() {
        let dir = std::env::temp_dir().join(format!("ectomatter-pad-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut pm = resvg::tiny_skia::Pixmap::new(40, 20).unwrap();
        pm.fill(resvg::tiny_skia::Color::from_rgba8(200, 40, 10, 255));
        let png = dir.join("mat & pad.png");
        pm.save_png(&png).unwrap();
        let photo = photo_of(&png).unwrap();
        assert_eq!(photo.size(), (40, 20));
        let bad = dir.join("not.png");
        std::fs::write(&bad, b"no picture here").unwrap();
        assert!(photo_of(&bad).unwrap_err().contains("isn't a picture"));
        let (kept, _) = import_picture(&png).unwrap();
        assert_eq!(kept, png, "a test keeps nothing");
        std::fs::remove_dir_all(&dir).unwrap();
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
        assert!((d.view.px - v.px - 100.0).abs() < 1e-3 && (d.view.py - v.py + 40.0).abs() < 1e-3, "the left button on the empty desk pans, as a trackpad can");
        let v = d.view;
        d.pan_from((40.0, 900.0));
        d.moved((140.0, 860.0));
        d.release();
        assert!((d.view.px - v.px - 100.0).abs() < 1e-3 && (d.view.py - v.py + 40.0).abs() < 1e-3, "panned with the middle button");
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
        let dot = Desk::step(d.view.z) / 2.0;
        assert!(it.x % dot == 0.0 && it.y % dot == 0.0, "on the dots: {} {}", it.x, it.y);
        assert_ne!((it.x, it.y), (864.0, 72.0));
        // (carried a dot's width across and a dot down, it moves one dot each way, not two)
        let (x, y) = (it.x, it.y);
        let [sx, sy, ..] = d.screen(1);
        d.press(Some(1), (sx + 5.0, sy + 5.0));
        d.moved((sx + 5.0 + dot * d.view.z, sy + 5.0 + dot * d.view.z));
        d.release();
        assert_eq!((d.items[1].x, d.items[1].y), (x + dot, y + dot), "a dot at a time");
        assert_eq!(dot, SNAP / 2.0, "the dots at this zoom are half SNAP apart");
        d.set_editing(false);
        d.tick(Instant::now() + EASE + FADE);
        assert_eq!(d.view, d.fitted_free(), "back to the fit");
        assert_eq!(d.dots, 0.0, "the dots put away");
    }
    /// A key's press sends a ring out across the dots: it runs out as it
    /// fades, and is gone after its run. The dots lie at rest too, a pad over
    /// them; in Arrange the ones devices snap onto show brighter, over the pads.
    #[test]
    fn a_press_sends_a_ripple_across_the_dots() {
        let mut d = desk();
        let rest = d.decor((0.0, 0.0), 1.0, 0.0, &[], &|_| None).dots.expect("the dots, at rest");
        assert!(rest.rings.is_empty());
        assert_eq!(rest.alpha, rest.snap_alpha, "at rest every dot alike");
        assert!(!rest.over_pads, "a pad covers them at rest");
        d.ripple((400.0, 300.0), [255, 60, 0], Duration::ZERO);
        let t0 = Instant::now();
        assert!(d.rippling());
        d.tick(t0 + Duration::from_millis(300));
        let early = d.decor((0.0, 0.0), 1.0, 0.0, &[], &|_| None).dots.unwrap().rings[0].clone();
        d.tick(t0 + Duration::from_millis(900));
        let late = d.decor((0.0, 0.0), 1.0, 0.0, &[], &|_| None).dots.unwrap().rings[0].clone();
        assert!(late.radius > early.radius && late.strength < early.strength, "it runs out and fades");
        assert_eq!((early.x, early.y), (400.0, 300.0));
        assert!(early.color[0] > early.color[1], "in the key's light");
        assert!(!d.tick(t0 + RING_LIFE + Duration::from_millis(50)), "and after its run it's gone");
        assert!(!d.rippling());
        // more presses than run at once: the oldest goes
        for i in 0..RINGS_MOST + 3 {
            d.ripple((i as f32, 0.0), [255; 3], Duration::ZERO);
        }
        assert_eq!(d.rings.len(), RINGS_MOST);
        // in Arrange, the dots devices snap onto, brighter, and over the pads
        d.set_editing(true);
        d.tick(Instant::now() + EASE + FADE);
        let arranged = d.decor((0.0, 0.0), 1.0, 0.0, &[], &|_| None).dots.unwrap();
        assert!(arranged.snap_alpha > arranged.alpha && arranged.over_pads);
    }

    /// The dots run to the desk's edges, whole, and none where the window's
    /// rounded corner would cut one (Grant, 2026-10-31, of a gutter a row and
    /// a column wide). At rest each edge is as far from the dots as they are
    /// from each other, all the way round (2026-11-01: "equal padding though
    /// all the way around", "the gap at the edges to match the gap between
    /// dots"); in Arrange, where devices snap onto them, at the snap
    /// spacing, moving with the desk, never a gutter wider than that spacing
    /// and a whole dot's room; and going into Arrange they stay put.
    #[test]
    fn the_dots_run_to_the_edges_evenly_and_whole() {
        let mut d = desk();
        let (w, h) = d.size;
        let corner = 10.0;
        let whole = DOT_R + RING_GROW;
        let laid = |d: &Desk| d.decor((0.0, 0.0), 1.0, corner, &[], &|_| None).dots.unwrap();
        // the outermost dots' distance from each edge: left, top, right, bottom
        let gaps = |dots: &Dots| {
            let first = |o: f32, b: f32, s: f32| o + ((b - o) / s).ceil() * s;
            let last = |o: f32, b: f32, s: f32| o + ((b - o) / s).floor() * s;
            let [sx, sy] = dots.step;
            let (x0, y0) = (first(dots.ox, dots.bounds[0], sx), first(dots.oy, dots.bounds[1], sy));
            let (x1, y1) = (last(dots.ox, dots.bounds[2], sx), last(dots.oy, dots.bounds[3], sy));
            ([x0, y0, w - x1, h - y1], (x1, y1))
        };
        let corner_whole = |dots: &Dots, (x1, y1): (f32, f32), what: &str| {
            let [sx, sy] = dots.step;
            let mut y = y1;
            while y > h - corner - sy {
                let mut x = x1;
                while x > w - corner - sx {
                    let (cx, cy) = ((x - (w - corner)).max(0.0), (y - (h - corner)).max(0.0));
                    let cut = (cx * cx + cy * cy).sqrt() + whole + 0.5 > corner && cx > 0.0 && cy > 0.0;
                    let left_out = dots.holes.iter().any(|b| x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]);
                    assert!(!cut || left_out, "{what}: the dot at {x}, {y} is cut by the window's corner");
                    x -= sx;
                }
                y -= sy;
            }
        };
        for z in [0.6, 1.0, 1.16, 2.5] {
            let pitch = Desk::step(z) * z / 2.0;
            for k in 0..48 {
                let pan = View { z, px: 3.0 + k as f32 * 0.7, py: 5.0 + k as f32 * 0.45 };
                // at rest
                d.editing = false;
                d.view = pan;
                let dots = laid(&d);
                let (g, far) = gaps(&dots);
                let [sx, sy] = dots.step;
                for (gap, s) in g.iter().zip([sx, sy, sx, sy]) {
                    assert!((gap - s).abs() < 0.01, "zoom {z}, pan {k}: at rest an edge's gap {g:?} isn't the dots' spacing {:?}", dots.step);
                }
                for s in dots.step {
                    assert!((s / pitch - 1.0).abs() < 0.01, "zoom {z}: spaced {s} against {pitch}");
                }
                corner_whole(&dots, far, &format!("at rest, zoom {z}, pan {k}"));
                // in Arrange, the desk panned or zoomed
                d.editing = true;
                d.entering = false;
                let dots = laid(&d);
                assert_eq!(dots.step, [pitch; 2], "the snap spacing in Arrange");
                let (g, far) = gaps(&dots);
                for (side, gap) in ["left", "top", "right", "bottom"].into_iter().zip(g) {
                    assert!(gap >= whole, "zoom {z}, pan {k}: a dot cut at the {side} edge ({gap})");
                    assert!(gap <= pitch + whole + 1.0, "zoom {z}, pan {k}: a gutter at the {side} edge ({gap}) wider than the dots' spacing ({pitch})");
                }
                corner_whole(&dots, far, &format!("in Arrange, zoom {z}, pan {k}"));
            }
        }
        // going into Arrange the dots stay where the ease ends, the devices settling onto them
        let mut d = desk();
        d.set_editing(true);
        let start = laid(&d);
        d.tick(Instant::now() + EASE / 2);
        let mid = laid(&d);
        d.tick(Instant::now() + EASE + FADE);
        let end = laid(&d);
        assert_eq!((start.ox, start.oy, start.step), (end.ox, end.oy, end.step), "held from the start");
        assert_eq!((mid.ox, mid.oy), (end.ox, end.oy), "and through the ease");
    }

    /// A ripple runs in the light nearest where it starts: a key's own, the
    /// nearest lit one for a key with none of its own (dark or near black
    /// passed over), and white with nothing lit.
    #[test]
    fn a_ripple_runs_in_the_light_nearest_it() {
        let (orange, blue) = ([255, 109, 13], [20, 60, 255]);
        let lights = [((0.0, 0.0), Some(orange)), ((40.0, 0.0), Some(blue)), ((80.0, 0.0), None), ((100.0, 0.0), Some([10, 10, 10]))];
        assert_eq!(ripple_light((0.0, 0.0), &lights), orange, "a key's own light");
        assert_eq!(ripple_light((38.0, 30.0), &lights), blue, "the nearest lit");
        assert_eq!(ripple_light((90.0, 0.0), &lights), blue, "past the dark and the near-black ones");
        assert_eq!(ripple_light((0.0, 0.0), &[((0.0, 0.0), None)]), [255; 3], "white with none lit");
    }
}
