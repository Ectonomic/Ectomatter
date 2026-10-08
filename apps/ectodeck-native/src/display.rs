//! The Z10's Settings tab: one column of cards, the same on every profile
//! but the screen's: the keyboard (its connection, its firmware and what that
//! adds, its model), and the screen's light and the screens it shows by
//! itself. What the sliders control and the keys game mode blocks are on Keys.

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
/// adds, the model), then the screen's own settings for the profile.
pub fn tiles(page: &Page, fw: &Value, connected: Option<bool>) -> Tiles {
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
    let mut y = k.close(&mut cells);

    // the screen, for this profile: its light and the screens it shows by itself
    let l = &page.draft()["lighting"];
    let mut sc = Card::new(y, "Screen", &mut cells);
    sc.note(24.0, &format!("For {}, when Ectodeck isn’t drawing on the screen.", page.profile_name(page.editing)));
    let lc = &l["lcd"]["color"];
    let c = &lc["color"];
    let col = slint::Color::from_rgb_u8(c[0].as_u64().unwrap_or(0) as u8, c[1].as_u64().unwrap_or(0) as u8, c[2].as_u64().unwrap_or(0) as u8);
    sc.row("Screen light", 28.0, |cs, x, y| cs.push(Cell { color: col, id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x - 2.0, y - 4.0, 28.0, 28.0) }));
    let b = lc["brightness"].as_f64().unwrap_or(255.0);
    sc.row("Brightness", 20.0, |cs, x, y| {
        let w = CARD_W - 24.0 - x - 60.0;
        cs.push(Cell { value: (b / 255.0) as f32, color: col, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x, y, w, 20.0) });
        let v = format!("{}%", (b / 2.55).round());
        let pw = crate::text::width(500, 11.5, &v) + 16.0;
        cs.push(Cell { text: v.into(), ..cell(kind::PILL, CARD_W - 24.0 - pw, y, pw, 20.0) });
    });
    let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
    // (not EVGA Precision X1's stats: EVGA makes no graphics cards now, so nothing sends them)
    let shown: Vec<(usize, &str, bool)> = LCD_APPS.iter().enumerate().filter(|(_, (bit, _))| *bit != 0x02).map(|(i, (bit, n))| (i, *n, apps & bit != 0)).collect();
    sc.row("Built-in screens", shown.len().div_ceil(2) as f32 * 28.0 - 8.0, |cs, x, y| {
        let colw = (CARD_W - 24.0 - x) / 2.0;
        for (k, (i, n, on)) in shown.iter().enumerate() {
            cs.push(Cell { on: *on, id: id(Ctl::LcdApp), index: *i as i32, text: (*n).into(), ..cell(kind::CHECKBOX, x + (k % 2) as f32 * colw, y + (k / 2) as f32 * 28.0, colw - 8.0, 20.0) });
        }
    });
    sc.y += 10.0;
    sc.note(24.0 + LABEL_W, "Screens the keyboard shows by itself. Ectodeck’s own pages draw over them while it runs.");
    y = sc.close(&mut cells);
    Tiles { cells, height: y - 12.0 }
}

/// Use a piece of the tab. `on` is a checkbox's new state; `index` which box.
pub fn act(page: &mut Page, c: Ctl, index: usize, value: f32, text: Option<&str>) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let l = &mut d["lighting"];
    match c {
        Ctl::LcdBrightness => l["lcd"]["color"]["brightness"] = json!((value.clamp(0.0, 1.0) * 255.0).round() as u64),
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
