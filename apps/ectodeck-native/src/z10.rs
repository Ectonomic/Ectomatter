//! The Z10's page: where its keys are, what each is set to, and how the
//! keyboard on screen is lit. Ported from the web app (public/app.js): the
//! same tables, the same drafts and the same preview, so the two agree.

use std::collections::HashMap;

use ectodeck_draw::hdr::Light;
use serde_json::{json, Value};

/// drawn gap between neighbouring keys, in the drawing's units
pub const GAP: f64 = 3.0;

pub const PROFILE_NUMBERS: [u8; 5] = [1, 2, 3, 4, 5];
const FACTORY: &str = "254";

/// A key as the page knows it: where it is in the drawing, and whether a
/// light sits behind it.
#[derive(Clone, Debug)]
pub struct Key {
    pub pos: u32,
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub led: bool,
    pub cx: f64,
    pub cy: f64,
}

/// The page's three views, as the tabs name them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Keys,
    Lighting,
    Display,
}

/// Each cap's legend, for the key's name where it has no full name.
fn legend_main(id: &str) -> Option<&'static str> {
    Some(match id {
        "ESC" => "ESC",
        "PrintScreen" => "PRINT",
        "ScrollLock" => "SCROLL",
        "PauseBreak" => "PAUSE",
        "GraveAccent" => "`",
        "Minus" => "-",
        "Equals" => "=",
        "Backspace" => "BACKSPACE",
        "Insert" => "INSERT",
        "Home" => "HOME",
        "PageUp" => "PG",
        "Tab" => "TAB",
        "LBracket" => "[",
        "RBracket" => "]",
        "BackSlash" => "\\",
        "Delete" => "DELETE",
        "End" => "END",
        "PageDown" => "PG",
        "CapsLock" => "CAPS",
        "Semicolon" => ";",
        "SingleQuotation" => "'",
        "Enter" => "ENTER",
        "LSHIFT" | "RSHIFT" => "SHIFT",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "LCONTROL" | "RCONTROL" => "CTRL",
        "FN" => "FN",
        "LALT" | "RALT" => "ALT",
        "WIN" => "\u{229e}",
        "Menu" => "\u{2261}",
        "ArrowUp" => "\u{25b2}",
        "ArrowDown" => "\u{25bc}",
        "ArrowLeft" => "\u{25c0}",
        "ArrowRight" => "\u{25b6}",
        "NumLock" => "NUM",
        "KeypadDivide" => "/",
        "KeypadMultiply" => "*",
        "KeypadMinus" => "-",
        "KeypadPlus" => "+",
        "KeypadEnter" => "ENTER",
        "Keypad0" => "0",
        "Keypad1" => "1",
        "Keypad2" => "2",
        "Keypad3" => "3",
        "Keypad4" => "4",
        "Keypad5" => "5",
        "Keypad6" => "6",
        "Keypad7" => "7",
        "Keypad8" => "8",
        "Keypad9" => "9",
        "KeypadPeriod" => ".",
        "0" => "0",
        "1" => "1",
        "2" => "2",
        "3" => "3",
        "4" => "4",
        "5" => "5",
        "6" => "6",
        "7" => "7",
        "8" => "8",
        "9" => "9",
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
        _ => return None,
    })
}

fn full_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "ESC" => "Escape",
        "GraveAccent" => "Grave accent",
        "LBracket" => "Left bracket",
        "RBracket" => "Right bracket",
        "BackSlash" => "Backslash",
        "SingleQuotation" => "Quote",
        "LSHIFT" => "Left Shift",
        "RSHIFT" => "Right Shift",
        "LCONTROL" => "Left Ctrl",
        "RCONTROL" => "Right Ctrl",
        "LALT" => "Left Alt",
        "RALT" => "Right Alt",
        "FN" => "Fn",
        "WIN" => "Win",
        "GameMode" => "Game mode button",
        "MultimediaStop" => "Media stop",
        "MultimediaPreviousTrack" => "Previous track",
        "MultimediaPlayPause" => "Play/pause",
        "MultimediaNextTrack" => "Next track",
        "KeypadDivide" => "Keypad /",
        "KeypadMultiply" => "Keypad *",
        "KeypadMinus" => "Keypad -",
        "KeypadPlus" => "Keypad +",
        "KeypadEnter" => "Keypad Enter",
        "KeypadPeriod" => "Keypad .",
        "ArrowUp" => "Up arrow",
        "ArrowDown" => "Down arrow",
        "ArrowLeft" => "Left arrow",
        "ArrowRight" => "Right arrow",
        "PrintScreen" => "Print Screen",
        "ScrollLock" => "Scroll Lock",
        "PauseBreak" => "Pause/Break",
        "CapsLock" => "Caps Lock",
        "NumLock" => "Num Lock",
        "PageUp" => "Page Up",
        "PageDown" => "Page Down",
        "Space" => "Space",
        "Menu" => "Menu",
        _ => return None,
    })
}

