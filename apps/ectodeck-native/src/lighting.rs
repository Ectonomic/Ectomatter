//! The Lighting page, as the web app had it (renderLighting in app.js at
//! cd9ea82): the row of effects, then one tile of settings for the effect in
//! two columns, "Colour and motion" and "Where and when", each row a name in
//! a column of its own and then its control, and a reset at the foot.
//!
//! The window lays the tile out here, with the page's own measurements, and
//! the page draws each piece where it is told: so chips wrap and notes run on
//! as they did, and a slider being dragged is never rebuilt under the pointer.

use serde_json::{json, Value};

use crate::text;
use crate::z10::{Page, EFFECTS};
use crate::Cell;

/// What a piece of the tile does when used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ctl {
    None,
    /// a colour: the effect's one colour, one of its list, the colour between pulses, the paint
    Color,
    Colors,
    Background,
    Paint,
    Random,
    Brightness,
    Speed,
    Direction,
    Submode,
    Demo,
    Zone,
    Start,
    Stop,
    Reset,
    Fill,
    Scatter,
    Clear,
    Name,
    LcdColor,
    LcdBrightness,
    LcdApp,
    GameKey,
    SliderBind,
    Control,
    Temperature,
    Wind,
    HdrMode,
    PeakNits,
    Place,
    EffectRate,
}

const CTLS: [Ctl; 31] = [
    Ctl::None,
    Ctl::Color,
    Ctl::Colors,
    Ctl::Background,
    Ctl::Paint,
    Ctl::Random,
    Ctl::Brightness,
    Ctl::Speed,
    Ctl::Direction,
    Ctl::Submode,
    Ctl::Demo,
    Ctl::Zone,
    Ctl::Start,
    Ctl::Stop,
    Ctl::Reset,
    Ctl::Fill,
    Ctl::Scatter,
    Ctl::Clear,
    Ctl::Name,
    Ctl::LcdColor,
    Ctl::LcdBrightness,
    Ctl::LcdApp,
    Ctl::GameKey,
    Ctl::SliderBind,
    Ctl::Control,
    Ctl::Temperature,
    Ctl::Wind,
    Ctl::HdrMode,
    Ctl::PeakNits,
    Ctl::Place,
    Ctl::EffectRate,
];

pub fn ctl(id: i32) -> Ctl {
    CTLS.get(id as usize).copied().unwrap_or(Ctl::None)
}
pub fn id(c: Ctl) -> i32 {
    CTLS.iter().position(|x| *x == c).unwrap_or(0) as i32
}

/// the pieces' kinds, as the page draws them
pub mod kind {
    pub const TEXT: i32 = 0;
    pub const SWATCH: i32 = 1;
    pub const CHECK: i32 = 2;
    pub const SLIDER: i32 = 3;
    pub const SEG: i32 = 4;
    pub const SEG_BUTTON: i32 = 5;
    pub const CHIP: i32 = 6;
    pub const GHOST: i32 = 7;
    pub const LINK: i32 = 8;
    pub const TILE: i32 = 9;
    pub const FIELD: i32 = 10;
    pub const DROPDOWN: i32 = 11;
    pub const DEVICE: i32 = 12;
    pub const WBOX: i32 = 13;
    pub const DOT: i32 = 14;
    /// a box in its colour, rounded at its ends
    pub const BOX: i32 = 16;
    /// a place that tells the page when the pointer is over it
    pub const ZONE: i32 = 17;
}
/// the text styles
pub mod style {
    pub const LABEL: i32 = 0;
    pub const HEAD: i32 = 1;
    pub const NOTE: i32 = 2;
    pub const EMPTY: i32 = 3;
    pub const VALUE: i32 = 4;
    pub const VALUE_BRIGHT: i32 = 5;
    pub const ITEM: i32 = 6;
    pub const TITLE: i32 = 7;
    pub const NAME: i32 = 8;
    pub const SMALL: i32 = 9;
    pub const BIG: i32 = 10;
    pub const LABEL_SOFT: i32 = 11;
    pub const FACT: i32 = 12;
    pub const FACT_VALUE: i32 = 13;
    pub const CREDIT: i32 = 14;
    /// a note in its own colour (the Light lab's menus), and a menu's
    /// words in their own colour
    pub const NOTE_IN: i32 = 19;
    pub const WORDS_IN: i32 = 20;
    /// the Light lab's small words in their own colour, and its
    /// stronger ones (a reading's number, the name of what its line tells of)
    pub const SMALL_IN: i32 = 21;
    pub const LABEL_IN: i32 = 22;
    /// a segmented button that tells the page when the pointer is over it, as a zone does
    pub const TELLS: i32 = 23;
}

