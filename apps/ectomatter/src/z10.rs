//! The Z10's page: where its keys are, what each is set to, and how the
//! keyboard on screen is lit. Ported from the web app that came before it
//! (its app.js, kept in design/, where the mocks still draw with it): the
//! same tables, the same drafts and the same preview, so the two agree.

use std::collections::{HashMap, HashSet};

use ectomatter_draw::hdr::Light;
use ectomatter_protocol::{keys as hid, media, Action, Jump, Plays};
use serde_json::{json, Value};

/// The Z10's core: its folder among Ectomatter's cores.
pub const CORE: &str = "evga-z10";

/// drawn gap between neighboring keys, in the drawing's units
pub const GAP: f64 = 3.0;

pub const PROFILE_NUMBERS: [u8; 5] = [1, 2, 3, 4, 5];

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
    /// the screen: what Ectomatter plays on it, and what it shows when Ectomatter is closed
    Screen,
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

/// USB HID keyboard usages, by the names the page shows: the core protocol's
/// words for them (ectomatter_protocol::keys), the modifiers named apart.
pub fn hid_name(h: u32) -> Option<String> {
    u8::try_from(h).ok().filter(|u| !hid::is_modifier(*u)).and_then(hid::label)
}

/// every HID usage the page names, in the order the web app lists them
fn hid_codes() -> Vec<u32> {
    let mut v: Vec<u32> = (4..=99).chain([101]).chain(104..=115).filter(|h| hid_name(*h).is_some()).collect();
    v.sort();
    v
}

/// A modifier's words, by its usage: in full ("Left Ctrl"), or short ("Ctrl").
fn modifier(u: u8, short: bool) -> &'static str {
    let m = hid::MODIFIERS[(u - hid::FIRST_MODIFIER) as usize];
    if short { m.2 } else { m.1 }
}

/// The screen's jobs a key can have, each a key code the USB standard reserves: no key is
/// meant to send one and Linux never turns one into a key press, so Ectomatter hears them from
/// the keyboard's own reports (see crate::screen_keys). These four step through the layouts
/// of the playlist playing and through the playlists; each of the seven after them plays one
/// playlist, as Ectomatter's settings say which.
pub const NEXT_LAYOUT: u32 = 0xA5;
pub const PREV_LAYOUT: u32 = 0xA6;
pub const NEXT_PLAYLIST: u32 = 0xA7;
pub const PREV_PLAYLIST: u32 = 0xA8;
pub const PLAY_CODES: std::ops::RangeInclusive<u32> = 0xA9..=0xAF;

/// The screen job a key has: its code, when the key is given one of the
/// keyboard's host slots (each sends one reserved code alone).
pub fn screen_job(a: &Action) -> Option<u32> {
    match a {
        Action::Host(n) if *n <= *PLAY_CODES.end() - NEXT_LAYOUT => Some(NEXT_LAYOUT + n),
        _ => None,
    }
}

/// The host slot a screen job's code is sent from.
pub fn screen_slot(code: u32) -> Action {
    Action::Host(code - NEXT_LAYOUT)
}


/// The action "Press a key" gives a key: the modifiers held (by their bits,
/// as `evdev_to_hid` gives them) and the key pressed, if one was.
pub fn pressed_action(mods: u32, key: u32) -> Action {
    let mut codes: Vec<u8> = (0..8).filter(|i| mods & (1 << i) != 0).map(|i| hid::FIRST_MODIFIER + i).collect();
    if key != 0 {
        codes.push(key as u8);
    }
    match codes.len() {
        0 => Action::None,
        1 => Action::Key(codes[0]),
        _ => Action::Shortcut(codes),
    }
}

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
/// An action as a profile holds it, or none (an action Ectomatter can't read,
/// as no key has).
fn action_of(v: &Value) -> Action {
    Action::read(v).unwrap_or(Action::None)
}
pub fn hex(c: [f64; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0] as u8, c[1] as u8, c[2] as u8)
}

/// A screen light of black leaves the keyboard's screen unlit, so nothing on
/// it can be read. A profile the keyboard holds with black (as profiles set
/// to Off elsewhere can be) is taken with white, the keyboard's own default
/// screen light, which saving the profile then stores.
pub fn lit_screen(p: &mut Value) {
    if let Some(c) = p.pointer_mut("/lighting/screen/color/color") {
        if c.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|v| v.as_u64() == Some(0))) {
            *c = json!([255, 255, 255]);
        }
    }
}

/// A profile as the page edits it: its name, lighting and the keys of both layers.
fn pick(p: &Value) -> Value {
    json!({ "name": p["name"], "lighting": p["lighting"], "keys": p["keys"] })
}

/// The keys' effect a profile's lighting is set to.
fn effect(l: &Value) -> &str {
    l["keys"]["effect"].as_str().unwrap_or("")
}

pub fn is_nothing(a: &Action) -> bool {
    matches!(a, Action::None | Action::Off)
}

/// What kind of action a key has, as its tag in the action button.
pub fn kind_of(a: &Action) -> &'static str {
    match a {
        Action::Host(_) if screen_job(a).is_some() => "Screen",
        Action::Key(_) | Action::Shortcut(_) => "Key",
        Action::Media(_) => "Media",
        Action::Macro { .. } => "Macro",
        Action::Fn | Action::Own(_) => "Keyboard",
        Action::Mouse(_) | Action::Scroll { .. } => "Mouse",
        Action::Profile(_) => "Profile",
        _ => "",
    }
}

/// One entry in the action menu: its label, the action it sets, its kind.
#[derive(Clone, Debug)]
pub struct Entry {
    pub label: String,
    pub act: Action,
    pub tag: &'static str,
}