/// USB HID keyboard usages, by the names the page shows.
pub fn hid_name(h: u32) -> Option<String> {
    Some(match h {
        4..=29 => ((b'A' + (h - 4) as u8) as char).to_string(),
        30..=38 => (h - 29).to_string(),
        39 => "0".into(),
        40 => "Enter".into(),
        41 => "Escape".into(),
        42 => "Backspace".into(),
        43 => "Tab".into(),
        44 => "Space".into(),
        45 => "-".into(),
        46 => "=".into(),
        47 => "[".into(),
        48 => "]".into(),
        49 => "\\".into(),
        51 => ";".into(),
        52 => "'".into(),
        53 => "`".into(),
        54 => ",".into(),
        55 => ".".into(),
        56 => "/".into(),
        57 => "Caps Lock".into(),
        58..=69 => format!("F{}", h - 57),
        70 => "Print Screen".into(),
        71 => "Scroll Lock".into(),
        72 => "Pause".into(),
        73 => "Insert".into(),
        74 => "Home".into(),
        75 => "Page Up".into(),
        76 => "Delete".into(),
        77 => "End".into(),
        78 => "Page Down".into(),
        79 => "Right arrow".into(),
        80 => "Left arrow".into(),
        81 => "Down arrow".into(),
        82 => "Up arrow".into(),
        83 => "Num Lock".into(),
        84 => "Keypad /".into(),
        85 => "Keypad *".into(),
        86 => "Keypad -".into(),
        87 => "Keypad +".into(),
        88 => "Keypad Enter".into(),
        89..=97 => format!("Keypad {}", h - 88),
        98 => "Keypad 0".into(),
        99 => "Keypad .".into(),
        101 => "Menu".into(),
        104..=115 => format!("F{}", h - 91),
        _ => return None,
    })
}

/// every HID usage the page names, in the order the web app lists them
fn hid_codes() -> Vec<u32> {
    let mut v: Vec<u32> = (4..=99).chain([101]).chain(104..=115).filter(|h| hid_name(*h).is_some()).collect();
    v.sort();
    v
}

pub const MODS: [(u32, &str); 8] = [
    (1, "Left Ctrl"),
    (2, "Left Shift"),
    (4, "Left Alt"),
    (8, "Left Super"),
    (16, "Right Ctrl"),
    (32, "Right Shift"),
    (64, "Right Alt"),
    (128, "Right Super"),
];
const MOD_SHORT: [(u32, &str); 8] = [(1, "Ctrl"), (2, "Shift"), (4, "Alt"), (8, "Super"), (16, "Ctrl"), (32, "Shift"), (64, "Alt"), (128, "Super")];

const CONSUMER: [(u32, &str); 18] = [
    (181, "Next track"),
    (182, "Previous track"),
    (183, "Stop"),
    (205, "Play/pause"),
    (226, "Mute"),
    (233, "Volume up"),
    (234, "Volume down"),
    (387, "Media player"),
    (394, "Mail"),
    (402, "Calculator"),
    (404, "My computer"),
    (545, "Web search"),
    (547, "Web home"),
    (548, "Web back"),
    (549, "Web forward"),
    (550, "Web stop"),
    (551, "Web refresh"),
    (554, "Web favourites"),
];

pub const EFFECTS: [(&str, &str); 9] = [
    ("off", "Off"),
    ("static", "Static"),
    ("breathing", "Breathing"),
    ("pulse", "Pulse"),
    ("spiral", "Spiral"),
    ("wave", "Wave"),
    ("stars", "Stars"),
    ("trigger", "Reactive"),
    ("custom", "Per key"),
];

/// A Linux key code (evdev) as the HID usage the keyboard stores, or as a
/// modifier's bit: what "Press a key" assigns, and how a key typed on the Z10
/// is found on the board.
pub fn evdev_to_hid(code: u32) -> Option<(u32, u32)> {
    // (modifier bits, usage)
    let h = match code {
        1 => 41,
        2..=10 => 30 + (code - 2),
        11 => 39,
        12 => 45,
        13 => 46,
        14 => 42,
        15 => 43,
        16 => 20,
        17 => 26,
        18 => 8,
        19 => 21,
        20 => 23,
        21 => 28,
        22 => 24,
        23 => 12,
        24 => 18,
        25 => 19,
        26 => 47,
        27 => 48,
        28 => 40,
        29 => return Some((1, 0)),
        30 => 4,
        31 => 22,
        32 => 7,
        33 => 9,
        34 => 10,
        35 => 11,
        36 => 13,
        37 => 14,
        38 => 15,
        39 => 51,
        40 => 52,
        41 => 53,
        42 => return Some((2, 0)),
        43 => 49,
        44 => 29,
        45 => 27,
        46 => 6,
        47 => 25,
        48 => 5,
        49 => 17,
        50 => 16,
        51 => 54,
        52 => 55,
        53 => 56,
        54 => return Some((32, 0)),
        55 => 85,
        56 => return Some((4, 0)),
        57 => 44,
        58 => 57,
        59..=68 => 58 + (code - 59),
        69 => 83,
        70 => 71,
        71 => 95,
        72 => 96,
        73 => 97,
        74 => 86,
        75 => 92,
        76 => 93,
        77 => 94,
        78 => 87,
        79 => 89,
        80 => 90,
        81 => 91,
        82 => 98,
        83 => 99,
        87 => 68,
        88 => 69,
        96 => 88,
        97 => return Some((16, 0)),
        98 => 84,
        99 => 70,
        100 => return Some((64, 0)),
        102 => 74,
        103 => 82,
        104 => 75,
        105 => 80,
        106 => 79,
        107 => 77,
        108 => 81,
        109 => 78,
        110 => 73,
        111 => 76,
        119 => 72,
        125 => return Some((8, 0)),
        126 => return Some((128, 0)),
        127 => 101,
        183..=194 => 104 + (code - 183),
        _ => return None,
    };
    Some((0, h))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}