/// the effects' speeds: stored as a cycle time, the slider runs slow to fast, as EVGA's does
fn speed(mode: &str) -> Option<(f64, f64, &'static str)> {
    Some(match mode {
        "breathing" | "spiral" | "wave" => (1000.0, 5000.0, "cycle"),
        "pulse" => (6000.0, 10000.0, "cycle"),
        "stars" => (500.0, 4000.0, "cycle"),
        "trigger" => (50.0, 1000.0, "shift"),
        _ => return None,
    })
}

pub const ZONES: [(u64, &str); 6] = [(0x80, "Whole keyboard"), (0x02, "F-keys"), (0x04, "Number row"), (0x10, "WASD"), (0x08, "Arrows"), (0x01, "Numpad")];

fn rgb(v: &Value) -> slint::Color {
    slint::Color::from_rgb_u8(v[0].as_u64().unwrap_or(0) as u8, v[1].as_u64().unwrap_or(0) as u8, v[2].as_u64().unwrap_or(0) as u8)
}

/// The tile's pieces, placed; and its height.
pub struct Tile {
    pub cells: Vec<Cell>,
    pub height: f32,
}

pub(crate) struct Col {
    pub x: f32,
    pub w: f32,
    pub y: f32,
    pub cells: Vec<Cell>,
}

const COL_W: f32 = 584.0;
const LABEL_W: f32 = 112.0;
const CTRL_X: f32 = 128.0;

pub(crate) fn cell(kind: i32, x: f32, y: f32, w: f32, h: f32) -> Cell {
    Cell { kind, x, y, w, h, text: "".into(), color: slint::Color::from_rgb_u8(0, 0, 0), on: false, value: 0.0, id: 0, index: 0, style: 0 }
}
pub(crate) fn text_cell(style: i32, x: f32, y: f32, w: f32, h: f32, s: &str) -> Cell {
    Cell { text: s.into(), style, ..cell(kind::TEXT, x, y, w, h) }
}

