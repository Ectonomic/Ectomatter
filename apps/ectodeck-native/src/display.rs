//! The Z10's Settings tab: the keyboard (its connection, its firmware and
//! what that adds, its model), the same on every profile. What the sliders
//! control and the keys game mode blocks are on Keys; the screen's light and
//! the screens it shows by itself are on Screen (see [closed]).

use serde_json::{json, Value};

use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::z10::Page;
use crate::Cell;

pub const LCD_APPS: [(u64, &str); 5] = [(0x20, "Clock"), (0x04, "Profile number"), (0x08, "Keyboard mode"), (0x01, "Rollover setting"), (0x02, "EVGA Precision X1 stats")];
pub const GAME_KEYS: [(u64, &str); 7] = [(0x40, "Windows key"), (0x01, "Alt+Tab"), (0x08, "Alt+F4"), (0x04, "Ctrl+Esc"), (0x02, "Tab"), (0x10, "Left Shift"), (0x20, "Menu key")];

/// What each slider can control. "own" is what the keyboard does by itself.
pub const BIND_CHOICES: [[(&str, &str); 4]; 2] = [
    [("own", "Keyboard brightness"), ("output", "System volume"), ("microphone", "Microphone volume"), ("none", "Nothing")],
    [("own", "System volume"), ("backlight", "Keyboard brightness"), ("microphone", "Microphone volume"), ("none", "Nothing")],
];

/// What a slider really drives: left on its own job, the dimmer drives the
/// backlight and the volume slider the computer's volume.
pub fn drives(i: usize, v: &str) -> String {
    if v == "own" { if i == 0 { "backlight".into() } else { "output".into() } } else { v.into() }
}

pub fn has(fw: &Value, feature: &str) -> bool {
    fw["features"].as_array().is_some_and(|a| a.iter().any(|f| f["id"] == feature))
}

pub struct Tiles {
    pub cells: Vec<Cell>,
    pub height: f32,
}

/// The cards' width: the tab is one column of them, in the middle of the page.
pub const CARD_W: f32 = 820.0;
const LABEL_W: f32 = 198.0;

/// A card's pieces, from its top: its title, then rows of a name and its value, a hairline between.
struct Card {
    y0: f32,
    y: f32,
    cells: Vec<Cell>,
}

impl Card {
    fn new(y0: f32, title: &str, cells: &mut Vec<Cell>) -> Card {
        let _ = cells;
        let mut c = Card { y0, y: y0 + 48.0, cells: Vec::new() };
        c.cells.push(text_cell(style::BOX_TITLE, 24.0, y0 + 16.0, CARD_W - 48.0, 22.0, title));
        c
    }
    fn rule(&mut self) {
        self.y += 14.0;
        self.cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, 24.0, self.y, CARD_W - 48.0, 1.0) });
        self.y += 15.0;
    }
    /// a row: its name, and what goes in the value's column, `h` tall
    fn row(&mut self, label: &str, h: f32, value: impl FnOnce(&mut Vec<Cell>, f32, f32)) {
        self.rule();
        self.cells.push(text_cell(style::FACT, 24.0, self.y, LABEL_W, 20.0, label));
        value(&mut self.cells, 24.0 + LABEL_W, self.y);
        self.y += h.max(20.0);
    }
    fn note(&mut self, x: f32, s: &str) {
        let w = CARD_W - 24.0 - x;
        let lines = crate::text::wrap(400, 12.5, s, w);
        for (k, line) in lines.iter().enumerate() {
            self.cells.push(text_cell(style::NOTE, x, self.y + k as f32 * 18.0, w, 18.0, line));
        }
        self.y += lines.len() as f32 * 18.0;
    }
    /// the card itself under its pieces; where the next one goes
    fn close(mut self, out: &mut Vec<Cell>) -> f32 {
        let h = self.y + 22.0 - self.y0;
        out.push(Cell { style: 1, ..cell(kind::TILE, 0.0, self.y0, CARD_W, h) });
        out.append(&mut self.cells);
        self.y0 + h + 12.0
    }
}

