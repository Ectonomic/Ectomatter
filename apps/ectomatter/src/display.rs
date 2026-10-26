//! The Z10's Settings tab: the keyboard (its connection, its firmware and
//! what that adds, its model), the same on every profile. What the sliders
//! control and the keys game mode blocks are on Keys; the screen's light and
//! what the keyboard shows on it by itself are on Screen (see [screen_cells]).

use serde_json::{json, Value};

use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::tokens::{font, line, part, size, space};
use crate::keyboard::Page;
use crate::Cell;

/// Bits and their names, as the keyboard's core describes them at `at`.
fn bits(at: &[&str]) -> Vec<(u64, String)> {
    let d = crate::cores::keyboard().map_or(&Value::Null, |k| k.described());
    let v = at.iter().fold(d, |v, k| match k.parse::<usize>() {
        Ok(i) => &v[i],
        Err(_) => &v[*k],
    });
    v.as_array().into_iter().flatten().filter_map(|b| Some((b["bit"].as_u64()?, b["label"].as_str()?.to_string()))).collect()
}

/// The screens the keyboard's screen shows by itself when Ectomatter
/// doesn't, each by its bit in a profile's `screen.apps`, as its core
/// describes them (`displays`, `apps`).
pub fn screen_apps() -> Vec<(u64, String)> {
    bits(&["capabilities", "displays", "0", "apps"])
}

/// The keys game mode can turn off, each by its bit in a profile's
/// `game_keys`, as the keyboard's core describes them (`keys`, `game_keys`).
pub fn game_keys() -> Vec<(u64, String)> {
    bits(&["capabilities", "keys", "game_keys"])
}

/// Who makes the keyboard, as its core says, for its firmware's words.
fn maker() -> String {
    crate::cores::keyboard().and_then(|k| k.described()["vendor"].as_str().map(String::from)).unwrap_or_else(|| "its maker".into())
}

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
                let w = crate::text::width(font::REGULAR, font::TEXT, &v).ceil() + 2.0;
                k.cells.push(text_cell(style::VALUE_BRIGHT, x, top, w, line::TEXT, &v));
                crate::form::pill(&mut k.cells, 1, x + w + space::INLINE, top, "Unofficial");
                k.next(top, line::TEXT);
                let adds: Vec<String> = fw["features"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
                k.row();
                k.label_text("Adds");
                let top = k.y;
                if adds.is_empty() {
                    let end = k.note_at(k.control_x(), top + 1.0, k.control_w(), "Nothing Ectomatter controls yet. This build fixes how the volume slider steps.");
                    k.next(top, end - top);
                } else {
                    // two across, a tick before each, Space.tight between the lines
                    let (x, col) = (k.control_x(), k.control_w() / 2.0);
                    let pitch = line::TEXT + space::TIGHT;
                    for (i, a) in adds.iter().enumerate() {
                        k.cells.push(Cell { text: a.into(), ..cell(kind::TICKED, x + (i % 2) as f32 * col, top + (i / 2) as f32 * pitch, col - space::INLINE, line::TEXT) });
                    }
                    k.next(top, adds.len().div_ceil(2) as f32 * pitch - space::TIGHT);
                }
                let later: Vec<String> = fw["later"].as_array().map(|a| a.iter().map(|x| format!("{} (build {})", x["name"].as_str().unwrap_or(""), x["since"])).collect()).unwrap_or_default();
                if !later.is_empty() {
                    k.row();
                    k.label_text("In later builds");
                    let (x, w, top) = (k.control_x(), k.control_w(), k.y);
                    let pitch = line::TEXT + space::TIGHT;
                    for (i, l) in later.iter().enumerate() {
                        k.cells.push(text_cell(style::VALUE_BRIGHT, x, top + i as f32 * pitch, w, line::TEXT, l));
                    }
                    k.next(top, later.len() as f32 * pitch - space::TIGHT);
                }
            }
            "retail" => {
                let v = format!("{} v{}", maker(), fw["version"].as_str().unwrap_or(""));
                k.fact("Firmware", &v, "Features that need Ectomatter’s custom firmware show here when the keyboard runs it.");
            }
            _ => {
                let v = fw["version"].as_str().unwrap_or("");
                let v = if v.is_empty() { "Not known".to_string() } else { format!("v{v}") };
                k.fact("Firmware", &v, &format!("This version is neither {}’s nor Ectomatter’s, so no custom features are offered.", maker()));
            }
        }
        // (the keyboard as its core names it, and the USB device its manifest matches)
        if let Some(core) = crate::cores::keyboard() {
            let model = match crate::core_host::Manifest::read(&core.folder).ok().and_then(|m| m.usb_ids()) {
                Some((vendor, product)) => format!("{}, USB {vendor:04x}:{product:04x}", core.name()),
                None => core.name().to_string(),
            };
            k.fact("Model", &model, "");
        }
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
/// the slider's least and most across, as the hint line's room allows
const SLIDER_LEAST: f32 = 72.0;
const SLIDER_MOST: f32 = 120.0;