/// One part of a row's control, as it stacks in the control's column.
pub(crate) enum Part {
    Swatches(Ctl, Vec<slint::Color>),
    Check(Ctl, bool, &'static str),
    Slider(Ctl, f32, String),
    Seg(Ctl, Vec<(i64, &'static str)>, i64),
    Chips(Vec<(&'static str, bool)>),
    Note(String),
    Paint(slint::Color, &'static str),
    Ghosts(Vec<(Ctl, &'static str)>),
    /// a line of text to type in: the profile's name
    Field(Ctl, String),
    /// a button that opens a menu of choices, showing the one chosen
    Dropdown(Ctl, usize, String),
    /// a value, as text
    Value(String),
    /// a list, one line an item
    List(Vec<String>),
}

impl Col {
    fn new(x: f32, title: &str) -> Col {
        Col::sized(x, COL_W, title)
    }

    /// A column `w` wide: its heading, then its rows 14 apart.
    pub fn sized(x: f32, w: f32, title: &str) -> Col {
        if title.is_empty() {
            return Col { x, w, y: 2.0 + 14.0, cells: Vec::new() };
        }
        Col { x, w, y: 18.0 + 2.0 + 14.0, cells: vec![text_cell(style::HEAD, x, 0.0, w, 18.0, title)] }
    }

    /// A column's own list of checkboxes, 9 apart.
    pub fn checks(&mut self, items: Vec<(Ctl, usize, bool, String)>) {
        let n = items.len();
        for (k, (c, index, on, label)) in items.into_iter().enumerate() {
            let y = self.y + k as f32 * 28.0;
            self.cells.push(Cell { on, id: id(c), index: index as i32, text: label.clone().into(), ..cell(kind::CHECK, self.x, y, 28.0 + text::width(400, 13.5, &label), 19.0) });
        }
        self.y += n as f32 * 28.0 - 9.0 + 14.0;
    }

    /// A row: its name in a column of its own, then its control, whose parts
    /// stack 7 apart and sit in the middle of the row when it is short.
    pub fn row(&mut self, label: &str, parts: Vec<Part>) {
        let (x0, cx) = (self.x, self.x + CTRL_X);
        let ctrl_w = self.w - CTRL_X;
        let mut placed: Vec<Cell> = Vec::new();
        let mut h = 0.0f32;
        for (i, p) in parts.into_iter().enumerate() {
            if i > 0 {
                h += 7.0;
            }
            let y = h;
            h += match p {
                Part::Swatches(c, cols) => {
                    for (j, col) in cols.into_iter().enumerate() {
                        placed.push(Cell { color: col, id: id(c), index: j as i32, ..cell(kind::SWATCH, cx + j as f32 * 40.0, y, 34.0, 34.0) });
                    }
                    34.0
                }
                Part::Check(c, on, label) => {
                    placed.push(Cell { on, id: id(c), text: label.into(), ..cell(kind::CHECK, cx, y, 28.0 + text::width(400, 13.5, label), 19.0) });
                    19.0
                }
                Part::Slider(c, frac, value) => {
                    placed.push(Cell { value: frac, id: id(c), ..cell(kind::SLIDER, cx + 2.0, y + 9.0, ctrl_w - 48.0 - 14.0 - 4.0, 16.0) });
                    placed.push(text_cell(style::VALUE, cx + ctrl_w - 48.0, y + 8.0, 48.0, 18.0, &value));
                    34.0
                }
                Part::Seg(c, opts, current) => {
                    let mut bx = cx + 3.0;
                    let mut buttons = Vec::new();
                    for (j, (v, name)) in opts.iter().enumerate() {
                        let w = text::width(400, 13.0, name) + 24.0;
                        buttons.push(Cell { text: (*name).into(), on: *v == current, id: id(c), index: j as i32, ..cell(kind::SEG_BUTTON, bx, y + 3.0, w, 30.0) });
                        bx += w + 2.0;
                    }
                    placed.push(cell(kind::SEG, cx, y, bx - 2.0 + 3.0 - cx, 36.0));
                    placed.extend(buttons);
                    36.0
                }
                Part::Chips(chips) => {
                    // chips 6 apart, wrapping at the control's edge
                    let (mut bx, mut by) = (0.0f32, 0.0f32);
                    for (j, (name, on)) in chips.iter().enumerate() {
                        let w = text::width(400, 13.0, name) + 24.0;
                        if bx > 0.0 && bx + w > ctrl_w {
                            bx = 0.0;
                            by += 36.0;
                        }
                        placed.push(Cell { text: (*name).into(), on: *on, id: id(Ctl::Zone), index: j as i32, ..cell(kind::CHIP, cx + bx, y + by, w, 30.0) });
                        bx += w + 6.0;
                    }
                    by + 30.0
                }
                Part::Note(s) => {
                    // a line a piece, 1.45 apart, as the page spaces them
                    let lines = text::wrap(400, 12.5, &s, ctrl_w);
                    for (k, line) in lines.iter().enumerate() {
                        placed.push(text_cell(style::NOTE, cx, y + k as f32 * 18.125, ctrl_w, 18.0, line));
                    }
                    (lines.len() as f32 * 18.125).floor()
                }
                Part::Paint(col, note) => {
                    placed.push(Cell { color: col, id: id(Ctl::Paint), ..cell(kind::SWATCH, cx, y, 34.0, 34.0) });
                    placed.push(text_cell(style::NOTE, cx + 44.0, y + 8.0, ctrl_w - 44.0, 18.0, note));
                    34.0
                }
                Part::Field(c, v) => {
                    placed.push(Cell { text: v.into(), id: id(c), ..cell(kind::FIELD, cx, y, ctrl_w.min(280.0), 35.0) });
                    35.0
                }
                Part::Dropdown(c, index, v) => {
                    placed.push(Cell { text: v.into(), id: id(c), index: index as i32, ..cell(kind::DROPDOWN, cx, y, ctrl_w.min(272.0), 32.0) });
                    32.0
                }
                Part::Value(v) => {
                    placed.push(text_cell(style::VALUE_BRIGHT, cx, y, ctrl_w, 34.0, &v));
                    34.0
                }
                Part::List(items) => {
                    // each item 1.5 lines tall, 4 apart; the list starts on the row's line
                    let n = items.len();
                    for (k, it) in items.iter().enumerate() {
                        placed.push(text_cell(style::ITEM, cx, y + 7.0 + k as f32 * 24.25, ctrl_w, 20.0, it));
                    }
                    (n as f32 * 24.25 - 4.0 + 14.0).max(34.0)
                }
                Part::Ghosts(list) => {
                    let mut bx = cx;
                    for (c, name) in list {
                        let w = text::width(500, 13.0, name) + 24.0;
                        placed.push(Cell { text: name.into(), id: id(c), ..cell(kind::GHOST, bx, y + 2.0, w, 30.0) });
                        bx += w + 10.0;
                    }
                    34.0
                }
            };
        }
        let row_h = h.max(34.0);
        // a short control sits in the middle of the row
        let shift = ((row_h - h) / 2.0).max(0.0);
        let top = self.y;
        self.cells.push(text_cell(style::LABEL, x0, top, LABEL_W, 34.0, label));
        for mut c in placed {
            c.y += top + shift;
            self.cells.push(c);
        }
        self.y += row_h + 14.0;
    }

    /// A paragraph across the column.
    pub fn para(&mut self, s: &str) {
        let lines = text::wrap(400, 12.5, s, self.w);
        let h = lines.len() as f32 * 18.125;
        for (k, line) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::NOTE, self.x, self.y + k as f32 * 18.125, self.w, 18.0, line));
        }
        self.y += h.floor() + 14.0;
    }

    pub fn height(&self) -> f32 {
        self.y - 14.0
    }
}

/// The colours a profile paints its keys with, one a key: kept by the window,
/// since the keyboard stores none.
pub fn custom_colors(n: u8) -> Vec<[u8; 3]> {
    crate::uistate::get(&format!("custom:{n}")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_else(|| vec![[0; 3]; 116])
}
pub fn save_custom(n: u8, cols: &[[u8; 3]]) {
    crate::uistate::set(&format!("custom:{n}"), json!(cols));
}

/// The tile for the profile being edited.
pub fn tile(page: &Page, paint: [u8; 3], demo: bool) -> Tile {
    let l = &page.draft()["lighting"];
    let mode = l["mode"].as_str().unwrap_or("off").to_string();
    let (px, pad) = (24.0, 20.0);
    let mut cells = Vec::new();
    if mode == "off" {
        cells.push(text_cell(style::EMPTY, px, pad, 1216.0, 20.0, "The backlight stays dark while this profile is active."));
        return Tile { cells, height: pad * 2.0 + 20.0 };
    }
    let (mut look, mut when);
    if mode == "custom" {
        look = Col::new(px, "Painting");
        look.row("Colour", vec![Part::Paint(slint::Color::from_rgb_u8(paint[0], paint[1], paint[2]), "Click keys on the keyboard to paint them.")]);
        look.row("Every key", vec![Part::Ghosts(vec![(Ctl::Fill, "Fill"), (Ctl::Scatter, "Scatter"), (Ctl::Clear, "Clear")])]);
        when = Col::new(px + COL_W + 48.0, "How it shows");
        when.para("The spacebar, the mode button and the media keys have no light, so they stay dark.");
        when.para("The keyboard can’t store colours per key, so they show only while Ectodeck is open. The profile saves with its backlight off.");
        let h = look.height().max(when.height());
        cells.extend(look.cells);
        cells.extend(when.cells);
        for c in &mut cells {
            c.y += pad;
        }
        return Tile { cells, height: pad + h + pad };
    }
    let e = &l[mode.as_str()];
    let sp = speed(&mode);
    look = Col::new(px, if sp.is_some() { "Colour and motion" } else { "Colour" });
    let list = |n: usize| -> Vec<slint::Color> { e["colors"].as_array().map(|a| a.iter().take(n).map(|c| rgb(&c["color"])).collect()).unwrap_or_default() };
    match mode.as_str() {
        "static" => look.row("Colour", vec![Part::Swatches(Ctl::Color, vec![rgb(&e["color"])])]),
        "breathing" => look.row("Colours", vec![Part::Swatches(Ctl::Colors, list(2)), Part::Note("Breathes in the first colour, then the second.".into())]),
        "pulse" | "wave" | "trigger" | "spiral" => look.row(
            "Colours",
            vec![
                Part::Swatches(Ctl::Colors, list(7)),
                Part::Note(if mode == "trigger" { "Each key press takes the next colour." } else { "Runs through these colours in order." }.into()),
            ],
        ),
        _ => {}
    }
    if e.get("random").is_some() && mode != "trigger" {
        look.row("Order", vec![Part::Check(Ctl::Random, e["random"].as_u64().unwrap_or(0) != 0, "Random colours")]);
    }
    if mode == "pulse" {
        look.row("Between pulses", vec![Part::Swatches(Ctl::Background, vec![rgb(&e["background"]["color"])])]);
    }
    let bri = if mode == "static" || mode == "stars" { e["brightness"].as_f64() } else { e["colors"][0]["brightness"].as_f64() }.unwrap_or(255.0);
    look.row("Brightness", vec![Part::Slider(Ctl::Brightness, (bri / 255.0) as f32, format!("{}%", (bri / 2.55).round()))]);
    if let Some((lo, hi, key)) = sp {
        let v = lo + hi - e[key].as_f64().unwrap_or(lo);
        look.row("Speed", vec![Part::Slider(Ctl::Speed, ((v - lo) / (hi - lo)) as f32, format!("{:.2} s", (lo + hi - v) / 1000.0))]);
    }
    let int = |k: &str| e[k].as_i64().unwrap_or(0);
    if mode == "spiral" {
        let d = if int("direction") > 1 { 0 } else { int("direction") };
        look.row("Direction", vec![Part::Seg(Ctl::Direction, vec![(0, "Clockwise"), (1, "Counterclockwise")], d)]);
    }
    if mode == "wave" {
        let d = if int("direction") > 3 { 0 } else { int("direction") };
        look.row("Direction", vec![Part::Seg(Ctl::Direction, vec![(0, "Up"), (1, "Down"), (3, "Left"), (2, "Right")], d)]);
    }
    if mode == "trigger" {
        let s = if int("submode") > 2 { 0 } else { int("submode") };
        look.row(
            "Lights up",
            vec![
                Part::Seg(Ctl::Submode, vec![(0, "Each key typed"), (1, "One key"), (2, "3 by 3 around it")], s),
                Part::Check(Ctl::Demo, demo, "Preview with pretend typing"),
                Part::Note("Type, or click keys above, to see it.".into()),
            ],
        );
    }
    when = Col::new(px + COL_W + 48.0, "Where and when");
    let zone = e["zone"].as_u64().unwrap_or(0x80);
    let chips = ZONES.iter().map(|(bit, name)| (*name, if *bit == 0x80 { zone & 0x80 != 0 } else { zone & 0x80 == 0 && zone & bit != 0 })).collect();
    when.row("Where", vec![Part::Chips(chips), Part::Note("Keys outside these areas stay dark.".into())]);
    when.row("Starts", vec![Part::Seg(Ctl::Start, vec![(0, "With the profile"), (1, "On a key press")], int("start"))]);
    when.row("Stops", vec![Part::Seg(Ctl::Stop, vec![(0, "Never"), (1, "On key press"), (2, "On key release")], int("stop"))]);

    let h = look.height().max(when.height());
    cells.extend(look.cells);
    cells.extend(when.cells);
    // the reset, at the foot, on the right
    let name = EFFECTS.iter().find(|(m, _)| *m == mode).map_or(mode.clone(), |(_, n)| n.to_lowercase());
    let label = format!("Reset {name} to factory settings");
    let w = text::width(500, 12.5, &label) + 18.0;
    let fy = h + 8.0 + 4.0;
    cells.push(Cell { text: label.into(), id: id(Ctl::Reset), ..cell(kind::LINK, px + 1216.0 - w, fy, w, 28.0) });
    for c in &mut cells {
        c.y += pad;
    }
    Tile { cells, height: pad + fy + 28.0 + pad }
}

/// Use a piece of the tile. `value` is a slider's place (0 to 1) or a
/// checkbox's state; `index` which swatch, chip or button.
pub fn act(page: &mut Page, c: Ctl, index: usize, value: f32) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let mode = d["lighting"]["mode"].as_str().unwrap_or("off").to_string();
    let e = &mut d["lighting"][mode.as_str()];
    match c {
        Ctl::Random => e["random"] = json!(if value > 0.5 { 1 } else { 0 }),
        Ctl::Brightness => {
            let v = (value.clamp(0.0, 1.0) * 255.0).round() as u64;
            if mode == "static" || mode == "stars" {
                e["brightness"] = json!(v);
            } else {
                if let Some(a) = e["colors"].as_array_mut() {
                    for c in a {
                        c["brightness"] = json!(v);
                    }
                }
                if e.get("background").is_some() {
                    e["background"]["brightness"] = json!(v);
                }
            }
        }
        Ctl::Speed => {
            if let Some((lo, hi, key)) = speed(&mode) {
                let v = lo + value.clamp(0.0, 1.0) as f64 * (hi - lo);
                e[key] = json!((lo + hi - v).round() as u64);
            }
        }
        Ctl::Direction => {
            let opts: &[i64] = if mode == "wave" { &[0, 1, 3, 2] } else { &[0, 1] };
            if let Some(v) = opts.get(index) {
                e["direction"] = json!(v);
            }
        }
        Ctl::Submode => e["submode"] = json!(index.min(2)),
        Ctl::Start => e["start"] = json!(index.min(1)),
        Ctl::Stop => e["stop"] = json!(index.min(2)),
        Ctl::Zone => {
            let bit = ZONES.get(index).map_or(0x80, |z| z.0);
            if bit == 0x80 {
                e["zone"] = json!(0x80);
            } else {
                let z = e["zone"].as_u64().unwrap_or(0x80);
                let mut z = if z & 0x80 != 0 { 0 } else { z };
                z ^= bit;
                e["zone"] = json!(if z == 0 { 0x80 } else { z });
            }
        }
        Ctl::Reset => {
            let fac = page.dev.as_ref().map(|dev| dev["profiles"]["254"]["lighting"][mode.as_str()].clone()).unwrap_or(Value::Null);
            if let Some(d) = page.drafts.get_mut(&n) {
                d["lighting"][mode.as_str()] = fac;
            }
        }
        _ => {}
    }
}

/// Set a colour the picker chose: the effect's colour, one of its list, the colour between pulses.
pub fn set_color(page: &mut Page, c: Ctl, index: usize, rgb: [u8; 3]) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let mode = d["lighting"]["mode"].as_str().unwrap_or("off").to_string();
    let e = &mut d["lighting"][mode.as_str()];
    match c {
        Ctl::Color => e["color"] = json!(rgb),
        Ctl::Colors => e["colors"][index]["color"] = json!(rgb),
        Ctl::Background => e["background"]["color"] = json!(rgb),
        Ctl::LcdColor => d["lighting"]["lcd"]["color"]["color"] = json!(rgb),
        _ => {}
    }
}

/// The colour a swatch shows now.
pub fn color_of(page: &Page, c: Ctl, index: usize, paint: [u8; 3]) -> [u8; 3] {
    let l = &page.draft()["lighting"];
    let e = &l[l["mode"].as_str().unwrap_or("off")];
    let v = match c {
        Ctl::Color => &e["color"],
        Ctl::Colors => &e["colors"][index]["color"],
        Ctl::Background => &e["background"]["color"],
        Ctl::LcdColor => &l["lcd"]["color"]["color"],
        _ => return paint,
    };
    [v[0].as_u64().unwrap_or(0) as u8, v[1].as_u64().unwrap_or(0) as u8, v[2].as_u64().unwrap_or(0) as u8]
}

/// Each effect's small picture, as the web app drew it with CSS, at `scale`.
pub fn icon(mode: &str, scale: f32) -> slint::Image {
    let (w, h) = ((60.0 * scale).round() as u32, (24.0 * scale).round() as u32);
    let hex = |s: &str| -> [f32; 3] {
        let s = s.trim_start_matches('#');
        let s: String = if s.len() == 3 { s.chars().flat_map(|c| [c, c]).collect() } else { s.into() };
        let v = u32::from_str_radix(&s, 16).unwrap_or(0);
        [((v >> 16) & 255) as f32, ((v >> 8) & 255) as f32, (v & 255) as f32]
    };
    // an even run of colours, as a CSS gradient with no stops given lays them
    let run = |cols: &[&str], t: f32| -> [f32; 3] {
        let c: Vec<[f32; 3]> = cols.iter().map(|s| hex(s)).collect();
        let x = t.clamp(0.0, 1.0) * (c.len() - 1) as f32;
        let i = (x.floor() as usize).min(c.len() - 2);
        let f = x - i as f32;
        [0, 1, 2].map(|k| c[i][k] + (c[i + 1][k] - c[i][k]) * f)
    };
    let dark = hex("#0c0e10");
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    let (fw, fh) = (w as f32, h as f32);
    let r = 6.0 * scale;
    for (i, p) in buf.make_mut_slice().iter_mut().enumerate() {
        let (px, py) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
        let u = px / fw;
        // in the picture's own CSS pixels
        let (cx, cy) = (px / scale, py / scale);
        let c = match mode {
            "static" => hex("#d8421e"),
            "breathing" => run(&["#140806", "#d8421e", "#140806"], u),
            "pulse" => {
                if cx % 16.0 < 10.0 {
                    hex("#1d6bff")
                } else {
                    dark
                }
            }
            "spiral" => {
                // conic, from 90deg: 0 at the right, going round clockwise
                let a = (cy - 12.0).atan2(cx - 30.0).to_degrees();
                let t = ((a + 360.0) % 360.0) / 360.0;
                run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f", "#f33"], t)
            }
            "wave" => run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f"], u),
            "stars" => {
                let mut c = dark;
                for (sx, sy, col) in [(0.2, 0.4, "#fff"), (0.6, 0.7, "#9cf"), (0.82, 0.3, "#fc9")] {
                    let d = ((cx - sx * 60.0).powi(2) + (cy - sy * 24.0).powi(2)).sqrt();
                    let a = (3.0 - d).clamp(0.0, 1.0);
                    let s = hex(col);
                    c = [0, 1, 2].map(|k| c[k] * (1.0 - a) + s[k] * a);
                }
                c
            }
            "trigger" => {
                let d = ((cx - 30.0).powi(2) + (cy - 12.0).powi(2)).sqrt();
                if d <= 5.0 {
                    hex("#3cf")
                } else if d <= 12.0 {
                    run(&["#3cf", "#123"], (d - 5.0) / 7.0)
                } else {
                    run(&["#123", "#0c0e10"], ((d - 12.0) / 10.0).min(1.0))
                }
            }
            "custom" => {
                // 135deg, as CSS lays it: along (sin, -cos) of the angle, the line
                // as long as the box's projection onto it
                let (sn, cs) = (0.70710677f32, 0.70710677f32);
                let len = 60.0 * sn + 24.0 * cs;
                let t = ((cx - 30.0) * sn + (cy - 12.0) * cs) / len + 0.5;
                match t {
                    t if t < 0.25 => hex("#f33"),
                    t if t < 0.5 => hex("#3cf"),
                    t if t < 0.75 => hex("#fd3"),
                    _ => hex("#8f5"),
                }
            }
            _ => dark,
        };
        // the rounded corners
        let qx = (r - px).max(px - (fw - r)).max(0.0);
        let qy = (r - py).max(py - (fh - r)).max(0.0);
        let a = if qx > 0.0 && qy > 0.0 { (r - (qx * qx + qy * qy).sqrt() + 0.5).clamp(0.0, 1.0) } else { 1.0 };
        *p = slint::Rgba8Pixel { r: (c[0] * a).round() as u8, g: (c[1] * a).round() as u8, b: (c[2] * a).round() as u8, a: (a * 255.0).round() as u8 };
    }
    slint::Image::from_rgba8_premultiplied(buf)
}
