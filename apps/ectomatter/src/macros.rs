//! The Macro Editor, as the web app had it (app.js at cd9ea82, the macros
//! section): the library of the keyboard's 100 macro slots, and the chosen
//! macro as a chain of steps, recorded from the keyboard or built by hand. A
//! step is the core protocol's {key|media, down, after}: a key or media key,
//! by its name, going down or up, then how long to wait before the next step.
//! One of Ectomatter's own macros can also do one of Ectomatter's jobs as a step,
//! {job, after}: a device's profile switched, a screen job, or another of its
//! macros played (Grant: "build it into the macro editor instead to allow
//! adding actions and other macros").

use std::collections::HashMap;

use ectomatter_protocol::{keys as hid, Action};
use serde_json::{json, Value};

use crate::lighting::cell;
use crate::text;
use crate::keyboard::Page;
use crate::Cell;

pub use ectomatter_protocol::Plays as How;

/// The ways a macro plays on its keys, as the Plays menu lists them (a number
/// of times is one choice, whatever the number).
pub const PLAYS: [(How, &str); 4] = [(How::Once, "Play once"), (How::Times(2), "Play a number of times"), (How::WhileHeld, "Repeat while held"), (How::UntilPressedAgain, "Repeat until pressed again")];

/// Whether two ways of playing are the same choice in the Plays menu.
pub fn same_choice(a: How, b: How) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// A way of playing, with `count` its number of times if it plays a number of times.
pub fn with_count(h: How, count: u32) -> How {
    match h {
        How::Times(_) => How::Times(count),
        h => h,
    }
}
pub const MEDIA: [(u32, &str); 16] = [
    (0xB5, "Next track"), (0xB6, "Previous track"), (0xB7, "Stop"), (0xCD, "Play/pause"), (0xE2, "Mute"), (0xE9, "Volume up"),
    (0xEA, "Volume down"), (0x183, "Media player"), (0x18A, "Mail"), (0x192, "Calculator"), (0x194, "My computer"),
    (0x221, "Browser search"), (0x223, "Browser home"), (0x224, "Browser back"), (0x225, "Browser forward"), (0x227, "Browser refresh"),
];

/// The keys a macro's step can press, by the protocol's names: the modifiers,
/// then each key the menu lists, as (words for it, the step's key).
pub fn step_keys() -> (Vec<(String, Value)>, Vec<(String, Value)>) {
    let mods = hid::MODIFIERS.iter().map(|(name, label, _)| (label.to_string(), json!(name))).collect();
    let keys = (1u8..104).filter_map(|u| crate::keyboard::hid_name(u as u32).map(|label| (label, hid::name(u).map_or(json!(u), |n| json!(n))))).collect();
    (mods, keys)
}

pub use crate::keyboard::step_name;

fn is_media(st: &Value) -> bool {
    st.get("media").is_some()
}

/// A step that does one of Ectomatter's jobs (jobs.rs), by its id.
pub fn is_job(st: &Value) -> bool {
    st.get("job").is_some()
}

pub fn job_of(st: &Value) -> Option<crate::jobs::Job> {
    st.get("job").and_then(Value::as_str).and_then(crate::jobs::Job::of)
}

/// A step in words: its key's or media key's name, or its job's, as the
/// jobs' names say it now.
pub fn step_words(st: &Value, names: &crate::jobs::Names) -> String {
    match job_of(st) {
        Some(j) => names.of(&j),
        None if is_job(st) => "An action Ectomatter no longer knows".into(),
        None => step_name(st),
    }
}

/// A step that plays another of Ectomatter's macros, and which.
pub fn macro_of(st: &Value) -> Option<u32> {
    match job_of(st) {
        Some(crate::jobs::Job::Macro { n, .. }) => Some(n),
        _ => None,
    }
}

/// Whether Ectomatter's macro `n` plays `target` among its steps, itself or
/// through the macros it plays (`seen` keeps a loop from going round).
pub fn plays_macro(n: u32, target: u32, seen: &mut Vec<u32>) -> bool {
    if n == target {
        return true;
    }
    if seen.contains(&n) {
        return false;
    }
    seen.push(n);
    let Some(m) = crate::library::get(n) else { return false };
    m["steps"].as_array().is_some_and(|st| st.iter().filter_map(macro_of).any(|k| plays_macro(k, target, seen)))
}

