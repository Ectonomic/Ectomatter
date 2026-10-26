//! Recording a macro from a keyboard's own keys: Ectomatter's, for any
//! keyboard, since the computer's input reaches Ectomatter and no core
//! (docs/core-protocol.md, section 10). The keyboard is found by the USB
//! device its core's manifest matches.
//!
//! While recording, the keyboard's input devices are taken for the recording
//! alone, so its keys reach nothing else: Super does not open the launcher
//! and Mute does not mute. They are handed back when recording stops, is
//! canceled, fills the macro, runs for a minute, or when the editor stops
//! asking about it for three seconds. If Ectomatter itself goes away, the
//! system hands them back with it.
//!
//! Each press becomes a step in the protocol's own words (section 5,
//! macros): `{"key": "LeftCtrl", "down": true, "after": 120}` for a key by
//! its name, `{"media": ...}` for a media key; a step costs what the
//! keyboard's description says it does (`budget`, `cost`).
//!
//! Linux is done here. Windows and macOS need their own ways of reading one
//! keyboard (a low-level hook, and IOHIDManager with Input Monitoring), and
//! say so until they are written.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ectomatter_protocol::{keys, media};
use serde_json::{Value, json};

pub const LIMIT: Duration = Duration::from_secs(60);
pub const ORPHAN: Duration = Duration::from_secs(3);

// The kernel's HID usage -> key code table (drivers/hid/hid-input.c,
// hid_keyboard), usages 0x00 to 0x65, read backward by `key_usage`.
const HID_KEYBOARD: [u16; 102] = [
    0, 0, 0, 0, 30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 28, 1, 14, 15, 57, 12, 13, 26, 27, 43, 43, 39, 40, 41, 51, 52, 53, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 87, 88, 99, 70, 119, 110, 102, 104, 111, 107, 109, 106, 105, 108, 103, 69, 98, 55, 74, 78, 96, 79, 80, 81, 75, 76, 77, 71, 72, 73, 82, 83, 86, 127,
];
// the modifiers' kernel key codes, in HID's order from Left Ctrl
const MODIFIER_CODES: [u16; 8] = [29, 42, 56, 125, 97, 54, 100, 126];
// media keys: kernel key code -> consumer usage
const KEY_TO_MEDIA: [(u16, u16); 16] = [(163, 0xB5), (165, 0xB6), (166, 0xB7), (164, 0xCD), (113, 0xE2), (115, 0xE9), (114, 0xEA), (171, 0x183), (155, 0x18A), (140, 0x192), (144, 0x194), (217, 0x221), (172, 0x223), (158, 0x224), (159, 0x225), (173, 0x227)];

/// A kernel key code as the HID usage it is, if a macro can hold it.
pub fn key_usage(code: u16) -> Option<u8> {
    if let Some(i) = MODIFIER_CODES.iter().position(|&c| c == code) {
        return Some(keys::FIRST_MODIFIER + i as u8);
    }
    HID_KEYBOARD.iter().position(|&c| c != 0 && c == code).map(|u| u as u8)
}

/// A kernel key code as the consumer usage of a media key, if it is one.
pub fn key_media(code: u16) -> Option<u16> {
    KEY_TO_MEDIA.iter().find(|(c, _)| *c == code).map(|(_, u)| *u)
}

/// What a step costs of a keyboard's room for a macro, as its description
/// says (`macros`, `budget`, `cost`: `{"key": 4, "media": 6}`); nothing, for
/// macros with no budget.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Costs {
    pub key: usize,
    pub media: usize,
}

impl Costs {
    pub fn of(budget: &Value) -> Costs {
        let n = |k: &str| budget["cost"][k].as_u64().unwrap_or(0) as usize;
        Costs { key: n("key"), media: n("media") }
    }