/// The tab: the keyboard (how it is connected, its firmware and what that
/// adds, the model).
pub fn tiles(fw: &Value, connected: Option<bool>) -> Tiles {
    let mut cells = Vec::new();
    let mut k = Card::new(0.0, "Keyboard", &mut cells);
    let (tone, words) = match connected {
        Some(true) => (1, "Connected by USB."),
        Some(false) => (2, "Not connected. Ectodeck tries again every few seconds."),
        None => (0, "Ectodeck leaves this keyboard to other programs."),
    };
    k.cells.push(Cell { style: tone, ..cell(kind::DOT, 24.0, k.y + 6.5, 7.0, 7.0) });
    k.cells.push(text_cell(style::SMALL, 39.0, k.y, CARD_W - 63.0, 20.0, words));
    k.y += 20.0;
    match fw["kind"].as_str().unwrap_or("unknown") {
        "custom" => {
            let v = format!("Ectodeck v{}, build {}", fw["version"].as_str().unwrap_or(""), fw["build"]);
            k.row("Firmware", 20.0, |c, x, y| {
                let w = crate::text::width(500, 13.0, &v);
                c.push(text_cell(style::FACT_VALUE, x, y, w + 4.0, 20.0, &v));
                c.push(Cell { text: "Unofficial".into(), style: 1, ..cell(kind::PILL, x + w + 12.0, y + 1.0, crate::text::width(400, 11.0, "Unofficial") + 14.0, 18.0) });
            });
            let adds: Vec<String> = fw["features"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
            if adds.is_empty() {
                k.rule();
                k.cells.push(text_cell(style::FACT, 24.0, k.y, LABEL_W, 20.0, "Adds"));
                k.note(24.0 + LABEL_W, "Nothing Ectodeck controls yet. This build fixes how the volume slider steps.");
            } else {
                // two across, a tick before each
                let rows = adds.len().div_ceil(2);
                k.row("Adds", rows as f32 * 24.0 - 4.0, |c, x, y| {
                    let col = (CARD_W - 24.0 - x) / 2.0;
                    for (i, a) in adds.iter().enumerate() {
                        c.push(Cell { text: a.into(), ..cell(kind::TICKED, x + (i % 2) as f32 * col, y + (i / 2) as f32 * 24.0, col - 8.0, 20.0) });
                    }
                });
            }
            let later: Vec<String> = fw["later"].as_array().map(|a| a.iter().map(|x| format!("{} (build {})", x["name"].as_str().unwrap_or(""), x["since"])).collect()).unwrap_or_default();
            if !later.is_empty() {
                k.row("In later builds", later.len() as f32 * 24.0 - 4.0, |c, x, y| {
                    for (i, l) in later.iter().enumerate() {
                        c.push(text_cell(style::FACT_VALUE, x, y + i as f32 * 24.0, CARD_W - 24.0 - x, 20.0, l));
                    }
                });
            }
        }
        "retail" => {
            let v = format!("EVGA v{}", fw["version"].as_str().unwrap_or(""));
            k.row("Firmware", 20.0, |c, x, y| c.push(text_cell(style::FACT_VALUE, x, y, CARD_W - 24.0 - x, 20.0, &v)));
            k.y += 6.0;
            k.note(24.0 + LABEL_W, "Features that need Ectodeck’s custom firmware show here when the keyboard runs it.");
        }
        _ => {
            let v = fw["version"].as_str().unwrap_or("");
            let v = if v.is_empty() { "Not known".to_string() } else { format!("v{v}") };
            k.row("Firmware", 20.0, |c, x, y| c.push(text_cell(style::FACT_VALUE, x, y, CARD_W - 24.0 - x, 20.0, &v)));
            k.y += 6.0;
            k.note(24.0 + LABEL_W, "This version is neither EVGA’s nor Ectodeck’s, so no custom features are offered.");
        }
    }
    let model = format!("EVGA Z10 RGB, USB {:04x}:{:04x}", ectotype_core::protocol::VID, ectotype_core::protocol::PID);
    k.row("Model", 20.0, |c, x, y| c.push(text_cell(style::FACT_VALUE, x, y, CARD_W - 24.0 - x, 20.0, &model)));
    let y = k.close(&mut cells);
    Tiles { cells, height: y - 12.0 }
}

/// The screen light's ready colors: white first, the keyboard's own default.
pub const LIGHTS: [[u8; 3]; 7] = [[0xff, 0xff, 0xff], [0xff, 0x20, 0x00], [0xff, 0x8a, 0x00], [0xff, 0xd0, 0x00], [0x3b, 0xdc, 0x6b], [0x3f, 0xa9, 0xff], [0xb0, 0x6b, 0xff]];

/// The Screen tab's "When Ectodeck is closed" box, `w` wide, for the profile
/// being edited: its screen light (the ready colors, or one of its own from
/// the picker), how bright, and which of the keyboard's own screens show.
/// The pieces sit in the box, under its title.
pub fn closed(page: &Page, w: f32) -> Tiles {
    let mut c = Vec::new();
    let l = &page.draft()["lighting"];
    let x = 16.0;
    let inner = w - 32.0;
    c.push(text_cell(style::END_R, w - 16.0 - 150.0, 14.0, 150.0, 22.0, &page.profile_name(page.editing)));
    let lc = &l["lcd"]["color"];
    let b = |v: &Value| v.as_u64().unwrap_or(0).min(255) as u8;
    let rgb = [b(&lc["color"][0]), b(&lc["color"][1]), b(&lc["color"][2])];
    let mut y = 48.0;
    c.push(text_cell(style::PROP, x, y, inner, 20.0, "Default screen light"));
    y += 26.0;
    let step = 34.0;
    let preset = LIGHTS.iter().position(|p| *p == rgb);
    for (i, p) in LIGHTS.iter().enumerate() {
        let col = slint::Color::from_rgb_u8(p[0], p[1], p[2]);
        c.push(Cell { color: col, on: preset == Some(i), id: id(Ctl::LcdPreset), index: i as i32, ..cell(kind::COLOR_DOT, x - 2.0 + i as f32 * step, y, 28.0, 28.0) });
    }
    // a color of its own: shown as itself once chosen, otherwise a dot to choose one
    let own = preset.is_none();
    let col = slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]);
    c.push(Cell { color: col, on: own, style: if own { 0 } else { 1 }, id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x - 2.0 + LIGHTS.len() as f32 * step, y, 28.0, 28.0) });
    y += 28.0 + 16.0;
    let br = lc["brightness"].as_f64().unwrap_or(255.0);
    c.push(text_cell(style::PROP, x, y, inner - 60.0, 20.0, "Brightness"));
    let v = format!("{}%", (br / 2.55).round());
    let pw = crate::text::width(500, 11.5, &v) + 16.0;
    c.push(Cell { text: v.into(), ..cell(kind::PILL, w - 16.0 - pw, y, pw, 20.0) });
    y += 28.0;
    c.push(Cell { value: (br / 255.0) as f32, color: col, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x, y, inner, 20.0) });
    y += 20.0 + 18.0;
    c.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, x, y, inner, 1.0) });
    y += 15.0;
    c.push(text_cell(style::NOTE, x, y, inner, 18.0, "The Z10 shows its own screens:"));
    y += 28.0;
    let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
    // (not EVGA Precision X1's stats: EVGA makes no graphics cards now, so nothing sends them)
    for (i, (bit, n)) in LCD_APPS.iter().enumerate().filter(|(_, (bit, _))| *bit != 0x02) {
        c.push(Cell { on: apps & bit != 0, id: id(Ctl::LcdApp), index: i as i32, text: (*n).into(), ..cell(kind::CHECKBOX, x, y, inner, 20.0) });
        y += 28.0;
    }
    Tiles { cells: c, height: y - 8.0 + 16.0 }
}