/// Every action a key can be given, grouped as the action menu shows them.
pub struct Groups {
    pub macros: Vec<Entry>,
    pub keys: Vec<Entry>,
    pub media: Vec<Entry>,
    pub mouse: Vec<Entry>,
    pub profiles: Vec<Entry>,
    /// the screen's jobs: stepping through the layouts, and through the playlists (the
    /// action menu adds a job for each playlist after these)
    pub layouts: Vec<Entry>,
    pub playlists: Vec<Entry>,
    pub board: Vec<Entry>,
}

pub struct Page {
    pub layout: Vec<Key>,
    by_pos: HashMap<u32, usize>,
    /// each key's id in the core's description, by its place in the keyboard's reports
    ids: HashMap<u32, String>,
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
    /// the Keys tab is on its Function layer: the keyboard picture shows what the keys do
    /// with Fn held there (on other tabs, what they do pressed alone)
    pub fn_layer: bool,
    pub busy: bool,
    stars: HashMap<u32, (f64, [f64; 3])>,
    flashes: HashMap<u32, (f64, [f64; 3])>,
    flash_idx: usize,
    /// keys held down on the keyboard, darkened where they stand until they are let go
    pub pressed: HashSet<u32>,
    /// the keyboard's lock lights as it shows them (bit 0 Num Lock, 1 Caps Lock,
    /// 2 Scroll Lock), and its dimmer's level, 0 to 255, once the firmware says
    pub locks: u8,
    pub dimmer: Option<u8>,
    /// the colors the profile being edited paints its keys with, one a key
    pub custom: Vec<[u8; 3]>,
    /// Reactive's "Preview with pretend typing"
    pub demo: bool,
    /// the playlists keys can play, by the reserved code each one's keys send (see
    /// PLAY_CODES), and their names; a code with none plays a playlist since deleted
    pub playlist_keys: std::collections::BTreeMap<u32, String>,
    /// the codes Ectomatter's own jobs have (see jobs.rs), and each job in words, in full
    /// and as its key prints it (a profile by its name, as a key switching to one of the
    /// keyboard's own is printed)
    pub job_keys: std::collections::BTreeMap<u32, String>,
    pub job_prints: std::collections::BTreeMap<u32, String>,
    /// each lit key's color and light at the last frame, before the LEDs' lean: what streams to the keyboard
    pub last_colors: HashMap<u32, [f64; 4]>,
    /// what the keyboard's core says of it (`device/describe`): its
    /// capabilities, the keys' effects the Lighting page is drawn from among them
    pub desc: Value,
    rng: u64,
}

