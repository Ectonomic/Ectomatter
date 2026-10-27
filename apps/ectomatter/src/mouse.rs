//! The mouse's page: a mouse whose profiles Ectomatter keeps, through its
//! core, drawn from what the core describes (its buttons, lights, picture
//! and callouts), built from the keyboard page's parts as its mock has it
//! (mock/sensei-310.html): Buttons, Lighting, Performance and Settings.
//!
//! The core is asked on a thread of its own, which tells the page what the
//! mouse is set to after each change and whenever the mouse comes or goes.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use crate::shell::Sender;

use crate::core_host::Core;
use crate::lighting::{Ctl, Stack};
use crate::{Cell, MouseCall, MouseRow};

/// A button, as the mouse's core describes it: its id, its name, the cap
/// beside it in the Key detail box, its surfaces in the drawing (`pieces`),
/// and its callout on the page (on the left or not, how far down its words
/// are, and the line from them to its dot, in the mock's canvas).
#[derive(Clone, Debug)]
pub struct Button {
    pub id: String,
    pub label: String,
    pub short: String,
    pub pieces: Vec<String>,
    pub callout: (bool, f32, Vec<(f32, f32)>),
}

/// The mouse Ectomatter shows, as its core describes it: the core's id and
/// folder, the device's id, its art's file, its buttons, its lights in the order its colors
/// stream (`direct`, `lights`), each light's groups in the drawing (each by
/// its index among the lights) in the order they're laid, what of them is the
/// drawing's own glow, the part of the drawing the mouse fills, and its settings.
#[derive(Debug, Default)]
pub struct Shape {
    pub core: String,
    pub folder: PathBuf,
    pub device: String,
    pub art: PathBuf,
    pub buttons: Vec<Button>,
    pub lights: Vec<String>,
    pub groups: Vec<(usize, String)>,
    pub glow: Vec<String>,
    pub view: (f32, f32, f32, f32),
    /// the ids of its settings (`settings`, `items`)
    pub settings: Vec<String>,
    /// all it describes, under what it tells of the device once met
    pub described: Value,
}

impl Shape {
    /// The mouse its core (`core`, in `folder`) describes so (`device/describe`).
    pub fn of(core: &str, folder: &Path, d: &Value) -> Shape {
        let n = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
        let buttons = d["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                let id = c["id"].as_str()?.to_string();
                let word = |k: &str| c[k].as_str().map(String::from);
                let label = word("label").unwrap_or_else(|| id.clone());
                let co = &c["callout"];
                let line = co["line"].as_array().into_iter().flatten().map(|p| (n(&p[0]), n(&p[1]))).collect();
                Some(Button { short: word("short").unwrap_or_else(|| label.clone()), pieces: c["pieces"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).collect(), callout: (co["side"] == "left", n(&co["at"]), line), label, id })
            })
            .collect();
        let described = d["capabilities"]["lighting"]["lights"].as_array().cloned().unwrap_or_default();
        let lights: Vec<String> = d["capabilities"]["lighting"]["direct"]["lights"].as_array().map(|l| l.iter().filter_map(|l| l.as_str().map(String::from)).collect()).unwrap_or_else(|| described.iter().filter_map(|l| l["id"].as_str().map(String::from)).collect());
        // (each light's pieces, those under the buttons first, then those over everything)
        let art = |l: &str| described.iter().find(|x| x["id"] == l).map(|x| x["art"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect::<Vec<_>>()).unwrap_or_default();
        let mut groups = Vec::new();
        for over in [false, true] {
            for (i, l) in lights.iter().enumerate() {
                groups.extend(art(l).into_iter().filter(|a| (*a >= 200) == over).map(|a| (i, format!("illumination_{a}"))));
            }
        }
        let glow = described.iter().filter_map(|l| l["glow"].as_u64()).map(|a| format!("illumination_{a}")).collect();
        let v = &d["view"];
        let settings = d["capabilities"]["settings"]["items"].as_array().into_iter().flatten().filter_map(|i| i["id"].as_str().map(String::from)).collect();
        Shape { core: core.to_string(), folder: folder.to_path_buf(), device: d["device"].as_str().unwrap_or_default().to_string(), art: folder.join(d["art"].as_str().unwrap_or_default()), buttons, lights, groups, glow, view: (n(&v[0]), n(&v[1]), n(&v[2]), n(&v[3])), settings, described: d.clone() }
    }
}

static SHAPE: OnceLock<Shape> = OnceLock::new();

/// The mouse Ectomatter shows, once its core has described it (`describe`);
/// with none, a mouse of nothing.
pub fn shape() -> &'static Shape {
    #[cfg(test)]
    if SHAPE.get().is_none() {
        let _ = SHAPE.set(described_alone());
    }
    static NONE: OnceLock<Shape> = OnceLock::new();
    SHAPE.get().unwrap_or_else(|| NONE.get_or_init(Shape::default))
}

/// The mouse's key, what Ectomatter keeps its profiles, jobs, scenes, Library
/// choices and place on the desk under: its device's id as its core names it
/// ("" with no mouse).
pub fn key() -> &'static str {
    &shape().device
}

/// The mouse's record, from what its core, started, describes.
pub fn describe(core: &str, folder: &Path, described: &Value) {
    if described["kind"] == "mouse" {
        let _ = SHAPE.set(Shape::of(core, folder, described));
    }
}

/// A test's mouse: the first core that describes one, run alone with no device.
#[cfg(test)]
fn described_alone() -> Shape {
    for p in crate::packages::of(crate::packages::Kind::Core) {
        let d = crate::core_host::described_alone(&p.dir, std::time::Duration::from_secs(60));
        if d["kind"] == "mouse" {
            return Shape::of(p.id(), &p.dir, &d);
        }
    }
    Shape::default()
}

/// What the page asks of the mouse.
pub enum Ask {
    Key(String, Value),
    ResetKey(String),
    Light(String, Value),
    Setting(String, Value),
    /// the lights in colors streamed to them (the desk's light), kept from the
    /// mouse's memory; and its lights as it keeps them again
    Direct(Value),
    HandBack,
    /// a whole profile put on the mouse (the core's profiles/apply)
    Apply(Value),
    /// the core told something (a button pressed): look now
    Nudge,
}

/// What the page hears: everything the core says of the mouse, a change it
/// refused, a light's effect the mouse has taken (what the light plays from
/// that moment), or the mouse's picture drawn in layers, for its page or for
/// Home's desk.
pub enum Heard {
    State(Value),
    Failed(String),
    Played(String, Value, Instant),
    /// (the mouse's width in pixels it was drawn at, and the window's scale then)
    Drawn(u32, f32, Box<crate::mouse_art::Layers>),
    /// a button given one of Ectomatter's jobs pressed (true) or let go
    Input(String, bool),
}

