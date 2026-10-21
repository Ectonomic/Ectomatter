//! The mouse's page: the SteelSeries Sensei 310, through SteelSeries' core
//! (cores/steelseries), built from the keyboard page's parts as its mock has
//! it (mock/sensei-310.html): Buttons, Lighting, Performance and Settings.
//!
//! The core is asked on a thread of its own, which tells the page what the
//! mouse is set to after each change and whenever the mouse comes or goes.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

use crate::core_host::Core;
use crate::lighting::{Ctl, Stack, cell, kind, style, text_cell};
use crate::{Cell, MouseCall, MouseRow};

pub const DEVICE: &str = "sensei310-1";

/// What the page asks of the mouse.
pub enum Ask {
    Key(String, Value),
    ResetKey(String),
    Light(String, Value),
    Setting(String, u64),
    /// a whole profile put on the mouse (the core's profiles/apply)
    Apply(Value),
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
}

/// Start the mouse's thread, with SteelSeries' core.
pub fn start(core: Core, tell: Sender<Heard>) -> mpsc::Sender<Ask> {
    let (ask, asked) = mpsc::channel::<Ask>();
    std::thread::spawn(move || {
        if let Err(e) = core.call("initialize", &json!({ "protocol": 3 })) {
            let _ = tell.send(Heard::Failed(e));
        }
        // (what the mouse ships as, which a new profile starts as)
        let factory = core.call("profiles/factory", &json!({ "device": DEVICE })).unwrap_or(Value::Null);
        let (mut present, mut device) = (false, Value::Null);
        let read = |present: bool, device: &Value| {
            let d = json!({ "device": DEVICE });
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
                    };
                    let mut newest: Vec<Ask> = Vec::new();
                    for a in all {
                        newest.retain(|b| key(b) != key(&a));
                        newest.push(a);
                    }
                    for a in newest {
                        let r = match a {
                            Ask::Key(control, action) => core.call("keys/set", &json!({ "device": DEVICE, "layer": "base", "control": control, "action": action })).map(|_| ()),
                            Ask::ResetKey(control) => core.call("keys/reset", &json!({ "device": DEVICE, "control": control })).map(|_| ()),
                            // (the moment the mouse takes an effect is when it starts its cycle)
                            Ask::Light(light, l) => core.call("lighting/set", &json!({ "device": DEVICE, "light": light, "effect": l["effect"], "params": l["params"] })).map(|v| {
                                let _ = tell.send(Heard::Played(light, v["plays"].clone(), Instant::now()));
                            }),
                            Ask::Setting(setting, v) => core.call("settings/set", &json!({ "device": DEVICE, "id": setting, "value": v })).map(|_| ()),
                            Ask::Apply(mut body) => {
                                body["device"] = json!(DEVICE);
                                core.call("profiles/apply", &body).map(|v| {
                                    // (each light from the moment the mouse took the whole)
                                    let at = Instant::now();
                                    for l in LIGHTS {
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

/// The buttons as the core lists them, with their short names on the Key
/// detail box's cap, and the callout each has on the picture.
pub const BUTTONS: [(&str, &str, &str); 8] = [
    ("left", "Left button", "L"),
    ("right", "Right button", "R"),
    ("wheel-click", "Wheel click", "W"),
    ("left-side-back", "Left side, back", "LB"),
    ("left-side-front", "Left side, front", "LF"),
    ("right-side-back", "Right side, back", "RB"),
    ("right-side-front", "Right side, front", "RF"),
    ("cpi", "CPI button", "CPI"),
];

/// Each button's callout on the stage, in the mock's own canvas (1092 by 470,
/// the mouse at 480, 30, 240 by 410): its side, how far down its words are,
/// and the line from them to its dot on the button.
pub const CALLS: [(bool, f32, &[(f32, f32)]); 8] = [
    (true, 70.0, &[(424.0, 96.0), (538.0, 96.0), (552.0, 110.0)]),
    (false, 70.0, &[(776.0, 96.0), (662.0, 96.0), (648.0, 110.0)]),
    (true, 128.0, &[(424.0, 154.0), (565.0, 154.0), (599.0, 118.0)]),
    (true, 245.0, &[(424.0, 271.0), (486.0, 271.0), (499.0, 260.0)]),
    (true, 193.0, &[(424.0, 219.0), (490.0, 219.0), (501.0, 200.0)]),
    (false, 245.0, &[(776.0, 271.0), (714.0, 271.0), (701.0, 260.0)]),
    (false, 193.0, &[(776.0, 219.0), (710.0, 219.0), (699.0, 200.0)]),
    (false, 128.0, &[(776.0, 154.0), (640.0, 154.0), (601.0, 186.0)]),
];

/// The kinds of job a button can have, as the Key detail box's switch shows
/// them, each drawn on its 24 grid (the Key, Media, Mouse and Nothing glyphs
/// as the keyboard's own).
pub const KINDS: [(&str, &str); 6] = [
    ("Mouse", "M7 3.5h10v17H7zM12 7v3"),
    ("Key", "M4 5h16v14H4zM9 10h6M12 10v5"),
    ("Media", "M5 6l8 6-8 6zM15 6v12M19 6v12"),
    ("Scroll", "M12 4v16M8 8l4-4 4 4M8 16l4 4 4-4"),
    ("Sensitivity", "M4 17a8 8 0 1 1 16 0M12 17l4.5-5"),
    ("Nothing", "M4 12a8 8 0 1 0 16 0a8 8 0 1 0-16 0M6.5 17.5l11-11"),
];

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
        u => crate::z10::hid_name(u).unwrap_or_else(|| format!("Key {u}")),
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
    } else if a.get("key").is_some() {
        1
    } else if a.get("media").is_some() {
        2
    } else if a.get("scroll").is_some() {
        3
    } else if a.get("sensitivity").is_some() {
        4
    } else {
        5
    }
}

/// A job in words.
pub fn job_label(a: &Value) -> String {
    match kind_of(a) {
        0 => a["mouse"].as_u64().and_then(|n| MOUSE_JOBS.get(n as usize - 1)).map_or("A mouse button".into(), |s| s.to_string()),
        1 => usage_of(&a["key"]).map_or("A key".into(), key_label),
        2 => MEDIA.iter().find(|(n, _)| a["media"] == *n).map_or("A media key".into(), |(_, l)| l.to_string()),
        3 => if a["scroll"] == "down" { "Scroll down".into() } else { "Scroll up".into() },
        4 => "Switch sensitivity".into(),
        _ => "Nothing".into(),
    }
}

/// The jobs of a kind, in words and as the core takes them.
pub fn choices(kind: usize) -> Vec<(String, Value)> {
    match kind {
        0 => MOUSE_JOBS.iter().enumerate().map(|(i, l)| (l.to_string(), json!({ "mouse": i + 1 }))).collect(),
        1 => keys().into_iter().map(|(u, n)| (key_label(u), json!({ "key": n }))).collect(),
        2 => MEDIA.iter().map(|(n, l)| (l.to_string(), json!({ "media": n }))).collect(),
        3 => vec![("Scroll up".into(), json!({ "scroll": "up" })), ("Scroll down".into(), json!({ "scroll": "down" }))],
        4 => vec![("Switch sensitivity".into(), json!({ "sensitivity": "switch" }))],
        _ => vec![("Nothing".into(), json!({ "off": true }))],
    }
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
    /// the kind of job whose menu is open
    pub menu: Option<usize>,
    pub failed: Option<String>,
    /// what each light (the logo, the wheel) plays, and from when
    pub plays: [Option<(Plays, Instant)>; 2],
    /// where the picture's layers go once drawn, off the page's thread
    pub tell: Option<Sender<Heard>>,
    /// the mouse's profiles, kept by Ectodeck (the mouse keeps one setup in its
    /// memory: the one in use), once the mouse has been met
    pub profiles: Option<crate::profiles::Store>,
    /// what was last done to the profiles, for the bar, and a profile deleted
    /// with when, which Undo puts back for a few seconds
    pub said: Option<(String, Instant)>,
    pub gone: Option<((usize, crate::profiles::Profile), Instant)>,
}

/// How long Undo stays in the bar after a profile is deleted.
pub const UNDO_FOR: Duration = Duration::from_secs(6);

/// The lights, as the core names them, in the page's order.
pub const LIGHTS: [&str; 2] = ["logo", "wheel"];

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
        Page { ask: None, state: Value::Null, tab: 0, button: None, light: 0, same_wheel, menu: None, failed: None, plays: [None, None], tell: None, profiles: None, said: None, gone: None }
    }

    /// What the core says of the mouse: what each light plays is kept with
    /// when it began, unless the light plays something new.
    pub fn heard_state(&mut self, v: Value) {
        for (i, l) in LIGHTS.iter().enumerate() {
            let now = Plays::of(&v["lighting"][l]["plays"]);
            if now.as_ref() != self.plays[i].as_ref().map(|(p, _)| p) {
                self.plays[i] = now.map(|p| (p, Instant::now()));
            }
        }
        self.state = v;
        // the first time the mouse is met: its profiles as kept, or what it is
        // set to now as Profile 1
        if self.profiles.is_none() && self.known() {
            let path = crate::profiles::path_for("sensei310");
            self.profiles = Some(crate::profiles::Store::load(path.clone()).unwrap_or_else(|| crate::profiles::Store::first(path, self.body_now())));
        }
    }

    /// Everything the mouse is set to now, as a profile holds it.
    pub fn body_now(&self) -> Value {
        let lights: serde_json::Map<String, Value> = LIGHTS.iter().map(|l| (l.to_string(), json!({ "effect": self.state["lighting"][l]["effect"], "params": self.state["lighting"][l]["params"] }))).collect();
        let v = &self.state["settings"]["values"];
        json!({
            "keys": { "base": self.state["keys"]["layers"]["base"] },
            "lighting": lights,
            "settings": { "cpi1": v["cpi1"], "cpi2": v["cpi2"], "polling": v["polling"] },
        })
    }

    /// A profile put on the mouse: shown as it is at once, and sent whole.
    fn put_on(&mut self, body: &Value) {
        if let Some(base) = body["keys"]["base"].as_object() {
            for (id, a) in base {
                self.state["keys"]["layers"]["base"][id] = a.clone();
            }
        }
        for l in LIGHTS {
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
        p.add(crate::profiles::Profile { name: name.clone(), body: factory.clone() });
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
                sub: effect_name(pr.body["lighting"]["logo"]["effect"].as_str().unwrap_or("static")).into(),
                active: i == p.active,
                current: i == p.active,
                ..Default::default()
            })
            .collect()
    }

    /// A light's effect taken by the mouse at `at`: the light plays it from then.
    pub fn played(&mut self, light: &str, v: &Value, at: Instant) {
        if let (Some(i), Some(p)) = (LIGHTS.iter().position(|l| *l == light), Plays::of(v)) {
            self.plays[i] = Some((p, at));
        }
    }

    /// A light's color at `now`, as the mouse shows it; black for off.
    pub fn color_at(&self, i: usize, now: Instant) -> [u8; 3] {
        match &self.plays[i] {
            Some((p, from)) => p.at(now.saturating_duration_since(*from)),
            None => self.lit(LIGHTS[i]).unwrap_or([0; 3]),
        }
    }

    /// Whether a light's effect moves, so the picture is drawn again as it plays.
    pub fn moving(&self) -> bool {
        self.plays.iter().flatten().any(|(p, _)| p.moves())
    }

    /// Whether Ectodeck has met the mouse: it is plugged in, or the core
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

    pub fn job(&self, i: usize) -> Value {
        self.state["keys"]["layers"]["base"][BUTTONS[i].0].clone()
    }

    pub fn factory(&self, i: usize) -> Value {
        self.state["keys"]["factory"]["base"][BUTTONS[i].0].clone()
    }

    pub fn rebound(&self, i: usize) -> bool {
        let (j, f) = (self.job(i), self.factory(i));
        !f.is_null() && j != f
    }

    pub fn light_name(&self) -> &'static str {
        if self.light == 0 { "logo" } else { "wheel" }
    }

    pub fn light_of(&self, l: &str) -> Value {
        self.state["lighting"][l].clone()
    }

    /// A light's color as the picture shows it: none while it is off.
    pub fn lit(&self, l: &str) -> Option<[u8; 3]> {
        let v = self.light_of(l);
        match v["effect"].as_str() {
            Some("off") => None,
            Some("colorshift") => Some(shift_colors(&v)[0]),
            _ => Some(rgb(&v["params"]["color"]).unwrap_or([0xff, 0x10, 0x00])),
        }
    }

    /// A light set: and the wheel the same, from the logo, while it copies it.
    pub fn set_light(&mut self, l: Value) {
        let name = self.light_name();
        self.state["lighting"][name] = l.clone();
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
            self.send(Ask::Light("wheel".into(), l));
        }
    }

    /// The wheel set as the logo is (Same on the wheel), on the mouse and in the profile.
    pub fn copy_logo_to_wheel(&mut self) {
        let l = self.light_of("logo");
        self.state["lighting"]["wheel"] = l.clone();
        let kept = json!({ "effect": l["effect"], "params": l["params"] });
        self.keep_in_profile(|b| b["lighting"]["wheel"] = kept);
        self.send(Ask::Light("wheel".into(), l));
    }

    /// A button given back the job it had from the factory.
    pub fn reset_job(&mut self, i: usize) {
        let f = self.factory(i);
        if !f.is_null() {
            self.state["keys"]["layers"]["base"][BUTTONS[i].0] = f.clone();
            self.keep_in_profile(|b| b["keys"]["base"][BUTTONS[i].0] = f);
            self.send(Ask::ResetKey(BUTTONS[i].0.into()));
        }
    }

    pub fn set_job(&mut self, i: usize, a: Value) {
        self.state["keys"]["layers"]["base"][BUTTONS[i].0] = a.clone();
        let kept = a.clone();
        self.keep_in_profile(|b| b["keys"]["base"][BUTTONS[i].0] = kept);
        self.send(Ask::Key(BUTTONS[i].0.into(), a));
    }

    pub fn setting(&self, id: &str) -> u64 {
        self.state["settings"]["values"][id].as_u64().unwrap_or(0)
    }

    pub fn set_setting(&mut self, id: &str, v: u64) {
        self.state["settings"]["values"][id] = json!(v);
        self.keep_in_profile(|b| b["settings"][id] = json!(v));
        self.send(Ask::Setting(id.into(), v));
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
            return [("logo", "Logo"), ("wheel", "Wheel")]
                .iter()
                .enumerate()
                .map(|(i, (id, name))| {
                    let l = self.light_of(id);
                    let lit = self.lit(id);
                    MouseRow { name: (*name).into(), job: effect_name(l["effect"].as_str().unwrap_or("static")).into(), chosen: i == self.light, rebound: false, swatch: color(lit.unwrap_or([0x2a, 0x2a, 0x30])), swatched: true }
                })
                .collect();
        }
        BUTTONS
            .iter()
            .enumerate()
            .map(|(i, (_, name, _))| MouseRow { name: (*name).into(), job: job_label(&self.job(i)).into(), chosen: Some(i) == self.button, rebound: self.rebound(i), swatch: color([0; 3]), swatched: false })
            .collect()
    }

    /// Each button's words on the picture, its line and its dot.
    pub fn calls(&self) -> Vec<MouseCall> {
        BUTTONS
            .iter()
            .enumerate()
            .map(|(i, (_, name, _))| {
                let (left, top, _) = CALLS[i];
                MouseCall { name: (*name).into(), job: job_label(&self.job(i)).into(), left, top, chosen: Some(i) == self.button, rebound: self.rebound(i) }
            })
            .collect()
    }

    /// The callouts' lines and dots, as path commands: the others', and the chosen button's.
    pub fn lines(&self) -> (String, String, String, String) {
        let (mut lines, mut dots, mut on_line, mut on_dot) = (String::new(), String::new(), String::new(), String::new());
        for (i, (_, _, pts)) in CALLS.iter().enumerate() {
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
        let state = if rebound { format!("Now {}", job_label(&job)) } else { "As it came from the factory".into() };
        let note = if self.state["keys"]["known"].as_bool() == Some(false) {
            "The mouse can’t tell Ectodeck what its buttons do. Until you set one here, this shows what it did from the factory.".into()
        } else {
            String::new()
        };
        (BUTTONS[i].2.into(), BUTTONS[i].1.into(), format!("From the factory: {}", job_label(&factory)), state, rebound, kind_of(&job), note)
    }

    /// With no button chosen, the box for every button: what it says, and
    /// whether any button does something other than the factory's job.
    pub fn all_buttons(&self) -> (String, bool) {
        let n = (0..BUTTONS.len()).filter(|&i| self.rebound(i)).count();
        let note = match n {
            0 => "Every button does what it did from the factory.".to_string(),
            1 => "One button has a new job, shown in amber. Back to factory gives it its old one.".to_string(),
            n => format!("{n} buttons have new jobs, shown in amber. Back to factory gives every one its old job."),
        };
        (note, n > 0)
    }

    // ------------------------------------------------------------ Lighting, Performance, Settings

    /// The chosen light's effect, by its place in EFFECTS.
    pub fn effect(&self) -> usize {
        let e = self.light_of(self.light_name())["effect"].as_str().unwrap_or("static").to_string();
        EFFECTS.iter().position(|(id, _)| *id == e).unwrap_or(1)
    }

    /// The boxes under the picture as cells: on Lighting, the chosen effect's
    /// settings; on Performance, the sensitivities and the polling rate; on
    /// Settings, the card. `w` is the room across.
    pub fn cells(&self, w: f32) -> (Vec<Cell>, f32) {
        match self.tab {
            1 => self.light_cells(w),
            2 => self.performance_cells(w),
            3 => self.settings_cells(w),
            _ => (Vec::new(), 0.0),
        }
    }

    fn light_cells(&self, w: f32) -> (Vec<Cell>, f32) {
        let l = self.light_of(self.light_name());
        let e = l["effect"].as_str().unwrap_or("static");
        let mut s = Stack::new(w);
        match e {
            "off" => s.note("The light stays dark."),
            "static" => s.note("One color, steady."),
            "breathing" => s.note("The color fades in and out."),
            _ => s.note("The colors below play in turn, each blending into the next."),
        }
        if e == "static" || e == "breathing" {
            s.gap(14.0);
            s.cells.push(text_cell(style::GROUP, 16.0, s.y, s.inner(), 14.0, "COLOR"));
            s.gap(14.0 + 10.0);
            let c = rgb(&l["params"]["color"]).unwrap_or([0xff, 0x10, 0x00]);
            s.dots(Ctl::MouseColor, &[c], None, true);
        }
        if e == "colorshift" {
            // its colors as dots you can change, over the strip they blend along
            s.gap(14.0);
            s.cells.push(text_cell(style::GROUP, 16.0, s.y, s.inner(), 14.0, "COLORS"));
            s.gap(14.0 + 10.0);
            let cols = shift_colors(&l);
            s.strip(&cols);
            s.dots(Ctl::MouseColors, &cols, None, false);
        }
        if e == "breathing" || e == "colorshift" {
            let ms = l["params"]["speed"].as_u64().unwrap_or(if e == "breathing" { 3000 } else if l["params"]["colors"].is_array() { 1000 } else { 5000 }) as f32;
            s.gap(18.0);
            s.head("Speed", Some(&format!("{:.1} s a cycle", ms / 1000.0)));
            // quick at the left, as the keyboard's speed sliders run
            s.slider(Ctl::MouseSpeed, 1.0 - (ms - 500.0) / (30000.0 - 500.0), color([0xa1, 0xa1, 0xaa]), Some(("Slow", "Fast")));
        }
        if self.light == 0 {
            s.gap(22.0);
            s.check(Ctl::MouseSameWheel, self.same_wheel, "Same on the wheel");
            s.gap(6.0);
            let lines = crate::text::wrap(400, 12.5, "The wheel copies the logo’s effect and colors.", s.inner() - 26.0);
            for line in lines {
                s.cells.push(text_cell(style::NOTE, 42.0, s.y, s.inner() - 26.0, 18.0, &line));
                s.y += 18.0;
            }
        }
        boxed(s, 0.0, 0.0, effect_name(e))
    }

    fn performance_cells(&self, w: f32) -> (Vec<Cell>, f32) {
        let poll_w = 420.0f32.min(w * 0.42);
        let cpi_w = w - poll_w - 12.0;
        let mut a = Stack::new(cpi_w);
        for (k, id) in ["cpi1", "cpi2"].iter().enumerate() {
            let v = self.setting(id).max(100);
            if k > 0 {
                a.gap(22.0);
            }
            a.head(if k == 0 { "First" } else { "Second" }, Some(&format!("{v} CPI")));
            a.slider(if k == 0 { Ctl::MouseCpi1 } else { Ctl::MouseCpi2 }, (v as f32 - 100.0) / 11900.0, color([0xa1, 0xa1, 0xaa]), None);
        }
        a.gap(18.0);
        a.note("From 100 to 12,000 CPI, in steps of 100. The CPI button behind the wheel switches between the two.");
        if let Some(p) = self.state["settings"]["active"].as_u64() {
            a.gap(6.0);
            a.note(&format!("When Ectodeck last asked, the mouse was using the {}.", if p == 1 { "first" } else { "second" }));
        }
        let (mut cells, ha) = boxed(a, 0.0, 0.0, "Sensitivity");
        let mut b = Stack::new(poll_w);
        let rates = [125u64, 250, 500, 1000];
        let now = self.setting("polling");
        let names: Vec<String> = rates.iter().map(|r| format!("{r} Hz")).collect();
        let opts: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "")).collect();
        b.switch(Ctl::MousePolling, &opts, rates.iter().position(|r| *r == now).unwrap_or(3));
        b.gap(12.0);
        b.note("How many times a second the mouse tells the computer where it is. 1000 Hz follows your hand most closely.");
        if self.state["settings"]["known"]["polling"].as_bool() == Some(false) {
            b.gap(6.0);
            b.note("The mouse can’t tell Ectodeck its polling rate. Until you set one here, this shows the factory’s.");
        }
        let (more, hb) = boxed(b, cpi_w + 12.0, 0.0, "Polling rate");
        cells.extend(more);
        (cells, ha.max(hb))
    }

    fn settings_cells(&self, w: f32) -> (Vec<Cell>, f32) {
        let card_w = 760.0f32.min(w);
        let x0 = ((w - card_w) / 2.0).max(0.0);
        let mut cells = vec![Cell { style: 1, ..cell(kind::TILE, x0, 22.0, card_w, 0.0) }];
        cells.push(text_cell(style::BOX_TITLE, x0 + 20.0, 22.0 + 18.0, card_w - 40.0, 20.0, "Mouse"));
        let mut y = 22.0 + 46.0;
        let present = self.present();
        cells.push(Cell { style: if present { 1 } else { 2 }, ..cell(kind::DOT, x0 + 20.0, y + 6.0, 7.0, 7.0) });
        cells.push(text_cell(style::NOTE, x0 + 35.0, y, card_w - 60.0, 20.0, if present { "Connected by USB." } else { "Not connected. Plug the mouse in to change it; what it was set to stays here." }));
        y += 34.0;
        let firmware = self.state["device"]["firmware"]["version"].as_str().map_or("Not known yet".to_string(), |v| format!("SteelSeries {v}"));
        let rows: [(&str, String); 3] = [
            ("Firmware", firmware),
            ("Settings kept", "Your profiles are kept in Ectodeck. The one in use is also on the mouse, so the mouse keeps working that way without Ectodeck. A change made to the mouse in another app won’t show here.".into()),
            ("Model", "SteelSeries Sensei 310, USB 1038:1722".into()),
        ];
        for (k, v) in rows {
            cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, x0 + 20.0, y, card_w - 40.0, 1.0) });
            y += 13.0;
            cells.push(text_cell(style::LABEL_SOFT, x0 + 20.0, y, 190.0, 20.0, k));
            // (wrapped a little short of the column, as the card's words are measured, not drawn)
            let lines = crate::text::wrap(400, 13.0, &v, card_w - 40.0 - 198.0 - 16.0);
            for (j, line) in lines.iter().enumerate() {
                cells.push(text_cell(style::VALUE_BRIGHT, x0 + 20.0 + 198.0, y + j as f32 * 20.0, card_w - 40.0 - 198.0, 20.0, line));
            }
            y += lines.len().max(1) as f32 * 20.0 + 13.0;
        }
        let h = y + 8.0 - 22.0;
        cells[0].h = h;
        (cells, 22.0 + h + 40.0)
    }
}

