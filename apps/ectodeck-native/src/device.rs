//! The keyboard, on a thread of its own. Ectodeck's core does the talking,
//! which takes seconds for a whole read, and the window keeps drawing
//! meanwhile: the page asks, and hears back through the event loop.

use std::path::PathBuf;
use std::sync::mpsc;

use ectotype_core::{Conn, Keyboard};
use serde_json::Value;
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
    /// show a preview on the keyboard: an effect, each key's colour, or stop
    Preview(Value),
    /// hold the dimmer at a level, or give it back to the slider
    Dimmer(Option<u8>),
    /// ask the keyboard to stop doing a slider's job itself: (quiet volume, keep dimmer)
    Modes(bool, bool),
    /// save or delete a macro: its name, steps and play settings, or `delete: true`
    WriteMacro(u8, Value),
    /// save a profile's keys only, to change how a macro plays on them
    SaveKeys(u8, Value),
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

/// Start the keyboard's thread. `backups` is where the first write of a
/// session saves what it replaces, as the web app's does.
pub fn start(backups: PathBuf, tell: Sender<Heard>) -> mpsc::Sender<Ask> {
    let (ask, asked) = mpsc::channel::<Ask>();
    std::thread::spawn(move || {
        let kb = Keyboard::new(Conn::hid(), Some(backups));
        let mut queue: std::collections::VecDeque<Ask> = std::collections::VecDeque::new();
        loop {
            // Everything asked for so far: a slider reading goes first, so the
            // knobs keep up while the lighting streams, and of several previews
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
            let last_preview = queue.iter().rposition(|a| matches!(a, Ask::Preview(_)));
            let mut k = 0;
            queue.retain(|a| {
                let keep = !matches!(a, Ask::Preview(_)) || Some(k) == last_preview;
                k += 1;
                keep
            });
            let Some(a) = queue.pop_front() else { continue };
            let heard = match a {
                Ask::State => Heard::State(kb.state().map_err(|e| e.to_string())),
                Ask::Save(n, body) => Heard::Saved(n, kb.save(n, &body).map_err(|e| e.to_string())),
                Ask::Activate(n) => Heard::Active(n, kb.activate(n).map_err(|e| e.to_string())),
                Ask::ReadActive => Heard::ActiveNow(kb.active().map_err(|e| e.to_string())),
                Ask::Sliders => Heard::Sliders(kb.sliders().map_err(|e| e.to_string())),
                Ask::Preview(body) => Heard::Preview(kb.preview(&body).map_err(|e| e.to_string())),
                Ask::Dimmer(level) => {
                    let _ = kb.set_dimmer(level);
                    Heard::Done
                }
                Ask::WriteMacro(n, body) => {
                    let delete = body.get("delete").is_some();
                    Heard::Macros(delete, kb.write_macro(n, &body).map_err(|e| e.to_string()))
                }
                Ask::SaveKeys(n, body) => Heard::KeysSaved(n, kb.save(n, &body).map_err(|e| e.to_string())),
                Ask::Modes(quiet, keep) => {
                    let _ = kb.set_modes(quiet, keep);
                    Heard::Done
                }
            };
            if tell.send(heard).is_err() {
                break;
            }
        }
        kb.release();
    });
    ask
}
