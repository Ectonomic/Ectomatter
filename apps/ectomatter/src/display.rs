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

/// A slider as the keyboard's core describes it (its input capability's
/// `sliders`, docs/core-protocol.md, section 5, input): its control's id, its
/// name (`label`) and how a sentence calls it (`called`), its glyph, its job
/// when the keyboard does it by itself (`own`), the firmware features it can
/// be read with and freed with, and the keyboard's words for while it does
/// another job (`busy` while it can't be freed, `freed` once it is). Its
/// place among them is its number in the keyboard's picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slider {
    pub id: String,
    pub label: String,
    pub called: String,
    pub glyph: String,
    pub own: String,
    pub reads: Option<String>,
    pub frees: Option<String>,
    pub busy: String,
    pub freed: String,
    /// the lowest and highest the keyboard reads it at, as its core expects
    /// them (widened as the slider is seen further)
    pub range: [f64; 2],
}

/// The keyboard's sliders, as its core describes them.
pub fn sliders() -> Vec<Slider> {
    crate::cores::keyboard().map_or_else(Vec::new, |k| sliders_of(k.described()))
}

pub fn sliders_of(d: &Value) -> Vec<Slider> {
    let word = |v: &Value| v.as_str().map(String::from);
    d["capabilities"]["input"]["sliders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let id = word(&s["id"])?;
            let label = word(&s["label"]).unwrap_or_else(|| id.clone());
            Some(Slider {
                called: word(&s["called"]).unwrap_or_else(|| label.to_lowercase()),
                glyph: word(&s["glyph"]).unwrap_or_default(),
                own: word(&s["own"]).unwrap_or_else(|| "none".into()),
                reads: word(&s["reads"]),
                frees: word(&s["frees"]),
                busy: word(&s["notes"]["busy"]).unwrap_or_default(),
                freed: word(&s["notes"]["freed"]).unwrap_or_default(),
                // (none given: the first reading is both ends, until the slider is moved)
                range: match (s["range"][0].as_f64(), s["range"][1].as_f64()) {
                    (Some(a), Some(b)) => [a, b],
                    _ => [f64::MAX, f64::MIN],
                },
                id,
                label,
            })
        })
        .collect()
}

/// The jobs Ectomatter can give a slider besides its own, each in the words
/// a menu says it with.
const JOBS: [(&str, &str); 3] = [("output", "System volume"), ("backlight", "Keyboard brightness"), ("microphone", "Microphone volume")];

/// What a slider can control: its own job ("own", what the keyboard does
/// by itself), the others (the backlight only on a keyboard with a backlight
/// setting), and nothing.
pub fn choices(s: &Slider, backlight: bool) -> Vec<(String, String)> {
    let own = JOBS.iter().find(|(j, _)| *j == s.own).map_or_else(|| s.label.clone(), |(_, w)| w.to_string());
    let mut out = vec![("own".to_string(), own)];
    out.extend(JOBS.iter().filter(|(j, _)| *j != s.own && (*j != "backlight" || backlight)).map(|(j, w)| (j.to_string(), w.to_string())));
    out.push(("none".into(), "Nothing".into()));
    out
}

/// What a slider really drives: left on its own job, the one it does by itself.
pub fn drives(s: &Slider, v: &str) -> String {
    if v == "own" { s.own.clone() } else { v.into() }
}

