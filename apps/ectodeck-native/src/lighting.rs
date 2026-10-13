//! The Lighting page: beside the effects (which the page draws itself), a
//! box of the chosen effect's settings, in groups (Color, Motion), and a box
//! of where and when it plays; or, painting key by key, the brush.
//!
//! Each box is a fixed width, so its pieces are placed here from its own top
//! left, and the page places the boxes and wraps them: so chips wrap and
//! notes run on as they should, and a slider being dragged is never rebuilt
//! under the pointer.

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
    /// Where: the whole keyboard (0) or some areas (1); every area at once
    WhereMode,
    WhereAll,
    /// one of the brush's ready colors
    Preset,
    /// one of the screen light's ready colors
    LcdPreset,
    /// when the effect plays: one of PLAYS, a start and a stop together
    Plays,
    /// the screen's layout editor: a block's words, its reading (the menu),
    /// °C or °F, a 24- or 12-hour clock, what its words do when they don't fit
    /// (0 Cut off, 1 Scroll), its alignment, taking it off (it moves and sizes
    /// by dragging); the layout's screen light
    ScrText,
    ScrSource,
    ScrUnits,
    ScrHours,
    ScrFit,
    ScrAlign,
    ScrRemove,
    ScrLight,
    /// how a block showing the date writes it (the menu)
    ScrFormat,
}

const CTLS: [Ctl; 44] = [
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
    Ctl::WhereMode,
    Ctl::WhereAll,
    Ctl::Preset,
    Ctl::LcdPreset,
    Ctl::Plays,
    Ctl::ScrText,
    Ctl::ScrSource,
    Ctl::ScrUnits,
    Ctl::ScrHours,
    Ctl::ScrFit,
    Ctl::ScrAlign,
    Ctl::ScrRemove,
    Ctl::ScrLight,
    Ctl::ScrFormat,
];

pub fn ctl(id: i32) -> Ctl {
    CTLS.get(id as usize).copied().unwrap_or(Ctl::None)
}
pub fn id(c: Ctl) -> i32 {
    CTLS.iter().position(|x| *x == c).unwrap_or(0) as i32
}

/// the pieces' kinds, as the page draws them (every kind it draws is named
/// here, used from Rust or not)
#[allow(dead_code)]
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
    // the Lighting page's own pieces, in the shades of its boxes:
    // a value in a small well; a slider filled to its place; a color's dot;
    // a piece of a run of colors; a switch's choice; an area's chip; a
    // dropdown; a quiet link; a checkbox
    pub const PILL: i32 = 40;
    pub const FILLED: i32 = 41;
    pub const COLOR_DOT: i32 = 42;
    pub const STRIP: i32 = 43;
    pub const CHOICE: i32 = 44;
    pub const AREA: i32 = 45;
    pub const DROP: i32 = 46;
    pub const QUIET: i32 = 47;
    pub const CHECKBOX: i32 = 48;
    /// a thing the firmware adds, a tick before it
    pub const TICKED: i32 = 49;
}
/// the text styles
#[allow(dead_code)]
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
    /// a note in its own colour (the Key lab's menus), and a menu's
    /// words in their own colour
    pub const NOTE_IN: i32 = 19;
    pub const WORDS_IN: i32 = 20;
    /// the Key lab's small words in their own colour, and its
    /// stronger ones (a reading's number, the name of what its line tells of)
    pub const SMALL_IN: i32 = 21;
    pub const LABEL_IN: i32 = 22;
    /// a segmented button that tells the page when the pointer is over it, as a zone does
    pub const TELLS: i32 = 23;
    /// a group's heading, in small capitals; a color's code; a setting's
    /// name; the words at a slider's ends, left and right
    pub const GROUP: i32 = 33;
    pub const CODE: i32 = 34;
    pub const PROP: i32 = 35;
    pub const END_L: i32 = 36;
    pub const END_R: i32 = 37;
    /// a card's title, as a box's
    pub const BOX_TITLE: i32 = 38;
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