fn rgb(v: &Value) -> [f64; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn def_of(v: &Value) -> [u32; 4] {
    [u(&v[0]), u(&v[1]), u(&v[2]), u(&v[3])]
}
pub fn hex(c: [f64; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0] as u8, c[1] as u8, c[2] as u8)
}

/// A profile as the page edits it: its name, lighting and both layers.
fn pick(p: &Value) -> Value {
    json!({ "name": p["name"], "lighting": p["lighting"], "keys": p["keys"], "fnkeys": p["fnkeys"] })
}

pub fn is_nothing(d: [u32; 4]) -> bool {
    (d[0] == 0 && d[1] == 0 && d[2] == 0) || d[0] == 255
}

/// What kind of action a key has, as its tag in the action button.
pub fn kind_of(d: [u32; 4]) -> &'static str {
    match d[0] {
        0 if d[1] != 0 || d[2] != 0 => "Key",
        0 => "",
        2 => "Media",
        3 => "Macro",
        4 | 5 => "Keyboard",
        6..=9 => "Mouse",
        17..=23 => "Profile",
        _ => "",
    }
}

/// One entry in the action menu: its label, the definition it sets, its kind.
#[derive(Clone, Debug)]
pub struct Action {
    pub label: String,
    pub def: [u32; 4],
    pub tag: &'static str,
}

/// Every action a key can be given, grouped as the action menu shows them.
pub struct Groups {
    pub macros: Vec<Action>,
    pub keys: Vec<Action>,
    pub media: Vec<Action>,
    pub mouse: Vec<Action>,
    pub profiles: Vec<Action>,
    pub board: Vec<Action>,
}

pub struct Page {
    pub layout: Vec<Key>,
    by_pos: HashMap<u32, usize>,
    zones: Vec<(u8, Vec<u32>)>,
    /// the drawing's view: its left, top, width and height
    pub view_box: (f64, f64, f64, f64),
    center: (f64, f64),
    span: [f64; 4],
    /// what the keyboard said: firmware, the active profile, every profile and macro
    pub dev: Option<Value>,
    pub drafts: HashMap<u8, Value>,
    pub editing: u8,
    pub view: View,
    pub sel: Option<u32>,
    pub busy: bool,
    stars: HashMap<u32, (f64, [f64; 3])>,
    flashes: HashMap<u32, (f64, [f64; 3])>,
    flash_idx: usize,
    hid_to_pos: HashMap<u32, u32>,
    mod_to_pos: HashMap<u32, u32>,
    /// keys pressed a moment ago, darkened where they stand, and when
    pub pressed: HashMap<u32, f64>,
    rng: u64,
}