/// The setting that holds the keyboard's backlight (the one whose `role` is
/// "backlight"), and the firmware feature it needs, if any.
pub fn backlight_setting() -> Option<(String, Option<String>)> {
    let d = crate::cores::keyboard()?.described().clone();
    let s = d["capabilities"]["settings"]["items"].as_array()?.iter().find(|s| s["role"] == "backlight")?.clone();
    Some((s["id"].as_str()?.to_string(), s["feature"].as_str().map(String::from)))
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
        // the firmware, in its core's own words: its row (with a tag beside it), what it
        // adds, and what later builds would
        let words = fw["words"].as_str().map(String::from).unwrap_or_else(|| match fw["version"].as_str().filter(|v| !v.is_empty()) {
            Some(v) => format!("v{v}"),
            None => "Not known".into(),
        });
        match fw["tag"].as_str() {
            Some(tag) => {
                k.row();
                k.label_text("Firmware");
                let (x, top) = (k.control_x(), k.y);
                let w = crate::text::width(font::REGULAR, font::TEXT, &words).ceil() + 2.0;
                k.cells.push(text_cell(style::VALUE_BRIGHT, x, top, w, line::TEXT, &words));
                crate::form::pill(&mut k.cells, 1, x + w + space::INLINE, top, tag);
                k.next(top, line::TEXT);
                if let Some(n) = fw["note"].as_str() {
                    let top = k.y;
                    let end = k.note_at(k.control_x(), top + 1.0, k.control_w(), n);
                    k.next(top, end - top);
                }
            }
            None => k.fact("Firmware", &words, fw["note"].as_str().unwrap_or("")),
        }
        let adds: Vec<String> = fw["features"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
        if !adds.is_empty() || fw["none"].is_string() {
            k.row();
            k.label_text("Adds");
            let top = k.y;
            if adds.is_empty() {
                let end = k.note_at(k.control_x(), top + 1.0, k.control_w(), fw["none"].as_str().unwrap_or(""));
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
        }
        let later: Vec<String> = fw["later"].as_array().map(|a| a.iter().filter_map(|x| x["words"].as_str().or(x["name"].as_str()).map(String::from)).collect()).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Boundary: a keyboard Ectomatter has never seen, with three sliders of
    /// its own, gets each one as its core describes it: its name, how a
    /// sentence calls it, its own job, and what else it can control (the
    /// backlight only on a keyboard with a backlight setting); a slider
    /// described without a range reads its ends from the slider itself.
    #[test]
    fn a_keyboard_of_its_own_gets_its_sliders() {
        let d = json!({ "capabilities": { "input": { "sliders": [
            { "id": "knob-a", "label": "Knob A", "own": "output", "frees": "quiet.knob", "notes": { "busy": "It still turns the volume too." } },
            { "id": "knob-b", "label": "Knob B", "called": "right knob", "own": "backlight", "range": [0, 1023] },
            { "id": "knob-c", "label": "Knob C" },
        ] } } });
        let s = sliders_of(&d);
        assert_eq!(s.iter().map(|s| (s.id.as_str(), s.called.as_str(), s.own.as_str())).collect::<Vec<_>>(), [("knob-a", "knob a", "output"), ("knob-b", "right knob", "backlight"), ("knob-c", "knob c", "none")]);
        assert_eq!((s[0].frees.as_deref(), s[0].busy.as_str()), (Some("quiet.knob"), "It still turns the volume too."));
        assert_eq!((s[1].range, s[0].range), ([0.0, 1023.0], [f64::MAX, f64::MIN]));
        let words = |s: &Slider, backlight: bool| choices(s, backlight).into_iter().map(|(_, w)| w).collect::<Vec<_>>();
        assert_eq!(words(&s[0], true), ["System volume", "Keyboard brightness", "Microphone volume", "Nothing"]);
        assert_eq!(words(&s[0], false), ["System volume", "Microphone volume", "Nothing"], "no backlight to set");
        assert_eq!(words(&s[1], true), ["Keyboard brightness", "System volume", "Microphone volume", "Nothing"]);
        assert_eq!(words(&s[2], true)[0], "Knob C", "a slider with no job of its own: its own name");
        assert_eq!((drives(&s[0], "own"), drives(&s[1], "own"), drives(&s[0], "microphone")), ("output".to_string(), "backlight".to_string(), "microphone".to_string()));
    }

    /// The Z10's two sliders, as its core describes them, offer what they
    /// always did.
    #[test]
    fn the_z10s_sliders_offer_what_they_did() {
        let s = sliders_of(&json!({ "capabilities": evga_z10::shared::capabilities() }));
        let words = |s: &Slider| choices(s, true).into_iter().map(|(_, w)| w).collect::<Vec<_>>();
        assert_eq!(words(&s[0]), ["Keyboard brightness", "System volume", "Microphone volume", "Nothing"]);
        assert_eq!(words(&s[1]), ["System volume", "Keyboard brightness", "Microphone volume", "Nothing"]);
        assert_eq!((s[0].called.as_str(), s[1].called.as_str()), ("dimmer", "volume slider"));
    }
}