/// Start the mouse's thread, with the mouse's core.
pub fn start(core: Core, tell: Sender<Heard>) -> mpsc::Sender<Ask> {
    let (ask, asked) = mpsc::channel::<Ask>();
    // (a button pressed is told at once, not at the next look)
    let nudge = std::sync::Mutex::new(ask.clone());
    core.on_told(move || {
        let _ = nudge.lock().map(|n| n.send(Ask::Nudge));
    });
    // (the device, as the mouse's core names it, in every call)
    let device_id = shape().device.clone();
    std::thread::spawn(move || {
        // every button heard as it's pressed, as the keyboard's every key is
        // (Home's desk answers each with a ripple)
        if let Err(e) = core.call("input/watch", &json!({ "device": device_id, "all": true })) {
            eprintln!("ectomatter: the mouse's buttons won't be heard: {e}");
        }
        // (what the mouse ships as, which a new profile starts as)
        let factory = core.call("profiles/factory", &json!({ "device": device_id })).unwrap_or(Value::Null);
        let (mut present, mut device) = (false, Value::Null);
        let read = |present: bool, device: &Value| {
            let d = json!({ "device": device_id });
            json!({
                "present": present,
                "device": device,
                "keys": core.call("keys/get", &d).unwrap_or(Value::Null),
                "lighting": core.call("lighting/get", &d).unwrap_or(Value::Null),
                "settings": core.call("settings/get", &d).unwrap_or(Value::Null),
                "factory": factory,
            })
        };
        let mut last = Value::Null;
        loop {
            for (method, p) in core.told() {
                match method.as_str() {
                    "device/added" => {
                        present = true;
                        device = p;
                    }
                    "device/removed" => present = false,
                    "input/down" | "input/up" => {
                        let control = p["control"].as_str().unwrap_or_default().to_string();
                        let _ = tell.send(Heard::Input(control, method == "input/down"));
                    }
                    // (its core stopped: the mouse is gone until it's back, and the page
                    // says why; started again, it meets the mouse anew, and is asked to
                    // hear every button again)
                    crate::core_host::STOPPED => {
                        present = false;
                        let why = p["why"].as_str().unwrap_or("it stopped");
                        let _ = tell.send(Heard::Failed(format!("The mouse’s core stopped: {why}. Ectomatter starts it again {}.", crate::core_host::again_in(p["again"].as_u64().unwrap_or(1)))));
                    }
                    crate::core_host::RESTARTED => {
                        if let Err(e) = core.call("input/watch", &json!({ "device": device_id, "all": true })) {
                            eprintln!("ectomatter: the mouse's buttons won't be heard: {e}");
                        }
                    }
                    _ => {}
                }
            }
            let now = read(present, &device);
            if now != last {
                last = now.clone();
                if tell.send(Heard::State(now)).is_err() {
                    break;
                }
            }
            match asked.recv_timeout(Duration::from_secs(1)) {
                Ok(a) => {
                    // of several asked at once, only the newest of each kind (a slider dragged)
                    let mut all = vec![a];
                    while let Ok(b) = asked.try_recv() {
                        all.push(b);
                    }
                    let key = |a: &Ask| match a {
                        Ask::Key(c, _) | Ask::ResetKey(c) => format!("key {c}"),
                        Ask::Light(l, _) => format!("light {l}"),
                        Ask::Setting(s, _) => format!("setting {s}"),
                        Ask::Apply(_) => "profile".into(),
                        Ask::Nudge => "nudge".into(),
                        Ask::Direct(_) => "direct".into(),
                        Ask::HandBack => "hand back".into(),
                    };
                    let mut newest: Vec<Ask> = Vec::new();
                    for a in all {
                        newest.retain(|b| key(b) != key(&a));
                        newest.push(a);
                    }
                    for a in newest {
                        let r = match a {
                            Ask::Key(control, action) => core.call("keys/set", &json!({ "device": device_id, "layer": "base", "control": control, "action": action })).map(|_| ()),
                            Ask::ResetKey(control) => core.call("keys/reset", &json!({ "device": device_id, "control": control })).map(|_| ()),
                            // (the moment the mouse takes an effect is when it starts its cycle)
                            Ask::Light(light, l) => core.call("lighting/set", &json!({ "device": device_id, "light": light, "effect": l["effect"], "params": l["params"] })).map(|v| {
                                let _ = tell.send(Heard::Played(light, v["plays"].clone(), Instant::now()));
                            }),
                            Ask::Setting(setting, v) => core.call("settings/set", &json!({ "device": device_id, "id": setting, "value": v })).map(|_| ()),
                            // (what the core told is read at the top of the loop)
                            Ask::Nudge => Ok(()),
                            Ask::Direct(colors) => core.call("lighting/direct", &json!({ "device": device_id, "colors": colors })).map(|_| ()),
                            Ask::HandBack => core.call("lighting/hand-back", &json!({ "device": device_id })).map(|v| {
                                let at = Instant::now();
                                for l in shape().lights.iter().map(String::as_str) {
                                    if v["plays"][l].is_object() {
                                        let _ = tell.send(Heard::Played(l.into(), v["plays"][l].clone(), at));
                                    }
                                }
                            }),
                            Ask::Apply(mut body) => {
                                body["device"] = json!(device_id);
                                core.call("profiles/apply", &body).map(|v| {
                                    // (each light from the moment the mouse took the whole)
                                    let at = Instant::now();
                                    for l in shape().lights.iter().map(String::as_str) {
                                        let _ = tell.send(Heard::Played(l.into(), v["plays"][l].clone(), at));
                                    }
                                })
                            }
                        };
                        if let Err(e) = r {
                            let _ = tell.send(Heard::Failed(e));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = core.call("shutdown", &Value::Null);
    });
    ask
}

// ---------------------------------------------------------------- the buttons

/// The kinds of job a button can have, as the Key detail box's switch shows
/// them, each drawn on its 24 grid (the Key, Media, Mouse and Nothing glyphs
/// as the keyboard's own).
pub const KINDS: [(&str, &str); 8] = [
    ("Mouse", "M7 3.5h10v17H7zM12 7v3"),
    ("Key", "M4 5h16v14H4zM9 10h6M12 10v5"),
    ("Media", "M5 6l8 6-8 6zM15 6v12M19 6v12"),
    // (one of Ectomatter's macros, which Ectomatter plays: the keyboard's Macro glyph)
    ("Macro", "M4 7h10M4 12h16M4 17h7"),
    ("Scroll", "M12 4v16M8 8l4-4 4 4M8 16l4 4 4-4"),
    ("Sensitivity", "M4 17a8 8 0 1 1 16 0M12 17l4.5-5"),
    // (a job on another device, which Ectomatter does: a keyboard and a mouse)
    ("Devices", "M2.5 8h12v9h-12zM5.5 11h1M8.5 11h1M11.5 11h1M5.5 14h7M17.5 9.5a2 2 0 0 1 4 0v6a2 2 0 0 1-4 0z"),
    ("Nothing", "M4 12a8 8 0 1 0 16 0a8 8 0 1 0-16 0M6.5 17.5l11-11"),
];

/// The place in KINDS of the macros' kind, of another device's jobs, and of nothing's.
pub const MACRO_KIND: usize = 3;
pub const DEVICES_KIND: usize = 6;
pub const NOTHING_KIND: usize = 7;

const MOUSE_JOBS: [&str; 8] = ["Left click", "Right click", "Middle click", "Back", "Forward", "Button 6", "Button 7", "Button 8"];
const MEDIA: [(&str, &str); 6] = [("PlayPause", "Play or pause"), ("Next", "Next track"), ("Previous", "Previous track"), ("VolumeUp", "Volume up"), ("VolumeDown", "Volume down"), ("Mute", "Mute")];

/// The keys a button can send: their USB HID usages and the core protocol's
/// names for them.
fn keys() -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = Vec::new();
    for (i, c) in ('A'..='Z').enumerate() {
        v.push((4 + i as u32, c.to_string()));
    }
    for d in 1..=9u32 {
        v.push((29 + d, d.to_string()));
    }
    v.push((39, "0".into()));
    let named: [(u32, &str); 45] = [
        (40, "Enter"), (41, "Escape"), (42, "Backspace"), (43, "Tab"), (44, "Space"), (45, "Minus"), (46, "Equal"),
        (47, "LeftBracket"), (48, "RightBracket"), (49, "Backslash"), (51, "Semicolon"), (52, "Quote"), (53, "Grave"),
        (54, "Comma"), (55, "Period"), (56, "Slash"), (57, "CapsLock"), (70, "PrintScreen"), (71, "ScrollLock"),
        (72, "Pause"), (73, "Insert"), (74, "Home"), (75, "PageUp"), (76, "Delete"), (77, "End"), (78, "PageDown"),
        (79, "ArrowRight"), (80, "ArrowLeft"), (81, "ArrowDown"), (82, "ArrowUp"), (83, "NumLock"), (84, "KeypadSlash"),
        (85, "KeypadAsterisk"), (86, "KeypadMinus"), (87, "KeypadPlus"), (88, "KeypadEnter"), (99, "KeypadPeriod"), (101, "Menu"),
        (224, "LeftCtrl"), (225, "LeftShift"), (226, "LeftAlt"), (227, "LeftMeta"), (228, "RightCtrl"), (229, "RightShift"), (230, "RightAlt"),
    ];
    v.extend(named.iter().map(|(u, n)| (*u, n.to_string())));
    for f in 1..=24u32 {
        v.push((if f <= 12 { 57 + f } else { 91 + f }, format!("F{f}")));
    }
    for d in 1..=9u32 {
        v.push((88 + d, format!("Keypad{d}")));
    }
    v.push((98, "Keypad0".into()));
    v.sort();
    v
}

fn key_label(usage: u32) -> String {
    match usage {
        224 => "Left Ctrl".into(),
        225 => "Left Shift".into(),
        226 => "Left Alt".into(),
        227 => "Left Super".into(),
        228 => "Right Ctrl".into(),
        229 => "Right Shift".into(),
        230 => "Right Alt".into(),
        u => crate::keyboard::hid_name(u).unwrap_or_else(|| format!("Key {u}")),
    }
}

/// A key action's usage: the core protocol's name, or a number.
fn usage_of(k: &Value) -> Option<u32> {
    k.as_u64().map(|n| n as u32).or_else(|| k.as_str().and_then(|n| keys().into_iter().find(|(_, name)| name == n).map(|(u, _)| u)))
}

/// The kind of a job, by its place in KINDS.
pub fn kind_of(a: &Value) -> usize {
    if a.get("mouse").is_some() {
        0
    } else if a.get("key").is_some() || a.get("shortcut").is_some() {
        1
    } else if a.get("media").is_some() {
        2
    } else if a.get("macro").is_some() {
        MACRO_KIND
    } else if a.get("scroll").is_some() {
        4
    } else if a.get("sensitivity").is_some() {
        5
    } else if a.get("ectomatter").is_some() {
        DEVICES_KIND
    } else {
        NOTHING_KIND
    }
}

/// A job in words.
pub fn job_label(a: &Value) -> String {
    job_label_with(a, &crate::jobs::Names::default())
}

/// A job in words, another device's jobs named as `names` has them.
pub fn job_label_with(a: &Value, names: &crate::jobs::Names) -> String {
    match kind_of(a) {
        DEVICES_KIND => crate::jobs::Job::read(a).map_or("Another device's job".into(), |j| names.of(&j)),
        0 => a["mouse"].as_u64().and_then(|n| MOUSE_JOBS.get(n as usize - 1)).map_or("A mouse button".into(), |s| s.to_string()),
        // (a key with modifiers held as the keyboard's page says one: Left Ctrl + T)
        1 if a.get("shortcut").is_some() => {
            let keys: Option<Vec<String>> = a["shortcut"].as_array().map(|l| l.iter().map(|k| usage_of(k).map_or_else(|| k.as_str().unwrap_or("?").to_string(), key_label)).collect());
            keys.filter(|k| !k.is_empty()).map_or("A shortcut".into(), |k| k.join(" + "))
        }
        1 => usage_of(&a["key"]).map_or("A key".into(), key_label),
        2 => MEDIA.iter().find(|(n, _)| a["media"] == *n).map_or("A media key".into(), |(_, l)| l.to_string()),
        MACRO_KIND => {
            let n = a["macro"].as_u64().unwrap_or(0) as u32;
            match crate::library::get(n) {
                Some(m) => m["name"].as_str().filter(|s| !s.is_empty()).map_or(format!("Macro {n}"), String::from),
                None => "Deleted macro".into(),
            }
        }
        4 => if a["scroll"] == "down" { "Scroll down".into() } else { "Scroll up".into() },
        5 => "Switch sensitivity".into(),
        _ => "Nothing".into(),
    }
}

/// The jobs of a kind, in words and as the core takes them.
pub fn choices(kind: usize) -> Vec<(String, Value)> {
    match kind {
        0 => MOUSE_JOBS.iter().enumerate().map(|(i, l)| (l.to_string(), json!({ "mouse": i + 1 }))).collect(),
        1 => keys().into_iter().map(|(u, n)| (key_label(u), json!({ "key": n }))).collect(),
        2 => MEDIA.iter().map(|(n, l)| (l.to_string(), json!({ "media": n }))).collect(),
        // Ectomatter's macros, each played once a press unless the Macro Editor says otherwise
        MACRO_KIND => {
            let all = crate::library::all();
            let mut v: Vec<(u32, String)> = all.as_object().map(|m| m.iter().filter_map(|(n, v)| Some((n.parse().ok()?, v["name"].as_str().unwrap_or("").to_string()))).collect()).unwrap_or_default();
            v.sort();
            v.into_iter().map(|(n, name)| (if name.is_empty() { format!("Macro {n}") } else { name }, json!({ "macro": n, "plays": "once" }))).collect()
        }
        4 => vec![("Scroll up".into(), json!({ "scroll": "up" })), ("Scroll down".into(), json!({ "scroll": "down" }))],
        5 => vec![("Switch sensitivity".into(), json!({ "sensitivity": "switch" }))],
        // (other devices' jobs, as the page names them: see Page::choices_for)
        DEVICES_KIND => Vec::new(),
        _ => vec![("Nothing".into(), json!({ "off": true }))],
    }
}

/// A button's job as the mouse is given it: one of Ectomatter's macros the
/// mouse can hold itself (some modifiers and a key, see held_by_mouse) as
/// those keys, so the button plays it with Ectomatter closed; any other macro,
/// or another job Ectomatter does, as the button's own host slot, which the
/// mouse's core listens for. The profile keeps the job itself.
pub fn for_mouse(i: usize, a: &Value) -> Value {
    if let Some(keys) = held_by_mouse(a) {
        return json!({ "shortcut": keys });
    }
    if a.get("macro").is_some() || a.get("ectomatter").is_some() { json!({ "host": i }) } else { a.clone() }
}

/// The keys one of Ectomatter's macros on a button (`a`) presses, if the mouse
/// can hold it in its own memory and play it itself: the macro still kept,
/// played once a press (or while the button is held), and no more than some
/// modifiers and one key pressed together (see chord_of).
pub fn held_by_mouse(a: &Value) -> Option<Vec<String>> {
    let n = a.get("macro")?.as_u64()? as u32;
    if !matches!(a["plays"].as_str(), None | Some("once") | Some("while-held")) {
        return None;
    }
    chord_of(&crate::library::get(n)?["steps"])
}

/// The keys a macro's steps press together, if that's all they do: up to
/// three modifiers and one key pressed, none twice, each let go, no wait in
/// it longer than a hand takes between keys of a shortcut (what a mouse's
/// button keeps, as libratbag writes a one-key macro). The modifiers first,
/// in their order, then the key, by the protocol's names.
pub fn chord_of(steps: &Value) -> Option<Vec<String>> {
    let steps = steps.as_array().filter(|s| !s.is_empty())?;
    let (mut down, mut up): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    for s in steps {
        let k = &s["key"];
        let u = k.as_u64().map(|n| n as u8).or_else(|| k.as_str().and_then(ectomatter_protocol::keys::usage))?;
        if s["after"].as_u64().unwrap_or(0) > 400 {
            return None;
        }
        if s["down"].as_bool()? {
            // (a press after a key was let go, or a key pressed twice, is no shortcut)
            if !up.is_empty() || down.contains(&u) {
                return None;
            }
            down.push(u);
        } else {
            if !down.contains(&u) || up.contains(&u) {
                return None;
            }
            up.push(u);
        }
    }
    if up.len() != down.len() {
        return None;
    }
    let (mut mods, keys): (Vec<u8>, Vec<u8>) = down.into_iter().partition(|u| ectomatter_protocol::keys::is_modifier(*u));
    if keys.len() != 1 || mods.len() > 3 {
        return None;
    }
    mods.sort_unstable();
    mods.into_iter().chain(keys).map(ectomatter_protocol::keys::name).collect()
}

/// Each button that runs Ectomatter's macro `n`, on every profile, given `to`
/// instead, and the profiles kept. Back come the buttons changed on the
/// profile in use, which is the one on the mouse.
pub fn rewrite_macro(store: &mut crate::profiles::Store, n: u32, to: &Value) -> Vec<usize> {
    let (active, mut in_use, mut changed) = (store.active, Vec::new(), false);
    for (pi, p) in store.list.iter_mut().enumerate() {
        for (bi, id) in shape().buttons.iter().map(|b| b.id.as_str()).enumerate() {
            if matches!(ectomatter_protocol::Action::read(&p.body["keys"]["base"][id]), Some(ectomatter_protocol::Action::Macro { n: m, .. }) if m == n) {
                p.body["keys"]["base"][id] = to.clone();
                changed = true;
                if pi == active {
                    in_use.push(bi);
                }
            }
        }
    }
    if changed {
        store.save();
    }
    in_use
}

// ---------------------------------------------------------------- the page

/// The page: which tab, which button and light are chosen, and what the
/// mouse is set to as the core last said.
pub struct Page {
    pub ask: Option<mpsc::Sender<Ask>>,
    pub state: Value,
    pub tab: usize,
    /// the chosen button, if any: none until one is selected, and none again
    /// once it is let go (see Front::let_chosen_go)
    pub button: Option<usize>,
    pub light: usize,
    pub same_wheel: bool,
    /// the kind of job whose menu is open, and each of its rows' job as it
    /// was offered (none for a heading or a note)
    pub menu: Option<usize>,
    pub menu_picks: Vec<Option<Value>>,
    pub failed: Option<String>,
    /// what each light (the logo, the wheel) plays, and from when
    pub plays: [Option<(Plays, Instant)>; 2],
    /// where the picture's layers go once drawn, off the page's thread
    pub tell: Option<Sender<Heard>>,
    /// the mouse's profiles, kept by Ectomatter (the mouse keeps one setup in its
    /// memory: the one in use), once the mouse has been met
    pub profiles: Option<crate::profiles::Store>,
    /// what was last done to the profiles, for the bar, and a profile deleted
    /// with when, which Undo puts back for a few seconds
    pub said: Option<(String, Instant)>,
    pub gone: Option<((usize, crate::profiles::Profile), Instant)>,
    /// the names other devices' jobs are said by, as the pages last said them
    pub names: crate::jobs::Names,
}

/// How long Undo stays in the bar after a profile is deleted.
pub const UNDO_FOR: Duration = Duration::from_secs(6);


/// What a light plays, as the core says the mouse runs it: one color held,
/// or colors at points through a cycle (0 to 255), blended straight from
/// each to the next, `ms` long and round again.
#[derive(Clone, Debug, PartialEq)]
pub struct Plays {
    ms: u64,
    held: bool,
    stops: Vec<(f32, [u8; 3])>,
}

impl Plays {
    pub fn of(v: &Value) -> Option<Plays> {
        let stops: Vec<(f32, [u8; 3])> = v["stops"].as_array()?.iter().filter_map(|s| Some((s["at"].as_f64()? as f32, rgb(&s["color"])?))).collect();
        (!stops.is_empty()).then(|| Plays { ms: v["ms"].as_u64().unwrap_or(1000).max(1), held: v["held"].as_bool().unwrap_or(true), stops })
    }

    pub fn moves(&self) -> bool {
        !self.held && self.stops.len() > 1
    }

    /// The light's color `since` the cycle began.
    pub fn at(&self, since: Duration) -> [u8; 3] {
        let first = self.stops[0].1;
        if !self.moves() {
            return first;
        }
        let pos = (since.as_millis() as u64 % self.ms) as f32 / self.ms as f32 * 255.0;
        for w in self.stops.windows(2) {
            let ((a, ca), (b, cb)) = (w[0], w[1]);
            if pos >= a && pos < b {
                let t = (pos - a) / (b - a);
                return [0, 1, 2].map(|i| (ca[i] as f32 + (cb[i] as f32 - ca[i] as f32) * t).round() as u8);
            }
        }
        self.stops[self.stops.len() - 1].1
    }
}

impl Page {
    pub fn new() -> Page {
        let same_wheel = crate::uistate::get("mouse_same_wheel").and_then(|v| v.as_bool()).unwrap_or(true);
        Page { ask: None, state: Value::Null, tab: 0, button: None, light: 0, same_wheel, menu: None, menu_picks: Vec::new(), failed: None, plays: [None, None], tell: None, profiles: None, said: None, gone: None, names: Default::default() }
    }

    /// What the core says of the mouse: what each light plays is kept with
    /// when it began, unless the light plays something new.
    pub fn heard_state(&mut self, v: Value) {
        // (the mouse as its core described it at the start, what it told since over it)
        let mut v = v;
        if let (Some(told), Some(described)) = (v["device"].as_object_mut(), shape().described.as_object()) {
            for (k, d) in described {
                told.entry(k.clone()).or_insert_with(|| d.clone());
            }
        }
        for (i, l) in shape().lights.iter().map(String::as_str).enumerate() {
            let now = Plays::of(&v["lighting"][l]["plays"]);
            if now.as_ref() != self.plays[i].as_ref().map(|(p, _)| p) {
                self.plays[i] = now.map(|p| (p, Instant::now()));
            }
        }
        self.state = v;
        // the first time the mouse is met: its profiles as kept, or what it is
        // set to now as Profile 1
        if self.profiles.is_none() && self.known() {
            let path = crate::profiles::path_for(key());
            let mut store = crate::profiles::Store::load(path.clone()).unwrap_or_else(|| crate::profiles::Store::first(path, self.body_now()));
            // (a button's job kept by an old id, from when a job named the keyboard or the
            // mouse in place of a device: kept by the device's key now)
            if store.list.iter_mut().fold(false, |renamed, p| crate::jobs::rename_in(&mut p.body) | renamed) {
                store.save();
            }
            self.profiles = Some(store);
        }
    }

    /// Everything the mouse is set to now, as a profile holds it: what the
    /// mouse can't say and Ectomatter hasn't set left out, so putting the
    /// profile on the mouse never writes a guess.
    pub fn body_now(&self) -> Value {
        let lights: serde_json::Map<String, Value> = shape()
            .lights
            .iter()
            .filter(|l| self.light_known(l))
            .map(|l| (l.to_string(), json!({ "effect": self.state["lighting"][l]["effect"], "params": self.state["lighting"][l]["params"] })))
            .collect();
        let (v, known) = (&self.state["settings"]["values"], &self.state["settings"]["known"]);
        // (each setting the core describes)
        let settings: serde_json::Map<String, Value> = shape().settings.iter().filter(|id| known[id.as_str()].as_bool() != Some(false)).map(|id| (id.clone(), v[id.as_str()].clone())).collect();
        let mut body = json!({ "lighting": lights, "settings": settings });
        if self.buttons_known() {
            body["keys"] = json!({ "base": self.state["keys"]["layers"]["base"] });
        }
        body
    }

    /// Whether Ectomatter knows what the mouse's buttons do. The mouse can't
    /// say, so not until Ectomatter has set them.
    pub fn buttons_known(&self) -> bool {
        self.state["keys"]["known"].as_bool() != Some(false)
    }

    /// Whether a button's job is known: Ectomatter has set the mouse's
    /// buttons, or the profile in use keeps a job for it.
    pub fn job_known(&self, i: usize) -> bool {
        self.buttons_known() || self.profiles.as_ref().is_some_and(|p| !p.current().body["keys"]["base"][shape().buttons[i].id.as_str()].is_null())
    }

    /// Whether a light's effect is known. The mouse says only its color, so
    /// not until Ectomatter has set it.
    pub fn light_known(&self, id: &str) -> bool {
        let k = &self.state["lighting"]["known"];
        let k = if k.is_object() { &k[id] } else { k };
        k.is_null() || k.as_bool() == Some(true)
    }

    /// A profile put on the mouse: shown as it is at once, and sent whole (a
    /// macro's button as its host slot, see for_mouse).
    fn put_on(&mut self, body: &Value) {
        let mut body = body.clone();
        if let Some(base) = body["keys"]["base"].as_object_mut() {
            for (id, a) in base.iter_mut() {
                if let Some(i) = shape().buttons.iter().position(|b| b.id == *id) {
                    *a = for_mouse(i, a);
                }
            }
        }
        let body = &body;
        if let Some(base) = body["keys"]["base"].as_object() {
            for (id, a) in base {
                self.state["keys"]["layers"]["base"][id] = a.clone();
            }
        }
        for l in shape().lights.iter().map(String::as_str) {
            if body["lighting"][l].is_object() {
                self.state["lighting"][l]["effect"] = body["lighting"][l]["effect"].clone();
                self.state["lighting"][l]["params"] = body["lighting"][l]["params"].clone();
            }
        }
        if let Some(set) = body["settings"].as_object() {
            for (id, v) in set {
                self.state["settings"]["values"][id] = v.clone();
            }
        }
        self.send(Ask::Apply(body.clone()));
    }

    /// The profile in use changed as the mouse just was.
    fn keep_in_profile(&mut self, f: impl FnOnce(&mut Value)) {
        if let Some(p) = &mut self.profiles {
            p.change(f);
        }
    }

    /// Choose a profile: all of it on the mouse at once.
    pub fn choose_profile(&mut self, i: usize) {
        let Some(p) = &mut self.profiles else { return };
        if i == p.active || i >= p.list.len() {
            return;
        }
        p.choose(i);
        let (name, body) = (p.current().name.clone(), p.current().body.clone());
        self.put_on(&body);
        self.say(format!("{name} is now on the mouse."));
    }

    /// A new profile, as SteelSeries GG ships the mouse, in use from now.
    pub fn new_profile(&mut self) {
        let factory = self.state["factory"].clone();
        let Some(p) = &mut self.profiles else { return };
        if !factory.is_object() {
            return;
        }
        let name = p.new_name();
        p.add(crate::profiles::Profile::new(name.clone(), factory.clone()));
        self.put_on(&factory);
        self.say(format!("{name} is now on the mouse."));
    }

    /// A copy of the profile in use, in use from now (the mouse already shows it).
    pub fn duplicate_profile(&mut self) {
        if let Some(p) = &mut self.profiles {
            p.duplicate();
        }
    }

    /// The profile in use deleted, at once: the one above it goes on the mouse,
    /// and Undo puts it back for a few seconds.
    pub fn delete_profile(&mut self) {
        let Some(p) = &mut self.profiles else { return };
        let Some(gone) = p.delete() else { return };
        let body = p.current().body.clone();
        let name = gone.1.name.clone();
        self.gone = Some((gone, Instant::now()));
        self.put_on(&body);
        self.say(format!("Deleted {name}."));
    }

    /// Undo: the profile deleted put back, and on the mouse again.
    pub fn undo(&mut self) {
        let Some((gone, at)) = self.gone.take() else { return };
        if at.elapsed() > UNDO_FOR {
            return;
        }
        let Some(p) = &mut self.profiles else { return };
        let name = gone.1.name.clone();
        p.restore(gone);
        let body = p.current().body.clone();
        self.put_on(&body);
        self.say(format!("{name} is back on the mouse."));
    }

    pub fn rename_profile(&mut self, name: &str) {
        if let Some(p) = &mut self.profiles {
            let i = p.active;
            p.rename(i, name);
        }
    }

    /// Whether Undo shows in the bar now.
    pub fn undo_shown(&self) -> bool {
        self.gone.as_ref().is_some_and(|(_, at)| at.elapsed() <= UNDO_FOR)
    }

    fn say(&mut self, what: String) {
        self.said = Some((what, Instant::now()));
    }

    /// What the bar says: the last thing done to the profiles, for a few seconds.
    pub fn said_now(&self) -> String {
        self.said.as_ref().filter(|(_, at)| at.elapsed() <= UNDO_FOR).map_or(String::new(), |(s, _)| s.clone())
    }

    /// The profiles as the Profiles box lists them: each one's name and its
    /// logo's effect, the one in use chosen and marked.
    pub fn profile_items(&self) -> Vec<crate::ProfileItem> {
        let Some(p) = &self.profiles else { return Vec::new() };
        p.list
            .iter()
            .enumerate()
            .map(|(i, pr)| crate::ProfileItem {
                n: i as i32,
                name: pr.name.as_str().into(),
                // (one that doesn't set the logo leaves it as it is when it's put on)
                sub: pr.body["lighting"]["logo"]["effect"].as_str().map_or("Logo left as it is".to_string(), |e| self.effect_label(e)).into(),
                active: i == p.active,
                current: i == p.active,
                ..Default::default()
            })
            .collect()
    }

    /// A light's effect taken by the mouse at `at`: the light plays it from then.
    pub fn played(&mut self, light: &str, v: &Value, at: Instant) {
        if let (Some(i), Some(p)) = (shape().lights.iter().position(|l| *l == light), Plays::of(v)) {
            self.plays[i] = Some((p, at));
        }
    }

    /// A light's color at `now`, as the mouse shows it; black for off.
    pub fn color_at(&self, i: usize, now: Instant) -> [u8; 3] {
        match &self.plays[i] {
            Some((p, from)) => p.at(now.saturating_duration_since(*from)),
            None => shape().lights.get(i).and_then(|l| self.lit(l)).unwrap_or([0; 3]),
        }
    }

    /// Whether a light's effect moves, so the picture is drawn again as it plays.
    pub fn moving(&self) -> bool {
        self.plays.iter().flatten().any(|(p, _)| p.moves())
    }

    /// Whether Ectomatter has met the mouse: it is plugged in, or the core
    /// keeps a record of it from before (an unplugged mouse stays, marked as
    /// not connected).
    pub fn known(&self) -> bool {
        self.present() || !self.state["keys"]["layers"]["base"]["left"].is_null()
    }

    pub fn present(&self) -> bool {
        self.state["present"].as_bool().unwrap_or(false)
    }

    pub fn send(&self, a: Ask) {
        if let Some(t) = &self.ask {
            let _ = t.send(a);
        }
    }

    /// The jobs of a kind for a button, in words and as the profile keeps
    /// them: another device's, as the pages last named them, or the kind's own.
    pub fn choices_for(&self, kind: usize) -> Vec<(String, Value)> {
        if kind == DEVICES_KIND {
            return self.names.for_device(crate::jobs::mouse()).into_iter().map(|(w, j)| (w, j.write())).collect();
        }
        choices(kind)
    }

    /// A button's job: as the profile in use keeps it (a macro is Ectomatter's
    /// to play, which the mouse knows only as a host slot), or as the mouse says.
    pub fn job(&self, i: usize) -> Value {
        let kept = self.profiles.as_ref().map(|p| p.current().body["keys"]["base"][shape().buttons[i].id.as_str()].clone()).filter(|a| !a.is_null());
        kept.unwrap_or_else(|| self.state["keys"]["layers"]["base"][shape().buttons[i].id.as_str()].clone())
    }

    pub fn factory(&self, i: usize) -> Value {
        self.state["keys"]["factory"]["base"][shape().buttons[i].id.as_str()].clone()
    }

    pub fn rebound(&self, i: usize) -> bool {
        let (j, f) = (self.job(i), self.factory(i));
        self.job_known(i) && !f.is_null() && j != f
    }

    /// A button's job in words: "Not known" while it isn't.
    fn job_words(&self, i: usize) -> String {
        if self.job_known(i) {
            job_label_with(&self.job(i), &self.names)
        } else {
            "Not known".into()
        }
    }

    pub fn light_name(&self) -> &'static str {
        if self.light == 0 { "logo" } else { "wheel" }
    }

    /// The lights in the order the core lists them (as they are on the
    /// mouse, front to back), each by its place among Shape's lights, its id and its
    /// name; in that order where the core says nothing of them.
    pub fn light_order(&self) -> Vec<(usize, &'static str, String)> {
        let listed = self.state["device"]["capabilities"]["lighting"]["lights"].as_array().cloned().unwrap_or_default();
        let mut out: Vec<(usize, &'static str, String)> = listed
            .iter()
            .filter_map(|l| {
                let i = shape().lights.iter().position(|id| l["id"] == id.as_str())?;
                Some((i, shape().lights[i].as_str(), l["label"].as_str().unwrap_or(&shape().lights[i]).to_string()))
            })
            .collect();
        for (i, id) in shape().lights.iter().map(String::as_str).enumerate() {
            if !out.iter().any(|(j, ..)| *j == i) {
                let mut c = id.chars();
                let name = c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str());
                out.push((i, id, name));
            }
        }
        out
    }

    pub fn light_of(&self, l: &str) -> Value {
        self.state["lighting"][l].clone()
    }

    /// A light's color as the picture shows it: none while it is off.
    pub fn lit(&self, l: &str) -> Option<[u8; 3]> {
        let v = self.light_of(l);
        match v["effect"].as_str() {
            Some("off") => None,
            Some("colorshift") => Some(crate::described::value_of(&v["params"], "colors", &self.param_desc(&v, "colors")).get(0).and_then(rgb).unwrap_or([0xff, 0, 0])),
            _ => Some(rgb(crate::described::value_of(&v["params"], "color", &self.param_desc(&v, "color"))).unwrap_or([0xff, 0x10, 0x00])),
        }
    }

    /// A light set: and the wheel the same, from the logo, while it copies it.
    pub fn set_light(&mut self, l: Value) {
        let name = self.light_name();
        self.state["lighting"][name] = l.clone();
        self.light_now_known(name);
        self.send(Ask::Light(name.into(), l.clone()));
        let kept = json!({ "effect": l["effect"], "params": l["params"] });
        let wheel_too = name == "logo" && self.same_wheel;
        self.keep_in_profile(|b| {
            b["lighting"][name] = kept.clone();
            if wheel_too {
                b["lighting"]["wheel"] = kept.clone();
            }
        });
        if wheel_too {
            self.state["lighting"]["wheel"] = l.clone();
            self.light_now_known("wheel");
            self.send(Ask::Light("wheel".into(), l));
        }
    }

    /// A light set by Ectomatter: its effect known from now, as the core will say.
    fn light_now_known(&mut self, id: &str) {
        let k = &mut self.state["lighting"]["known"];
        if !k.is_object() {
            let was = k.clone();
            *k = json!({ "logo": was, "wheel": was });
        }
        k[id] = json!(true);
    }

    /// The wheel set as the logo is (Same on the wheel), on the mouse and in the profile.
    pub fn copy_logo_to_wheel(&mut self) {
        let l = self.light_of("logo");
        self.state["lighting"]["wheel"] = l.clone();
        self.light_now_known("wheel");
        let kept = json!({ "effect": l["effect"], "params": l["params"] });
        self.keep_in_profile(|b| b["lighting"]["wheel"] = kept);
        self.send(Ask::Light("wheel".into(), l));
    }

    /// A button given back the job it had from the factory.
    pub fn reset_job(&mut self, i: usize) {
        let f = self.factory(i);
        if !f.is_null() {
            self.knowing_buttons();
            self.state["keys"]["layers"]["base"][shape().buttons[i].id.as_str()] = f.clone();
            self.keep_in_profile(|b| b["keys"]["base"][shape().buttons[i].id.as_str()] = f);
            self.send(Ask::ResetKey(shape().buttons[i].id.clone()));
        }
    }

    pub fn set_job(&mut self, i: usize, a: Value) {
        self.knowing_buttons();
        let on_mouse = for_mouse(i, &a);
        self.state["keys"]["layers"]["base"][shape().buttons[i].id.as_str()] = on_mouse.clone();
        self.keep_in_profile(|b| b["keys"]["base"][shape().buttons[i].id.as_str()] = a);
        self.send(Ask::Key(shape().buttons[i].id.clone(), on_mouse));
    }

    /// Before a button is set while the mouse's buttons aren't known: the
    /// mouse takes all eight at once, so the others go on it as the core
    /// keeps them (their factory jobs, on a mouse Ectomatter hasn't set), and
    /// are known, and kept in the profile in use, from then.
    fn knowing_buttons(&mut self) {
        if self.buttons_known() {
            return;
        }
        let base = self.state["keys"]["layers"]["base"].clone();
        let kept: Vec<(usize, Value)> = (0..shape().buttons.len()).filter(|&j| !self.job_known(j)).map(|j| (j, base[shape().buttons[j].id.as_str()].clone())).collect();
        self.keep_in_profile(|b| {
            for (j, a) in kept {
                b["keys"]["base"][shape().buttons[j].id.as_str()] = a;
            }
        });
        self.state["keys"]["known"] = json!(true);
    }

    /// The lights in streamed colors (logo and wheel, "#RRGGBB"), which the
    /// mouse's memory never keeps.
    pub fn direct(&self, colors: Value) {
        self.send(Ask::Direct(colors));
    }

    /// The lights as the mouse keeps them again, after streamed colors.
    pub fn hand_back(&self) {
        self.send(Ask::HandBack);
    }

    pub fn set_setting(&mut self, id: &str, v: Value) {
        self.state["settings"]["values"][id] = v.clone();
        self.keep_in_profile(|b| b["settings"][id] = v.clone());
        self.send(Ask::Setting(id.into(), v));
    }

    // ------------------------------------------------------------ what the core describes

    /// The effects the core offers the lights, as it describes them.
    pub fn effects(&self) -> Vec<Value> {
        self.state["device"]["capabilities"]["lighting"]["effects"].as_array().cloned().unwrap_or_default()
    }

    /// One effect, as the core describes it.
    pub fn effect_desc(&self, e: &str) -> Value {
        self.effects().into_iter().find(|d| d["id"] == e).unwrap_or_else(|| json!({ "id": e, "label": e }))
    }

    pub fn effect_label(&self, e: &str) -> String {
        self.effect_desc(e)["label"].as_str().unwrap_or(e).to_string()
    }

    /// A setting of the effect a light plays, as the core describes it.
    fn param_desc(&self, l: &Value, id: &str) -> Value {
        self.effect_desc(l["effect"].as_str().unwrap_or("static"))["params"][id].clone()
    }

    /// A setting the page drew from the core's description, by its piece: one
    /// of the chosen light's effect's, or one of the mouse's own; its id,
    /// description and value now.
    pub fn described(&self, piece: crate::described::Piece) -> Option<(String, Value, Value)> {
        match piece {
            crate::described::Piece::Effect(k) => {
                let l = self.light_of(self.light_name());
                let e = self.effect_desc(l["effect"].as_str().unwrap_or("static"));
                let (id, d) = crate::described::params(&e).get(k).cloned()?;
                let v = crate::described::value_of(&l["params"], &id, &d).clone();
                Some((id, d, v))
            }
            crate::described::Piece::Setting(k) => {
                let (id, d) = crate::described::setting(&self.state["device"], k)?;
                let v = crate::described::value_of(&self.state["settings"]["values"], &id, &d).clone();
                Some((id, d, v))
            }
        }
    }

    /// One of those settings given a new value: on the chosen light, or the mouse.
    pub fn set_described(&mut self, piece: crate::described::Piece, v: Value) {
        let Some((id, ..)) = self.described(piece) else { return };
        match piece {
            crate::described::Piece::Effect(_) => {
                let mut l = self.light_of(self.light_name());
                if !l["params"].is_object() {
                    l["params"] = json!({});
                }
                l["params"][&id] = v;
                self.set_light(l);
            }
            crate::described::Piece::Setting(_) => self.set_setting(&id, v),
        }
    }

    pub fn hint(&self) -> &'static str {
        match self.tab {
            0 if self.button.is_some() => "Escape clears the choice.",
            0 => "Select a button on the mouse or in the list, then choose its new job.",
            1 => "Choose a light, then its effect and its color.",
            2 => "Set the two sensitivities the CPI button switches between, and how often the mouse reports.",
            _ => "Check the mouse’s connection, its firmware and its model.",
        }
    }

    // ------------------------------------------------------------ Buttons

    /// The list beside the picture: on Buttons each button and its job, on
    /// Lighting each light, its effect and its color.
    pub fn rows(&self) -> Vec<MouseRow> {
        if self.tab == 1 {
            return self
                .light_order()
                .into_iter()
                .map(|(i, id, name)| {
                    let l = self.light_of(id);
                    let lit = self.lit(id);
                    let effect = if self.light_known(id) { self.effect_label(l["effect"].as_str().unwrap_or("static")) } else { "Not known".into() };
                    MouseRow { name: name.into(), job: effect.into(), chosen: i == self.light, rebound: false, swatch: color(lit.unwrap_or([0x2a, 0x2a, 0x30])), swatched: true }
                })
                .collect();
        }
        shape()
            .buttons
            .iter()
            .enumerate()
            .map(|(i, b)| MouseRow { name: b.label.as_str().into(), job: self.job_words(i).into(), chosen: Some(i) == self.button, rebound: self.rebound(i), swatch: color([0; 3]), swatched: false })
            .collect()
    }

    /// The buttons as the Key library lists them on Buttons, in the groups
    /// the core gives them (`group` on each control; one group, Buttons,
    /// where it gives none), each group in the order its first button comes:
    /// each button's place, its short name for the cap (`short`), its name,
    /// its job and whether that is a new one. Only those whose name or job
    /// has what is typed in it (`query`, already lowercased), where something is.
    pub fn lib_list(&self, query: &str) -> Vec<(String, usize, Vec<(usize, String, String, String, bool)>)> {
        let controls = self.state["device"]["controls"].as_array();
        let mut groups: Vec<(String, usize, Vec<(usize, String, String, String, bool)>)> = Vec::new();
        for (i, b) in shape().buttons.iter().enumerate() {
            let (id, name, short) = (b.id.as_str(), b.label.as_str(), b.short.as_str());
            let c = controls.and_then(|cs| cs.iter().find(|c| c["id"].as_str() == Some(id)));
            let field = |k: &str| c.and_then(|c| c[k].as_str()).filter(|v| !v.is_empty());
            let group = field("group").unwrap_or("Buttons").to_string();
            let (cap, name) = (field("short").unwrap_or(short).to_string(), field("label").unwrap_or(name).to_string());
            let job = self.job_words(i);
            let at = match groups.iter().position(|(g, ..)| *g == group) {
                Some(at) => at,
                None => {
                    groups.push((group, 0, Vec::new()));
                    groups.len() - 1
                }
            };
            groups[at].1 += 1;
            if query.is_empty() || name.to_lowercase().contains(query) || job.to_lowercase().contains(query) {
                groups[at].2.push((i, cap, name, job, self.rebound(i)));
            }
        }
        groups
    }

    /// Each button's words on the picture, its line and its dot.
    pub fn calls(&self) -> Vec<MouseCall> {
        shape()
            .buttons
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (left, top) = (b.callout.0, b.callout.1);
                MouseCall { name: b.label.as_str().into(), job: self.job_words(i).into(), left, top, chosen: Some(i) == self.button, rebound: self.rebound(i) }
            })
            .collect()
    }

    /// The callouts' lines and dots, as path commands: the others', and the chosen button's.
    pub fn lines(&self) -> (String, String, String, String) {
        let (mut lines, mut dots, mut on_line, mut on_dot) = (String::new(), String::new(), String::new(), String::new());
        for (i, pts) in shape().buttons.iter().map(|b| &b.callout.2).enumerate().filter(|(_, p)| !p.is_empty()) {
            let (l, d) = if Some(i) == self.button { (&mut on_line, &mut on_dot) } else { (&mut lines, &mut dots) };
            for (k, (x, y)) in pts.iter().enumerate() {
                l.push_str(&format!("{}{x} {y} ", if k == 0 { "M" } else { "L" }));
            }
            let (x, y) = pts[pts.len() - 1];
            d.push_str(&format!("M{} {y} a2.5 2.5 0 1 0 5 0 a2.5 2.5 0 1 0 -5 0 ", x - 2.5));
        }
        (lines, dots, on_line, on_dot)
    }

    /// The Key detail box for the chosen button: its cap, its factory job,
    /// what it does now, and its kind.
    pub fn detail(&self, i: usize) -> (String, String, String, String, bool, usize, String) {
        let (job, factory) = (self.job(i), self.factory(i));
        let rebound = self.rebound(i);
        let state = if !self.job_known(i) {
            "Not known".into()
        } else if rebound {
            format!("Now {}", job_label_with(&job, &self.names))
        } else {
            "As it came from the factory".into()
        };
        // (a macro, or another device's job, is Ectomatter's to do: the mouse keeps only a sign for it)
        let note = if !self.job_known(i) {
            "The mouse can’t tell Ectomatter what this button does. Giving it a job sets all eight buttons, and the others get their factory jobs.".into()
        } else if let Some(crate::jobs::Job::Scene(crate::jobs::SceneTo::Id(id))) = crate::jobs::Job::read(&job).filter(|_| kind_of(&job) == DEVICES_KIND) {
            if self.names.scenes.iter().any(|(k, _)| *k == id) {
                "Ectomatter does this while it’s running. With Ectomatter closed, this button does nothing.".into()
            } else {
                "The scene this button turned on was deleted, so the button does nothing until you give it another job.".into()
            }
        } else if kind_of(&job) == DEVICES_KIND {
            "Ectomatter does this while it’s running. With Ectomatter closed, this button does nothing.".into()
        } else if kind_of(&job) == MACRO_KIND {
            if held_by_mouse(&job).is_some() {
                "The mouse plays this macro itself, so the button works with Ectomatter closed.".into()
            } else if crate::library::get(job["macro"].as_u64().unwrap_or(0) as u32).is_some() {
                "Ectomatter plays this macro while it’s running. With Ectomatter closed, this button does nothing.".into()
            } else {
                "The macro this button ran was deleted. Choose another job for it.".into()
            }
        } else {
            String::new()
        };
        (shape().buttons[i].short.as_str().into(), shape().buttons[i].label.as_str().into(), format!("From the factory: {}", job_label(&factory)), state, rebound, kind_of(&job), note)
    }

    /// With no button chosen, the box for every button: what it says, and
    /// whether any button does something other than the factory's job.
    pub fn all_buttons(&self) -> (String, bool) {
        // (Back to factory sets them all, as giving one a job would)
        if !self.buttons_known() {
            return ("The mouse can’t tell Ectomatter what its buttons do. Giving one a job sets all eight, and the others get their factory jobs.".into(), true);
        }
        let n = (0..shape().buttons.len()).filter(|&i| self.rebound(i)).count();
        let note = match n {
            0 => "Every button does what it did from the factory.".to_string(),
            1 => "One button has a new job, shown in amber. Back to factory gives it its old one.".to_string(),
            n => format!("{n} buttons have new jobs, shown in amber. Back to factory gives every one its old job."),
        };
        (note, n > 0)
    }

    // ------------------------------------------------------------ Lighting, Performance, Settings

    /// The chosen light's effect, by its place in the core's list.
    pub fn effect(&self) -> usize {
        let e = self.light_of(self.light_name())["effect"].as_str().unwrap_or("static").to_string();
        self.effects().iter().position(|d| d["id"] == e.as_str()).unwrap_or(1)
    }

    /// The boxes under the picture as cells: on Lighting, the chosen effect's
    /// settings; on Performance, the sensitivities and the polling rate; on
    /// Settings, the card. `w` is the room across.
    /// The tab's pieces laid out here as a form (Settings): its cells, and how tall they are.
    pub fn cells(&self, w: f32) -> (Vec<Cell>, f32) {
        match self.tab {
            3 => self.settings_cells(w),
            _ => (Vec::new(), 0.0),
        }
    }

    /// The tab's boxes laid out here, for the page's row of boxes: on Lighting the
    /// chosen light's effect's settings, on Performance a box for each of the
    /// sections the core's layout gives the mouse's settings; each laid out to
    /// the width the row made it (`width(i, least)`); `open` is the color the
    /// picker is open on.
    pub fn boxes(&self, width: impl Fn(usize, f32) -> f32, open: Option<(usize, usize)>) -> Vec<crate::boxes::Laid> {
        match self.tab {
            1 => vec![self.light_box(width(0, crate::tokens::size::SETTINGS), open)],
            2 => self.performance_boxes(width),
            _ => Vec::new(),
        }
    }

    /// The chosen light's effect's settings, drawn from the core's
    /// description of the effect; `open` is the color the picker is open on.
    fn light_box(&self, w: f32, open: Option<(usize, usize)>) -> crate::boxes::Laid {
        let laid = |s: Stack, title: &str| crate::boxes::Laid { title: title.into(), least: crate::tokens::size::SETTINGS, h: s.height(), cells: s.cells };
        let l = self.light_of(self.light_name());
        if !self.light_known(self.light_name()) {
            let mut s = Stack::new(w);
            s.note("The mouse can’t tell Ectomatter this light’s effect, only its color. Choose an effect to set it.");
            return laid(s, "Not known");
        }
        let e = self.effect_desc(l["effect"].as_str().unwrap_or("static"));
        let mut s = Stack::new(w);
        crate::described::draw_effect(&mut s, &e, &l["params"], open);
        if self.light == 0 {
            s.next();
            s.check(Ctl::MouseSameWheel, self.same_wheel, "Same on the wheel");
            s.close();
            s.check_note("The wheel copies the logo’s effect and colors.");
        }
        laid(s, e["label"].as_str().unwrap_or(""))
    }

    /// The mouse's own settings, a box for each of the sections the core's
    /// layout gives them, drawn from its description; the page's row of boxes
    /// lays them out side by side as it does any boxes.
    fn performance_boxes(&self, width: impl Fn(usize, f32) -> f32) -> Vec<crate::boxes::Laid> {
        let desc = &self.state["device"];
        let (values, known) = (&self.state["settings"]["values"], &self.state["settings"]["known"]);
        let secs = crate::described::sections(desc, values);
        let least = crate::tokens::size::SETTINGS;
        let mut out = Vec::new();
        for (k, sec) in secs.iter().enumerate() {
            let mut s = Stack::new(width(k, least));
            crate::described::draw_section(&mut s, desc, sec, values, known);
            // (the device's own word on which of its presets was in use: the setting its core
            // numbers as that preset, in a section that shows one)
            let active = self.state["settings"]["active"].as_u64();
            let used = sec.items.iter().filter_map(|&k| crate::described::setting(desc, k)).find(|(_, s)| active.is_some() && s["preset"].as_u64() == active);
            if let Some((_, item)) = used {
                let kind = desc["kind"].as_str().unwrap_or("device");
                s.close();
                let name = item["short"].as_str().or(item["label"].as_str()).unwrap_or("").to_lowercase();
                s.note(&format!("When Ectomatter last asked, the {kind} was using the {name}."));
            }
            out.push(crate::boxes::Laid { title: sec.title.clone(), least, h: s.height(), cells: s.cells });
        }
        out
    }

    /// The mouse's own facts, a form (src/form.rs) `w` across: how it's connected,
    /// its firmware, where its settings are kept, its model.
    fn settings_cells(&self, w: f32) -> (Vec<Cell>, f32) {
        let mut f = crate::form::Form::new(0.0, 0.0, w);
        f.section("Mouse", "", |k| {
            let present = self.present();
            k.status_row(if present { 1 } else { 2 }, if present { "Connected by USB." } else { "Not connected. Plug the mouse in to change it; what it was set to stays here." });
            let d = &self.state["device"];
            let maker = d["vendor"].as_str().or(shape().described["vendor"].as_str()).unwrap_or_default();
            let firmware = d["firmware"]["version"].as_str().map_or("Not known yet".to_string(), |v| format!("{maker} {v}").trim().to_string());
            k.fact("Firmware", &firmware, "");
            k.fact("Settings kept", "Your profiles are kept in Ectomatter. The one in use is also on the mouse, so the mouse keeps working that way without Ectomatter. A change made to the mouse in another app won’t show here.", "");
            // (the mouse as its core names it, and the USB device its manifest matches)
            let name = d["name"].as_str().or(shape().described["name"].as_str()).unwrap_or_default();
            let model = match crate::core_host::Manifest::read(&shape().folder).ok().and_then(|m| m.usb_ids()) {
                Some((vendor, product)) => format!("{name}, USB {vendor:04x}:{product:04x}"),
                None => name.to_string(),
            };
            k.fact("Model", &model, "");
        });
        f.end()
    }
}

