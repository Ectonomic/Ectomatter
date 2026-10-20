//! The keyboard, on a thread of its own. The Z10's core does the talking
//! (cores/evga-z10, run as a module by core_host.rs), which takes seconds for
//! a whole read, and the window keeps drawing meanwhile: the page asks, and
//! hears back through the event loop.

use std::path::PathBuf;
use std::sync::mpsc;

use serde_json::{Value, json};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

pub enum Ask {
    /// everything the editor shows: firmware, the active profile, every profile and macro
    State,
    /// save a profile's name, lighting and keys, and read it back
    Save(u8, Value),
    /// make a profile the one in use
    Activate(u8),
    /// which profile the keyboard is using now
    ReadActive,
    /// where both sliders are, on firmware that can say
    Sliders,
    /// show a preview on the keyboard: an effect, each key's color, or stop
    Preview(Value),
    /// hold the dimmer at a level, or give it back to the slider
    Dimmer(Option<u8>),
    /// ask the keyboard to stop doing a slider's job itself: (quiet volume, keep dimmer)
    Modes(bool, bool),
    /// save or delete a macro: its name, steps and play settings, or `delete: true`
    WriteMacro(u8, Value),
    /// save a profile's keys only, to change how a macro plays on them
    SaveKeys(u8, Value),
    /// the screen as a profile being edited has it, shown now without saving:
    /// its light (brightness, red, green, blue) and its built-in screens
    Screen([u8; 4], u8),
}

pub enum Heard {
    State(Result<Value, String>),
    Saved(u8, Result<Value, String>),
    Active(u8, Result<Option<u8>, String>),
    /// the profile the keyboard is using, as read now
    ActiveNow(Result<Option<u8>, String>),
    Sliders(Result<Value, String>),
    Preview(Result<Value, String>),
    /// every macro, after one was written; whether it was a delete
    Macros(bool, Result<Value, String>),
    KeysSaved(u8, Result<Value, String>),
    Done,
}

/// The Z10's core, started: its manifest and module in `dir`, its own files
/// (where the first write of a session saves what it replaces) in `data`.
pub fn z10(dir: &std::path::Path, data: PathBuf) -> Result<crate::core_host::Core, String> {
    crate::core_host::Core::start(dir, data, Box::new(crate::core_host::Hidapi::new()))
}

/// The keyboard, as the page asks of it: each ask a call to the Z10's core.
#[derive(Clone)]
struct Kb(Result<crate::core_host::Core, String>);

impl Kb {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.0.as_ref().map_err(|e| e.clone())?.call(method, &params)
    }
    fn flag(&self, method: &str, params: Value) -> Result<bool, String> {
        self.call(method, params).map(|v| v.as_bool().unwrap_or(false))
    }
    fn profile(&self, method: &str, params: Value) -> Result<Option<u8>, String> {
        self.call(method, params).map(|v| v.as_u64().map(|n| n as u8))
    }
}

/// Start the keyboard's thread, with the Z10's core in `dir` and its own
/// files in `data`. Previews take a thread of their own to the core, so one
/// never waits behind a whole read or a save: the core takes it between that
/// job's exchanges with the keyboard, and the keys keep up with the page.
pub fn start(dir: PathBuf, data: PathBuf, tell: Sender<Heard>) -> mpsc::Sender<Ask> {
    let (ask, asked) = mpsc::channel::<Ask>();
    let kb = Kb(z10(&dir, data));
    let (look, looks) = mpsc::channel::<Value>();
    let (work, asked_work) = mpsc::channel::<Ask>();
    let (quick, told) = (kb.clone(), tell.clone());
    let previews = std::thread::spawn(move || {
        while let Ok(mut body) = looks.recv() {
            // of several, only the newest is worth sending
            while let Ok(b) = looks.try_recv() {
                body = b;
            }
            if told.send(Heard::Preview(quick.call("preview", body))).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for a in asked {
            let gone = match a {
                Ask::Preview(body) => look.send(body).is_err(),
                other => work.send(other).is_err(),
            };
            if gone {
                break;
            }
        }
        // the last preview (a stop, as the window closes) before the keyboard is let go
        drop(look);
        let _ = previews.join();
    });
    let asked = asked_work;
    std::thread::spawn(move || {
        let mut queue: std::collections::VecDeque<Ask> = std::collections::VecDeque::new();
        let mut handed_back = false;
        loop {
            // Everything asked for so far: a slider reading goes first, so the
            // knobs keep up while the lighting streams, and of several screens
            // only the newest is sent (the older ones are already out of date).
            if queue.is_empty() {
                match asked.recv() {
                    Ok(a) => queue.push_back(a),
                    Err(_) => break,
                }
            }
            while let Ok(a) = asked.try_recv() {
                queue.push_back(a);
            }
            if let Some(i) = queue.iter().position(|a| matches!(a, Ask::Sliders)) {
                let a = queue.remove(i).unwrap();
                queue.push_front(a);
            }
            let last_screen = queue.iter().rposition(|a| matches!(a, Ask::Screen(..)));
            let mut k = 0;
            queue.retain(|a| {
                let keep = !matches!(a, Ask::Screen(..)) || Some(k) == last_screen;
                k += 1;
                keep
            });
            let Some(a) = queue.pop_front() else { continue };
            let heard = match a {
                Ask::State => {
                    let state = kb.call("state", Value::Null);
                    // The first time the keyboard is read, its lighting is handed back to
                    // its stored profile: a run of Ectodeck stopped short (killed, or
                    // crashed) while the Lighting page streamed a preview leaves it
                    // waiting for frames that never come.
                    if state.is_ok() && !handed_back {
                        handed_back = true;
                        let _ = kb.call("hand_back", Value::Null);
                    }
                    Heard::State(state)
                }
                Ask::Save(n, body) => Heard::Saved(n, kb.call("save", json!({ "n": n, "body": body }))),
                Ask::Activate(n) => Heard::Active(n, kb.profile("activate", json!({ "n": n }))),
                Ask::ReadActive => Heard::ActiveNow(kb.profile("active", Value::Null)),
                Ask::Sliders => Heard::Sliders(kb.call("sliders", Value::Null)),
                // previews go on a thread of their own (above)
                Ask::Preview(_) => continue,
                Ask::Dimmer(level) => {
                    let _ = kb.call("set_dimmer", json!({ "level": level }));
                    Heard::Done
                }
                Ask::Screen(light, apps) => {
                    // each until the keyboard says it took it: an answer can be lost to the
                    // screen's own frames going to it at the same moment
                    for _ in 0..4 {
                        if kb.flag("set_lcd_light", json!({ "brightness": light[0], "rgb": [light[1], light[2], light[3]] })).unwrap_or(false) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(15));
                    }
                    for _ in 0..4 {
                        if kb.flag("set_lcd_apps", json!({ "apps": apps })).unwrap_or(false) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(15));
                    }
                    Heard::Done
                }
                Ask::WriteMacro(n, body) => {
                    let delete = body.get("delete").is_some();
                    Heard::Macros(delete, kb.call("write_macro", json!({ "index": n, "body": body })))
                }
                Ask::SaveKeys(n, body) => Heard::KeysSaved(n, kb.call("save", json!({ "n": n, "body": body }))),
                Ask::Modes(quiet, keep) => {
                    let _ = kb.call("set_modes", json!({ "quiet_volume": quiet, "keep_dimmer": keep }));
                    Heard::Done
                }
            };
            if tell.send(heard).is_err() {
                break;
            }
        }
        let _ = kb.call("release", Value::Null);
    });
    ask
}