/// The same box laid out across the Library's width, `w`: the screen light,
/// how bright, and the keyboard's own screens, side by side.
pub fn closed_wide(page: &Page, w: f32) -> Tiles {
    let mut c = Vec::new();
    let l = &page.draft()["lighting"];
    c.push(text_cell(style::END_R, w - 16.0 - 150.0, 14.0, 150.0, 22.0, &page.profile_name(page.editing)));
    let lc = &l["lcd"]["color"];
    let b = |v: &Value| v.as_u64().unwrap_or(0).min(255) as u8;
    let rgb = [b(&lc["color"][0]), b(&lc["color"][1]), b(&lc["color"][2])];
    // three columns, 1.15 to 1 to 1.25, 32 apart
    let inner = w - 32.0;
    let unit = (inner - 64.0) / 3.4;
    let (x1, w1) = (16.0, unit * 1.15);
    let (x2, w2) = (x1 + w1 + 32.0, unit);
    let (x3, w3) = (x2 + w2 + 32.0, unit * 1.25);
    let (head, row) = (48.0, 74.0);
    c.push(text_cell(style::PROP, x1, head, w1, 20.0, "Default screen light"));
    let step = 34.0;
    let preset = LIGHTS.iter().position(|p| *p == rgb);
    for (i, p) in LIGHTS.iter().enumerate() {
        let col = slint::Color::from_rgb_u8(p[0], p[1], p[2]);
        c.push(Cell { color: col, on: preset == Some(i), id: id(Ctl::LcdPreset), index: i as i32, ..cell(kind::COLOR_DOT, x1 - 2.0 + i as f32 * step, row, 28.0, 28.0) });
    }
    let own = preset.is_none();
    let col = slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]);
    c.push(Cell { color: col, on: own, style: if own { 0 } else { 1 }, id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x1 - 2.0 + LIGHTS.len() as f32 * step, row, 28.0, 28.0) });
    let br = lc["brightness"].as_f64().unwrap_or(255.0);
    c.push(text_cell(style::PROP, x2, head, w2 - 60.0, 20.0, "Brightness"));
    let v = format!("{}%", (br / 2.55).round());
    let pw = crate::text::width(500, 11.5, &v) + 16.0;
    c.push(Cell { text: v.into(), ..cell(kind::PILL, x2 + w2 - pw, head, pw, 20.0) });
    c.push(Cell { value: (br / 255.0) as f32, color: col, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x2, row + 4.0, w2, 20.0) });
    c.push(text_cell(style::NOTE, x3, head, w3, 20.0, "The Z10 shows its own screens:"));
    let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
    // (not EVGA Precision X1's stats: EVGA makes no graphics cards now, so nothing sends them)
    let shown: Vec<(usize, &(u64, &str))> = LCD_APPS.iter().enumerate().filter(|(_, (bit, _))| *bit != 0x02).collect();
    for (k, (i, (bit, n))) in shown.iter().enumerate() {
        let (cx, cy) = (x3 + (k % 2) as f32 * (w3 / 2.0), row + 2.0 + (k / 2) as f32 * 28.0);
        c.push(Cell { on: apps & bit != 0, id: id(Ctl::LcdApp), index: *i as i32, text: (*n).into(), ..cell(kind::CHECKBOX, cx, cy, w3 / 2.0 - 8.0, 20.0) });
    }
    let rows = shown.len().div_ceil(2) as f32;
    Tiles { cells: c, height: row + 2.0 + rows * 28.0 - 8.0 + 22.0 }
}