/// A macro's steps as Ectomatter's player plays them: each other macro it plays
/// put in its place, as deep as they go, its wait after carried onto the last
/// of what it put in; one that would play a macro already being played (a
/// macro playing itself, directly or through others) is left out where it
/// would be. What is left is keys, waits and jobs.
pub fn flatten(steps: &[Value], playing: &mut Vec<u32>) -> Vec<Value> {
    let mut out = Vec::new();
    for st in steps {
        let Some(n) = macro_of(st) else {
            out.push(st.clone());
            continue;
        };
        let Some(m) = crate::library::get(n).filter(|_| !playing.contains(&n)) else { continue };
        playing.push(n);
        let at = out.len();
        out.extend(flatten(m["steps"].as_array().map_or(&[][..], |v| v.as_slice()), playing));
        playing.pop();
        if delay(st) > 0 && out.len() > at {
            let last = out.len() - 1;
            out[last]["after"] = json!(delay(&out[last]) + delay(st));
        }
    }
    out
}
fn step_key(st: &Value) -> String {
    if is_media(st) { format!("m{}", st["media"]) } else { format!("k{}", st["key"]) }
}
fn down(st: &Value) -> bool {
    st["down"].as_bool().unwrap_or(false)
}
fn delay(st: &Value) -> u64 {
    st["after"].as_u64().unwrap_or(0)
}
/// What a macro's steps cost of a keyboard's room, at its costs.
pub fn bytes(steps: &[Value], costs: crate::recording::Costs) -> usize {
    steps.iter().map(|s| costs.of_step(s)).sum()
}