pub(crate) fn cell(kind: i32, x: f32, y: f32, w: f32, h: f32) -> Cell {
    Cell { kind, x, y, w, h, text: "".into(), fill: "".into(), lines: "".into(), color: slint::Color::from_rgb_u8(0, 0, 0), on: false, value: 0.0, id: 0, index: 0, style: 0 }
}
pub(crate) fn text_cell(style: i32, x: f32, y: f32, w: f32, h: f32, s: &str) -> Cell {
    Cell { text: s.into(), style, ..cell(kind::TEXT, x, y, w, h) }
}

/// The colors a profile paints its keys with, one a key: kept by the window,
/// since the keyboard stores none.
pub fn custom_colors(n: u8) -> Vec<[u8; 3]> {
    crate::uistate::get(&format!("custom:{n}")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_else(|| vec![[0; 3]; 116])
}
pub fn save_custom(n: u8, cols: &[[u8; 3]]) {
    crate::uistate::set(&format!("custom:{n}"), json!(cols));
}

/// The Lighting page's boxes beside the effects: the chosen effect's
/// settings, and where and when it plays (or, painting key by key, the
/// brush). Each box is a fixed width, so its pieces are placed here, from
/// its own top left; the page places the boxes and wraps them.
pub struct Boxes {
    /// 0 an effect's settings, 1 the keys dark, 2 painting key by key
    pub kind: i32,
    pub title: String,
    pub set: Vec<Cell>,
    pub set_h: f32,
    pub place: Vec<Cell>,
    pub place_h: f32,
}

pub const SET_W: f32 = 360.0;
pub const PLACE_W: f32 = 320.0;
/// every area at once, when Some areas is first chosen
pub const ALL_AREAS: u64 = 0x1F;
/// the brush's ready colors; black turns a key off
pub const PRESETS: [[u8; 3]; 9] = [[0xff, 0x08, 0x00], [0xff, 0x80, 0x00], [0xff, 0xd0, 0x00], [0x3b, 0xdc, 0x6b], [0x3f, 0xa9, 0xff], [0x6a, 0x3b, 0xff], [0xff, 0x3b, 0xd2], [0xff, 0xff, 0xff], [0, 0, 0]];

const RESET_ICON: &str = "M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4";
const CW: &str = "M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5";
const CCW: &str = "M4 12a8 8 0 1 0 2.6-5.9M4 4v5h5";
const UP: &str = "M12 20V5M7 10l5-5 5 5";
const DOWN: &str = "M12 4v15M7 14l5 5 5-5";
const LEFT: &str = "M20 12H5M10 7l-5 5 5 5";
const RIGHT: &str = "M4 12h15M14 7l5 5-5 5";

pub(crate) fn hex_of(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}
fn rgb3(v: &Value) -> [u8; 3] {
    [v[0].as_u64().unwrap_or(0) as u8, v[1].as_u64().unwrap_or(0) as u8, v[2].as_u64().unwrap_or(0) as u8]
}

/// A box's pieces, top to bottom, 16 in from its sides.
pub(crate) struct Stack {
    pub(crate) w: f32,
    pub(crate) y: f32,
    pub(crate) cells: Vec<Cell>,
    groups: usize,
}

impl Stack {
    pub(crate) fn new(w: f32) -> Stack {
        // under the box's title
        Stack { w, y: 48.0, cells: Vec::new(), groups: 0 }
    }
    pub(crate) fn inner(&self) -> f32 {
        self.w - 32.0
    }
    pub(crate) fn gap(&mut self, h: f32) {
        self.y += h;
    }
    pub(crate) fn note(&mut self, s: &str) {
        let lines = text::wrap(400, 12.5, s, self.inner());
        for (k, line) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::NOTE, 16.0, self.y + k as f32 * 18.0, self.inner(), 18.0, line));
        }
        self.y += lines.len() as f32 * 18.0;
    }
    /// a group: a hairline over all but the first, then its name in small capitals
    pub(crate) fn group(&mut self, name: &str) {
        if self.groups > 0 {
            self.y += 16.0;
            self.cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, 16.0, self.y, self.inner(), 1.0) });
            self.y += 15.0;
        } else {
            self.y += 6.0;
        }
        self.groups += 1;
        self.cells.push(text_cell(style::GROUP, 16.0, self.y, self.inner(), 14.0, &name.to_uppercase()));
        self.y += 14.0 + 12.0;
    }
    /// a setting's name, and its value in a small well at the right
    pub(crate) fn head(&mut self, label: &str, value: Option<&str>) {
        self.cells.push(text_cell(style::PROP, 16.0, self.y, self.inner(), 20.0, label));
        if let Some(v) = value {
            let w = text::width(500, 11.5, v) + 16.0;
            self.cells.push(Cell { text: v.into(), ..cell(kind::PILL, self.w - 16.0 - w, self.y, w, 20.0) });
        }
        self.y += 20.0 + 8.0;
    }
    /// a slider, filled to its place in `fill`, its ends named under it
    pub(crate) fn slider(&mut self, c: Ctl, frac: f32, fill: slint::Color, ends: Option<(&str, &str)>) {
        self.cells.push(Cell { value: frac, color: fill, id: id(c), ..cell(kind::FILLED, 16.0, self.y, self.inner(), 20.0) });
        self.y += 20.0;
        if let Some((l, r)) = ends {
            self.y += 4.0;
            self.cells.push(text_cell(style::END_L, 16.0, self.y, self.inner() / 2.0, 14.0, l));
            self.cells.push(text_cell(style::END_R, 16.0 + self.inner() / 2.0, self.y, self.inner() / 2.0, 14.0, r));
            self.y += 14.0;
        }
    }
    /// a run of colors, as the effect blends them
    pub(crate) fn strip(&mut self, cols: &[[u8; 3]]) {
        let n = cols.len().max(2) - 1;
        let w = self.inner() / n as f32;
        for k in 0..n {
            let (a, b) = (cols[k], cols[(k + 1).min(cols.len() - 1)]);
            let ends = (if k == 0 { 1 } else { 0 }) | (if k == n - 1 { 2 } else { 0 });
            self.cells.push(Cell { color: color(a), value: (b[0] as u32 * 65536 + b[1] as u32 * 256 + b[2] as u32) as f32, style: ends, ..cell(kind::STRIP, 16.0 + k as f32 * w, self.y, w + 0.5, 10.0) });
        }
        self.y += 10.0 + 12.0;
    }
    /// colors as dots, 8 apart, wrapping; the one the picker is open on ringed
    pub(crate) fn dots(&mut self, c: Ctl, cols: &[[u8; 3]], open: Option<usize>, code: bool) {
        let (mut x, mut y) = (16.0f32, self.y);
        for (j, col) in cols.iter().enumerate() {
            if x > 16.0 && x + 28.0 > self.w - 16.0 {
                x = 16.0;
                y += 36.0;
            }
            self.cells.push(Cell { color: color(*col), on: open == Some(j), id: id(c), index: j as i32, ..cell(kind::COLOR_DOT, x, y, 28.0, 28.0) });
            x += 36.0;
        }
        if code && cols.len() == 1 {
            self.cells.push(text_cell(style::CODE, x - 4.0, y, 90.0, 28.0, &hex_of(cols[0])));
        }
        self.y = y + 28.0;
    }
    /// a switch across the box: each choice its share, with its icon if it has one
    pub(crate) fn switch(&mut self, c: Ctl, opts: &[(&str, &str)], chosen: usize) {
        self.cells.push(Cell { style: 1, ..cell(kind::SEG, 16.0, self.y, self.inner(), 34.0) });
        let n = opts.len() as f32;
        let each = (self.inner() - 6.0 - 2.0 * (n - 1.0)) / n;
        for (j, (name, icon)) in opts.iter().enumerate() {
            self.cells.push(Cell {
                text: (*name).into(),
                lines: (*icon).into(),
                on: j == chosen,
                id: id(c),
                index: j as i32,
                ..cell(kind::CHOICE, 16.0 + 3.0 + j as f32 * (each + 2.0), self.y + 3.0, each, 28.0)
            });
        }
        self.y += 34.0;
    }
    pub(crate) fn check(&mut self, c: Ctl, on: bool, label: &str) {
        self.cells.push(Cell { on, id: id(c), text: label.into(), ..cell(kind::CHECKBOX, 16.0, self.y, 28.0 + text::width(400, 13.0, label), 20.0) });
        self.y += 20.0;
    }
    pub(crate) fn dropdown(&mut self, c: Ctl, shown: &str, open: bool) {
        self.cells.push(Cell { text: shown.into(), on: open, id: id(c), ..cell(kind::DROP, 16.0, self.y, self.inner(), 34.0) });
        self.y += 34.0;
    }
    /// areas as chips, 6 apart, wrapping, a chosen one ticked; `index` is each
    /// one's place in ZONES. Faded (style 2) while the whole keyboard is lit;
    /// `hot` is the one the pointer is over on the keyboard's picture.
    pub(crate) fn chips(&mut self, c: Ctl, chips: &[(usize, &str, bool)], faded: bool, hot: Option<usize>) {
        let (mut x, mut y) = (0.0f32, self.y);
        for (index, name, on) in chips {
            let w = text::width(400, 12.5, name) + 24.0 + if *on { 14.0 } else { 0.0 };
            if x > 0.0 && x + w > self.inner() {
                x = 0.0;
                y += 34.0;
            }
            let hot = if hot == Some(*index) { 1.0 } else { 0.0 };
            self.cells.push(Cell { text: (*name).into(), on: *on, style: if faded { 2 } else { 1 }, value: hot, id: id(c), index: *index as i32, ..cell(kind::AREA, 16.0 + x, y, w, 28.0) });
            x += w + 6.0;
        }
        self.y = y + 28.0;
    }
    /// a quiet link at the right of the row it sits on
    pub(crate) fn link_right(&mut self, c: Ctl, label: &str, y: f32, icon: &str) {
        let w = text::width(500, 12.5, label) + 18.0 + if icon.is_empty() { 0.0 } else { 20.0 };
        self.cells.push(Cell { text: label.into(), lines: icon.into(), id: id(c), ..cell(kind::QUIET, self.w - 16.0 - w + 8.0, y, w, 24.0) });
    }
    pub(crate) fn height(&self) -> f32 {
        self.y + 18.0
    }
}