/// The mouse's effects, as the core offers them, with the tile picture each borrows
/// from the keyboard's effects.
pub const EFFECTS: [(&str, &str); 4] = [("off", "Off"), ("static", "Static"), ("breathing", "Breathing"), ("colorshift", "Color shift")];

pub fn effect_name(e: &str) -> &'static str {
    EFFECTS.iter().find(|(id, _)| *id == e).map_or("Static", |(_, n)| n)
}

/// A light set to effect `e`, keeping the color and speed it had; Color shift
/// chosen for the first time starts as SteelSeries GG's own, red, green and
/// blue, a second around.
pub fn with_effect(old: &Value, e: &str) -> Value {
    let mut params = old["params"].clone();
    if !params.is_object() {
        params = json!({});
    }
    if params.get("color").is_none() {
        params["color"] = json!("#FF1000");
    }
    if e == "colorshift" && !params["colors"].is_array() {
        params["colors"] = json!(GG_SHIFT);
        params["speed"] = json!(1000);
    }
    json!({ "effect": e, "params": params })
}

/// SteelSeries GG's Color shift for the mouse: red, green and blue.
const GG_SHIFT: [&str; 3] = ["#FF0000", "#00FF00", "#0000FF"];

/// The colors a light set to Color shift plays before it was given colors of
/// its own: round the color wheel, as the core plays it then.
const WHEEL: [&str; 6] = ["#FF0000", "#FFFF00", "#00FF00", "#00FFFF", "#0000FF", "#FF00FF"];