pub fn screen_cells(page: &Page, w: f32, from: f32, own: Option<(&str, [u8; 3])>, picking: bool) -> Vec<Cell> {
    let mut c = Vec::new();
    let l = &page.draft()["lighting"];
    let lc = &l["screen"]["color"];
    let b = |v: &Value| v.as_u64().unwrap_or(0).min(255) as u8;
    let rgb = [b(&lc["color"][0]), b(&lc["color"][1]), b(&lc["color"][2])];
    let col = crate::tokens::color(rgb);
    let br = lc["brightness"].as_f64().unwrap_or(255.0);
    // from the row's right end leftward: how bright, the slider, its name; the color (its dot
    // and its code, one button), the name of both, each at the middle of the hint line. A
    // control's own words are Space.inline before it, the slider Space.row before its value,
    // the two groups Space.margin apart. (Each text's cell runs 2 past its words, into the
    // gap after them, so the gaps are the words' own.)
    let t = |s: &str| crate::text::width(font::REGULAR, font::NOTE, s);
    let mid = |h: f32| (size::HINT - h) / 2.0;
    let v = format!("{}%", (br / 2.55).round());
    // (the pill as wide at every value as at 100%, so the slider never shifts under the pointer)
    let pw = crate::form::pill_width(2, "100%");
    let hex = crate::lighting::hex_of(rgb);
    let code_w = |s: &str| crate::text::width(font::REGULAR, font::CONTROL, s);
    let cw = part::SWATCH_WORDS + code_w(&hex) + part::QUIET_PAD;
    // In a window too narrow for all of it beside the tabs (which end at `from`), the slider
    // shortens, to SLIDER_LEAST, then the words go: Brightness's first (the slider and its
    // value still read as how bright), then the default backlight's; the slider takes back
    // what a word leaves, to SLIDER_MOST. Reckoned with the widest color code, so a color
    // picked never moves it.
    let widest = part::SWATCH_WORDS + code_w("#") + 6.0 * "0123456789ABCDEF".chars().map(|d| code_w(&d.to_string())).fold(0.0, f32::max) + part::QUIET_PAD;
    let (bw, lw) = (t("Brightness"), t("Default backlight"));
    let room = w - from - pw - space::ROW - space::MARGIN - widest;
    let word = space::INLINE;
    let (with_bright, with_name) = if SLIDER_LEAST + word + bw + word + lw <= room { (true, true) } else if SLIDER_LEAST + word + lw <= room { (false, true) } else { (false, false) };
    let sl = (room - if with_bright { word + bw } else { 0.0 } - if with_name { word + lw } else { 0.0 }).clamp(SLIDER_LEAST, SLIDER_MOST);
    let mut x = w - pw;
    c.push(Cell { text: v.into(), style: 2, ..cell(kind::PILL, x, mid(size::PILL), pw, size::PILL) });
    x -= space::ROW + sl;
    let at = (br / 255.0) as f32;
    c.push(Cell { value: at, color: col, id: id(Ctl::LcdBrightness), ..cell(kind::FILLED, x, mid(size::SLIDER), sl, size::SLIDER) });
    if with_bright {
        x -= word + bw;
        c.push(text_cell(style::SMALL, x, mid(line::TEXT), bw + 2.0, line::TEXT, "Brightness"));
    }
    x -= space::MARGIN + cw;
    c.push(Cell { color: col, on: picking, style: 2, text: hex.into(), id: id(Ctl::LcdColor), ..cell(kind::COLOR_DOT, x, mid(size::SWATCH), cw, size::SWATCH) });
    if with_name {
        x -= word + lw;
        c.push(text_cell(style::SMALL, x, mid(line::TEXT), lw + 2.0, line::TEXT, "Default backlight"));
    }
    // while a layout with a color of its own plays, a line saying so, in what room the tabs
    // leave: the layout's name cut short to fit, or the line not there where not even a
    // letter of the name fits (the default backlight never moves for it); its color's dot
    // before it, as a status's
    if let Some((name, light)) = own {
        let tail = " has its own color";
        let lead = size::DOT + space::INLINE;
        let most = x - space::MARGIN - from - lead - t(tail);
        let least = name.chars().next().map_or(f32::MAX, |f| t(&format!("{f}…")));
        if most >= least {
            let s = format!("{}{tail}", crate::text::elide(name, most, font::REGULAR, font::NOTE));
            let sw = t(&s);
            x -= space::MARGIN + sw;
            c.push(text_cell(style::NOTE, x, mid(line::TEXT), sw + 2.0, line::TEXT, &s));
            c.push(Cell { color: crate::tokens::color(light), ..cell(kind::BOX, x - lead, mid(size::DOT), size::DOT, size::DOT) });
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
            let bit = screen_apps().get(index).map_or(0, |a| a.0);
            let apps = l["screen"]["apps"].as_u64().unwrap_or(0);
            l["screen"]["apps"] = json!(if value > 0.5 { apps | bit } else { apps & !bit });
        }
        Ctl::GameKey => {
            let bit = game_keys().get(index).map_or(0, |a| a.0);
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