/// The boxes for the profile being edited. `open` is the swatch the color
/// picker is open on, `menu` the dropdown open.
pub fn boxes(page: &Page, paint: [u8; 3], demo: bool, open: Option<(Ctl, usize)>, menu: Option<Ctl>, hot_area: Option<usize>) -> Boxes {
    let l = &page.draft()["lighting"];
    let mode = l["mode"].as_str().unwrap_or("off").to_string();
    let title = EFFECTS.iter().find(|(m, _)| *m == mode).map_or(mode.clone(), |(_, n)| n.to_string());
    let profile = page.profile_name(page.editing);
    if mode == "off" {
        let mut s = Stack::new(SET_W);
        s.note(&format!("The keys stay dark on {profile}. Choose an effect to light them."));
        return Boxes { kind: 1, title, set_h: s.height(), set: s.cells, place: Vec::new(), place_h: 0.0 };
    }
    if mode == "custom" {
        let mut s = Stack::new(SET_W);
        s.group("Color");
        let mut cols: Vec<[u8; 3]> = PRESETS.to_vec();
        let chosen = PRESETS.iter().position(|c| *c == paint);
        if chosen.is_none() {
            cols.push(paint);
        }
        let at = chosen.unwrap_or(PRESETS.len());
        // (a little closer than the effects' colors, so the ready ones and "+" share a line)
        let (mut x, mut y) = (16.0f32, s.y);
        for (j, col) in cols.iter().enumerate() {
            if x > 16.0 && x + 28.0 > SET_W - 16.0 {
                x = 16.0;
                y += 36.0;
            }
            s.cells.push(Cell { color: color(*col), on: j == at, id: id(Ctl::Preset), index: j as i32, ..cell(kind::COLOR_DOT, x, y, 28.0, 28.0) });
            x += 33.0;
        }
        // any other color, from the picker
        if x + 28.0 > SET_W - 16.0 {
            x = 16.0;
            y += 36.0;
        }
        s.cells.push(Cell { style: 1, on: open.map(|o| o.0) == Some(Ctl::Paint), id: id(Ctl::Paint), ..cell(kind::COLOR_DOT, x, y, 28.0, 28.0) });
        s.y = y + 28.0;
        s.group("Fill a zone");
        let chips: Vec<(usize, &str, bool)> = ZONES.iter().enumerate().map(|(i, (_, n))| (i, *n, false)).collect();
        s.chips(Ctl::Fill, &chips, false, None);
        s.gap(10.0);
        let row = s.y;
        let w1 = text::width(500, 12.5, "Scatter colors") + 18.0;
        s.cells.push(Cell { text: "Scatter colors".into(), id: id(Ctl::Scatter), ..cell(kind::QUIET, 8.0, row, w1, 24.0) });
        s.cells.push(Cell { text: "Clear every key".into(), id: id(Ctl::Clear), ..cell(kind::QUIET, 8.0 + w1 + 4.0, row, text::width(500, 12.5, "Clear every key") + 18.0, 24.0) });
        s.y = row + 24.0 + 12.0;
        s.note("Select keys on the keyboard to paint them. Black turns a key off. The keyboard can’t store a color for each key, so they show only while Ectodeck is open.");
        return Boxes { kind: 2, title: "Brush".into(), set_h: s.height(), set: s.cells, place: Vec::new(), place_h: 0.0 };
    }

    let e = &l[mode.as_str()];
    let int = |k: &str| e[k].as_i64().unwrap_or(0);
    let mut s = Stack::new(SET_W);
    // the reset, at the top right, level with the title
    s.link_right(Ctl::Reset, "Reset", 13.0, RESET_ICON);
    s.y -= 6.0;
    s.note(match mode.as_str() {
        "static" => "One color on every key.",
        "breathing" => "The keys fade in and out, a color each breath.",
        "pulse" => "The keys jump from one color to the next, in turn.",
        "spiral" => "Colors turn around the keyboard like a pinwheel.",
        "wave" => "Colors roll across the keyboard.",
        "stars" => "Keys twinkle on at random.",
        "trigger" => "A key lights when you press it, then fades.",
        _ => "",
    });
    s.gap(4.0);
    let picking = |c: Ctl| open.filter(|o| o.0 == c).map(|o| o.1);
    let list = |n: usize| -> Vec<[u8; 3]> { e["colors"].as_array().map(|a| a.iter().take(n).map(|c| rgb3(&c["color"])).collect()).unwrap_or_default() };
    let mut first = [255u8, 255, 255];
    if mode != "stars" {
        s.group("Color");
        match mode.as_str() {
            "static" => {
                first = rgb3(&e["color"]);
                s.dots(Ctl::Color, &[first], picking(Ctl::Color), true);
            }
            _ => {
                let cols = list(if mode == "breathing" { 2 } else { 7 });
                first = cols.first().copied().unwrap_or(first);
                s.strip(&cols);
                s.dots(Ctl::Colors, &cols, picking(Ctl::Colors), false);
            }
        }
        if e.get("random").is_some() && mode != "trigger" {
            s.gap(14.0);
            s.check(Ctl::Random, e["random"].as_u64().unwrap_or(0) != 0, "In a random order");
        }
        s.gap(16.0);
    } else {
        s.group("Color");
    }
    let bri = if mode == "static" || mode == "stars" { e["brightness"].as_f64() } else { e["colors"][0]["brightness"].as_f64() }.unwrap_or(255.0);
    s.head("Brightness", Some(&format!("{}%", (bri / 2.55).round())));
    s.slider(Ctl::Brightness, (bri / 255.0) as f32, color(first), None);
    if let Some((lo, hi, key)) = speed(&mode) {
        s.group("Motion");
        let stored = e[key].as_f64().unwrap_or(lo).clamp(lo, hi);
        // the slider runs slow to fast: right is faster
        let frac = ((hi - stored) / (hi - lo)) as f32;
        s.head("Speed", Some(&format!("{:.1} s", stored / 1000.0)));
        s.slider(Ctl::Speed, frac, slint::Color::from_rgb_u8(0xd4, 0xd4, 0xd8), Some(("Slow", "Fast")));
        if mode == "spiral" {
            s.gap(16.0);
            s.head("Direction", None);
            let d = if int("direction") > 1 { 0 } else { int("direction") } as usize;
            s.switch(Ctl::Direction, &[("Clockwise", CW), ("Counterclockwise", CCW)], d);
        }
        if mode == "wave" {
            s.gap(16.0);
            s.head("Direction", None);
            // stored 0 up, 1 down, 2 right, 3 left; shown up, down, left, right
            let d = match int("direction") {
                1 => 1,
                3 => 2,
                2 => 3,
                _ => 0,
            };
            s.switch(Ctl::Direction, &[("Up", UP), ("Down", DOWN), ("Left", LEFT), ("Right", RIGHT)], d);
        }
        if mode == "trigger" {
            s.gap(16.0);
            s.head("Lights up", None);
            let sub = if int("submode") > 2 { 0 } else { int("submode") } as usize;
            s.dropdown(Ctl::Submode, SUBMODES[sub], menu == Some(Ctl::Submode));
            s.gap(14.0);
            s.check(Ctl::Demo, demo, "Preview with pretend typing");
        }
    }

    // where and when: the whole keyboard or some areas, the areas as chips
    // (faded on the whole keyboard, where picking one lights only that), and
    // when it plays, one choice for the keyboard's start and stop together.
    // Everything sits where it always does, so nothing moves as it changes.
    let mut p = Stack::new(PLACE_W);
    p.y = 46.0;
    let zone = e["zone"].as_u64().unwrap_or(0x80);
    let whole = zone & 0x80 != 0;
    p.switch(Ctl::WhereMode, &[("Whole keyboard", ""), ("Some zones", "")], if whole { 0 } else { 1 });
    p.gap(14.0);
    let picks = if whole { 0 } else { zone & ALL_AREAS };
    let chips: Vec<(usize, &str, bool)> = ZONES.iter().enumerate().skip(1).map(|(i, (bit, name))| (i, *name, picks & bit != 0)).collect();
    p.chips(Ctl::Zone, &chips, whole, hot_area);
    p.gap(8.0);
    p.note(if whole {
        "Every key lights. Pick a zone to light only that."
    } else if picks != 0 {
        "Pick zones here or on the keyboard."
    } else {
        "Nothing lights until you pick a zone."
    });
    p.y = p.y.max(182.0) + 14.0;
    p.cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, 16.0, p.y, p.inner(), 1.0) });
    p.y += 14.0;
    p.cells.push(text_cell(style::PROP, 16.0, p.y, p.inner(), 20.0, "Plays"));
    p.y += 24.0;
    let (start, stop) = (int("start").max(0) as u64, int("stop").max(0) as u64);
    p.dropdown(Ctl::Plays, &plays_label(start, stop), menu == Some(Ctl::Plays));
    p.y += 10.0;
    Boxes { kind: 0, title, set_h: s.height(), set: s.cells, place_h: p.height(), place: p.cells }
}