/// How the mouse's lights are made, as its core's light says (see
/// ectomatter_draw's lamp): the file in the core's folder its description
/// names, or the light the description carries itself; nothing, for a core
/// that says nothing of it.
pub fn light_model(desc: &Value) -> Value {
    match &desc["light"] {
        Value::String(file) => std::fs::read_to_string(crate::cores::tuning_read(&shape().core, &shape().folder.join(file))).ok().and_then(|t| ectomatter_draw::jsonc::parse(&t).ok()).unwrap_or(Value::Null),
        v => v.clone(),
    }
}

/// The tile an effect's picture borrows from the keyboard's effects, and how
/// it moves (1 a breath, 3 a wave): its own id's, or that of the one its
/// description says it looks like (`like`); an effect the app has no
/// picture for shows Static's.
pub fn tile_of(effect: &Value) -> (&'static str, i32) {
    match effect["like"].as_str().or(effect["id"].as_str()).unwrap_or("") {
        "off" => ("off", 0),
        "breathing" => ("breathing", 1),
        "wave" => ("wave", 3),
        _ => ("static", 0),
    }
}

pub fn rgb(v: &Value) -> Option<[u8; 3]> {
    let s = v.as_str()?.trim_start_matches('#');
    let n = u32::from_str_radix(s, 16).ok().filter(|_| s.len() == 6)?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of Ectomatter's macros that is only a shortcut (some modifiers and a
    /// key pressed together) goes on a button as those keys, for the mouse to
    /// play itself with Ectomatter closed; anything more, or played another way,
    /// stays Ectomatter's to play (the button's host slot). Grant's own: New
    /// tab, Previous tab.
    #[test]
    fn the_mouse_holds_a_macro_that_is_a_shortcut() {
        let _one = crate::library::TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let step = |k: &str, down: bool, after: u64| json!({ "key": k, "down": down, "after": after });
        crate::library::set(91, "New tab", vec![step("LeftCtrl", true, 0), step("T", true, 0), step("T", false, 10), step("LeftCtrl", false, 0)]);
        crate::library::set(92, "Previous tab", vec![step("LeftCtrl", true, 0), step("LeftShift", true, 0), step("Tab", true, 0), step("Tab", false, 10), step("LeftShift", false, 0), step("LeftCtrl", false, 0)]);
        crate::library::set(93, "Hi", vec![step("H", true, 0), step("H", false, 0), step("I", true, 0), step("I", false, 0)]);
        let m = |n: u32, plays: &str| json!({ "macro": n, "plays": plays });
        assert_eq!(for_mouse(4, &m(91, "once")), json!({ "shortcut": ["LeftCtrl", "T"] }));
        assert_eq!(for_mouse(3, &m(92, "once")), json!({ "shortcut": ["LeftCtrl", "LeftShift", "Tab"] }));
        assert_eq!(for_mouse(3, &m(93, "once")), json!({ "host": 3 }), "two keys typed");
        assert_eq!(for_mouse(4, &m(91, "until-pressed-again")), json!({ "host": 4 }), "played another way");
        assert_eq!(for_mouse(4, &m(99, "once")), json!({ "host": 4 }), "a macro no longer kept");
        assert_eq!(for_mouse(5, &json!({ "key": "F5" })), json!({ "key": "F5" }), "a key is a key");
        // a hand's own recording of Ctrl+W, with the pauses between its keys
        let rec = vec![step("LeftCtrl", true, 140), step("W", true, 90), step("W", false, 120), step("LeftCtrl", false, 0)];
        assert_eq!(chord_of(&Value::Array(rec)), Some(vec!["LeftCtrl".to_string(), "W".to_string()]));
        // a key pressed after another was let go, a long pause, four modifiers, a key never let go: none is one
        assert_eq!(chord_of(&json!([step("LeftCtrl", true, 0), step("A", true, 0), step("A", false, 0), step("B", true, 0), step("B", false, 0), step("LeftCtrl", false, 0)])), None);
        assert_eq!(chord_of(&json!([step("LeftCtrl", true, 2000), step("A", true, 0), step("A", false, 0), step("LeftCtrl", false, 0)])), None);
        assert_eq!(chord_of(&json!([step("LeftCtrl", true, 0), step("LeftShift", true, 0), step("LeftAlt", true, 0), step("LeftMeta", true, 0), step("A", true, 0), step("A", false, 0), step("LeftMeta", false, 0), step("LeftAlt", false, 0), step("LeftShift", false, 0), step("LeftCtrl", false, 0)])), None);
        assert_eq!(chord_of(&json!([step("LeftCtrl", true, 0), step("A", true, 0), step("A", false, 0)])), None);
        for n in [91, 92, 93] {
            crate::library::delete(n);
        }
    }

    /// Every job a button can be given reads back as itself, in the core's terms and in words.
    #[test]
    fn every_job_reads_back_as_itself() {
        let _one = crate::library::TESTS.lock().unwrap_or_else(|p| p.into_inner());
        for k in 0..KINDS.len() {
            for (label, a) in choices(k) {
                assert_eq!(kind_of(&a), k, "{a}");
                assert_eq!(job_label(&a), label, "{a}");
            }
        }
        assert_eq!(job_label(&json!({ "key": "PageDown" })), "Page Down");
        assert_eq!(job_label(&json!({ "key": 104 })), "F13");
    }

    /// The buttons as the Key library lists them: in the groups the core gives
    /// them, each group where its first button comes, with its short name for
    /// the cap; one group, Buttons, for a core that gives none; and only those
    /// whose name or job has what is typed, each group still counting all of
    /// its buttons.
    #[test]
    fn the_buttons_are_listed_in_the_cores_groups() {
        let mut p = Page::new();
        let base: serde_json::Map<String, Value> = shape().buttons.iter().map(|b| (b.id.clone(), json!({ "mouse": 1 }))).collect();
        let mut keys = json!({ "layers": { "base": base }, "factory": { "base": base }, "known": true });
        keys["layers"]["base"]["right-side-back"] = json!({ "key": "F5" });
        let groups = ["Top", "Top", "Top", "Side", "Side", "Side", "Side", "Top"];
        let controls: Vec<Value> = shape().buttons.iter().zip(groups).map(|(b, g)| json!({ "id": b.id, "label": b.label, "short": format!("{}!", &b.id[..2]), "group": g })).collect();
        p.state = json!({ "present": true, "device": { "controls": controls }, "keys": keys });
        let list = p.lib_list("");
        let names: Vec<(&str, usize, Vec<usize>)> = list.iter().map(|(g, n, rows)| (g.as_str(), *n, rows.iter().map(|r| r.0).collect())).collect();
        assert_eq!(names, [("Top", 4, vec![0, 1, 2, 7]), ("Side", 4, vec![3, 4, 5, 6])]);
        assert_eq!((list[0].2[0].1.as_str(), list[0].2[0].2.as_str()), ("le!", "Left button"), "the core's short name on the cap");
        assert!(list[1].2[2].4 && list[1].2[2].3 == "F5", "a new job, as the mouse has it");
        // what is typed: only the button it finds, its group still counting four
        let found = p.lib_list("f5");
        assert_eq!(found.iter().map(|(g, n, rows)| (g.as_str(), *n, rows.len())).collect::<Vec<_>>(), [("Top", 4, 0), ("Side", 4, 1)]);
        // a core that gives no groups
        p.state["device"]["controls"] = json!(shape().buttons.iter().map(|b| json!({ "id": b.id, "label": b.label })).collect::<Vec<_>>());
        let one = p.lib_list("");
        assert_eq!(one.iter().map(|(g, n, _)| (g.as_str(), *n)).collect::<Vec<_>>(), [("Buttons", 8)]);
        assert_eq!(one[0].2[7].1, "CPI", "the page's own short name where the core gives none");
    }

    /// A mouse that can't say what it's set to: a profile made from it
    /// leaves out what isn't known, so putting the profile on never writes a
    /// guess; its buttons, and the effect it can't say, show as not known;
    /// and a job given to one button sets all eight, so the others are kept
    /// in the profile, as the mouse has them from then.
    /// docs/core-protocol.md, section 4: a mouse Ectomatter has never seen
    /// gets its page from its core's description alone: its buttons, their
    /// surfaces and callouts; its lights in the order their colors stream,
    /// each one's pieces laid under the buttons first, then over everything;
    /// its glow; its view and its settings.
    #[test]
    fn a_mouse_of_its_own_gets_its_page_from_its_description() {
        let d = json!({ "device": "puck-1", "kind": "mouse", "art": "art/puck.svg", "view": [0, 0, 100, 200],
            "controls": [
                { "id": "tip", "label": "Tip", "short": "T", "pieces": ["tip-a", "tip-b"], "callout": { "side": "left", "at": 40, "line": [[10, 20], [30, 40]] } },
                { "id": "back", "label": "Back" }
            ],
            "capabilities": {
                "lighting": { "lights": [{ "id": "ring", "art": [2, 202], "glow": 2 }, { "id": "dot", "art": [5] }], "direct": { "lights": ["dot", "ring"] } },
                "settings": { "items": [{ "id": "dpi" }] }
            } });
        let s = Shape::of("made-up-puck", Path::new("/made-up"), &d);
        assert_eq!((s.core.as_str(), s.device.as_str(), s.art.as_path()), ("made-up-puck", "puck-1", Path::new("/made-up/art/puck.svg")));
        assert_eq!(s.buttons.iter().map(|b| (b.id.as_str(), b.short.as_str())).collect::<Vec<_>>(), [("tip", "T"), ("back", "Back")]);
        assert_eq!(s.buttons[0].pieces, ["tip-a", "tip-b"]);
        assert_eq!(s.buttons[0].callout, (true, 40.0, vec![(10.0, 20.0), (30.0, 40.0)]));
        assert_eq!(s.lights, ["dot", "ring"], "in the order their colors stream");
        assert_eq!(s.groups, [(0, "illumination_5".to_string()), (1, "illumination_2".to_string()), (1, "illumination_202".to_string())]);
        assert_eq!(s.glow, ["illumination_2"]);
        assert_eq!((s.view, s.settings.clone()), ((0.0, 0.0, 100.0, 200.0), vec!["dpi".to_string()]));
    }

    #[test]
    fn what_the_mouse_cant_say_is_never_written_as_a_guess() {
        let mut p = Page::new();
        let base: serde_json::Map<String, Value> = shape().buttons.iter().map(|b| (b.id.clone(), json!({ "mouse": 1 }))).collect();
        p.state = json!({
            "present": true,
            "keys": { "layers": { "base": base }, "factory": { "base": base }, "known": false },
            "lighting": { "logo": { "effect": "static", "params": { "color": "#FF1000" } }, "wheel": { "effect": "breathing", "params": {} }, "known": { "logo": "colors", "wheel": true } },
            "settings": { "values": { "cpi1": 800, "cpi2": 1600, "polling": 1000 }, "known": { "cpi1": true, "cpi2": true, "polling": false } },
        });
        let body = p.body_now();
        assert!(body.get("keys").is_none(), "{body}");
        assert_eq!(body["lighting"].as_object().unwrap().keys().collect::<Vec<_>>(), ["wheel"]);
        assert_eq!(body["settings"], json!({ "cpi1": 800, "cpi2": 1600 }));
        assert!(p.rows().iter().all(|r| r.job == "Not known" && !r.rebound));
        assert_eq!(p.detail(3).3, "Not known");
        assert!(p.all_buttons().1, "Back to factory offered: it sets them all");
        p.tab = 1;
        let lights: Vec<String> = p.rows().iter().map(|r| r.job.to_string()).collect();
        assert!(lights.contains(&"Not known".to_string()) && lights.iter().any(|l| l != "Not known"), "{lights:?}");
        // a job given to one button: all eight kept in the profile, known from now
        p.tab = 0;
        p.profiles = Some(crate::profiles::Store::first(None, body));
        p.set_job(3, json!({ "key": "F5" }));
        let kept = &p.profiles.as_ref().unwrap().current().body["keys"]["base"];
        assert_eq!(kept.as_object().unwrap().len(), shape().buttons.len());
        assert_eq!(kept[shape().buttons[3].id.as_str()], json!({ "key": "F5" }));
        assert!(p.buttons_known() && p.rows().iter().all(|r| r.job != "Not known"));
    }

    /// A macro's new way of playing reaches every button that runs it, on
    /// every profile, and a macro deleted leaves them doing nothing, so the
    /// next macro given its number isn't run by them; the buttons on the
    /// profile in use come back, for the mouse.
    #[test]
    fn a_macros_buttons_follow_it_on_every_profile() {
        let body = |left: Value, cpi: Value| json!({ "keys": { "base": { "left": { "mouse": 1 }, "left-side-back": left, "cpi": cpi } } });
        let mut s = crate::profiles::Store::first(None, body(json!({ "macro": 2, "plays": "once" }), json!({ "sensitivity": "switch" })));
        s.add(crate::profiles::Profile::new("Profile 2", body(json!({ "macro": 2, "plays": "once" }), json!({ "macro": 5, "plays": "once" }))));
        s.choose(0);
        let plays = ectomatter_protocol::Action::Macro { n: 2, plays: ectomatter_protocol::Plays::WhileHeld }.write();
        assert_eq!(rewrite_macro(&mut s, 2, &plays), [3]);
        assert_eq!(s.list[1].body["keys"]["base"]["left-side-back"], plays);
        assert_eq!(s.list[1].body["keys"]["base"]["cpi"]["macro"], 5);
        let off = json!({ "off": true });
        assert_eq!(rewrite_macro(&mut s, 2, &off), [3]);
        for p in &s.list {
            assert_eq!(p.body["keys"]["base"]["left-side-back"], off);
            assert_eq!(p.body["keys"]["base"]["left"]["mouse"], 1);
        }
        assert!(rewrite_macro(&mut s, 9, &off).is_empty());
    }

    /// A light's cycle played as the core says the mouse runs it: a breath
    /// dark at its start and end and its color halfway, halfway up a quarter
    /// of the way in; a color shift yellow a sixth of the way round and red
    /// again at the end; one color held, held.
    #[test]
    fn a_cycle_plays_as_the_mouse_runs_it() {
        let ms = Duration::from_millis;
        let breath = Plays::of(&json!({ "ms": 3000, "held": false, "stops": [{ "at": 0, "color": "#000000" }, { "at": 127, "color": "#FF1000" }, { "at": 255, "color": "#000000" }] })).unwrap();
        assert!(breath.moves());
        assert_eq!(breath.at(ms(0)), [0, 0, 0]);
        assert_eq!(breath.at(ms(1494)), [255, 16, 0], "its color at stop 127");
        let quarter = breath.at(ms(750));
        assert!((quarter[0] as i32 - 128).abs() <= 2, "halfway up a quarter in: {quarter:?}");
        assert!(breath.at(ms(2999))[0] < 4, "dark again at the end");
        assert_eq!(breath.at(ms(3000)), [0, 0, 0], "and round again");
        let shift = Plays::of(&json!({ "ms": 6000, "held": false, "stops": [
            { "at": 0, "color": "#FF0000" }, { "at": 43, "color": "#FFFF00" }, { "at": 85, "color": "#00FF00" }, { "at": 128, "color": "#00FFFF" },
            { "at": 170, "color": "#0000FF" }, { "at": 213, "color": "#FF00FF" }, { "at": 255, "color": "#FF0000" } ] })).unwrap();
        assert_eq!(shift.at(ms(1012)), [255, 255, 0], "yellow at stop 43");
        let end = shift.at(ms(5990));
        assert!(end[0] == 255 && end[2] < 10, "nearly red again at the end: {end:?}");
        let held = Plays::of(&json!({ "ms": 1000, "held": true, "stops": [{ "at": 0, "color": "#402000" }] })).unwrap();
        assert!(!held.moves());
        assert_eq!(held.at(ms(12345)), [0x40, 0x20, 0]);
    }
}