impl Page {
    pub fn new() -> Page {
        let layout: Value = serde_json::from_str(include_str!("../../ectodeck/public/layout.json")).unwrap();
        let board: Value = serde_json::from_str(include_str!("../../ectodeck/public/board.json")).unwrap();
        let zones: Value = serde_json::from_str(include_str!("../../ectodeck/public/zones.json")).unwrap();
        let v = &board["view"];
        let view_box = (f(&v[0]), f(&v[1]), f(&v[2]) - f(&v[0]), f(&v[3]) - f(&v[1]));
        let mut keys: Vec<Key> = layout
            .as_array()
            .unwrap()
            .iter()
            .map(|k| Key {
                pos: u(&k["pos"]),
                id: k["id"].as_str().unwrap_or("").to_string(),
                x: f(&k["x"]),
                y: f(&k["y"]),
                w: f(&k["w"]),
                h: f(&k["h"]),
                led: k["led"].as_bool().unwrap_or(false),
                cx: 0.0,
                cy: 0.0,
            })
            .collect();
        // the spacebar stands taller than its row and overhangs the front of its well
        if let Some(s) = keys.iter_mut().find(|k| k.id == "Space") {
            s.h = f(&board["space"][3]) + GAP / 2.0 - s.y;
        }
        // the keys EVGA's layout leaves out: the mode button and the four media keys
        let extra = |pos: u32, id: &str, b: &Value| Key {
            pos,
            id: id.into(),
            x: f(&b[0]) - GAP / 2.0,
            y: f(&b[1]) - GAP / 2.0,
            w: f(&b[2]) - f(&b[0]) + GAP,
            h: f(&b[3]) - f(&b[1]) + GAP,
            led: false,
            cx: 0.0,
            cy: 0.0,
        };
        keys.push(extra(0, "GameMode", &board["gpc"]));
        keys.push(extra(17, "MultimediaStop", &board["media"][0]));
        keys.push(extra(18, "MultimediaPreviousTrack", &board["media"][1]));
        keys.push(extra(19, "MultimediaPlayPause", &board["media"][2]));
        keys.push(extra(20, "MultimediaNextTrack", &board["media"][3]));
        for k in &mut keys {
            k.cx = k.x + k.w / 2.0;
            k.cy = k.y + k.h / 2.0;
        }
        let by_pos = keys.iter().enumerate().map(|(i, k)| (k.pos, i)).collect();
        let list = |name: &str| zones[name].as_array().map(|a| a.iter().map(u).collect()).unwrap_or_default();
        let zones = vec![(0x10, list("WASD")), (0x08, list("Arrows")), (0x04, list("Numbers")), (0x02, list("FKeys")), (0x01, list("Numpad"))];
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for k in &keys {
            x0 = x0.min(k.cx);
            x1 = x1.max(k.cx);
            y0 = y0.min(k.cy);
            y1 = y1.max(k.cy);
        }
        Page {
            layout: keys,
            by_pos,
            zones,
            view_box,
            center: ((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            span: [x1 - x0, y1 - y0, x0, y0],
            dev: None,
            drafts: HashMap::new(),
            editing: 1,
            view: View::Keys,
            sel: None,
            busy: false,
            stars: HashMap::new(),
            flashes: HashMap::new(),
            flash_idx: 0,
            hid_to_pos: HashMap::new(),
            mod_to_pos: HashMap::new(),
            pressed: HashMap::new(),
            rng: 0x9e3779b97f4a7c15,
        }
    }

    pub fn key(&self, pos: u32) -> Option<&Key> {
        self.by_pos.get(&pos).map(|&i| &self.layout[i])
    }

    /// The keyboard's state has come: keep it, and start a draft of each profile.
    pub fn loaded(&mut self, dev: Value) {
        for n in PROFILE_NUMBERS {
            self.drafts.entry(n).or_insert_with(|| pick(&dev["profiles"][n.to_string()]));
        }
        self.editing = dev["active"].as_u64().filter(|n| (1..=5).contains(n)).unwrap_or(1) as u8;
        // a key typed on the page is found on the board by the factory keymap
        self.hid_to_pos.clear();
        self.mod_to_pos.clear();
        if let Some(keys) = dev["profiles"][FACTORY]["keys"].as_array() {
            for (pos, d) in keys.iter().enumerate() {
                let d = def_of(d);
                if d[0] != 0 {
                    continue;
                }
                if d[2] != 0 {
                    self.hid_to_pos.insert(d[2], pos as u32);
                } else if d[1] != 0 {
                    self.mod_to_pos.insert(d[1], pos as u32);
                }
            }
        }
        self.dev = Some(dev);
    }

    fn saved(&self, n: &str) -> &Value {
        &self.dev.as_ref().expect("loaded")["profiles"][n]
    }

    pub fn draft(&self) -> &Value {
        &self.drafts[&self.editing]
    }

    pub fn is_dirty(&self, n: u8) -> bool {
        self.dev.is_some() && self.drafts.get(&n).is_some_and(|d| *d != pick(self.saved(&n.to_string())))
    }

    pub fn discard(&mut self) {
        let p = pick(self.saved(&self.editing.to_string()));
        self.drafts.insert(self.editing, p);
    }

    /// A profile saved and read back: it is the draft now.
    pub fn saved_profile(&mut self, n: u8, p: Value) {
        if let Some(dev) = &mut self.dev {
            dev["profiles"][n.to_string()] = p.clone();
        }
        self.drafts.insert(n, pick(&p));
    }

    /// What the page sends to save the profile being edited.
    pub fn save_body(&self) -> Value {
        let mut body = self.draft().clone();
        if body["lighting"]["mode"] == "custom" {
            body["lighting"]["mode"] = json!("off");
        }
        body
    }

    pub fn set_active(&mut self, n: u8) {
        if let Some(dev) = &mut self.dev {
            dev["active"] = json!(n);
        }
    }

    pub fn active(&self) -> u8 {
        self.dev.as_ref().and_then(|d| d["active"].as_u64()).unwrap_or(0) as u8
    }

    fn macro_name(&self, n: u32) -> Option<String> {
        self.dev.as_ref()?["macros"][n.to_string()]["name"].as_str().map(String::from)
    }

    pub fn describe(&self, d: [u32; 4]) -> String {
        let [f, a, b, c] = d;
        match f {
            0 => {
                let mut parts: Vec<String> = MODS.iter().filter(|(m, _)| a & m != 0).map(|(_, n)| n.to_string()).collect();
                parts.extend([b, c].into_iter().filter(|h| *h != 0).map(|h| hid_name(h).unwrap_or(format!("Key {h}"))));
                if parts.is_empty() { "Nothing".into() } else { parts.join(" + ") }
            }
            2 => {
                let code = a | b << 8;
                CONSUMER.iter().find(|(c, _)| *c == code).map_or(format!("Media code {code}"), |(_, n)| n.to_string())
            }
            3 => format!("Macro: {}", self.macro_name(a).unwrap_or(format!("number {a}"))),
            4 => "Fn".into(),
            5 => "Game mode toggle".into(),
            6 => if a != 0 { "Scroll down".into() } else { "Scroll up".into() },
            7 => "Left click".into(),
            8 => "Right click".into(),
            9 => "Middle click".into(),
            17..=21 => format!("Switch to profile {}", f - 16),
            22 => "Next profile".into(),
            23 => "Previous profile".into(),
            255 => "Disabled".into(),
            _ => format!("Function {f}"),
        }
    }

    /// A remapped key's new meaning, printed on it like a second legend.
    pub fn short_label(&self, d: [u32; 4], macro_names: &[String]) -> String {
        let [f, a, b, c] = d;
        match f {
            0 => {
                let mut parts: Vec<String> = Vec::new();
                for (m, n) in MOD_SHORT {
                    if a & m != 0 && !parts.iter().any(|p| p == n) {
                        parts.push(n.into());
                    }
                }
                parts.extend([b, c].into_iter().filter(|h| *h != 0).map(|h| {
                    let n = hid_name(h).unwrap_or(format!("Key {h}"));
                    n.strip_prefix("Keypad ").map_or(n.clone(), |r| format!("Num {r}"))
                }));
                if parts.is_empty() { "Nothing".into() } else { parts.join("+") }
            }
            3 => distinct_part(&self.macro_name(a).unwrap_or(format!("Macro {a}")), macro_names),
            255 => "Off".into(),
            _ => self.describe(d),
        }
    }

    pub fn key_label(&self, k: &Key) -> String {
        if let Some(n) = full_name(&k.id) {
            return n.into();
        }
        match legend_main(&k.id) {
            Some(m) if m.chars().count() > 1 => {
                let mut c = m.chars();
                let first = c.next().unwrap();
                format!("{first}{}", c.as_str().to_lowercase())
            }
            Some(m) => m.into(),
            None => k.id.clone(),
        }
    }

    pub fn def(&self, layer: &str, pos: u32) -> [u32; 4] {
        def_of(&self.draft()[layer][pos as usize])
    }

    pub fn factory(&self, layer: &str, pos: u32) -> [u32; 4] {
        def_of(&self.saved(FACTORY)[layer][pos as usize])
    }

    pub fn set_def(&mut self, layer: &str, pos: u32, d: [u32; 4]) {
        let n = self.editing;
        if let Some(draft) = self.drafts.get_mut(&n) {
            draft[layer][pos as usize] = json!(d);
        }
    }

    pub fn reset_key(&mut self, pos: u32) {
        for layer in ["keys", "fnkeys"] {
            let d = self.factory(layer, pos);
            self.set_def(layer, pos, d);
        }
    }

    /// A macro keeps playing the way it already plays on its other keys.
    pub fn with_macro_play(&self, d: [u32; 4]) -> [u32; 4] {
        if d[0] != 3 {
            return d;
        }
        let mut modes = Vec::new();
        for n in PROFILE_NUMBERS {
            for layer in ["keys", "fnkeys"] {
                if let Some(a) = self.drafts[&n][layer].as_array() {
                    for v in a {
                        let v = def_of(v);
                        if v[0] == 3 && v[1] == d[1] {
                            modes.push((v[2], if v[2] == 11 { v[3] } else { 0 }, v[3]));
                        }
                    }
                }
            }
        }
        let first = modes.first().copied();
        let varies = modes.iter().any(|m| (m.0, m.1) != first.map_or((0, 0), |f| (f.0, f.1)));
        match first {
            Some((mode, _, count)) if !varies => [3, d[1], mode, if mode == 11 { if count == 0 { 2 } else { count } } else { 0 }],
            _ => [3, d[1], 0, 0],
        }
    }

    /// The keys a macro presses, as the key's editor lists them.
    pub fn macro_keys_held(&self, n: u32) -> Vec<String> {
        let Some(steps) = self.dev.as_ref().and_then(|d| d["macros"][n.to_string()]["steps"].as_array().cloned()) else {
            return Vec::new();
        };
        steps
            .iter()
            .filter(|s| s["down"].as_bool().unwrap_or(false))
            .map(|s| step_name(s))
            .take(6)
            .collect()
    }

    pub fn macro_label(&self, n: u32) -> String {
        self.macro_name(n).unwrap_or(format!("Macro {n}"))
    }

    pub fn groups(&self) -> Groups {
        let keys = hid_codes().into_iter().map(|h| Action { label: hid_name(h).unwrap(), def: [0, 0, h, 0], tag: "Key" });
        let mods = MODS.iter().map(|(m, n)| Action { label: n.to_string(), def: [0, *m, 0, 0], tag: "Key" });
        let mut media: Vec<Action> = CONSUMER.iter().map(|(c, n)| Action { label: n.to_string(), def: [2, c & 255, c >> 8, 0], tag: "Media" }).collect();
        media.sort_by_key(|a| a.def[1] | a.def[2] << 8);
        let mut macros: Vec<(u32, Action)> = self
            .dev
            .as_ref()
            .and_then(|d| d["macros"].as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(i, v)| {
                        let i: u32 = i.parse().ok()?;
                        Some((i, Action { label: v["name"].as_str().map_or(format!("Macro {i}"), String::from), def: [3, i, 0, 0], tag: "Macro" }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        macros.sort_by_key(|(i, _)| *i);
        let mouse = [("Left click", [7, 0, 0, 0]), ("Right click", [8, 0, 0, 0]), ("Middle click", [9, 0, 0, 0]), ("Scroll up", [6, 0, 0, 0]), ("Scroll down", [6, 1, 0, 0])]
            .into_iter()
            .map(|(l, d)| Action { label: l.into(), def: d, tag: "Mouse" })
            .collect();
        let profiles = PROFILE_NUMBERS
            .iter()
            .map(|n| Action { label: format!("Switch to profile {n}"), def: [16 + *n as u32, 0, 0, 0], tag: "Profile" })
            .chain([
                Action { label: "Next profile".into(), def: [22, 0, 0, 0], tag: "Profile" },
                Action { label: "Previous profile".into(), def: [23, 0, 0, 0], tag: "Profile" },
            ])
            .collect();
        let board = vec![
            Action { label: "Fn".into(), def: [4, 0, 0, 0], tag: "Keyboard" },
            Action { label: "Game mode toggle".into(), def: [5, 0, 0, 0], tag: "Keyboard" },
        ];
        Groups { macros: macros.into_iter().map(|(_, a)| a).collect(), keys: keys.chain(mods).collect(), media, mouse, profiles, board }
    }

    /// The names of the macros on the board's first layer, so each one's
    /// printed label keeps only the words that tell it apart.
    pub fn board_macro_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if self.dev.is_none() {
            return out;
        }
        for k in &self.layout {
            let d = self.def("keys", k.pos);
            if d[0] == 3 {
                if let Some(n) = self.macro_name(d[1]) {
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
        }
        out
    }

    /// What each key prints as its new meaning, where it has one: on the Keys
    /// view, a key set to something other than its factory action.
    pub fn remap(&self, pos: u32, names: &[String]) -> Option<String> {
        if self.dev.is_none() || self.view != View::Keys {
            return None;
        }
        let d = self.def("keys", pos);
        (d != self.factory("keys", pos)).then(|| self.short_label(d, names))
    }

    /// The name, effect and small colour sample of each profile's button.
    pub fn profile_effect(&self, n: u8) -> String {
        let mode = self.drafts[&n]["lighting"]["mode"].as_str().unwrap_or("").to_string();
        EFFECTS.iter().find(|(m, _)| *m == mode).map_or(mode, |(_, name)| name.to_string())
    }

    pub fn profile_name(&self, n: u8) -> String {
        let name = self.drafts[&n]["name"].as_str().unwrap_or("");
        if name.is_empty() || name.to_uppercase().starts_with("DEFAULT") { format!("Profile {n}") } else { name.to_string() }
    }

    /// The profile's colours for its swatch: one colour, or up to four for a gradient.
    pub fn profile_swatch(&self, n: u8) -> Vec<[f64; 3]> {
        let l = &self.drafts[&n]["lighting"];
        match l["mode"].as_str().unwrap_or("") {
            "off" | "custom" => vec![[0.0; 3]],
            "static" => vec![rgb(&l["static"]["color"])],
            m => {
                let cols: Vec<[f64; 3]> = l[m]["colors"].as_array().map(|a| a.iter().take(4).map(|c| rgb(&c["color"])).collect()).unwrap_or_default();
                if cols.is_empty() { vec![[136.0; 3], [255.0; 3]] } else { cols }
            }
        }
    }

    fn zone_set(&self, zone: u8) -> Option<Vec<u32>> {
        if zone & 0x80 != 0 {
            return None;
        }
        let mut s = Vec::new();
        for (bit, list) in &self.zones {
            if zone & bit != 0 {
                s.extend(list.iter().copied());
            }
        }
        Some(s)
    }

    fn random(&mut self) -> f64 {
        // xorshift: the stars' twinkle needs no better
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Whether the preview moves on its own, and so needs drawing every frame.
    pub fn animated(&self) -> bool {
        let Some(_) = &self.dev else { return false };
        match self.draft()["lighting"]["mode"].as_str().unwrap_or("") {
            "breathing" | "pulse" | "spiral" | "wave" | "stars" => true,
            "trigger" => !self.flashes.is_empty(),
            _ => false,
        }
    }

    /// Each lit key's light at time `t` (ms), as the keyboard shows the draft.
    pub fn lights(&mut self, t: f64) -> Vec<Light> {
        if self.dev.is_none() {
            return Vec::new();
        }
        let l = self.draft()["lighting"].clone();
        let mode = l["mode"].as_str().unwrap_or("off").to_string();
        if mode == "stars" && self.random() < 0.35 {
            let i = (self.random() * self.layout.len() as f64) as usize % self.layout.len();
            let pos = self.layout[i].pos;
            if !self.stars.contains_key(&pos) {
                let c = [80.0 + (self.random() * 176.0).floor(), 80.0 + (self.random() * 176.0).floor(), 80.0 + (self.random() * 176.0).floor()];
                self.stars.insert(pos, (t, c));
            }
        }
        let colors = self.effect_colors(&l, &mode, t);
        let names = self.board_macro_names();
        self.layout
            .iter()
            .filter(|k| k.led)
            .filter_map(|k| {
                let v = colors.get(&k.pos)?;
                Some(Light { pos: k.pos, rgb: led_look([v[0], v[1], v[2]]), strength: ((v[3] * 1000.0).round() / 1000.0) as f32, dim: self.remap(k.pos, &names).is_some() })
            })
            .collect()
    }

    fn effect_colors(&mut self, l: &Value, mode: &str, t: f64) -> HashMap<u32, [f64; 4]> {
        let e = &l[if mode == "off" { "static" } else { mode }];
        let in_zone = self.zone_set(e["zone"].as_u64().unwrap_or(0x80) as u8);
        let cols: Vec<[f64; 3]> = e["colors"].as_array().map(|a| a.iter().take(7).map(|c| rgb(&c["color"])).collect()).unwrap_or_default();
        let bri = e["colors"][0]["brightness"].as_f64().or(e["brightness"].as_f64()).unwrap_or(255.0) / 255.0;
        let cycle = f(&e["cycle"]).max(1.0);
        let mut out = HashMap::new();
        let keys: Vec<(u32, f64, f64)> = self.layout.iter().filter(|k| k.led).map(|k| (k.pos, k.cx, k.cy)).collect();
        for (pos, cx, cy) in keys {
            if in_zone.as_ref().is_some_and(|z| !z.contains(&pos)) {
                out.insert(pos, [0.0; 4]);
                continue;
            }
            let (mut c, mut a) = ([0.0; 3], 0.0);
            match mode {
                "static" => {
                    c = rgb(&e["color"]);
                    a = f(&e["brightness"]) / 255.0;
                }
                "breathing" => {
                    let u = (t % (2.0 * cycle)) / cycle;
                    let which = if u < 1.0 { 0 } else { 1 };
                    c = rgb(&e["colors"][which]["color"]);
                    a = (std::f64::consts::PI * (u % 1.0)).sin().powi(2) * bri;
                }
                "pulse" => {
                    let n = (e["count"].as_u64().filter(|c| *c > 0).unwrap_or(7) as usize).clamp(1, 7);
                    let step = cycle / n as f64;
                    let i = ((t / step).floor() as usize) % n;
                    let uu = (t % step) / step;
                    let col = cols.get(i).copied().unwrap_or([255.0; 3]);
                    let on = (uu * 6.0).min(1.0) * ((1.0 - uu) * 6.0).min(1.0);
                    c = mix(rgb(&e["background"]["color"]), col, on);
                    a = bri;
                }
                "spiral" => {
                    let ang = (cy - self.center.1).atan2((cx - self.center.0) / 2.2) / (2.0 * std::f64::consts::PI);
                    let dir = if e["direction"] == 1 { -1.0 } else { 1.0 };
                    let rgbs = if cols.is_empty() { vec![[255.0, 0.0, 0.0], [0.0, 255.0, 0.0], [0.0, 0.0, 255.0]] } else { cols.clone() };
                    c = ramp(&rgbs, ang + dir * t / cycle);
                    a = bri;
                }
                "wave" => {
                    let d = e["direction"].as_u64().unwrap_or(0);
                    let d = if d > 3 { 0 } else { d };
                    let along = if d >= 2 { (cx - self.span[2]) / self.span[0] } else { (cy - self.span[3]) / self.span[1] };
                    let sign = if d == 1 || d == 2 { -1.0 } else { 1.0 };
                    c = ramp(&cols, sign * along + t / cycle);
                    a = bri;
                }
                "stars" => {
                    if let Some(&(st, sc)) = self.stars.get(&pos) {
                        let uu = (t - st) / cycle;
                        if uu < 1.0 {
                            c = sc;
                            a = (std::f64::consts::PI * uu).sin() * f(&e["brightness"]) / 255.0;
                        } else {
                            self.stars.remove(&pos);
                        }
                    }
                }
                "trigger" => {
                    if let Some(&(ft, fc)) = self.flashes.get(&pos) {
                        let uu = (t - ft) / (f(&e["shift"]) * 3.0).max(250.0);
                        if uu < 1.0 {
                            c = fc;
                            a = (1.0 - uu) * bri;
                        } else {
                            self.flashes.remove(&pos);
                        }
                    }
                }
                _ => {}
            }
            out.insert(pos, [c[0], c[1], c[2], a]);
        }
        out
    }

    /// A key typed on the keyboard: where it is on the board.
    pub fn pos_for(&self, mods: u32, hid: u32) -> Option<u32> {
        if hid == 0 { self.mod_to_pos.get(&mods).copied() } else { self.hid_to_pos.get(&hid).copied() }
    }

    /// Light a key as pressing it would, for Reactive.
    pub fn flash(&mut self, pos: u32, t: f64) {
        if self.dev.is_none() || self.draft()["lighting"]["mode"] != "trigger" {
            return;
        }
        let e = self.draft()["lighting"]["trigger"].clone();
        let cols: Vec<[f64; 3]> = e["colors"].as_array().map(|a| a.iter().take(7).map(|c| rgb(&c["color"])).collect()).unwrap_or_default();
        if cols.is_empty() {
            return;
        }
        self.flash_idx = (self.flash_idx + 1) % cols.len();
        let c = cols[self.flash_idx];
        let Some(k) = self.key(pos).cloned() else { return };
        let sub = e["submode"].as_u64().unwrap_or(0);
        let hit: Vec<u32> = if (if sub > 3 { 0 } else { sub }) == 2 {
            self.layout.iter().filter(|o| (o.cx - k.cx).abs() < 34.0 && (o.cy - k.cy).abs() < 34.0).map(|o| o.pos).collect()
        } else {
            vec![k.pos]
        };
        for p in hit {
            self.flashes.insert(p, (t, c));
        }
    }

    /// Choosing a profile starts its effect from the beginning.
    pub fn choose(&mut self, n: u8) {
        self.editing = n;
        self.stars.clear();
        self.flashes.clear();
    }

    /// The display's backlight as it looks: leaning as the keys' LEDs do, and
    /// dimmer with its brightness.
    pub fn lcd_color(&self) -> [f64; 3] {
        if self.dev.is_none() {
            return [90.0; 3];
        }
        let c = &self.draft()["lighting"]["lcd"]["color"];
        let b = f(&c["brightness"]);
        let look = led_look(rgb(&c["color"]));
        look.map(|v| (v as f64 * (0.3 + 0.7 * b / 255.0)).round())
    }

    /// Whether the keyboard's firmware can say where its sliders are.
    pub fn reads_sliders(&self) -> bool {
        let fw = self.dev.as_ref().map_or(0, |d| u(&d["firmware"]));
        let minor = fw & 0xff;
        minor >= 171 && minor - 170 >= 2
    }

    pub fn firmware_line(&self) -> (String, Option<String>) {
        let fw = self.dev.as_ref().map_or(0, |d| u(&d["firmware"]));
        let minor = fw & 0xff;
        let line = format!("Connected, firmware v{}.{:02}", fw >> 8, minor);
        let chip = (minor >= 100).then(|| if minor > 170 { format!("Unofficial build {}", minor - 170) } else { "Unofficial build".into() });
        (line, chip)
    }
}

/// the words at the end of a name that tell it apart from the others
fn distinct_part(name: &str, names: &[String]) -> String {
    let w: Vec<&str> = name.split_whitespace().collect();
    let mut keep = 0;
    for other in names {
        if other == name {
            continue;
        }
        let o: Vec<&str> = other.split_whitespace().collect();
        let mut n = 0;
        while n + 1 < w.len() && n < o.len() && w[n] == o[n] {
            n += 1;
        }
        keep = keep.max(n);
    }
    w[keep.min(w.len())..].join(" ")
}

/// A macro step's key, as the macro list names it.
fn step_name(s: &Value) -> String {
    if let Some(m) = s.get("media") {
        let m = u(m);
        let name = match m {
            0xB5 => "Next track",
            0xB6 => "Previous track",
            0xB7 => "Stop",
            0xCD => "Play/pause",
            0xE2 => "Mute",
            0xE9 => "Volume up",
            0xEA => "Volume down",
            0x183 => "Media player",
            0x18A => "Mail",
            0x192 => "Calculator",
            0x194 => "My computer",
            0x221 => "Browser search",
            0x223 => "Browser home",
            0x224 => "Browser back",
            0x225 => "Browser forward",
            0x227 => "Browser refresh",
            _ => return format!("Media {m}"),
        };
        return name.into();
    }
    let code = u(&s["code"]);
    let name = match code {
        112 => Some("Left Ctrl".to_string()),
        113 => Some("Left Shift".into()),
        114 => Some("Left Alt".into()),
        115 => Some("Left Super".into()),
        116 => Some("Right Ctrl".into()),
        117 => Some("Right Shift".into()),
        118 => Some("Right Alt".into()),
        119 => Some("Right Super".into()),
        c if c > 0 && c < 104 => hid_name(c),
        _ => None,
    };
    name.unwrap_or(format!("Code {code}"))
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// a loop of colours, `u` in [0, 1) around it
fn ramp(colors: &[[f64; 3]], u: f64) -> [f64; 3] {
    let n = colors.len();
    if n == 0 {
        return [0.0; 3];
    }
    let x = ((u % 1.0) + 1.0) % 1.0 * n as f64;
    let i = (x.floor() as usize).min(n - 1);
    mix(colors[i], colors[(i + 1) % n], x - i as f64)
}

/// The Z10's LEDs do not show a pure colour: each channel spills into
/// another (red and blue into green, green into blue, 0x60 of each at full),
/// so the keyboard on screen shows a colour as the keys do.
fn led_look(c: [f64; 3]) -> [u8; 3] {
    let spill = 0x60 as f64 / 255.0;
    [c[0] as u8, (c[1] + c[0] * spill + c[2] * spill).min(255.0) as u8, (c[2] + c[1] * spill).min(255.0) as u8]
}