/// When an effect plays, as one choice: what it is called, and the
/// keyboard's start (0 with the profile, 1 on a key press) and stop (0 never,
/// 1 on a key press, 2 on key release), EVGA's LedStartMode and LedStopMode.
pub const PLAYS: [(&str, u64, u64); 6] = [
    ("All the time", 0, 0),
    ("Until you press a key", 0, 1),
    ("Until you let go of a key", 0, 2),
    ("From your first key press", 1, 0),
    ("While you hold a key", 1, 2),
    ("From one key press to the next", 1, 1),
];

/// Which of PLAYS a stored start and stop are, if either is one the keyboard knows.
pub fn plays_of(start: u64, stop: u64) -> Option<usize> {
    PLAYS.iter().position(|(_, a, b)| *a == start && *b == stop)
}

/// What Plays shows: its choice, or what is stored if it is none of them.
pub fn plays_label(start: u64, stop: u64) -> String {
    plays_of(start, stop).map_or_else(|| format!("Something else (start {start}, stop {stop})"), |i| PLAYS[i].0.to_string())
}

/// The choices the dropdowns list, in the order the keyboard numbers them.
pub const STARTS: [&str; 2] = ["With the profile", "On a key press"];
pub const STOPS: [&str; 3] = ["Never", "On the next key press", "On key release"];
pub const SUBMODES: [&str; 3] = ["Each key you type", "One key", "The key and the keys around it"];