impl Page {
    pub fn new() -> Page {
        let layout: Value = serde_json::from_str(include_str!("../data/layout.json")).unwrap();
        let board: Value = serde_json::from_str(include_str!("../data/board.json")).unwrap();
        let zones: Value = serde_json::from_str(include_str!("../data/zones.json")).unwrap();
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
        let ids = crate::cores::keyboard()
            .and_then(|c| c.device["controls"].as_array())
            .map(|c| c.iter().filter_map(|c| Some((c["pos"].as_u64()? as u32, c["id"].as_str()?.to_string()))).collect())
            .unwrap_or_default();
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
            ids,
            zones,
            view_box,
            center: ((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            span: [x1 - x0, y1 - y0, x0, y0],
            dev: None,
            drafts: HashMap::new(),
            editing: 1,
            view: View::Keys,
            sel: None,
            fn_layer: false,
            busy: false,
            stars: HashMap::new(),
            flashes: HashMap::new(),
            flash_idx: 0,
            pressed: HashSet::new(),
            locks: 0,
            dimmer: None,
            custom: vec![[0; 3]; 116],
            demo: false,
            playlist_keys: Default::default(),
            job_keys: Default::default(),
            job_prints: Default::default(),
            last_colors: HashMap::new(),
            desc: Value::Null,
            rng: 0x9e3779b97f4a7c15,
        }
    }

    pub fn key(&self, pos: u32) -> Option<&Key> {
        self.by_pos.get(&pos).map(|&i| &self.layout[i])
    }

    /// A key's id, by its place in the keyboard's reports.
    pub fn id(&self, pos: u32) -> &str {
        self.ids.get(&pos).map_or("", String::as_str)
    }

    /// The keyboard's state has come: keep it, and start a draft of each profile.
    pub fn loaded(&mut self, mut dev: Value) {
        for n in PROFILE_NUMBERS {
            if let Some(p) = dev.pointer_mut(&format!("/profiles/{n}")) {
                lit_screen(p);
            }
        }
        for n in PROFILE_NUMBERS {
            self.drafts.entry(n).or_insert_with(|| pick(&dev["profiles"][n.to_string()]));
        }
        self.editing = dev["active"].as_u64().filter(|n| (1..=5).contains(n)).unwrap_or(1) as u8;
        self.custom = crate::lighting::custom_colors(self.editing);
        self.dev = Some(dev);
    }

    fn saved(&self, n: &str) -> &Value {
        &self.dev.as_ref().expect("loaded")["profiles"][n]
    }

    /// The keys as the keyboard was made: what each key is reset to.
    fn factory_keys(&self) -> &Value {
        &self.dev.as_ref().expect("loaded")["factory"]["keys"]
    }

    pub fn draft(&self) -> &Value {
        &self.drafts[&self.editing]
    }

    /// Whether a profile differs from what the keyboard holds. (Per key is
    /// held as the backlight off, its colors kept by Ectomatter, so the two are the same.)
    pub fn is_dirty(&self, n: u8) -> bool {
        self.dev.is_some() && self.drafts.get(&n).is_some_and(|d| stored_form(d) != pick(self.saved(&n.to_string())))
    }

    /// A profile saved and read back: what the keyboard holds now, and the
    /// draft too unless it changed while it was saving (`replace` false).
    pub fn saved_profile(&mut self, n: u8, mut p: Value, replace: bool) {
        lit_screen(&mut p);
        if let Some(dev) = &mut self.dev {
            dev["profiles"][n.to_string()] = p.clone();
        }
        if replace {
            self.drafts.insert(n, pick(&p));
        }
    }

    /// What the page sends to save a profile.
    pub fn save_body_of(&self, n: u8) -> Value {
        self.drafts.get(&n).map(stored_form).unwrap_or(Value::Null)
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

    pub fn describe(&self, a: &Action) -> String {
        // a key's name, or its modifier's in full
        let key = |u: u8| if hid::is_modifier(u) { modifier(u, false).to_string() } else { hid_name(u as u32).unwrap_or(format!("Key {u}")) };
        match a {
            Action::Host(_) if screen_job(a).is_some() => self.screen_job_name(screen_job(a).unwrap()),
            Action::None => "Nothing".into(),
            Action::Key(u) => key(*u),
            Action::Shortcut(codes) => codes.iter().map(|u| key(*u)).collect::<Vec<_>>().join(" + "),
            Action::Media(code) => media::label(*code).map_or(format!("Media code {code}"), String::from),
            Action::Macro { n, .. } => format!("Macro: {}", self.macro_name(*n).unwrap_or(format!("number {n}"))),
            Action::Fn => "Fn".into(),
            Action::Own(id) => evga_z10::shared::OWN.iter().find(|(o, _)| o == id).map_or(id.clone(), |(_, l)| l.to_string()),
            Action::Scroll { up } => if *up { "Scroll up".into() } else { "Scroll down".into() },
            Action::Mouse(1) => "Left click".into(),
            Action::Mouse(2) => "Right click".into(),
            Action::Mouse(3) => "Middle click".into(),
            Action::Mouse(b) => format!("Mouse button {b}"),
            Action::Profile(Jump::To(n)) => format!("Switch to {}", self.profile_name(*n as u8)),
            Action::Profile(Jump::Next) => "Next profile".into(),
            Action::Profile(Jump::Previous) => "Previous profile".into(),
            Action::Off => "Disabled".into(),
            Action::Sensitivity => "Next sensitivity".into(),
            Action::Host(n) => format!("Ectomatter's job {n}"),
            Action::Raw(b) => format!("Keyboard code {}", b.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")),
        }
    }

    /// The codes that play one playlist each (see PLAY_CODES) that some key sends, on any
    /// profile, on either layer.
    pub fn sent_codes(&self) -> HashSet<u32> {
        self.drafts
            .values()
            .flat_map(|d| ["base", "fn"].map(|l| d["keys"][l].as_object().map(|m| m.values().cloned().collect::<Vec<_>>()).unwrap_or_default()))
            .flatten()
            .filter_map(|k| screen_job(&action_of(&k)))
            .filter(|c| PLAY_CODES.contains(c))
            .collect()
    }

    /// A screen job, by its code, as the action menu names it.
    pub fn screen_job_name(&self, code: u32) -> String {
        match code {
            NEXT_LAYOUT => "Next screen layout".into(),
            PREV_LAYOUT => "Previous screen layout".into(),
            NEXT_PLAYLIST => "Next playlist".into(),
            PREV_PLAYLIST => "Previous playlist".into(),
            c => match self.job_keys.get(&c) {
                // (a code one of Ectomatter's own jobs has)
                Some(words) => words.clone(),
                None => self.playlist_keys.get(&c).map_or("Play a deleted playlist".into(), |n| format!("Play {n}")),
            },
        }
    }

    /// A remapped key's new meaning, printed on it like a second legend.
    pub fn short_label(&self, a: &Action, macro_names: &[String]) -> String {
        // the modifiers short, each once, then the keys, the keypad's as Num
        let keys = |codes: &[u8]| {
            let mut parts: Vec<String> = Vec::new();
            for u in codes.iter().filter(|u| hid::is_modifier(**u)) {
                let n = modifier(*u, true);
                if !parts.iter().any(|p| p == n) {
                    parts.push(n.into());
                }
            }
            parts.extend(codes.iter().filter(|u| !hid::is_modifier(**u)).map(|h| {
                let n = hid_name(*h as u32).unwrap_or(format!("Key {h}"));
                n.strip_prefix("Keypad ").map_or(n.clone(), |r| format!("Num {r}"))
            }));
            parts.join("+")
        };
        match a {
            // (a key that plays one playlist is printed with the playlist's name, as one that
            // switches to a profile is with the profile's)
            Action::Host(_) if screen_job(a).is_some() => match screen_job(a).unwrap() {
                NEXT_LAYOUT => "NEXT".into(),
                PREV_LAYOUT => "PREV".into(),
                NEXT_PLAYLIST => "PL+".into(),
                PREV_PLAYLIST => "PL-".into(),
                // (one of Ectomatter's own jobs printed in its words)
                c => self.job_prints.get(&c).or_else(|| self.playlist_keys.get(&c)).cloned().unwrap_or("PLAY".into()),
            },
            Action::None => "Nothing".into(),
            Action::Key(u) => keys(&[*u]),
            Action::Shortcut(codes) => keys(codes),
            Action::Macro { n, .. } => distinct_part(&self.macro_name(*n).unwrap_or(format!("Macro {n}")), macro_names),
            // (a key that switches to a profile is printed with the profile's name)
            Action::Profile(Jump::To(n)) => self.profile_name(*n as u8),
            Action::Off => "Off".into(),
            _ => self.describe(a),
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

    /// What a key does on a layer ("base", or "fn" with Fn held), in the profile being edited.
    pub fn def(&self, layer: &str, pos: u32) -> Action {
        action_of(&self.draft()["keys"][layer][self.id(pos)])
    }

    /// What a key did as the keyboard was made.
    pub fn factory(&self, layer: &str, pos: u32) -> Action {
        action_of(&self.factory_keys()[layer][self.id(pos)])
    }

    pub fn set_def(&mut self, layer: &str, pos: u32, a: &Action) {
        let (n, id) = (self.editing, self.id(pos).to_string());
        if let Some(draft) = self.drafts.get_mut(&n) {
            draft["keys"][layer][id] = a.write();
        }
    }

    pub fn reset_key(&mut self, pos: u32) {
        for layer in ["base", "fn"] {
            let a = self.factory(layer, pos);
            self.set_def(layer, pos, &a);
        }
    }

    /// A macro keeps playing the way it already plays on its other keys.
    pub fn with_macro_play(&self, a: &Action) -> Action {
        let Action::Macro { n: which, .. } = a else { return a.clone() };
        let mut plays = Vec::new();
        for n in PROFILE_NUMBERS {
            for layer in ["base", "fn"] {
                if let Some(m) = self.drafts[&n]["keys"][layer].as_object() {
                    for v in m.values() {
                        if let Action::Macro { n, plays: p } = action_of(v) {
                            if n == *which {
                                plays.push(p);
                            }
                        }
                    }
                }
            }
        }
        // (a number of times is the same way to play whatever the number)
        let same = |a: &Plays, b: &Plays| std::mem::discriminant(a) == std::mem::discriminant(b) && !matches!((a, b), (Plays::Times(x), Plays::Times(y)) if x != y);
        let first = plays.first().copied();
        let varies = plays.iter().any(|p| !first.is_some_and(|f| same(p, &f)));
        match first {
            Some(Plays::Times(0)) if !varies => Action::Macro { n: *which, plays: Plays::Times(2) },
            Some(p) if !varies => Action::Macro { n: *which, plays: p },
            _ => Action::Macro { n: *which, plays: Plays::Once },
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
        let keys = hid_codes().into_iter().map(|h| Entry { label: hid_name(h).unwrap(), act: Action::Key(h as u8), tag: "Key" });
        let mods = (0..8).map(|i| hid::FIRST_MODIFIER + i).map(|u| Entry { label: modifier(u, false).to_string(), act: Action::Key(u), tag: "Key" });
        let mut media: Vec<Entry> = media::MEDIA.iter().map(|(c, _, n)| Entry { label: n.to_string(), act: Action::Media(*c), tag: "Media" }).collect();
        media.sort_by_key(|e| if let Action::Media(c) = e.act { c } else { 0 });
        let mut macros: Vec<(u32, Entry)> = self
            .dev
            .as_ref()
            .and_then(|d| d["macros"].as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(i, v)| {
                        let i: u32 = i.parse().ok()?;
                        Some((i, Entry { label: v["name"].as_str().map_or(format!("Macro {i}"), String::from), act: Action::Macro { n: i, plays: Plays::Once }, tag: "Macro" }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        macros.sort_by_key(|(i, _)| *i);
        let mouse = [("Left click", Action::Mouse(1)), ("Right click", Action::Mouse(2)), ("Middle click", Action::Mouse(3)), ("Scroll up", Action::Scroll { up: true }), ("Scroll down", Action::Scroll { up: false })]
            .into_iter()
            .map(|(l, act)| Entry { label: l.into(), act, tag: "Mouse" })
            .collect();
        let profiles = PROFILE_NUMBERS
            .iter()
            .map(|n| Entry { label: format!("Switch to {}", self.profile_name(*n)), act: Action::Profile(Jump::To(*n as u32)), tag: "Profile" })
            .chain([
                Entry { label: "Next profile".into(), act: Action::Profile(Jump::Next), tag: "Profile" },
                Entry { label: "Previous profile".into(), act: Action::Profile(Jump::Previous), tag: "Profile" },
            ])
            .collect();
        let board = vec![
            Entry { label: "Fn".into(), act: Action::Fn, tag: "Keyboard" },
            Entry { label: "Game mode toggle".into(), act: Action::Own("game-mode".into()), tag: "Keyboard" },
        ];
        let screen = |codes: [u32; 2]| codes.map(|c| Entry { label: self.screen_job_name(c), act: screen_slot(c), tag: "Screen" }).to_vec();
        Groups {
            macros: macros.into_iter().map(|(_, a)| a).collect(),
            keys: keys.chain(mods).collect(),
            media,
            mouse,
            profiles,
            layouts: screen([NEXT_LAYOUT, PREV_LAYOUT]),
            playlists: screen([NEXT_PLAYLIST, PREV_PLAYLIST]),
            board,
        }
    }

    /// The names of the macros on the board's first layer, so each one's
    /// printed label keeps only the words that tell it apart.
    pub fn board_macro_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if self.dev.is_none() {
            return out;
        }
        for k in &self.layout {
            if let Action::Macro { n, .. } = self.def("base", k.pos) {
                if let Some(n) = self.macro_name(n) {
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
        // (on every tab that shows the keyboard, not only Keys; the Function layer's, while
        // the Keys tab has it open)
        if self.dev.is_none() {
            return None;
        }
        let layer = if self.fn_layer && self.view == View::Keys { "fn" } else { "base" };
        let a = self.def(layer, pos);
        (a != self.factory(layer, pos)).then(|| self.short_label(&a, names))
    }

    /// The name, effect and small color sample of each profile's button.
    pub fn profile_effect(&self, n: u8) -> String {
        let mode = effect(&self.drafts[&n]["lighting"]).to_string();
        self.effect_desc(&mode)["label"].as_str().map_or(mode, String::from)
    }

    // ------------------------------------------------------------ the keys' effects, as the core describes them

    /// The keys' light as the keyboard's core describes it: its zones.
    pub fn keys_light(&self) -> Value {
        self.desc["capabilities"]["lighting"]["lights"].as_array().and_then(|l| l.iter().find(|l| l["id"] == "keys")).cloned().unwrap_or(Value::Null)
    }

    /// The keys' effects, as the core describes them, in the order the page shows them.
    pub fn effects(&self) -> Vec<Value> {
        self.desc["capabilities"]["lighting"]["effects"].as_array().cloned().unwrap_or_default()
    }

    /// An effect's description, by its id, with its layout from the
    /// description's (`layout.lighting`, by effect), for the page to draw it in.
    pub fn effect_desc(&self, id: &str) -> Value {
        let mut e = self.effects().into_iter().find(|e| e["id"] == id).unwrap_or(Value::Null);
        if let Some(layout) = self.desc["layout"]["lighting"].get(id).filter(|_| e.is_object()) {
            e["layout"] = layout.clone();
        }
        e
    }

    /// Where a profile keeps the keys' effect, and an effect's own settings.
    fn kept(&self, what: &str, effect: &str) -> String {
        self.desc["capabilities"]["lighting"]["kept"]["keys"][what].as_str().unwrap_or_default().replace("{effect}", effect)
    }

    /// The keys' effect on the profile being edited, by its id.
    pub fn effect_id(&self) -> String {
        crate::described::read_at(self.draft(), &self.kept("effect", "")).and_then(Value::as_str).unwrap_or("off").to_string()
    }

    /// The effect in use's own settings, on the profile being edited.
    pub fn effect_settings(&self) -> Value {
        crate::described::read_at(self.draft(), &self.kept("settings", &self.effect_id())).cloned().unwrap_or(Value::Null)
    }

    /// One of the effect in use's settings, by its piece, as drawn from the
    /// description: its id and description, and the effect's own settings.
    pub fn described(&self, piece: crate::described::Piece) -> Option<(String, Value, Value)> {
        let crate::described::Piece::Effect(k) = piece else { return None };
        let (id, d) = crate::described::params(&self.effect_desc(&self.effect_id())).get(k)?.clone();
        Some((id, d, self.effect_settings()))
    }

    /// The keys' effect chosen, on the profile being edited.
    pub fn set_effect(&mut self, id: &str) {
        let at = self.kept("effect", "");
        if let Some(d) = self.drafts.get_mut(&self.editing) {
            crate::described::write_at(d, &at, &json!(id));
        }
    }

    /// The effect in use's own settings changed, on the profile being edited.
    pub fn change_settings(&mut self, f: impl FnOnce(&mut Value)) {
        let at = self.kept("settings", &self.effect_id());
        let Some(d) = self.drafts.get_mut(&self.editing) else { return };
        let mut e = crate::described::read_at(d, &at).cloned().unwrap_or(Value::Null);
        f(&mut e);
        crate::described::write_at(d, &at, &e);
    }

    /// The effect in use's own settings put back as the keyboard came from the factory.
    pub fn reset_effect(&mut self) {
        let at = self.kept("settings", &self.effect_id());
        let factory = self.dev.as_ref().and_then(|dev| crate::described::read_at(&dev["factory"], &at)).cloned().unwrap_or(Value::Null);
        if let Some(d) = self.drafts.get_mut(&self.editing) {
            crate::described::write_at(d, &at, &factory);
        }
    }

    /// A profile's name, or "Profile 2" while it has the name it came with (or none yet).
    pub fn profile_name(&self, n: u8) -> String {
        let name = self.drafts.get(&n).and_then(|d| d["name"].as_str()).unwrap_or("");
        if name.is_empty() || name.to_uppercase().starts_with("DEFAULT") { format!("Profile {n}") } else { name.to_string() }
    }

    /// The profile's colors for its swatch: one color, or up to four for a gradient.
    pub fn profile_swatch(&self, n: u8) -> Vec<[f64; 3]> {
        let l = &self.drafts[&n]["lighting"];
        let e = &l["keys"]["effects"];
        match effect(l) {
            "off" | "custom" => vec![[0.0; 3]],
            "static" => vec![rgb(&e["static"]["color"])],
            m => {
                let cols: Vec<[f64; 3]> = e[m]["colors"].as_array().map(|a| a.iter().take(4).map(|c| rgb(&c["color"])).collect()).unwrap_or_default();
                if cols.is_empty() { vec![[136.0; 3], [255.0; 3]] } else { cols }
            }
        }
    }

    pub fn zone_set(&self, zone: u8) -> Option<Vec<u32>> {
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
        match effect(&self.draft()["lighting"]) {
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
        let l = self.draft()["lighting"]["keys"]["effects"].clone();
        let mode = match effect(&self.draft()["lighting"]) {
            "" => "off".to_string(),
            m => m.to_string(),
        };
        if mode == "stars" && self.random() < 0.35 {
            let i = (self.random() * self.layout.len() as f64) as usize % self.layout.len();
            let pos = self.layout[i].pos;
            if !self.stars.contains_key(&pos) {
                let c = [80.0 + (self.random() * 176.0).floor(), 80.0 + (self.random() * 176.0).floor(), 80.0 + (self.random() * 176.0).floor()];
                self.stars.insert(pos, (t, c));
            }
        }
        if self.demo && mode == "trigger" && self.random() < 0.08 {
            let i = (self.random() * self.layout.len() as f64) as usize % self.layout.len();
            let pos = self.layout[i].pos;
            self.flash(pos, t);
        }
        let colors = if mode == "custom" {
            // each key its own color, lit wherever it has one
            self.layout.iter().filter(|k| k.led).map(|k| {
                let c = self.custom.get(k.pos as usize).copied().unwrap_or([0; 3]);
                (k.pos, [c[0] as f64, c[1] as f64, c[2] as f64, if c.iter().any(|v| *v > 0) { 1.0 } else { 0.0 }])
            }).collect()
        } else {
            self.effect_colors(&l, &mode, t)
        };
        self.last_colors = colors.clone();
        let names = self.board_macro_names();
        self.layout
            .iter()
            .filter(|k| k.led)
            .filter_map(|k| {
                let v = colors.get(&k.pos)?;
                // the dimmer scales every key's light inside the keyboard, as color x level / 256
                let dim = self.dimmer.map_or(1.0, |d| d as f64 / 256.0);
                Some(Light { pos: k.pos, rgb: led_look([v[0], v[1], v[2]]), strength: ((v[3] * dim * 1000.0).round() / 1000.0) as f32, remapped: self.remap(k.pos, &names).is_some() })
            })
            .collect()
    }

    /// Each lit key's color and light at time `t` (ms) for an effect, `l` every
    /// effect's settings as the profile keeps them.
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
                    // as the keyboard's own Pulse draws it (its firmware at 0xC7FC):
                    // each color for its share of the cycle, then straight to the
                    // next, with no fade and nothing between; in a random order, a
                    // random color each step. (The profile's background and hold
                    // times are stored, but the keyboard never reads them.)
                    let n = (e["count"].as_u64().filter(|c| *c > 0).unwrap_or(7) as usize).clamp(1, 7);
                    let step = cycle / n as f64;
                    let k = (t / step).floor() as u64;
                    c = if e["random"].as_u64().unwrap_or(0) != 0 {
                        random_color(k)
                    } else {
                        cols.get(k as usize % n).copied().unwrap_or([255.0; 3])
                    };
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

    /// Where a key the keyboard sent sits on the board (what it sent, as the
    /// keyboard's core tells it: a key, a modifier among them, a media key or
    /// a host slot): the key that sends it now, in the active profile as the
    /// keyboard has it saved, one that sends only it before one that sends
    /// more with it (C before Ctrl+C); failing that, one that sends it with Fn
    /// held. A key whose meaning is a macro, Fn or nothing sends nothing of
    /// its own.
    pub fn pos_for(&self, sent: &Action) -> Option<u32> {
        let profile = &self.dev.as_ref()?["profiles"][self.active().to_string()];
        // how closely a key's meaning sends it: 2 only it, 1 it among more, 0 not at all
        let sends = |a: &Action| match (a, sent) {
            (Action::Key(u), Action::Key(s)) => u32::from(u == s) * 2,
            // (a key among more: with a modifier, or beside another key)
            (Action::Shortcut(codes), Action::Key(s)) if !hid::is_modifier(*s) && codes.contains(s) => 1,
            (Action::Media(c), Action::Media(s)) => u32::from(c == s) * 2,
            (Action::Host(n), Action::Host(s)) => u32::from(n == s) * 2,
            _ => 0,
        };
        ["base", "fn"].iter().find_map(|layer| {
            let best = (0..evga_z10::protocol::NKEYS as u32)
                .filter_map(|pos| Some((sends(&action_of(profile["keys"][*layer].get(self.id(pos))?)), pos)))
                .filter(|(m, _)| *m > 0)
                .max_by_key(|(m, pos)| (*m, std::cmp::Reverse(*pos)))?;
            Some(best.1)
        })
    }

    /// A key's Reactive flash as its LED shows it, while one plays on it
    /// (from the moment it's pressed, before the keyboard is lit again);
    /// none with another effect on, which leaves any flash unseen.
    pub fn flash_look(&self, pos: u32) -> Option<[u8; 3]> {
        if self.dev.is_none() || effect(&self.draft()["lighting"]) != "trigger" {
            return None;
        }
        self.flashes.get(&pos).map(|(_, c)| led_look(*c))
    }

    /// Light a key as pressing it would, for Reactive.
    pub fn flash(&mut self, pos: u32, t: f64) {
        if self.dev.is_none() || effect(&self.draft()["lighting"]) != "trigger" {
            return;
        }
        let e = self.draft()["lighting"]["keys"]["effects"]["trigger"].clone();
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
        self.custom = crate::lighting::custom_colors(n);
    }

    /// What the keyboard is sent to show the draft while the Lighting page is
    /// open: each key's color for Per key, Stars and Reactive (which the
    /// keyboard cannot play from a preset), or the effect itself.
    pub fn preview_body(&self) -> Value {
        let l = &self.draft()["lighting"];
        match effect(l) {
            "custom" => json!({ "colors": (0..116).map(|i| self.custom.get(i).copied().unwrap_or([0; 3])).collect::<Vec<_>>() }),
            "stars" | "trigger" => json!({ "colors": self.streamed() }),
            _ => json!({ "lighting": l }),
        }
    }

    pub fn streams(&self) -> bool {
        matches!(effect(&self.draft()["lighting"]), "stars" | "trigger")
    }

    /// Each key's color as the screen shows it this frame, for the keyboard.
    pub fn streamed(&self) -> Vec<[u8; 3]> {
        (0..116u32).map(|i| self.last_colors.get(&i).map_or([0; 3], |v| [0, 1, 2].map(|j| (v[j] * v[3]).round().clamp(0.0, 255.0) as u8))).collect()
    }

    /// The display's backlight as it looks: leaning as the keys' LEDs do, and
    /// dimmer with its brightness.
    pub fn lcd_color(&self) -> [f64; 3] {
        if self.dev.is_none() {
            return [90.0; 3];
        }
        let c = &self.draft()["lighting"]["screen"]["color"];
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

/// A draft as the keyboard stores it: Per key as the backlight off.
fn stored_form(d: &Value) -> Value {
    let mut d = d.clone();
    if d["lighting"]["keys"]["effect"] == "custom" {
        d["lighting"]["keys"]["effect"] = json!("off");
    }
    d
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
pub fn step_name(s: &Value) -> String {
    if let Some(m) = s.get("media") {
        let m = m.as_str().and_then(media::code).map_or_else(|| u(m), u32::from);
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
    let k = &s["key"];
    let code = k.as_str().and_then(hid::usage).map_or_else(|| u(k), u32::from);
    let name = match u8::try_from(code) {
        Ok(m) if hid::is_modifier(m) => Some(modifier(m, false).to_string()),
        // (F13 to F24 by their names too, which Ectomatter's own macros can play)
        _ if code > 0 && code < 256 => hid_name(code),
        _ => None,
    };
    name.unwrap_or(format!("Code {code}"))
}

/// A color for Pulse's step `k` in a random order: the same one for the
/// whole step, as the keyboard picks one as each step begins.
fn random_color(k: u64) -> [f64; 3] {
    let mut x = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    [(x & 0xff) as f64, ((x >> 8) & 0xff) as f64, ((x >> 16) & 0xff) as f64]
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// a loop of colors, `u` in [0, 1) around it
fn ramp(colors: &[[f64; 3]], u: f64) -> [f64; 3] {
    let n = colors.len();
    if n == 0 {
        return [0.0; 3];
    }
    let x = ((u % 1.0) + 1.0) % 1.0 * n as f64;
    let i = (x.floor() as usize).min(n - 1);
    mix(colors[i], colors[(i + 1) % n], x - i as f64)
}

/// The Z10's LEDs do not show a pure color: each channel spills into
/// another (red and blue into green, green into blue, 0x60 of each at full),
/// so the keyboard on screen shows a color as the keys do.
fn led_look(c: [f64; 3]) -> [u8; 3] {
    bleed(c, f64::from_bits(KEYS_BLEED.load(std::sync::atomic::Ordering::Relaxed)))
}

/// How far a color bleeds, as a share of each channel at full: the core's
/// light says (switches.bleed for the keys' LEDs, display.bleed for the
/// screen's backlight; the lab tunes both), 0x60 of each until it does.
pub const BLEED: f64 = 0x60 as f64 / 255.0;
// (BLEED's bits, as an atomic starts from a constant)
static KEYS_BLEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x3FD8_1818_1818_1818);
static SCREEN_BLEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x3FD8_1818_1818_1818);

/// The bleeds the core's light asks for now.
pub fn set_bleeds(light: &Value) {
    let of = |v: &Value| v.as_f64().unwrap_or(BLEED).clamp(0.0, 1.0).to_bits();
    KEYS_BLEED.store(of(&light["switches"]["bleed"]), std::sync::atomic::Ordering::Relaxed);
    SCREEN_BLEED.store(of(&light["display"]["bleed"]), std::sync::atomic::Ordering::Relaxed);
}

fn bleed(c: [f64; 3], spill: f64) -> [u8; 3] {
    [c[0] as u8, (c[1] + c[0] * spill + c[2] * spill).min(255.0) as u8, (c[2] + c[1] * spill).min(255.0) as u8]
}

/// A screen light as the Z10's screen shows it: its backlight bleeds as the
/// keys' LEDs do, by its own share (display.bleed).
pub fn as_lit(c: [u8; 3]) -> [u8; 3] {
    bleed(c.map(f64::from), f64::from_bits(SCREEN_BLEED.load(std::sync::atomic::Ordering::Relaxed)))
}

#[cfg(test)]
mod saving_tests {
    use super::*;

    fn page() -> Page {
        let mut p = Page::new();
        let prof = json!({ "name": "DEFAULT 1", "lighting": { "keys": { "effect": "off", "effects": { "static": { "zone": 128 } } } }, "keys": { "base": {}, "fn": {} } });
        p.dev = Some(json!({ "active": 1, "profiles": { "1": prof.clone() }, "factory": { "keys": { "base": {}, "fn": {} } } }));
        p.drafts.insert(1, pick(&prof));
        p.editing = 1;
        p
    }

    #[test]
    fn per_key_is_saved_once_the_keyboard_holds_its_backlight_off() {
        let mut p = page();
        p.drafts.get_mut(&1).unwrap()["lighting"]["keys"]["effect"] = json!("custom");
        // the keyboard stores Per key as off, so a draft of Per key over a stored off is saved
        assert!(!p.is_dirty(1));
        assert_eq!(p.save_body_of(1)["lighting"]["keys"]["effect"], "off");
        p.drafts.get_mut(&1).unwrap()["lighting"]["keys"]["effect"] = json!("static");
        assert!(p.is_dirty(1));
    }

    #[test]
    fn pulse_jumps_from_color_to_color_as_the_keyboard_does() {
        let mut p = page();
        let c = |r: u8, g: u8, b: u8| json!({ "brightness": 255, "color": [r, g, b] });
        let l = json!({ "pulse": { "zone": 128, "cycle": 9000, "count": 3, "random": 0, "background": c(0, 0, 255),
            "colors": [c(255, 0, 0), c(0, 255, 0), c(255, 255, 255)] } });
        let at = |p: &mut Page, t: f64| p.effect_colors(&l, "pulse", t).values().next().copied().expect("a lit key");
        // 3 s a color: the whole color from the first moment of its step to the last, never the background
        assert_eq!(at(&mut p, 0.0), [255.0, 0.0, 0.0, 1.0]);
        assert_eq!(at(&mut p, 2999.0), [255.0, 0.0, 0.0, 1.0]);
        assert_eq!(at(&mut p, 3000.0), [0.0, 255.0, 0.0, 1.0]);
        assert_eq!(at(&mut p, 7500.0), [255.0, 255.0, 255.0, 1.0]);
        assert_eq!(at(&mut p, 9000.0), [255.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_screen_light_of_black_comes_as_white() {
        let mut p = Page::new();
        let screen = |rgb: [u8; 3]| json!({ "color": { "brightness": 255, "color": rgb }, "apps": 47 });
        let prof = |rgb: [u8; 3]| json!({ "name": "P", "lighting": { "keys": { "effect": "off" }, "screen": screen(rgb) }, "keys": { "base": {}, "fn": {} } });
        p.loaded(json!({ "active": 1, "profiles": { "1": prof([0, 0, 0]), "2": prof([255, 32, 0]) } }));
        // the unlit screen light shows and plays white, and is saved as white; a chosen light stays
        assert_eq!(p.drafts[&1]["lighting"]["screen"]["color"]["color"], json!([255, 255, 255]));
        assert!(!p.is_dirty(1));
        assert_eq!(p.save_body_of(1)["lighting"]["screen"]["color"]["color"], json!([255, 255, 255]));
        assert_eq!(p.drafts[&2]["lighting"]["screen"]["color"]["color"], json!([255, 32, 0]));
        // and read back after a save
        p.saved_profile(1, prof([0, 0, 0]), true);
        assert_eq!(p.drafts[&1]["lighting"]["screen"]["color"]["color"], json!([255, 255, 255]));
    }

    #[test]
    fn a_change_made_while_saving_is_kept() {
        let mut p = page();
        p.drafts.get_mut(&1).unwrap()["lighting"]["keys"]["effect"] = json!("static");
        let read_back = json!({ "name": "DEFAULT 1", "lighting": { "keys": { "effect": "static", "effects": { "static": { "zone": 128 } } } }, "keys": { "base": {}, "fn": {} } });
        // changed again before the keyboard answered
        p.drafts.get_mut(&1).unwrap()["name"] = json!("Games");
        p.saved_profile(1, read_back, false);
        assert_eq!(p.drafts[&1]["name"], "Games");
        assert!(p.is_dirty(1), "the newer change still saves");
    }

    #[test]
    fn a_key_with_a_screen_job_says_so_and_prints_it() {
        let mut p = page();
        p.playlist_keys.insert(0xA9, "Quiet".into());
        // (as the keyboard's core gives each: a host slot, or a key)
        let of = |d: [u8; 4]| evga_z10::shared::action_of(d);
        assert_eq!(kind_of(&of([0, 0, NEXT_LAYOUT as u8, 0])), "Screen");
        assert_eq!(kind_of(&of([0, 0, 0xAF, 0])), "Screen");
        // (with a modifier, or past the reserved run, it's a key like any other)
        assert_eq!(kind_of(&of([0, 1, NEXT_LAYOUT as u8, 0])), "Key");
        assert_eq!(kind_of(&of([0, 0, 0xB0, 0])), "Key");
        let says: Vec<(String, String)> = [NEXT_LAYOUT, PREV_LAYOUT, NEXT_PLAYLIST, PREV_PLAYLIST, 0xA9, 0xAA].iter().map(|c| (p.describe(&screen_slot(*c)), p.short_label(&screen_slot(*c), &[]))).collect();
        let want = [("Next screen layout", "NEXT"), ("Previous screen layout", "PREV"), ("Next playlist", "PL+"), ("Previous playlist", "PL-"), ("Play Quiet", "Quiet"), ("Play a deleted playlist", "PLAY")];
        assert_eq!(says, want.map(|(a, b)| (a.to_string(), b.to_string())));
        let g = p.groups();
        assert_eq!(g.layouts.iter().chain(&g.playlists).map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Next screen layout", "Previous screen layout", "Next playlist", "Previous playlist"]);
        // and none of the codes is a key in the menu's list of keys
        assert!(g.keys.iter().all(|e| screen_job(&e.act).is_none()));
    }

    #[test]
    fn a_profile_is_called_by_its_name_wherever_a_key_switches_to_it() {
        let mut p = page();
        p.drafts.get_mut(&1).unwrap()["name"] = json!("Games");
        // the actions a key can be given, and what a key given one says it does
        let labels: Vec<String> = p.groups().profiles.iter().map(|a| a.label.clone()).collect();
        assert_eq!(labels[..2], ["Switch to Games", "Switch to Profile 2"], "a profile still named as it came, by its number");
        assert_eq!(p.describe(&Action::Profile(Jump::To(1))), "Switch to Games");
        assert_eq!(p.short_label(&Action::Profile(Jump::To(1)), &[]), "Games", "printed on the key");
        // and where a macro is used
        assert_eq!(crate::macros::profiles_text(&[1, 2], |n| p.profile_name(n)), "Games and Profile 2");
        assert_eq!(crate::macros::profiles_text(&[1, 2, 3, 4, 5], |n| p.profile_name(n)), "all profiles");
    }
}