    /// What a step in the protocol's words costs.
    pub fn of_step(&self, step: &Value) -> usize {
        if step.get("media").is_some() { self.media } else { self.key }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Held {
    Key(u8),
    Media(u16),
}

impl Held {
    fn step(self, down: bool) -> Value {
        match self {
            Held::Key(u) => json!({ "key": keys::name(u).map_or_else(|| json!(u), |n| json!(n)), "down": down, "after": 0 }),
            Held::Media(u) => json!({ "media": media::name(u).map_or_else(|| json!(u), |n| json!(n)), "down": down, "after": 0 }),
        }
    }
}

struct Rec {
    steps: Vec<Value>,
    last: Option<f64>,
    t0: Instant,
    seen: Instant,
    room: usize,
    used: usize,
    costs: Costs,
    waits: bool,
    skipped: u32,
    full: bool,
    done: bool,
    reason: String,
    held: Vec<Held>,
    #[cfg(target_os = "linux")]
    fds: Vec<i32>,
}

impl Rec {
    fn new(room: usize, costs: Costs, waits: bool) -> Rec {
        Rec {
            steps: Vec::new(),
            last: None,
            t0: Instant::now(),
            seen: Instant::now(),
            room,
            used: 0,
            costs,
            waits,
            skipped: 0,
            full: false,
            done: false,
            reason: String::new(),
            held: Vec::new(),
            #[cfg(target_os = "linux")]
            fds: Vec::new(),
        }
    }

    fn view(&self) -> Value {
        json!({
            "recording": !self.done, "elapsed": (self.t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "steps": self.steps, "used": self.used, "room": self.room,
            "full": self.full, "skipped": self.skipped, "reason": self.reason,
        })
    }

    /// One key going down or up, at `t` seconds.
    fn event(&mut self, t: f64, code: u16, down: bool) {
        let held = if let Some(u) = key_media(code) {
            Held::Media(u)
        } else if let Some(u) = key_usage(code) {
            Held::Key(u)
        } else {
            self.skipped += 1;
            return;
        };
        if self.done {
            return;
        }
        // a release of something pressed before recording began
        if !down && !self.held.contains(&held) {
            return;
        }
        let cost = match held {
            Held::Media(_) => self.costs.media,
            Held::Key(_) => self.costs.key,
        };
        if self.used + cost > self.room {
            self.full = true;
            self.release("the macro is full");
            return;
        }
        if down {
            if !self.held.contains(&held) {
                self.held.push(held);
            }
        } else {
            self.held.retain(|h| *h != held);
        }
        // the wait after the previous step is the time since it; rounded half
        // to even, as the original editor rounds
        if self.waits
            && let (Some(last), Some(prev)) = (self.last, self.steps.last_mut())
        {
            prev["after"] = json!(((t - last) * 1000.0).round_ties_even().clamp(0.0, 65535.0) as u64);
        }
        self.last = Some(t);
        self.steps.push(held.step(down));
        self.used += cost;
    }

    fn release(&mut self, reason: &str) {
        if self.done {
            return;
        }
        self.done = true;
        self.reason = reason.to_string();
        #[cfg(target_os = "linux")]
        for fd in self.fds.drain(..) {
            linux::ungrab(fd);
        }
    }

    /// Anything still held when recording stopped is let go at the end, so a
    /// macro can never leave a key down.
    fn closed_steps(&self) -> Vec<Value> {
        let mut steps = self.steps.clone();
        steps.extend(self.held.iter().map(|h| h.step(false)));
        steps
    }
}

#[derive(Clone, Default)]
pub struct Recorder {
    state: Arc<Mutex<Option<Arc<Mutex<Rec>>>>>,
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder::default()
    }

    /// Start recording the keyboard that's the USB device `usb` (its vendor
    /// and product), with room for `room` of its steps' costs. `waits`
    /// keeps the time between presses.
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    pub fn start(&self, usb: (u16, u16), room: usize, costs: Costs, waits: bool) -> Result<Value, String> {
        let mut slot = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_some() {
            return Err("a recording is already running".into());
        }
        #[cfg(not(target_os = "linux"))]
        return Err("recording isn't available on this system yet".into());
        #[cfg(target_os = "linux")]
        {
            let fds = linux::grab_all(usb)?;
            let mut r = Rec::new(room, costs, waits);
            r.fds = fds.clone();
            let rec = Arc::new(Mutex::new(r));
            *slot = Some(rec.clone());
            drop(slot);
            let me = self.clone();
            std::thread::Builder::new().name("recorder".into()).spawn(move || me.run(rec, fds)).map_err(|e| e.to_string())?;
            Ok(self.status())
        }
    }

    pub fn status(&self) -> Value {
        let slot = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match &*slot {
            None => json!({ "recording": false }),
            Some(r) => {
                let mut r = r.lock().unwrap_or_else(|p| p.into_inner());
                r.seen = Instant::now();
                r.view()
            }
        }
    }

    /// Stop, and hand back what was recorded (`keep`) or nothing (cancel).
    pub fn stop(&self, keep: bool) -> Value {
        let mut slot = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let Some(r) = slot.take() else {
            return json!({ "recording": false, "steps": [] });
        };
        let mut r = r.lock().unwrap_or_else(|p| p.into_inner());
        r.release("stopped");
        let mut out = r.view();
        out["steps"] = if keep { Value::Array(r.closed_steps()) } else { json!([]) };
        out
    }

    #[cfg(target_os = "linux")]
    fn run(&self, rec: Arc<Mutex<Rec>>, fds: Vec<i32>) {
        loop {
            {
                let mut r = rec.lock().unwrap_or_else(|p| p.into_inner());
                if r.done {
                    return;
                }
                if r.t0.elapsed() > LIMIT {
                    r.release("a minute passed");
                    return;
                }
                if r.seen.elapsed() > ORPHAN {
                    r.release("the editor stopped asking");
                    drop(r);
                    let mut slot = self.state.lock().unwrap_or_else(|p| p.into_inner());
                    if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, &rec)) {
                        *slot = None;
                    }
                    return;
                }
            }
            for (t, code, down) in linux::read_keys(&fds, 100) {
                rec.lock().unwrap_or_else(|p| p.into_inner()).event(t, code, down);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;

    const EVIOCGRAB: libc::c_ulong = 0x4004_4590;
    const EV_KEY: u16 = 1;

    /// The input devices the kernel made for the USB device `(vendor, product)`.
    fn nodes((vendor, product): (u16, u16)) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(dir) = fs::read_dir("/sys/class/input") else {
            return out;
        };
        let mut names: Vec<String> = dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("event")).collect();
        names.sort();
        for n in names {
            let id = |f: &str| fs::read_to_string(format!("/sys/class/input/{n}/device/id/{f}")).map(|s| s.trim().to_string());
            if id("vendor").ok() == Some(format!("{vendor:04x}")) && id("product").ok() == Some(format!("{product:04x}")) {
                out.push(format!("/dev/input/{n}"));
            }
        }
        out
    }

    /// Every input device of the keyboard, taken from every other program.
    pub fn grab_all(usb: (u16, u16)) -> Result<Vec<i32>, String> {
        let nodes = nodes(usb);
        if nodes.is_empty() {
            return Err("the keyboard isn't plugged in".into());
        }
        let mut fds = Vec::new();
        for n in nodes {
            let path = std::ffi::CString::new(n).map_err(|e| e.to_string())?;
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
            if fd < 0 || unsafe { libc::ioctl(fd, EVIOCGRAB, 1) } < 0 {
                let e = std::io::Error::last_os_error();
                if fd >= 0 {
                    unsafe { libc::close(fd) };
                }
                for f in fds {
                    ungrab(f);
                }
                return Err(format!("the keyboard's keys couldn't be taken for recording: {e}"));
            }
            fds.push(fd);
        }
        Ok(fds)
    }

    pub fn ungrab(fd: i32) {
        unsafe {
            libc::ioctl(fd, EVIOCGRAB, 0);
            libc::close(fd);
        }
    }

    /// Key presses and releases waiting on `fds`, as (seconds, code, down),
    /// waiting up to `timeout_ms` for the first.
    pub fn read_keys(fds: &[i32], timeout_ms: i32) -> Vec<(f64, u16, bool)> {
        let mut polls: Vec<libc::pollfd> = fds.iter().map(|&fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
        let mut out = Vec::new();
        if unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout_ms) } <= 0 {
            return out;
        }
        for p in &polls {
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            let mut buf = [libc::input_event { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: 0, code: 0, value: 0 }; 64];
            let n = unsafe { libc::read(p.fd, buf.as_mut_ptr().cast(), std::mem::size_of_val(&buf)) };
            if n <= 0 {
                continue;
            }
            for ev in &buf[..n as usize / std::mem::size_of::<libc::input_event>()] {
                if ev.type_ == EV_KEY && (ev.value == 0 || ev.value == 1) {
                    out.push((ev.time.tv_sec as f64 + ev.time.tv_usec as f64 / 1e6, ev.code, ev.value == 1));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// Replay key events through the recorder's rules without a keyboard:
    /// the steps recorded, the releases added for keys still held, and its state.
    fn replay(room: usize, costs: Costs, waits: bool, events: &[(f64, u16, bool)]) -> (Vec<Value>, Vec<Value>, Value) {
        let mut r = Rec::new(room, costs, waits);
        for &(t, code, down) in events {
            r.event(t, code, down);
        }
        let all = r.closed_steps();
        let closing = all[r.steps.len()..].to_vec();
        let state = json!({ "used": r.used, "full": r.full, "skipped": r.skipped, "done": r.done });
        (r.steps, closing, state)
    }

    /// Recording makes the same steps the original editor recorded, from the
    /// same presses (parity/vectors.json.gz, in the Z10's own codes, read in
    /// the protocol's words), at the Z10's costs; what's held is let go at
    /// the end; a full macro stops.
    #[test]
    fn recording_turns_key_presses_into_the_same_steps() {
        let mut s = String::new();
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../parity/vectors.json.gz");
        flate2::read::GzDecoder::new(std::fs::File::open(file).unwrap()).read_to_string(&mut s).unwrap();
        let v: Value = serde_json::from_str(&s).unwrap();
        let r = &v["recorder"];
        // (the Z10's record codes: a modifier is 0x70 on, else the usage)
        for (code, usage) in r["tables"]["usage"].as_object().unwrap() {
            let ours = key_usage(code.parse().unwrap()).map(|u| if u >= keys::FIRST_MODIFIER { 0x70 + (u - keys::FIRST_MODIFIER) as u64 } else { u as u64 });
            assert_eq!(ours, usage.as_u64(), "key code {code}");
        }
        for (code, usage) in r["tables"]["media"].as_object().unwrap() {
            assert_eq!(key_media(code.parse().unwrap()).map(u64::from), usage.as_u64(), "media code {code}");
        }
        for code in 0..=767u16 {
            assert_eq!(key_usage(code).is_some(), r["tables"]["usage"].get(code.to_string()).is_some(), "key code {code}");
        }
        let z10 = Costs { key: 4, media: 6 };
        let words = |steps: &Value| Value::Array(evga_z10::shared::steps_of(steps.as_array().unwrap()));
        let key = |v: &Value| {
            let mut o: Vec<_> = v.as_object().unwrap().iter().map(|(k, v)| format!("{k}={v}")).collect();
            o.sort();
            o.join(",")
        };
        for (i, c) in r["cases"].as_array().unwrap().iter().enumerate() {
            let events: Vec<(f64, u16, bool)> = c["events"].as_array().unwrap().iter().map(|e| (e[0].as_f64().unwrap(), e[1].as_u64().unwrap() as u16, e[2] == true)).collect();
            let (steps, mut closing, state) = replay(c["room"].as_u64().unwrap() as usize, z10, c["waits"] == true, &events);
            assert_eq!(Value::Array(steps), words(&c["steps"]), "recording {i} steps");
            // (the original lets go of held keys in no fixed order)
            closing.sort_by_key(key);
            let mut want = words(&c["closing"]).as_array().unwrap().clone();
            want.sort_by_key(key);
            assert_eq!(closing, want, "recording {i} releases at the end");
            for k in ["used", "full", "skipped", "done"] {
                assert_eq!(state[k], c[k], "recording {i} {k}");
            }
        }
    }

    /// With no budget, steps cost nothing and a recording never fills.
    #[test]
    fn a_macro_with_no_budget_never_fills() {
        let (steps, _, state) = replay(0, Costs::of(&Value::Null), false, &[(0.0, 30, true), (0.1, 30, false)]);
        assert_eq!(steps, vec![json!({ "key": "A", "down": true, "after": 0 }), json!({ "key": "A", "down": false, "after": 0 })]);
        assert_eq!(state["full"], false);
    }
}