/// The choices for a dropdown, and the one chosen.
pub fn choices(page: &Page, c: Ctl) -> Option<(&'static [&'static str], usize)> {
    let l = &page.draft()["lighting"];
    let e = &l[l["mode"].as_str().unwrap_or("off")];
    let int = |k: &str| e[k].as_i64().unwrap_or(0).max(0) as usize;
    match c {
        Ctl::Start => Some((&STARTS[..], int("start").min(1))),
        Ctl::Stop => Some((&STOPS[..], int("stop").min(2))),
        Ctl::Submode => Some((&SUBMODES[..], if int("submode") > 2 { 0 } else { int("submode") })),
        _ => None,
    }
}

/// Where the effect plays: the whole keyboard or some areas, an area in or
/// out, or every area. `areas` keeps the areas chosen while the whole
/// keyboard is lit, so choosing Some areas again brings them back.
pub fn place(page: &mut Page, c: Ctl, index: usize, areas: &mut u64) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let mode = d["lighting"]["mode"].as_str().unwrap_or("off").to_string();
    let e = &mut d["lighting"][mode.as_str()];
    let zone = e["zone"].as_u64().unwrap_or(0x80);
    let z = match c {
        Ctl::WhereMode if index == 0 => {
            if zone & 0x80 == 0 {
                *areas = zone & ALL_AREAS;
            }
            0x80
        }
        Ctl::WhereMode => {
            if zone & 0x80 == 0 {
                zone
            } else {
                *areas
            }
        }
        Ctl::WhereAll => ALL_AREAS,
        Ctl::Zone => {
            let bit = ZONES.get(index).map_or(0, |z| z.0) & ALL_AREAS;
            // on the whole keyboard, an area picked is the only one lit
            if zone & 0x80 != 0 { bit } else { zone ^ bit }
        }
        _ => return,
    };
    if z & 0x80 == 0 {
        *areas = z;
    }
    e["zone"] = json!(z);
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
        Ctl::Plays => {
            if let Some((_, a, b)) = PLAYS.get(index) {
                e["start"] = json!(a);
                e["stop"] = json!(b);
            }
        }
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