/// A Color shift's colors, in order.
pub fn shift_colors(l: &Value) -> Vec<[u8; 3]> {
    match l["params"]["colors"].as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|c| rgb(c).unwrap_or([0; 3])).collect(),
        _ => WHEEL.iter().map(|c| rgb(&json!(c)).unwrap()).collect(),
    }
}

/// A Color shift with one of its colors changed: a light that went round the
/// color wheel takes the wheel's colors, and its speed, as its own first.
pub fn with_shift_color(l: &Value, i: usize, c: [u8; 3]) -> Value {
    let mut l = l.clone();
    if !l["params"]["colors"].is_array() {
        l["params"]["colors"] = json!(WHEEL);
        if l["params"]["speed"].is_null() {
            l["params"]["speed"] = json!(5000);
        }
    }
    if let Some(slot) = l["params"]["colors"].get_mut(i) {
        *slot = json!(crate::lighting::hex_of(c).to_uppercase());
    }
    l
}

/// A stack laid out as a box: its tile under it at `x`, `y`, its title, its pieces moved there.
fn boxed(s: Stack, x: f32, y: f32, title: &str) -> (Vec<Cell>, f32) {
    let h = s.height();
    let mut cells = vec![Cell { style: 1, ..cell(kind::TILE, x, y, s.w, h) }];
    cells.push(text_cell(style::BOX_TITLE, x + 16.0, y + 14.0, s.w - 32.0, 20.0, title));
    for mut c in s.cells {
        c.x += x;
        c.y += y;
        cells.push(c);
    }
    (cells, y + h)
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

    /// Every job a button can be given reads back as itself, in the core's terms and in words.
    #[test]
    fn every_job_reads_back_as_itself() {
        for k in 0..KINDS.len() {
            for (label, a) in choices(k) {
                assert_eq!(kind_of(&a), k, "{a}");
                assert_eq!(job_label(&a), label, "{a}");
            }
        }
        assert_eq!(job_label(&json!({ "key": "PageDown" })), "Page Down");
        assert_eq!(job_label(&json!({ "key": 104 })), "F13");
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