/// A macro in a few words: the keys it presses together, then each job it
/// does, in order, and whether it waits between its steps.
pub fn describe(steps: &[Value], names: &crate::jobs::Names) -> String {
    if steps.is_empty() {
        return "No steps yet".into();
    }
    let mut held: Vec<String> = Vec::new();
    for s in steps.iter().filter(|s| down(s)) {
        let n = step_name(s);
        if !held.contains(&n) {
            held.push(n);
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if !held.is_empty() {
        parts.push(held.join(" + "));
    }
    parts.extend(steps.iter().filter(|s| is_job(s)).map(|s| step_words(s, names)));
    parts.join(", ") + if steps.iter().any(|s| delay(s) > 0) { ", with timing" } else { "" }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Timing {
    Keep,
    None,
    Same,
}

/// A macro being edited.
#[derive(Clone, Debug)]
pub struct Draft {
    pub name: String,
    pub blank_name: Option<String>,
    pub steps: Vec<Value>,
    pub is_new: bool,
    pub starter: bool,
    pub timing: Timing,
    pub same: u64,
    /// how it plays, set here to be written to its keys, and the number of
    /// times it would play a number of times
    pub plays: Option<(How, u32)>,
}

/// How a macro plays on its keys: one way (with the number of times it would
/// play a number of times), or it varies by key.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Plays {
    One(How, u32),
    Varies,
}

pub struct Recording {
    pub steps: Vec<Value>,
    pub used: usize,
    pub elapsed: f64,
}

/// Which macros an editor shows: the keyboard's, kept in its own memory (100
/// of them, which its keys play themselves), or Ectomatter's (library.rs), as
/// many as you want, which Ectomatter plays for a control given one, as the
/// mouse's buttons can be.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lib {
    Keyboard,
    Ectomatter,
}

/// Where one of Ectomatter's macros runs: a profile of a device's, by its place
/// in the device's list, the control (its words, as the macro's delete
/// question names it), and how it plays there.
#[derive(Clone, Debug, PartialEq)]
pub struct Elsewhere {
    pub profile: u8,
    pub control: String,
    pub at: u32,
    pub n: u32,
    pub plays: How,
}

pub struct Editor {
    pub lib: Lib,
    /// where Ectomatter's macros run (the mouse's profiles' buttons), as the page
    /// last said, and the names of those profiles by their places
    pub elsewhere: Vec<Elsewhere>,
    pub names: Vec<String>,
    /// the words Ectomatter's jobs are said by (the devices' profiles, its
    /// macros), as the page last said them, for the steps that do one
    pub jobs: crate::jobs::Names,
    pub drafts: HashMap<u32, Draft>,
    pub current: Option<u32>,
    pub sel_step: Option<usize>,
    pub ins: Option<usize>,
    pub edit_name: bool,
    pub edit_wait: Option<usize>,
    pub rec: Option<Recording>,
    pub record_waits: bool,
    pub busy: bool,
}

impl Editor {
    pub fn new(lib: Lib) -> Editor {
        Editor {
            lib,
            elsewhere: Vec::new(),
            names: Vec::new(),
            jobs: crate::jobs::Names::default(),
            drafts: HashMap::new(),
            current: None,
            sel_step: None,
            ins: None,
            edit_name: false,
            edit_wait: None,
            rec: None,
            record_waits: crate::uistate::get("record_waits").and_then(|v| v.as_bool()).unwrap_or(true),
            busy: false,
        }
    }

    fn macros(&self, page: &Page) -> Value {
        match self.lib {
            Lib::Keyboard => page.dev.as_ref().map_or(Value::Null, |d| d["macros"].clone()),
            Lib::Ectomatter => crate::library::all(),
        }
    }

    pub fn saved(&self, page: &Page, n: u32) -> Option<Value> {
        let m = match self.lib {
            Lib::Keyboard => page.dev.as_ref()?["macros"][n.to_string()].clone(),
            Lib::Ectomatter => crate::library::get(n)?,
        };
        m.is_object().then_some(m)
    }

    pub fn macros_count(&self, page: &Page) -> usize {
        self.macros(page).as_object().map_or(0, |m| m.len())
    }

    /// How many macros the library holds, or none for no limit.
    pub fn room(&self) -> Option<u32> {
        (self.lib == Lib::Keyboard).then_some(100)
    }

    pub fn free_slots(&self, page: &Page) -> Vec<u32> {
        match self.lib {
            Lib::Keyboard => (1..=100).filter(|i| self.saved(page, *i).is_none() && !self.drafts.contains_key(i)).collect(),
            // (as many as there are drafts, and one more: there's no limit)
            Lib::Ectomatter => crate::library::free(self.drafts.len() + 1).into_iter().filter(|i| !self.drafts.contains_key(i)).collect(),
        }
    }

    /// The draft of a macro, made from the library's copy the first time.
    pub fn draft(&mut self, page: &Page, n: u32) -> Option<&mut Draft> {
        if !self.drafts.contains_key(&n) {
            let m = self.saved(page, n)?;
            self.drafts.insert(
                n,
                Draft {
                    name: m["name"].as_str().unwrap_or("").into(),
                    blank_name: None,
                    steps: m["steps"].as_array().cloned().unwrap_or_default(),
                    is_new: false,
                    starter: false,
                    timing: Timing::Keep,
                    same: 20,
                    plays: None,
                },
            );
        }
        self.drafts.get_mut(&n)
    }

    /// The steps as they will be saved, with the timing choice applied.
    pub fn effective(d: &Draft) -> Vec<Value> {
        let mut st = d.steps.clone();
        let n = st.len();
        match d.timing {
            Timing::None => st.iter_mut().for_each(|s| s["after"] = json!(0)),
            Timing::Same => st.iter_mut().enumerate().for_each(|(i, s)| s["after"] = json!(if i + 1 < n { d.same } else { 0 })),
            Timing::Keep => {}
        }
        st
    }

    fn pristine(d: &Draft) -> bool {
        d.is_new && d.blank_name.as_deref() == Some(d.name.as_str()) && d.steps.is_empty() && d.plays.is_none()
    }

    pub fn dirty(&self, page: &Page, n: u32) -> bool {
        let Some(d) = self.drafts.get(&n) else { return false };
        if d.is_new {
            return !Self::pristine(d);
        }
        let Some(m) = self.saved(page, n) else { return false };
        d.name != m["name"].as_str().unwrap_or("") || Value::Array(Self::effective(d)) != m["steps"] || d.plays.is_some()
    }

    /// The ways macro `n` plays on each key that runs it, on any profile and layer,
    /// with the key's place and layer and the profile: the keyboard's keys for
    /// its own macros, the controls the page said for Ectomatter's.
    fn runs(&self, page: &Page, n: u32) -> Vec<(u8, String, u32, How)> {
        if self.lib == Lib::Ectomatter {
            return self.elsewhere.iter().filter(|e| e.n == n).map(|e| (e.profile, e.control.clone(), e.at, e.plays)).collect();
        }
        let mut out = Vec::new();
        for p in crate::cores::profiles() {
            for layer in ["base", "fn"] {
                for pos in 0..crate::cores::places() as u32 {
                    let v = page.drafts.get(&p).map(|d| &d["keys"][layer][page.id(pos)]);
                    if let Some(Action::Macro { n: m, plays }) = v.and_then(Action::read) {
                        if m == n {
                            out.push((p, layer.to_string(), pos, plays));
                        }
                    }
                }
            }
        }
        out
    }

    /// Where a macro is assigned, one entry per key and layer, with its profiles.
    pub fn places(&self, page: &Page, n: u32) -> Vec<(String, u32, Vec<u8>)> {
        let mut out: Vec<(String, u32, Vec<u8>)> = Vec::new();
        for (p, layer, pos, _) in self.runs(page, n) {
            match out.iter_mut().find(|(l, q, _)| *l == layer && *q == pos) {
                Some(e) => e.2.push(p),
                None => out.push((layer, pos, vec![p])),
            }
        }
        out
    }

    pub fn plays_of(&self, page: &Page, n: u32) -> Option<Plays> {
        let all: Vec<How> = self.runs(page, n).into_iter().map(|r| r.3).collect();
        let first = *all.first()?;
        // (a number of times is the same way only at the same number)
        if all.iter().any(|h| !same_choice(*h, first) || matches!((h, first), (How::Times(a), How::Times(b)) if *a != b)) {
            return Some(Plays::Varies);
        }
        let c = match first {
            How::Times(c) if c != 0 => c,
            _ => 2,
        };
        Some(Plays::One(with_count(first, c), c))
    }

    fn new_name(&self, page: &Page) -> String {
        let used: Vec<String> = self.macros(page).as_object().map(|m| m.values().filter_map(|v| v["name"].as_str().map(String::from)).collect()).unwrap_or_default();
        let mut k = 1;
        loop {
            let n = format!("New Macro {k}");
            if !used.contains(&n) && !self.drafts.values().any(|d| d.name == n) {
                return n;
            }
            k += 1;
        }
    }

    /// The blank macro the editor opens on, kept in one free slot until it is used.
    pub fn starter(&mut self, page: &Page) -> Option<u32> {
        if let Some((n, _)) = self.drafts.iter().find(|(_, d)| d.starter) {
            return Some(*n);
        }
        let n = *self.free_slots(page).first()?;
        let name = self.new_name(page);
        self.drafts.insert(n, Draft { name: name.clone(), blank_name: Some(name), steps: Vec::new(), is_new: true, starter: true, timing: Timing::Keep, same: 20, plays: None });
        Some(n)
    }

    /// Make sure a macro is chosen: with none, the blank one.
    pub fn settle(&mut self, page: &Page) {
        let ok = self.current.is_some_and(|n| self.saved(page, n).is_some() || self.drafts.contains_key(&n));
        if !ok {
            self.current = self.starter(page);
        }
    }

    pub fn new_macro(&mut self, page: &Page) {
        if let Some(n) = self.current {
            if let Some(d) = self.drafts.get_mut(&n) {
                if d.starter {
                    d.starter = false;
                    self.sel_step = None;
                    self.ins = None;
                    return;
                }
            }
        }
        let Some(n) = self.free_slots(page).first().copied() else { return };
        let name = self.new_name(page);
        self.drafts.insert(n, Draft { name: name.clone(), blank_name: Some(name), steps: Vec::new(), is_new: true, starter: false, timing: Timing::Keep, same: 20, plays: None });
        self.current = Some(n);
        self.sel_step = None;
        self.ins = None;
    }

    /// The library: every saved macro, then the new ones something was put in.
    pub fn list(&self, page: &Page) -> Vec<u32> {
        let mut saved: Vec<u32> = self.macros(page).as_object().map(|m| m.keys().filter_map(|k| k.parse().ok()).collect()).unwrap_or_default();
        saved.sort();
        let mut drafts: Vec<u32> = self.drafts.iter().filter(|(_, d)| d.is_new && !(d.starter && Self::pristine(d))).map(|(n, _)| *n).collect();
        drafts.sort();
        saved.extend(drafts);
        saved
    }

    /// The timing choice becomes the steps' own waits the moment one is edited by hand.
    fn bake(d: &mut Draft) {
        if d.timing != Timing::Keep {
            d.steps = Self::effective(d);
            d.timing = Timing::Keep;
        }
    }

    pub fn insert_at(&mut self, page: &Page, steps: Vec<Value>) {
        let ins = self.ins;
        let Some(n) = self.current else { return };
        let Some(d) = self.draft(page, n) else { return };
        Self::bake(d);
        let i = ins.unwrap_or(d.steps.len()).min(d.steps.len());
        let k = steps.len();
        for (j, s) in steps.into_iter().enumerate() {
            d.steps.insert(i + j, s);
        }
        self.ins = ins.map(|_| i + k);
        self.sel_step = None;
    }

    /// A key added by hand goes down and comes back up, so it can never be left held.
    pub fn insert_tap(&mut self, page: &Page, v: &Value) {
        let mut a = v.clone();
        a["down"] = json!(true);
        a["after"] = json!(0);
        let mut b = v.clone();
        b["down"] = json!(false);
        b["after"] = json!(0);
        self.insert_at(page, vec![a, b]);
    }

    /// One of Ectomatter's jobs added as a step, by its id: done once, as the
    /// macro reaches it.
    pub fn insert_job(&mut self, page: &Page, id: &str) {
        self.insert_at(page, vec![json!({ "job": id, "after": 0 })]);
    }

    pub fn insert_wait(&mut self, page: &Page) {
        let ins = self.ins;
        let Some(n) = self.current else { return };
        let Some(d) = self.draft(page, n) else { return };
        Self::bake(d);
        let i = ins.unwrap_or(d.steps.len());
        if i == 0 {
            return;
        }
        let i = i - 1;
        if delay(&d.steps[i]) == 0 {
            d.steps[i]["after"] = json!(100);
        }
        self.edit_wait = Some(i);
    }

    pub fn set_wait(&mut self, page: &Page, i: usize, v: &str) {
        if self.edit_wait != Some(i) {
            return;
        }
        let Some(n) = self.current else { return };
        if let Some(d) = self.draft(page, n) {
            Self::bake(d);
            let ms: u64 = v.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            if let Some(s) = d.steps.get_mut(i) {
                s["after"] = json!(ms.min(65535));
            }
        }
        self.edit_wait = None;
    }

    pub fn start_wait_edit(&mut self, page: &Page, i: usize) {
        let Some(n) = self.current else { return };
        if let Some(d) = self.draft(page, n) {
            Self::bake(d);
        }
        self.edit_wait = Some(i);
    }

    pub fn move_step(&mut self, page: &Page, from: usize, to: usize) {
        let Some(n) = self.current else { return };
        let Some(d) = self.draft(page, n) else { return };
        Self::bake(d);
        if from == to || from >= d.steps.len() {
            return;
        }
        let st = d.steps.remove(from);
        let to = if from < to { to - 1 } else { to };
        d.steps.insert(to.min(d.steps.len()), st);
        self.sel_step = None;
        self.ins = None;
    }

    pub fn commit_name(&mut self, page: &Page, v: &str) {
        if !self.edit_name {
            return;
        }
        let Some(n) = self.current else { return };
        if let Some(d) = self.draft(page, n) {
            let t: String = v.trim().chars().take(50).collect();
            if !t.is_empty() {
                d.name = t;
            }
        }
        self.edit_name = false;
    }

    pub fn revert(&mut self, page: &Page) {
        let Some(n) = self.current else { return };
        if let Some(d) = self.drafts.get_mut(&n) {
            if d.is_new {
                d.steps.clear();
                d.timing = Timing::Keep;
                d.plays = None;
            } else {
                self.drafts.remove(&n);
            }
        }
        let _ = page;
        self.sel_step = None;
        self.ins = None;
        self.edit_wait = None;
    }

    pub fn duplicate(&mut self, page: &Page) -> Result<(), String> {
        let Some(n) = self.current else { return Ok(()) };
        let Some(k) = self.free_slots(page).first().copied() else { return Err("All 100 of the keyboard's macro slots are full.".into()) };
        let Some(d) = self.draft(page, n).cloned() else { return Ok(()) };
        self.drafts.insert(k, Draft { name: format!("Copy of {}", d.name).chars().take(50).collect(), blank_name: None, steps: Self::effective(&d), is_new: true, starter: false, timing: Timing::Keep, same: 20, plays: None });
        self.current = Some(k);
        self.sel_step = None;
        self.ins = None;
        Ok(())
    }

    /// What is saved: the steps with the timing applied, and anything pressed
    /// and never let go let go at the end, so the macro can never leave a key held.
    pub fn to_save(d: &Draft) -> (Vec<Value>, Vec<Value>) {
        let mut steps = Self::effective(d);
        let mut held: Vec<Value> = Vec::new();
        for st in &steps {
            if down(st) {
                held.push(st.clone());
            } else if let Some(j) = held.iter().rposition(|h| step_key(h) == step_key(st)) {
                held.remove(j);
            }
        }
        let added: Vec<Value> = held
            .iter()
            .rev()
            .map(|h| if is_media(h) { json!({ "media": h["media"], "down": false, "after": 0 }) } else { json!({ "key": h["key"], "down": false, "after": 0 }) })
            .collect();
        steps.extend(added.iter().cloned());
        (steps, added)
    }
}

/// The chain's pieces' kinds, as the page draws them
pub mod piece {
    pub const STEP: i32 = 15;
    pub const WAIT: i32 = 16;
    pub const JOINT: i32 = 17;
    pub const INS: i32 = 18;
    pub const CARET: i32 = 19;
    pub const WAIT_EDIT: i32 = 20;
}
/// what a click on a piece means
pub mod hit {
    pub const STEP: i32 = 1;
    pub const JOINT: i32 = 2;
    pub const WAIT: i32 = 3;
    pub const END: i32 = 4;
}

/// The chain laid out as the web page's flex row laid it: wrapping at the
/// canvas's edge, 12 between lines, each line as tall as a step and every
/// piece in its middle.
pub fn chain(ed: &Editor, d: &Draft, locked: bool, width: f32) -> (Vec<Cell>, f32) {
    let eff = Editor::effective(d);
    let rec = ed.rec.as_ref();
    let live: &[Value] = rec.map_or(&[], |r| r.steps.as_slice());
    let ins = ed.ins.unwrap_or(d.steps.len());
    let mut cells: Vec<Cell> = Vec::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let line = 29.0;
    let place = |cells: &mut Vec<Cell>, mut c: Cell, x: &mut f32, y: &mut f32, lead: f32| {
        if *x > 0.0 && *x + lead + c.w > width {
            *x = 0.0;
            *y += line + 12.0;
        }
        *x += lead;
        // (the chain Part.chain-in in from its canvas, which keeps the same room at its foot)
        c.x = crate::tokens::part::CHAIN_IN + *x;
        c.y = crate::tokens::part::CHAIN_IN + *y + (line - c.h) / 2.0;
        *x += c.w;
        cells.push(c);
    };
    // (a key's step has its name and which way it goes; a media key's, a
    // note before it; a job's, its own mark and no way to go: 2 a job on a
    // device or the screen, 3 another macro played)
    let step = |st: &Value, i: usize, on: bool, id: i32| -> Cell {
        let media = is_media(st);
        let name = step_words(st, &ed.jobs);
        let style = if macro_of(st).is_some() { 3 } else if is_job(st) { 2 } else if media { 1 } else { 0 };
        let mark = match style {
            1 => text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "♫") + 6.0,
            2 => text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "◆") + 6.0,
            3 => text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "▶") + 6.0,
            _ => 0.0,
        };
        let way = if style >= 2 { 0.0 } else { 6.0 + text::width(crate::tokens::font::MEDIUM, crate::tokens::font::NOTE, if down(st) { "↓" } else { "↑" }) };
        let w = 11.0 + mark + text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, &name) + way + 11.0;
        Cell { kind: piece::STEP, text: name.into(), on, value: if down(st) { 1.0 } else { 0.0 }, style, id, index: i as i32, ..cell(0, 0.0, 0.0, w, line) }
    };
    let wait = |ms: u64, i: usize, editing: bool, id: i32| -> Cell {
        if editing {
            return Cell { kind: piece::WAIT_EDIT, text: ms.to_string().into(), id, index: i as i32, ..cell(0, 0.0, 0.0, 92.0, 25.0) };
        }
        let t = format!("{} ms", group(ms));
        let w = 8.0 + text::width(crate::tokens::font::MEDIUM, crate::tokens::font::NOTE, &t) + 4.0 + 12.0 + 8.0;
        Cell { kind: piece::WAIT, text: t.into(), id, index: i as i32, ..cell(0, 0.0, 0.0, w, 24.5) }
    };
    let joint = |i: usize, hot: bool, plain: bool| Cell { kind: piece::JOINT, on: hot, style: if plain { 1 } else { 0 }, id: hit::JOINT, index: i as i32, ..cell(0, 0.0, 0.0, 20.0, 18.0) };
    let live_chain = |cells: &mut Vec<Cell>, x: &mut f32, y: &mut f32| {
        for (j, st) in live.iter().enumerate() {
            place(cells, joint(0, false, true), x, y, 0.0);
            place(cells, step(st, 1000 + j, false, 0), x, y, 0.0);
            if delay(st) > 0 {
                place(cells, joint(0, false, true), x, y, 0.0);
                place(cells, wait(delay(st), 1000 + j, false, 0), x, y, 0.0);
            }
        }
    };
    if eff.is_empty() && live.is_empty() {
        return (cells, 0.0);
    }
    let off = locked || rec.is_some();
    for (i, st) in eff.iter().enumerate() {
        if i > 0 {
            let hot = ed.ins == Some(i) && i < d.steps.len();
            place(&mut cells, joint(i, hot, off), &mut x, &mut y, 0.0);
        }
        if rec.is_some() && i == ins {
            live_chain(&mut cells, &mut x, &mut y);
        }
        place(&mut cells, step(st, i, ed.sel_step == Some(i), if off { 0 } else { hit::STEP }), &mut x, &mut y, 0.0);
        if delay(st) > 0 {
            place(&mut cells, joint(i, false, true), &mut x, &mut y, 0.0);
            place(&mut cells, wait(delay(st), i, ed.edit_wait == Some(i), if off { 0 } else { hit::WAIT }), &mut x, &mut y, 0.0);
        }
    }
    if rec.is_some() && ins >= eff.len() {
        live_chain(&mut cells, &mut x, &mut y);
    }
    if rec.is_some() {
        place(&mut cells, Cell { kind: piece::CARET, ..cell(0, 0.0, 0.0, 2.0, 26.0) }, &mut x, &mut y, 6.0);
    } else if !locked {
        let on = ed.ins.is_none() || ed.ins.is_some_and(|i| i >= eff.len());
        place(&mut cells, Cell { kind: piece::INS, on, id: hit::END, ..cell(0, 0.0, 0.0, 2.0, 22.0) }, &mut x, &mut y, 8.0);
    }
    (cells, y + line)
}