/// Each effect's small picture, for its tile on the Lighting page, at `scale`:
/// 72 by 40, and Wave's twice as wide, its colors round twice, to roll along.
pub fn icon(mode: &str, scale: f32) -> slint::Image {
    const W: f32 = 72.0;
    const H: f32 = 40.0;
    // (Wave rolls along a picture twice as wide; Pulse is five tiles wide, a color each)
    let across = if mode == "wave" { 2.0 } else if mode == "pulse" { 5.0 } else { 1.0 };
    let (w, h) = ((W * across * scale).round() as u32, (H * scale).round() as u32);
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
    // (square: the tile rounds its corners)
    let r = 0.0f32;
    for (i, p) in buf.make_mut_slice().iter_mut().enumerate() {
        let (px, py) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
        let u = px / fw;
        // in the picture's own CSS pixels
        let (cx, cy) = (px / scale, py / scale);
        let c = match mode {
            "static" => hex("#d8421e"),
            "breathing" => run(&["#140806", "#d8421e", "#140806"], u),
            // five colors side by side, hard against each other: the keys take
            // each in turn, with nothing between (the tile, chosen, shows one
            // at a time)
            "pulse" => hex(["#f33", "#fd3", "#3f6", "#3cf", "#94f"][((u * 5.0) as usize).min(4)]),
            "spiral" => {
                // conic, from 90deg: 0 at the right, going round clockwise
                let a = (cy - H / 2.0).atan2(cx - W / 2.0).to_degrees();
                let t = ((a + 360.0) % 360.0) / 360.0;
                run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f", "#f33"], t)
            }
            "wave" => run(&["#f33", "#fd3", "#3f6", "#3cf", "#94f", "#f33"], (u * across) % 1.0),
            "stars" => {
                let mut c = dark;
                for (sx, sy, col) in [(0.2, 0.4, "#fff"), (0.7, 0.65, "#9cf"), (0.85, 0.25, "#fc9"), (0.42, 0.78, "#fff")] {
                    let d = ((cx - sx * W).powi(2) + (cy - sy * H).powi(2)).sqrt();
                    let a = (3.2 - d).clamp(0.0, 1.0);
                    let s = hex(col);
                    c = [0, 1, 2].map(|k| c[k] * (1.0 - a) + s[k] * a);
                }
                c
            }
            "trigger" => {
                let d = ((cx - W / 2.0).powi(2) + (cy - H / 2.0).powi(2)).sqrt();
                if d <= 4.0 {
                    hex("#7fe3ff")
                } else if d <= 10.0 {
                    run(&["#7fe3ff", "#1a7dc4"], (d - 4.0) / 6.0)
                } else {
                    run(&["#1a7dc4", "#0e0e12"], ((d - 10.0) / 10.0).min(1.0))
                }
            }
            "custom" => {
                // 135deg, as CSS lays it: along (sin, -cos) of the angle, the line
                // as long as the box's projection onto it
                let (sn, cs) = (0.70710677f32, 0.70710677f32);
                let len = W * sn + H * cs;
                let t = ((cx - W / 2.0) * sn + (cy - H / 2.0) * cs) / len + 0.5;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with(zone: u64) -> Page {
        let mut p = Page::new();
        p.editing = 1;
        p.drafts.insert(1, json!({ "lighting": { "mode": "spiral", "spiral": { "zone": zone } } }));
        p
    }
    fn zone(p: &Page) -> u64 {
        p.drafts[&1]["lighting"]["spiral"]["zone"].as_u64().unwrap()
    }

    #[test]
    fn some_areas_remembers_its_areas_and_never_snaps_back() {
        let mut p = page_with(0x80);
        let mut areas = ALL_AREAS;
        // Some areas, first time: every area
        place(&mut p, Ctl::WhereMode, 1, &mut areas);
        assert_eq!(zone(&p), ALL_AREAS);
        // every area taken out, one by one: nothing lit, and still Some areas
        for i in 1..ZONES.len() {
            place(&mut p, Ctl::Zone, i, &mut areas);
        }
        assert_eq!(zone(&p), 0, "no areas chosen lights nothing; it is not the whole keyboard");
        // two put back, then the whole keyboard, then Some areas again: the two come back
        place(&mut p, Ctl::Zone, 1, &mut areas);
        place(&mut p, Ctl::Zone, 3, &mut areas);
        let two = ZONES[1].0 | ZONES[3].0;
        assert_eq!(zone(&p), two);
        place(&mut p, Ctl::WhereMode, 0, &mut areas);
        assert_eq!(zone(&p), 0x80);
        place(&mut p, Ctl::WhereMode, 1, &mut areas);
        assert_eq!(zone(&p), two);
        place(&mut p, Ctl::WhereAll, 0, &mut areas);
        assert_eq!(zone(&p), ALL_AREAS);
    }

    #[test]
    fn an_area_picked_on_the_whole_keyboard_is_the_only_one_lit() {
        let mut p = page_with(0x80);
        let mut areas = ALL_AREAS;
        place(&mut p, Ctl::Zone, 3, &mut areas);
        assert_eq!(zone(&p), ZONES[3].0, "WASD alone, and Some areas");
        // then another added, and the first taken out again
        place(&mut p, Ctl::Zone, 1, &mut areas);
        assert_eq!(zone(&p), ZONES[3].0 | ZONES[1].0);
        place(&mut p, Ctl::Zone, 3, &mut areas);
        assert_eq!(zone(&p), ZONES[1].0);
        // the whole keyboard and back keeps it
        place(&mut p, Ctl::WhereMode, 0, &mut areas);
        place(&mut p, Ctl::WhereMode, 1, &mut areas);
        assert_eq!(zone(&p), ZONES[1].0);
    }

    #[test]
    fn plays_is_the_keyboards_start_and_stop_both_ways() {
        // the six, each the pair EVGA's LedStartMode and LedStopMode name
        let want = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 2), (1, 1)];
        for (i, (start, stop)) in want.iter().enumerate() {
            let mut p = page_with(0x80);
            act(&mut p, Ctl::Plays, i, 0.0);
            let e = &p.drafts[&1]["lighting"]["spiral"];
            assert_eq!((e["start"].as_u64().unwrap(), e["stop"].as_u64().unwrap()), (*start, *stop), "{}", PLAYS[i].0);
            assert_eq!(plays_of(*start, *stop), Some(i));
            assert_eq!(plays_label(*start, *stop), PLAYS[i].0);
        }
        // every pair the keyboard knows has a name; one it doesn't is said as it is
        assert_eq!(plays_label(0, 0), "All the time");
        assert_eq!(plays_of(2, 0), None);
        assert_eq!(plays_label(3, 0), "Something else (start 3, stop 0)");
    }
}
