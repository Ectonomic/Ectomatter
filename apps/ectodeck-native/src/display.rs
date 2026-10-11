//! The Z10's Settings tab: the keyboard (its connection, its firmware and
//! what that adds, its model), the same on every profile. What the sliders
//! control and the keys game mode blocks are on Keys; the screen's light and
//! the screens it shows by itself are on Screen (see [closed]).

use serde_json::{json, Value};

use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::z10::Page;
use crate::Cell;

pub const LCD_APPS: [(u64, &str); 5] = [(0x20, "Time"), (0x04, "Profile number"), (0x08, "Game mode"), (0x01, "Key rollover"), (0x02, "EVGA Precision X1 stats")];
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

/// The Screen tab's two boxes under Profiles, `w` wide, for the profile being
/// edited: its default backlight (the ready colors, or one of its own from the
/// picker, and how bright), which the screen uses while Ectodeck isn't running
/// and for any screen layout without a set color; then, 18 under it, the items
/// on the keyboard's status screen, which shows while Ectodeck isn't running.
/// The cells, in the two boxes' own terms (the first box's corner); their
/// headings are drawn by the page, as the Profiles box's is. The height of
/// both, and of the first.
pub fn defaults(page: &Page, w: f32) -> (Tiles, f32) {
    let mut c = Vec::new();
    let l = &page.draft()["lighting"];
    let x = 16.0;
    let inner = w - 32.0;
    let lc = &l["lcd"]["color"];
    let b = |v: &Value| v.as_u64().unwrap_or(0).min(255) as u8;
    let rgb = [b(&lc["color"][0]), b(&lc["color"][1]), b(&lc["color"][2])];
    let col = slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]);
    // how bright first, its value at the right; the slider under it
    let br = lc["brightness"].as_f64().unwrap_or(255.0);
    c.push(text_cell(style::SMALL, x, 38.0, inner - 60.0, 20.0, "Brightness"));
    let v = format!("{}%", (br / 2.55).round());
    let pw = crate::text::width(400, 12.0, &v) + 14.0;
    c.push(Cell { text: v.into(), style: 2, ..cell(kind::PILL, w - 16.0 - pw, 38.0, pw, 20.0) });
    c.push(Cell { value: (br / 255.0) as f32, color: col, style: 1, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x, 60.0, inner, 20.0) });
    // then the colors: the dots 20 across, spread evenly from the box's inner edge to the
    // other (as the slider runs), each in a cell with room for its ring
    let (dot, ring) = (20.0, 3.5);
    let step = (inner - dot) / LIGHTS.len() as f32;
    let preset = LIGHTS.iter().position(|p| *p == rgb);
    for (i, p) in LIGHTS.iter().enumerate() {
        let col = slint::Color::from_rgb_u8(p[0], p[1], p[2]);
        c.push(Cell { color: col, on: preset == Some(i), id: id(Ctl::LcdPreset), index: i as i32, ..cell(kind::COLOR_DOT, x - ring + i as f32 * step, 94.0 - ring, dot + 2.0 * ring, dot + 2.0 * ring) });
    }
    // a color of its own: shown as itself once chosen, otherwise a dot to choose one
    let own = preset.is_none();
    c.push(Cell { color: col, on: own, style: if own { 0 } else { 1 }, id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x - ring + LIGHTS.len() as f32 * step, 94.0 - ring, dot + 2.0 * ring, dot + 2.0 * ring) });
    let mut y = 128.0;
    for line in crate::text::wrap(400, 12.0, "The color the LCD backlight uses when Ectodeck isn’t running, and for any screen layout without a set color.", inner) {
        c.push(text_cell(style::CREDIT, x, y, inner, 17.0, &line));
        y += 17.0;
    }
    let first = y + 16.0;
    // the status screen's box: its items, then what it is
    let top = first + 18.0;
    let mut y = top + 38.0;
    let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
    // (not EVGA Precision X1's stats: EVGA makes no graphics cards now, so nothing sends them)
    for (i, (bit, n)) in LCD_APPS.iter().enumerate().filter(|(_, (bit, _))| *bit != 0x02) {
        c.push(Cell { on: apps & bit != 0, id: id(Ctl::LcdApp), index: i as i32, text: (*n).into(), ..cell(kind::CHECKBOX, x, y, inner, 20.0) });
        y += 30.0;
    }
    let mut y = y - 30.0 + 20.0 + 14.0;
    for line in crate::text::wrap(400, 12.0, "What the keyboard shows when Ectodeck isn’t running.", inner) {
        c.push(text_cell(style::CREDIT, x, y, inner, 17.0, &line));
        y += 17.0;
    }
    (Tiles { cells: c, height: y + 16.0 }, first)
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
