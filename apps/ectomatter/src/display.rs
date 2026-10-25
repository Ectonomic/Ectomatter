//! The Z10's Settings tab: the keyboard (its connection, its firmware and
//! what that adds, its model), the same on every profile. What the sliders
//! control and the keys game mode blocks are on Keys; the screen's light and
//! what the keyboard shows on it by itself are on Screen (see [screen_cells]).

use serde_json::{json, Value};

use crate::form::TEXT_LINE;
use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::tokens::font;
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

/// The tab: the keyboard (how it is connected, its firmware and what that
/// adds, the model), a form (src/form.rs) in `room` across.
pub fn tiles(fw: &Value, connected: Option<bool>, room: f32) -> Tiles {
    let mut f = crate::form::Form::new(0.0, 0.0, room);
    f.section("Keyboard", "", |k| {
        let (tone, words) = match connected {
            Some(true) => (1, "Connected by USB."),
            Some(false) => (2, "Not connected. Ectomatter tries again every few seconds."),
            None => (0, "Ectomatter leaves this keyboard to other programs."),
        };
        k.status_row(tone, words);
        match fw["kind"].as_str().unwrap_or("unknown") {
            "custom" => {
                let v = format!("Ectomatter v{}, build {}", fw["version"].as_str().unwrap_or(""), fw["build"]);
                k.row();
                k.label_text("Firmware");
                let (x, top) = (k.control_x(), k.y);
                let w = crate::text::width(font::REGULAR, font::TEXT, &v);
                k.cells.push(text_cell(style::VALUE_BRIGHT, x, top, w + 4.0, TEXT_LINE, &v));
                k.cells.push(Cell { text: "Unofficial".into(), style: 1, ..cell(kind::PILL, x + w + 12.0, top + 1.0, crate::text::width(font::REGULAR, font::SMALL, "Unofficial") + 14.0, 18.0) });
                k.next(top, TEXT_LINE);
                let adds: Vec<String> = fw["features"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
                k.row();
                k.label_text("Adds");
                let top = k.y;
                if adds.is_empty() {
                    let end = k.note_at(k.control_x(), top + 1.0, k.control_w(), "Nothing Ectomatter controls yet. This build fixes how the volume slider steps.");
                    k.next(top, end - top);
                } else {
                    // two across, a tick before each
                    let (x, col) = (k.control_x(), k.control_w() / 2.0);
                    for (i, a) in adds.iter().enumerate() {
                        k.cells.push(Cell { text: a.into(), ..cell(kind::TICKED, x + (i % 2) as f32 * col, top + (i / 2) as f32 * 24.0, col - 8.0, TEXT_LINE) });
                    }
                    k.next(top, adds.len().div_ceil(2) as f32 * 24.0 - 4.0);
                }
                let later: Vec<String> = fw["later"].as_array().map(|a| a.iter().map(|x| format!("{} (build {})", x["name"].as_str().unwrap_or(""), x["since"])).collect()).unwrap_or_default();
                if !later.is_empty() {
                    k.row();
                    k.label_text("In later builds");
                    let (x, w, top) = (k.control_x(), k.control_w(), k.y);
                    for (i, l) in later.iter().enumerate() {
                        k.cells.push(text_cell(style::VALUE_BRIGHT, x, top + i as f32 * 24.0, w, TEXT_LINE, l));
                    }
                    k.next(top, later.len() as f32 * 24.0 - 4.0);
                }
            }
            "retail" => {
                let v = format!("EVGA v{}", fw["version"].as_str().unwrap_or(""));
                k.fact("Firmware", &v, "Features that need Ectomatter’s custom firmware show here when the keyboard runs it.");
            }
            _ => {
                let v = fw["version"].as_str().unwrap_or("");
                let v = if v.is_empty() { "Not known".to_string() } else { format!("v{v}") };
                k.fact("Firmware", &v, "This version is neither EVGA’s nor Ectomatter’s, so no custom features are offered.");
            }
        }
        let model = format!("EVGA Z10 RGB, USB {:04x}:{:04x}", evga_z10::protocol::VID, evga_z10::protocol::PID);
        k.fact("Model", &model, "");
    });
    let (cells, height) = f.end();
    Tiles { cells, height }
}

/// The screen light's ready colors: white first, the keyboard's own default.
pub const LIGHTS: [[u8; 3]; 7] = [[0xff, 0xff, 0xff], [0xff, 0x20, 0x00], [0xff, 0x8a, 0x00], [0xff, 0xd0, 0x00], [0x3b, 0xdc, 0x6b], [0x3f, 0xa9, 0xff], [0xb0, 0x6b, 0xff]];

/// The Screen tab's pieces round the screen's box, for the profile being edited, in the
/// right column's terms (its top left), `w` wide. On the tabs' row (36 tall), at its right
/// end, in the room the tabs leave from `from`, the default backlight, which lights the
/// screen in both its states: its color as its dot and its code, one button that opens
/// the color picker (`picking` while it is open on it), then how bright, on the app's
/// slider with its value. Before it, while a layout with a light of its own plays (`own`,
/// its name and its light), a line saying so.
pub fn screen_cells(page: &Page, w: f32, from: f32, own: Option<(&str, [u8; 3])>, picking: bool) -> Vec<Cell> {
    let mut c = Vec::new();
    let l = &page.draft()["lighting"];
    let lc = &l["screen"]["color"];
    let b = |v: &Value| v.as_u64().unwrap_or(0).min(255) as u8;
    let rgb = [b(&lc["color"][0]), b(&lc["color"][1]), b(&lc["color"][2])];
    let col = slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]);
    let br = lc["brightness"].as_f64().unwrap_or(255.0);
    // from the row's right end leftward: how bright, the slider, its name; the color (its dot
    // and its code, one button), the name of both. (Each text's cell runs 2 past its words,
    // into the gap after them, so the gaps are the words' own.)
    let t = |s: &str| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, s);
    let v = format!("{}%", (br / 2.55).round());
    // (the pill as wide at every value as at 100%, so the slider never shifts under the pointer)
    let pw = crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "100%") + 14.0;
    let hex = crate::lighting::hex_of(rgb);
    let cw = 4.0 + 20.0 + 8.0 + crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, &hex) + 9.0;
    // In a window too narrow for all of it beside the tabs (which end at `from`), the slider
    // shortens, to 72 at the least, then the words go: Brightness's first (the slider and its
    // value still read as how bright), then the default backlight's; the slider takes back
    // what a word leaves, to 120. Reckoned with the widest color code, so a color picked
    // never moves it.
    let widest = 4.0 + 20.0 + 8.0 + crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, "#") + 6.0 * "0123456789ABCDEF".chars().map(|d| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, &d.to_string())).fold(0.0, f32::max) + 9.0;
    let (bw, lw) = (t("Brightness"), t("Default backlight"));
    let room = w - from - pw - 12.0 - 22.0 - widest;
    let (with_bright, with_name) = if 72.0 + 10.0 + bw + 10.0 + lw <= room { (true, true) } else if 72.0 + 10.0 + lw <= room { (false, true) } else { (false, false) };
    let sl = (room - if with_bright { 10.0 + bw } else { 0.0 } - if with_name { 10.0 + lw } else { 0.0 }).clamp(72.0, 120.0);
    let mut x = w - pw;
    c.push(Cell { text: v.into(), style: 2, ..cell(kind::PILL, x, 8.0, pw, 20.0) });
    x -= 12.0 + sl;
    c.push(Cell { value: (br / 255.0) as f32, color: col, style: 2, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x, 8.0, sl, 20.0) });
    if with_bright {
        x -= 10.0 + bw;
        c.push(text_cell(style::SMALL, x, 8.0, bw + 2.0, 20.0, "Brightness"));
    }
    x -= 22.0 + cw;
    c.push(Cell { color: col, on: picking, style: 2, text: hex.into(), id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x, 4.0, cw, 28.0) });
    if with_name {
        x -= 10.0 + lw;
        c.push(text_cell(style::SMALL, x, 8.0, lw + 2.0, 20.0, "Default backlight"));
    }
    // while a layout with a color of its own plays, a line saying so, in what room the tabs
    // leave: the layout's name cut short to fit, or the line not there where not even a
    // letter of the name fits (the default backlight never moves for it)
    if let Some((name, light)) = own {
        let tail = " has its own color";
        let most = x - 20.0 - from - 16.0 - t(tail);
        let least = name.chars().next().map_or(f32::MAX, |f| t(&format!("{f}…")));
        if most >= least {
            let s = format!("{}{tail}", crate::text::elide(name, most, 400, 12.5));
            let sw = t(&s);
            x -= 20.0 + sw;
            c.push(text_cell(style::NOTE, x, 8.0, sw + 2.0, 20.0, &s));
            c.push(Cell { color: slint::Color::from_rgb_u8(light[0], light[1], light[2]), ..cell(kind::BOX, x - 16.0, 13.5, 9.0, 9.0) });
        }
    }
    c
}

/// Use a piece of the tab. `on` is a checkbox's new state; `index` which box.
pub fn act(page: &mut Page, c: Ctl, index: usize, value: f32, text: Option<&str>) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let l = &mut d["lighting"];
    match c {
        Ctl::LcdBrightness => l["screen"]["color"]["brightness"] = json!((value.clamp(0.0, 1.0) * 255.0).round() as u64),
        Ctl::LcdPreset => {
            if let Some(c) = LIGHTS.get(index) {
                l["screen"]["color"]["color"] = json!(c);
            }
        }
        Ctl::LcdApp => {
            let bit = LCD_APPS.get(index).map_or(0, |a| a.0);
            let apps = l["screen"]["apps"].as_u64().unwrap_or(0);
            l["screen"]["apps"] = json!(if value > 0.5 { apps | bit } else { apps & !bit });
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