/// thousands grouped, as the page's locale grouped them
fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn clock(s: f64) -> String {
    format!("{}:{:04.1}", (s / 60.0).floor(), s % 60.0)
}

/// The profiles a key does something on, by their names (`name` gives each one's), as a
/// phrase in a sentence: "all profiles", "Games", or "Games, Work and Profile 3".
pub fn profiles_text(ps: &[u8], name: impl Fn(u8) -> String) -> String {
    let names: Vec<String> = ps.iter().map(|p| name(*p)).collect();
    match names.len() {
        5 => "all profiles".into(),
        0 => String::new(),
        1 => names[0].clone(),
        n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A macro with keys and jobs in a few words: the keys it presses
    /// together, then each job, as the devices' profiles are called now.
    #[test]
    fn a_macro_with_jobs_says_them() {
        let device = |key: &str, name: &str, id: &str, profile: &str| crate::jobs::Profiled { key: key.into(), name: name.into(), profiles: vec![(id.into(), profile.into())], steps: true, screen: false, here: true, kind: String::new() };
        let names = crate::jobs::Names { devices: vec![device("z10", "EVGA Z10 RGB", "2", "FPS Gaming"), device("sensei310", "SteelSeries Sensei 310", "pa", "Gaming")], ..Default::default() };
        let k = |key: &str, down: bool| json!({ "key": key, "down": down, "after": 0 });
        let steps = vec![k("LeftCtrl", true), k("T", true), k("T", false), k("LeftCtrl", false), json!({ "job": "profile:z10:2", "after": 0 }), json!({ "job": "profile:sensei310:pa", "after": 0 })];
        assert_eq!(describe(&steps, &names), "Left Ctrl + T, EVGA Z10 RGB: FPS Gaming, SteelSeries Sensei 310: Gaming");
        assert_eq!(describe(&steps[4..], &names), "EVGA Z10 RGB: FPS Gaming, SteelSeries Sensei 310: Gaming");
        assert_eq!(step_words(&json!({ "job": "profile:sensei310:gone" }), &names), "SteelSeries Sensei 310: a deleted profile");
        assert_eq!(step_words(&json!({ "job": "nonsense" }), &names), "An action Ectomatter no longer knows");
    }

    /// The macros a macro plays are put in their places for the player, as
    /// deep as they go, each one's wait carried onto what it put in; one
    /// that would play a macro already playing (itself, directly or through
    /// another) is left out, and nothing loops.
    #[test]
    fn a_macro_plays_the_macros_in_it_but_never_itself() {
        let _one = crate::library::TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let k = |key: &str, down: bool| json!({ "key": key, "down": down, "after": 0 });
        let m = |n: u32, after: u64| json!({ "job": format!("macro:{n}:once"), "after": after });
        crate::library::set(61, "A", vec![k("A", true), k("A", false)]);
        crate::library::set(62, "A then a profile", vec![m(61, 30), json!({ "job": "profile:z10:2", "after": 0 })]);
        crate::library::set(63, "Itself", vec![m(63, 0), k("C", true), k("C", false)]);
        crate::library::set(64, "One way round", vec![m(65, 0)]);
        crate::library::set(65, "The other way", vec![m(64, 0), k("B", true), k("B", false)]);
        let flat = |n: u32| flatten(crate::library::get(n).unwrap()["steps"].as_array().unwrap(), &mut vec![n]);
        assert_eq!(flat(62), [k("A", true), json!({ "key": "A", "down": false, "after": 30 }), json!({ "job": "profile:z10:2", "after": 0 })]);
        assert_eq!(flat(63), [k("C", true), k("C", false)], "it never plays itself");
        assert_eq!(flat(64), [k("B", true), k("B", false)], "nor round through another");
        assert!(plays_macro(62, 61, &mut Vec::new()) && !plays_macro(61, 62, &mut Vec::new()));
        assert!(plays_macro(64, 65, &mut Vec::new()) && plays_macro(65, 64, &mut Vec::new()));
        for n in 61..=65 {
            crate::library::delete(n);
        }
    }
}