/// Whether the closed box fits across a Library `w` wide.
pub fn closed_fits_wide(w: f32) -> bool {
    w >= 880.0
}

/// Use a piece of the tab. `on` is a checkbox's new state; `index` which box.
pub fn act(page: &mut Page, c: Ctl, index: usize, value: f32, text: Option<&str>) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let l = &mut d["lighting"];
    match c {
        Ctl::LcdBrightness => l["lcd"]["color"]["brightness"] = json!((value.clamp(0.0, 1.0) * 255.0).round() as u64),
        Ctl::LcdPreset => {
            if let Some(c) = LIGHTS.get(index) {
                l["lcd"]["color"]["color"] = json!(c);
            }
        }
        Ctl::LcdApp => {
            let bit = LCD_APPS.get(index).map_or(0, |a| a.0);
            let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
            l["lcd"]["apps"] = json!(if value > 0.5 { apps | bit } else { apps & !bit });
        }
        Ctl::GameKey => {
            let bit = GAME_KEYS.get(index).map_or(0, |a| a.0);
            let blocked = !l["game_keys"].as_u64().unwrap_or(0) & 0x7f;
            let b = if value > 0.5 { blocked | bit } else { blocked & !bit };
            l["game_keys"] = json!((!b & 0x7f) | 0x80);
        }
        Ctl::Name => {
            if let Some(t) = text {
                d["name"] = json!(t.chars().take(60).collect::<String>());
            }
        }
        _ => {}
    }
}
