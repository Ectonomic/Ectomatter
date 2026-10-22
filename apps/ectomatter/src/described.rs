//! A device's settings drawn from what its core says of them, in the shared
//! settings language (the core protocol's section 6): an effect's settings,
//! and the device's own by its layout's sections, each with the app's own
//! controls, so a device whose core describes them needs no code of its own
//! here. The mouse's Lighting and Performance boxes are built this way.
//!
//! An effect with a layout of its own (the keyboard's) is drawn in it: its
//! settings in groups, in its order, where and when it plays in a second
//! box, each setting read and changed where the device's profile keeps it.
//! One without (the mouse's) has its settings go colors first, then
//! choices, then amounts and speeds, then switches.

use serde_json::{Value, json};

use crate::lighting::{Ctl, Stack, hex_of, kind, style, text_cell};
use crate::Cell;

/// The ids the pieces carry: an effect's setting is EFFECT_IDS and its place
/// in the effect's list (see `params`), one of the device's own SETTING_IDS
/// and its place in the core's list of them.
pub const EFFECT_IDS: i32 = 1000;
pub const SETTING_IDS: i32 = 2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    Effect(usize),
    Setting(usize),
}

/// Which setting a piece belongs to, for a piece this module drew.
pub fn piece_of(id: i32) -> Option<Piece> {
    if id >= SETTING_IDS {
        Some(Piece::Setting((id - SETTING_IDS) as usize))
    } else if id >= EFFECT_IDS {
        Some(Piece::Effect((id - EFFECT_IDS) as usize))
    } else {
        None
    }
}

/// An effect's settings, each with its id, in the order they're drawn:
/// colors first, then choices, then amounts and speeds, then switches, each
/// kind in the order the core lists them; or, for an effect with a layout of
/// its own, in the order its layout shows them.
pub fn params(effect: &Value) -> Vec<(String, Value)> {
    let rank = |d: &Value| match d["type"].as_str().unwrap_or("") {
        "color" | "colors" => 0,
        "choice" => 1,
        "int" | "number" | "duration" => 2,
        "bool" => 3,
        _ => 4,
    };
    let mut v: Vec<(String, Value)> = effect["params"].as_object().map(|m| m.iter().map(|(k, d)| (k.clone(), d.clone())).collect()).unwrap_or_default();
    v.sort_by_key(|(_, d)| rank(d));
    let shown = shown(effect);
    if !shown.is_empty() {
        v.sort_by_key(|(id, _)| shown.iter().position(|s| s == id).unwrap_or(usize::MAX));
    }
    v
}

/// The settings an effect's layout places, in its order, shown or not.
fn shown(effect: &Value) -> Vec<String> {
    effect["layout"].as_array().into_iter().flatten().flat_map(|g| g["show"].as_array().into_iter().flatten().filter_map(|i| item_id(i).map(String::from))).collect()
}

/// A layout item's setting: its id, or an item's `id` (one with a `when`).
fn item_id(i: &Value) -> Option<&str> {
    i.as_str().or_else(|| i["id"].as_str())
}

/// Whether a layout's condition holds for an effect's settings (`e`, kept as
/// its description `ps` says): every setting it names has the value given, or
/// one in a list given. No condition always holds.
fn holds(when: &Value, ps: &[(String, Value)], e: &Value) -> bool {
    when.as_object().is_none_or(|w| {
        w.iter().all(|(k, want)| {
            let v = ps.iter().find(|(id, _)| id == k).map_or(Value::Null, |(id, d)| kept(e, id, d));
            match want {
                Value::Array(any) => any.contains(&v),
                one => v == *one,
            }
        })
    })
}

/// A setting's value: as the device has it, or what it plays while it has none.
pub fn value_of<'a>(values: &'a Value, id: &str, d: &'a Value) -> &'a Value {
    match &values[id] {
        Value::Null => &d["default"],
        v => v,
    }
}

/// The words for a setting: its label, or its id with a capital.
fn label(id: &str, d: &Value) -> String {
    d["label"].as_str().map(String::from).unwrap_or_else(|| {
        let mut c = id.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
    })
}

pub fn rgb(v: &Value) -> Option<[u8; 3]> {
    // (kept as its three numbers, as the keyboard keeps its colors)
    if let Some(a) = v.as_array() {
        return Some([0, 1, 2].map(|i| a.get(i).and_then(Value::as_u64).unwrap_or(0) as u8));
    }
    let h = v.as_str()?.trim_start_matches('#');
    let n = u32::from_str_radix(h.get(..6)?, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn colors(v: &Value) -> Vec<[u8; 3]> {
    v.as_array().map(|a| a.iter().map(|c| rgb(c).unwrap_or([0; 3])).collect()).unwrap_or_default()
}

/// The span a slider covers, and the step between its values.
fn span(d: &Value) -> (f64, f64, f64) {
    let min = d["min"].as_f64().unwrap_or(0.0);
    let max = d["max"].as_f64().unwrap_or(min + 1.0).max(min + f64::EPSILON);
    let step = d["step"].as_f64().unwrap_or(if d["type"] == "duration" { 100.0 } else if d["type"] == "number" { 0.0 } else { 1.0 });
    (min, max, step)
}

/// Whether a duration is how fast something goes: a speed's slider runs slow
/// to fast, as the keyboard's do, its value the time a cycle takes. A
/// setting says so (`speed`), or is called `speed`.
fn is_speed(id: &str) -> bool {
    id == "speed"
}
fn is_speed_of(id: &str, d: &Value) -> bool {
    d["type"] == "duration" && (d["speed"] == true || is_speed(id))
}

/// Where a slider stands for a value, from 0 to 1.
fn frac(id: &str, d: &Value, v: &Value) -> f32 {
    let (min, max, _) = span(d);
    let f = ((v.as_f64().unwrap_or(min) - min) / (max - min)).clamp(0.0, 1.0);
    (if d["type"] == "duration" && is_speed(id) { 1.0 - f } else { f }) as f32
}

/// A slider moved to `f`: the value it stands for now.
pub fn slid(id: &str, d: &Value, f: f32) -> Value {
    let (min, max, step) = span(d);
    let f = (f as f64).clamp(0.0, 1.0);
    let f = if d["type"] == "duration" && is_speed(id) { 1.0 - f } else { f };
    let v = min + f * (max - min);
    let v = if step > 0.0 { (min + ((v - min) / step).round() * step).min(max) } else { v };
    if d["type"] == "number" { json!(v) } else { json!(v.round() as i64) }
}

/// An option's value: the option itself, or an option with words of its
/// own (`label`, and an `icon`), its `value`.
fn option_value(o: &Value) -> &Value {
    match o.get("value") {
        Some(v) if o.is_object() => v,
        _ => o,
    }
}

/// A choice's options, in words: their own, or each value with its unit after it.
fn options(d: &Value) -> Vec<String> {
    let unit = d["unit"].as_str();
    d["options"]
        .as_array()
        .map(|o| {
            o.iter()
                .map(|v| {
                    if let Some(l) = v["label"].as_str() {
                        return l.to_string();
                    }
                    let w = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
                    unit.map_or(w.clone(), |u| format!("{w} {u}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Which option a choice is on: its value's, or its default's, or the last.
fn chosen(d: &Value, v: &Value) -> usize {
    let o = d["options"].as_array().cloned().unwrap_or_default();
    o.iter().position(|x| option_value(x) == v).or_else(|| o.iter().position(|x| *option_value(x) == d["default"])).unwrap_or(o.len().saturating_sub(1))
}

/// A piece picked at `index`: the setting's new value, for a choice or a
/// switch. A color isn't changed here: it opens the picker.
pub fn picked(d: &Value, v: &Value, index: usize) -> Option<Value> {
    match d["type"].as_str()? {
        "choice" => d["options"].get(index).map(|o| option_value(o).clone()),
        // (a checkbox tells the state it is turned to; one kept as a number, 1 or 0)
        "bool" if d["form"] == "number" => Some(json!(if index % 2 == 1 { 1 } else { 0 })),
        "bool" => Some(json!(index % 2 == 1)),
        _ => {
            let _ = v;
            None
        }
    }
}

/// The color a setting shows at `index`, for the picker to open on.
pub fn color_at(d: &Value, v: &Value, index: usize) -> Option<[u8; 3]> {
    match d["type"].as_str()? {
        "color" => rgb(v),
        "colors" => colors(v).get(index).copied(),
        _ => None,
    }
}

/// A setting with a color set in the picker: one color, or one of a list
/// (a list the device played by default taking those colors as its own).
pub fn with_color(d: &Value, v: &Value, index: usize, c: [u8; 3]) -> Value {
    let code = if d["form"] == "rgb" { json!(c) } else { json!(hex_of(c).to_uppercase()) };
    match d["type"].as_str() {
        Some("colors") => {
            let mut list = v.as_array().cloned().unwrap_or_default();
            if let Some(slot) = list.get_mut(index) {
                *slot = code;
            }
            Value::Array(list)
        }
        _ => code,
    }
}

/// A group's name in small capitals, as a box's groups are named.
fn small_caps(s: &mut Stack, name: &str) {
    s.cells.push(text_cell(style::GROUP, 16.0, s.y, s.inner(), 14.0, &name.to_uppercase()));
    s.gap(14.0 + 10.0);
}

/// The sliders' fill.
fn fill() -> slint::Color {
    slint::Color::from_rgb_u8(0xa1, 0xa1, 0xaa)
}

/// An effect's settings, under its note, into a box's stack. `params` are
/// the light's, and `open` the color the picker is open on: its setting's
/// place, and the color's.
pub fn draw_effect(s: &mut Stack, effect: &Value, params: &Value, open: Option<(usize, usize)>) {
    if let Some(n) = effect["note"].as_str() {
        s.note(n);
    }
    for (k, (id, d)) in self::params(effect).iter().enumerate() {
        let at = EFFECT_IDS + k as i32;
        let v = value_of(params, id, d);
        let open = open.filter(|(p, _)| *p == k).map(|(_, i)| i);
        match d["type"].as_str().unwrap_or("") {
            "color" => {
                s.gap(14.0);
                small_caps(s, &label(id, d));
                s.dots_at(at, &[rgb(v).unwrap_or([0; 3])], open, true);
            }
            "colors" => {
                // its colors as dots you can change, over the strip they blend along
                let cols = colors(v);
                s.gap(14.0);
                small_caps(s, &label(id, d));
                s.strip(&cols);
                s.dots_at(at, &cols, open, false);
            }
            "duration" => {
                let secs = v.as_f64().unwrap_or(0.0) / 1000.0;
                s.gap(18.0);
                if is_speed(id) {
                    s.head(&label(id, d), Some(&format!("{secs:.1} s a cycle")));
                    // quick at the right, as the keyboard's speed sliders run
                    s.slider_at(at, frac(id, d, v), fill(), Some(("Slow", "Fast")));
                } else {
                    s.head(&label(id, d), Some(&format!("{secs:.1} s")));
                    s.slider_at(at, frac(id, d, v), fill(), None);
                }
            }
            "int" | "number" => {
                s.gap(18.0);
                s.head(&label(id, d), Some(&amount(d, v)));
                s.slider_at(at, frac(id, d, v), fill(), None);
            }
            "choice" => {
                s.gap(18.0);
                s.head(&label(id, d), None);
                let words = options(d);
                let opts: Vec<(&str, &str)> = words.iter().map(|w| (w.as_str(), "")).collect();
                s.switch_at(at, &opts, chosen(d, v));
            }
            "bool" => {
                s.gap(18.0);
                s.check_at(at, v.as_bool().unwrap_or(false), &label(id, d));
            }
            _ => {}
        }
    }
}

/// An amount in words: with its unit.
fn amount(d: &Value, v: &Value) -> String {
    let (min, max, _) = span(d);
    let n = v.as_f64().unwrap_or(min).clamp(min, max);
    let n = if d["type"] == "number" { format!("{n:.2}") } else { format!("{}", n.round() as i64) };
    d["unit"].as_str().map_or(n.clone(), |u| format!("{n} {u}"))
}

/// One of the sections a device's settings are shown in: its title, its
/// settings by their places in the core's list, and its note.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub items: Vec<usize>,
    pub note: Option<String>,
}

/// The device's own settings in the sections its layout gives them (the
/// layout's `device`), a section's setting shown only when its `when` holds;
/// what no section shows comes last, in a section of its own, so a missing
/// or out-of-date layout never hides a setting.
pub fn sections(desc: &Value, values: &Value) -> Vec<Section> {
    let items = desc["capabilities"]["settings"]["items"].as_array().cloned().unwrap_or_default();
    let place = |id: &str| items.iter().position(|i| i["id"] == id);
    let holds = |when: &Value| {
        when.as_object().is_none_or(|w| {
            w.iter().all(|(k, want)| match want {
                Value::Array(any) => any.contains(&values[k]),
                one => values[k] == *one,
            })
        })
    };
    let mut out = Vec::new();
    let mut placed = vec![false; items.len()];
    for sec in desc["layout"]["device"].as_array().into_iter().flatten() {
        let mut shown = Vec::new();
        for item in sec["show"].as_array().into_iter().flatten() {
            let (id, when) = match item {
                Value::String(id) => (id.as_str(), &Value::Null),
                o => (o["id"].as_str().unwrap_or(""), &o["when"]),
            };
            if let Some(k) = place(id) {
                placed[k] = true;
                if holds(when) {
                    shown.push(k);
                }
            }
        }
        if !shown.is_empty() && holds(&sec["when"]) {
            out.push(Section { title: sec["section"].as_str().unwrap_or("").into(), items: shown, note: sec["note"].as_str().map(String::from) });
        }
    }
    let rest: Vec<usize> = (0..items.len()).filter(|k| !placed[*k]).collect();
    if !rest.is_empty() {
        out.push(Section { title: "More".into(), items: rest, note: None });
    }
    out
}

/// A section of the device's settings into a box's stack: each setting, the
/// section's note, and a word for each setting the device can't tell back.
/// `values` and `known` are the settings as the core last said them.
pub fn draw_section(s: &mut Stack, desc: &Value, sec: &Section, values: &Value, known: &Value) {
    let items = desc["capabilities"]["settings"]["items"].as_array().cloned().unwrap_or_default();
    let kind = desc["kind"].as_str().unwrap_or("device");
    let mut last_switch = false;
    for (j, &k) in sec.items.iter().enumerate() {
        let Some(d) = items.get(k) else { continue };
        let id = d["id"].as_str().unwrap_or("");
        let at = SETTING_IDS + k as i32;
        let v = value_of(values, id, d);
        if j > 0 {
            s.gap(22.0);
        }
        // (a setting the section is named for needs no name of its own)
        let words = d["short"].as_str().map(String::from).unwrap_or_else(|| label(id, d));
        let own_name = label(id, d) != sec.title;
        last_switch = false;
        match d["type"].as_str().unwrap_or("") {
            "int" | "number" | "duration" => {
                let shown = if d["type"] == "duration" { format!("{:.1} s", v.as_f64().unwrap_or(0.0) / 1000.0) } else { amount(d, v) };
                s.head(&words, Some(&shown));
                s.slider_at(at, frac(id, d, v), fill(), None);
            }
            "choice" => {
                if own_name {
                    s.head(&words, None);
                }
                let w = options(d);
                let opts: Vec<(&str, &str)> = w.iter().map(|w| (w.as_str(), "")).collect();
                // (one the device can't tell back: none chosen until one is)
                let unknown = known[id].as_bool() == Some(false);
                s.switch_at(at, &opts, if unknown { usize::MAX } else { chosen(d, v) });
                last_switch = true;
            }
            "bool" => s.check_at(at, v.as_bool().unwrap_or(false), &words),
            _ => {}
        }
    }
    if let Some(n) = &sec.note {
        s.gap(if last_switch { 12.0 } else { 18.0 });
        s.note(n);
    }
    for &k in &sec.items {
        let Some(d) = items.get(k) else { continue };
        let id = d["id"].as_str().unwrap_or("");
        if known[id].as_bool() == Some(false) {
            s.gap(6.0);
            let name = label(id, d).to_lowercase();
            if d["type"] == "choice" {
                s.note(&format!("The {kind} can’t tell Ectomatter its {name}. Choose one to set it."));
            } else {
                s.note(&format!("The {kind} can’t tell Ectomatter its {name}. Until you set it here, this shows the factory’s."));
            }
        }
    }
}

/// The setting at a place in the core's list: its id and description.
pub fn setting(desc: &Value, k: usize) -> Option<(String, Value)> {
    let d = desc["capabilities"]["settings"]["items"].get(k)?.clone();
    Some((d["id"].as_str()?.to_string(), d))
}

/// A light set to effect `e` (the core's description of it), keeping the
/// settings it had. An effect whose colors the light doesn't have yet starts
/// as the core says it does (`start`, the maker's own look) whole.
pub fn with_effect(old: &Value, e: &Value) -> Value {
    let mut params = match &old["params"] {
        Value::Object(o) => Value::Object(o.clone()),
        _ => json!({}),
    };
    if let Some(start) = e["start"].as_object() {
        let new_colors = self::params(e).iter().any(|(id, d)| matches!(d["type"].as_str(), Some("color" | "colors")) && start.contains_key(id) && params.get(id).is_none());
        if new_colors {
            for (k, v) in start {
                params[k] = v.clone();
            }
        }
    }
    // (a color the effect shows, which a light kept from another effect or else takes as the core's default)
    for (id, d) in self::params(e) {
        if d["type"] == "color" && params.get(&id).is_none() {
            params[&id] = d["default"].clone();
        }
    }
    json!({ "effect": e["id"], "params": params })
}

// ------------------------------------------------------------ an effect's own layout

/// A pointer's steps (`/colors/*/brightness`), a `*` standing for every
/// element of a list.
fn steps(at: &str) -> Vec<String> {
    at.split('/').filter(|s| !s.is_empty()).map(|s| s.replace("~1", "/").replace("~0", "~")).collect()
}

/// What a pointer names, the first that is kept where a `*` names several.
fn read<'a>(v: &'a Value, steps: &[String]) -> Option<&'a Value> {
    let Some((first, rest)) = steps.split_first() else { return Some(v) };
    if first == "*" {
        return v.as_array()?.iter().find_map(|x| read(x, rest));
    }
    let next = match v {
        Value::Object(o) => o.get(first)?,
        Value::Array(a) => a.get(first.parse::<usize>().ok()?)?,
        _ => return None,
    };
    read(next, rest)
}

/// `x` written at every place a pointer names whose parent is kept (a `*`
/// every element of its list); the last step made where it isn't yet.
fn write(v: &mut Value, steps: &[String], x: &Value) {
    let Some((first, rest)) = steps.split_first() else {
        *v = x.clone();
        return;
    };
    if first == "*" {
        for e in v.as_array_mut().into_iter().flatten() {
            write(e, rest, x);
        }
        return;
    }
    if rest.is_empty() {
        match v {
            Value::Object(o) => {
                o.insert(first.clone(), x.clone());
            }
            Value::Array(a) => {
                if let Some(slot) = first.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                    *slot = x.clone();
                }
            }
            Value::Null => *v = json!({ first.as_str(): x }),
            _ => {}
        }
        return;
    }
    let next = match v {
        Value::Object(o) => o.get_mut(first),
        Value::Array(a) => first.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
        _ => None,
    };
    if let Some(n) = next {
        write(n, rest, x);
    }
}

/// What a pointer names in a whole profile, and changed there, for where a
/// device's profile keeps a light's effect and settings (the lighting's `kept`).
pub fn read_at<'a>(v: &'a Value, at: &str) -> Option<&'a Value> {
    read(v, &steps(at))
}
pub fn write_at(v: &mut Value, at: &str, x: &Value) {
    write(v, &steps(at), x);
}

/// A setting's value where the effect's own settings (`e`) keep it: at its
/// `at` (or its id), the items of a list (`item`, the first `count` of them),
/// or several `fields` as one list; its default where nothing is kept.
pub fn kept(e: &Value, id: &str, d: &Value) -> Value {
    if let Some(fields) = d["fields"].as_array() {
        return Value::Array(fields.iter().enumerate().map(|(i, f)| read(e, &steps(f.as_str().unwrap_or(""))).filter(|v| !v.is_null()).cloned().unwrap_or_else(|| d["default"][i].clone())).collect());
    }
    let at = d["at"].as_str().map(String::from).unwrap_or_else(|| format!("/{id}"));
    let v = read(e, &steps(&at)).cloned().unwrap_or(Value::Null);
    if let (Some(item), Value::Array(list)) = (d["item"].as_str(), &v) {
        let n = d["count"].as_u64().map_or(list.len(), |n| n as usize);
        return Value::Array(list.iter().take(n).map(|x| read(x, &steps(item)).cloned().unwrap_or(Value::Null)).collect());
    }
    if v.is_null() { d["default"].clone() } else { v }
}

/// A setting changed where the effect's own settings keep it: at every place
/// its `at` names, and at its `also` where those are kept; several `fields`
/// each from its place in a list.
pub fn keep(e: &mut Value, id: &str, d: &Value, v: &Value) {
    if let Some(fields) = d["fields"].as_array() {
        for (i, f) in fields.iter().enumerate() {
            write(e, &steps(f.as_str().unwrap_or("")), &v[i]);
        }
        return;
    }
    let at = d["at"].as_str().map(String::from).unwrap_or_else(|| format!("/{id}"));
    write(e, &steps(&at), v);
    for also in d["also"].as_array().into_iter().flatten() {
        write(e, &steps(also.as_str().unwrap_or("")), v);
    }
}

/// One color of a setting set in the picker where the effect keeps it: the
/// color, or the color of the list's item at `index`.
pub fn keep_color(e: &mut Value, id: &str, d: &Value, index: usize, c: [u8; 3]) {
    let code = if d["form"] == "rgb" { json!(c) } else { json!(hex_of(c).to_uppercase()) };
    let at = d["at"].as_str().map(String::from).unwrap_or_else(|| format!("/{id}"));
    match (d["type"].as_str(), d["item"].as_str()) {
        (Some("colors"), Some(item)) => write(e, &steps(&format!("{at}/{index}{item}")), &code),
        (Some("colors"), None) => write(e, &steps(&format!("{at}/{index}")), &code),
        _ => write(e, &steps(&at), &code),
    }
}

/// A slider of an effect with a layout of its own moved to `f`: the value it
/// stands for, as the keyboard's page has always worked it out (a level in
/// single precision, a speed the other way round).
pub fn slid_kept(id: &str, d: &Value, f: f32) -> Value {
    let (min, max, _) = span(d);
    if is_speed_of(id, d) {
        let v = min + f.clamp(0.0, 1.0) as f64 * (max - min);
        return json!((min + max - v).round() as u64);
    }
    json!((min as f32 + f.clamp(0.0, 1.0) * (max - min) as f32).round() as u64)
}

/// Whether a checkbox's setting is on: true, or a number other than 0.
fn truthy(v: &Value) -> bool {
    v.as_bool().unwrap_or_else(|| v.as_f64().is_some_and(|n| n != 0.0))
}

/// The words an option of a menu choice shows for a value: its option's, or
/// for a value no option has, `other` with the value's parts in it ({0},
/// {1}), or the option it falls back to.
fn chosen_label(d: &Value, v: &Value) -> String {
    let o = d["options"].as_array().cloned().unwrap_or_default();
    if let Some(i) = o.iter().position(|x| option_value(x) == v) {
        return options(d)[i].clone();
    }
    if let Some(other) = d["other"].as_str() {
        let parts: Vec<Value> = v.as_array().cloned().unwrap_or_else(|| vec![v.clone()]);
        return parts.iter().enumerate().fold(other.to_string(), |w, (i, p)| w.replace(&format!("{{{i}}}"), &p.to_string()));
    }
    options(d).get(chosen(d, v)).cloned().unwrap_or_default()
}

/// A menu choice's rows, as the window's menus take them: each option's
/// words (kind 0), ticked where the value is, a rule (kind 2) before an
/// option with `rule`; a value no option has comes last, after a rule of its
/// own, in `other`'s words and ticked, so it is never hidden.
pub fn menu_rows(d: &Value, v: &Value) -> Vec<(i32, String, bool)> {
    let o = d["options"].as_array().cloned().unwrap_or_default();
    let words = options(d);
    let at = o.iter().position(|x| option_value(x) == v);
    let tick = at.or_else(|| d["other"].is_null().then(|| chosen(d, v)));
    let mut rows = Vec::new();
    for (j, x) in o.iter().enumerate() {
        if x["rule"] == true {
            rows.push((2, String::new(), false));
        }
        rows.push((0, words[j].clone(), tick == Some(j)));
    }
    if at.is_none() && !d["other"].is_null() {
        rows.push((2, String::new(), false));
        rows.push((0, chosen_label(d, v), true));
    }
    rows
}

/// The value a row of a menu choice's menu stands for: its option's; none
/// for a rule, or the value no option has.
pub fn menu_pick(d: &Value, row: usize) -> Option<Value> {
    let mut k = 0;
    for x in d["options"].as_array().into_iter().flatten() {
        if x["rule"] == true {
            if k == row {
                return None;
            }
            k += 1;
        }
        if k == row {
            return Some(option_value(x).clone());
        }
        k += 1;
    }
    None
}

/// A light's zones, each by its bit, and the bit that lights every part of it.
fn zones(light: &Value) -> (Vec<(u64, String, bool)>, u64, u64) {
    let z: Vec<(u64, String, bool)> = light["zones"].as_array().into_iter().flatten().map(|z| (z["bit"].as_u64().unwrap_or(0), z["label"].as_str().unwrap_or("").to_string(), z["whole"] == true)).collect();
    let whole = z.iter().find(|z| z.2).map_or(0, |z| z.0);
    let all = z.iter().filter(|z| !z.2).fold(0, |a, z| a | z.0);
    (z, whole, all)
}

/// Every zone of a light but the whole, for Some zones the first time.
pub fn all_zones(light: &Value) -> u64 {
    zones(light).2
}

/// A zone's bit, by its place in the light's list of them.
pub fn zone_bit(light: &Value, index: usize) -> Option<u64> {
    zones(light).0.get(index).map(|z| z.0)
}

/// Whether a zones setting's value lights the whole light.
pub fn whole(light: &Value, z: u64) -> bool {
    z & zones(light).1 != 0
}

/// The zones an effect lights after a piece of its zones setting is used:
/// the whole light (`index` 100) or some zones (101), a zone's chip turned
/// on or off (`index` its place in the light's list). On the whole light, a
/// zone picked is the only one lit. `areas` keeps the zones chosen while
/// the whole light is lit, so choosing Some zones again brings them back.
pub fn zones_picked(light: &Value, z: u64, index: usize, areas: &mut u64) -> u64 {
    let (list, whole, all) = zones(light);
    let z = match index {
        100 => {
            if z & whole == 0 {
                *areas = z & all;
            }
            whole
        }
        101 => {
            if z & whole == 0 {
                z
            } else {
                *areas
            }
        }
        i => {
            let bit = list.get(i).map_or(0, |z| z.0) & all;
            if z & whole != 0 { bit } else { z ^ bit }
        }
    };
    if z & whole == 0 {
        *areas = z;
    }
    z
}

/// What the page adds to an effect's description to draw it: the device's
/// kind and the profile being edited, for the notes' words; the color the
/// picker is open on (its setting's place and the color's), or whether it is
/// open on the brush's own color; the setting whose menu is open; the zone
/// the pointer is over on the device's picture; whether pretend typing is
/// on; and the brush's color.
pub struct Look<'a> {
    pub kind: &'a str,
    pub profile: &'a str,
    pub open: Option<(usize, usize)>,
    pub painting: bool,
    pub menu: Option<usize>,
    pub hot: Option<usize>,
    pub demo: bool,
    pub paint: [u8; 3],
}

/// The boxes beside the effects for an effect with a layout of its own: its
/// settings, and where and when it plays (or, painting key by key, the
/// brush). Each box is a fixed width, so its pieces are placed here, from its
/// own top left; the page places the boxes and wraps them.
pub struct Boxes {
    /// 0 an effect's settings, 1 the light dark, 2 painting key by key
    pub kind: i32,
    pub title: String,
    pub set: Vec<Cell>,
    pub set_h: f32,
    pub place: Vec<Cell>,
    pub place_h: f32,
    pub place_title: String,
}

pub const SET_W: f32 = 360.0;
pub const PLACE_W: f32 = 320.0;
/// a speed's slider's fill
const SPEED_FILL: [u8; 3] = [0xd4, 0xd4, 0xd8];

/// A note's words with the profile's name and the device's kind in them.
fn words(s: &str, look: &Look) -> String {
    s.replace("{profile}", look.profile).replace("{kind}", look.kind)
}

/// Which box a setting of an effect with a layout is in: true for where and when.
pub fn in_place_box(effect: &Value, id: &str) -> bool {
    effect["layout"].as_array().into_iter().flatten().any(|g| g["box"] == "place" && g["show"].as_array().is_some_and(|s| s.iter().any(|x| item_id(x) == Some(id))))
}

/// The boxes for an effect with a layout of its own, its settings read from
/// `e`, the effect's own settings in the device's profile; `light` the light
/// it plays on (its zones).
pub fn boxes(light: &Value, effect: &Value, e: &Value, look: &Look) -> Boxes {
    let title = effect["label"].as_str().or(effect["id"].as_str()).unwrap_or("").to_string();
    if effect["paint"] == true {
        return brush(light, effect, look);
    }
    let ps = params(effect);
    if ps.is_empty() {
        let mut s = Stack::new(SET_W);
        s.note(&words(effect["note"].as_str().unwrap_or(""), look));
        return Boxes { kind: 1, title, set_h: s.height(), set: s.cells, place: Vec::new(), place_h: 0.0, place_title: String::new() };
    }
    let mut s = Stack::new(SET_W);
    // the reset, at the top right, level with the title
    if effect["reset"] == true {
        s.link_right(Ctl::Reset, "Reset", 13.0, crate::lighting::RESET_ICON);
        s.y -= 6.0;
    }
    if let Some(n) = effect["note"].as_str() {
        s.note(&words(n, look));
    }
    s.gap(4.0);
    // (a level's slider in the effect's first color, white with none)
    let tint = ps
        .iter()
        .find_map(|(id, d)| match d["type"].as_str() {
            Some("color") => rgb(&kept(e, id, d)),
            Some("colors") => colors(&kept(e, id, d)).first().copied(),
            _ => None,
        })
        .unwrap_or([255, 255, 255]);
    let place = |k: &str| ps.iter().position(|(id, _)| id == k);
    let layout = effect["layout"].as_array().cloned().unwrap_or_default();
    // (a section's settings that show, by their places: each a setting the
    // effect describes, its condition holding, the section's too)
    let showing = |g: &Value| -> Vec<usize> {
        if !holds(&g["when"], &ps, e) {
            return Vec::new();
        }
        g["show"].as_array().into_iter().flatten().filter(|i| holds(&i["when"], &ps, e)).filter_map(|i| item_id(i).and_then(place)).collect()
    };
    let mut sections: Vec<(String, Vec<usize>, Option<String>)> = layout.iter().filter(|g| g["box"] != "place").map(|g| (g["section"].as_str().unwrap_or("").to_string(), showing(g), g["note"].as_str().map(String::from))).collect();
    // what no section places comes last, in a section of its own, so a
    // missing or out-of-date layout never hides a setting
    let placed = shown(effect);
    let rest: Vec<usize> = (0..ps.len()).filter(|&k| !placed.contains(&ps[k].0)).collect();
    if !rest.is_empty() {
        sections.push(("More".into(), rest, None));
    }
    for (name, show, note) in sections.iter().filter(|(_, show, note)| !show.is_empty() || note.is_some()) {
        s.group(name);
        for (j, &k) in show.iter().enumerate() {
            let (id, d) = (&ps[k].0, &ps[k].1);
            if j > 0 {
                s.gap(if d["type"] == "bool" { 14.0 } else { 16.0 });
            }
            drawn(&mut s, k, id, d, &kept(e, id, d), look, tint);
        }
        if let Some(n) = note {
            s.gap(12.0);
            s.note(&words(n, look));
        }
    }
    if effect["reactive"] == true {
        s.gap(14.0);
        s.check(Ctl::Demo, look.demo, "Preview with pretend typing");
    }
    // where and when: the zones it lights, as a switch and chips (faded on the
    // whole light, where picking one lights only that), then the rest, each
    // a menu, under a hairline. Everything sits where it always does, so
    // nothing moves as it changes.
    let mut p = Stack::new(PLACE_W);
    p.y = 46.0;
    let mut place_title = String::new();
    let mut rule = true;
    for g in layout.iter().filter(|g| g["box"] == "place") {
        place_title = g["section"].as_str().unwrap_or("").to_string();
        for k in showing(g) {
            let (id, d) = (ps[k].0.as_str(), &ps[k].1);
            let v = kept(e, id, d);
            let at = EFFECT_IDS + k as i32;
            if d["type"] == "zones" {
                zones_drawn(&mut p, at, light, v.as_u64().unwrap_or(0), look);
                continue;
            }
            if rule {
                rule = false;
                p.y = p.y.max(182.0) + 14.0;
                p.cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..crate::lighting::cell(kind::BOX, 16.0, p.y, p.inner(), 1.0) });
                p.y += 14.0;
            }
            p.cells.push(text_cell(style::PROP, 16.0, p.y, p.inner(), 20.0, &label(id, d)));
            p.y += 24.0;
            p.dropdown_at(at, &chosen_label(d, &v), look.menu == Some(k));
            p.y += 10.0;
        }
    }
    Boxes { kind: 0, title, set_h: s.height(), set: s.cells, place_h: if place_title.is_empty() { 0.0 } else { p.height() }, place: p.cells, place_title }
}

/// One setting of an effect with a layout of its own, with the keyboard
/// page's controls: a color with its code, a run of colors, a checkbox, a
/// level (as a percent, its slider in the effect's first color where it
/// says `tint`), a speed from slow to fast, a choice as a switch (with each
/// option's icon) or, saying `menu`, a dropdown.
fn drawn(s: &mut Stack, k: usize, id: &str, d: &Value, v: &Value, look: &Look, tint: [u8; 3]) {
    let at = EFFECT_IDS + k as i32;
    let open = look.open.filter(|(p, _)| *p == k).map(|(_, i)| i);
    let rgb_of = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
    match d["type"].as_str().unwrap_or("") {
        "color" => s.dots_at(at, &[rgb(v).unwrap_or([0; 3])], open, true),
        "colors" => {
            let cols = colors(v);
            s.strip(&cols);
            s.dots_at(at, &cols, open, false);
        }
        "bool" => s.check_at(at, truthy(v), &label(id, d)),
        "int" | "number" => {
            let (min, max, _) = span(d);
            let n = v.as_f64().unwrap_or(min);
            let shown = if d["percent"] == true { format!("{}%", ((n - min) / (max - min) * 100.0).round()) } else { amount(d, v) };
            s.head(&label(id, d), Some(&shown));
            s.slider_at(at, ((n - min) / (max - min)) as f32, if d["tint"] == true { rgb_of(tint) } else { fill() }, None);
        }
        "duration" => {
            let (min, max, _) = span(d);
            let n = v.as_f64().unwrap_or(min).clamp(min, max);
            s.head(&label(id, d), Some(&format!("{:.1} s", n / 1000.0)));
            if is_speed_of(id, d) {
                // the slider runs slow to fast: right is faster
                s.slider_at(at, ((max - n) / (max - min)) as f32, rgb_of(SPEED_FILL), Some(("Slow", "Fast")));
            } else {
                s.slider_at(at, ((n - min) / (max - min)) as f32, fill(), None);
            }
        }
        "choice" if d["menu"] == true => {
            s.head(&label(id, d), None);
            s.dropdown_at(at, &chosen_label(d, v), look.menu == Some(k));
        }
        "choice" => {
            s.head(&label(id, d), None);
            let words = options(d);
            let icons: Vec<&str> = d["options"].as_array().into_iter().flatten().map(|o| crate::lighting::icon_path(o["icon"].as_str().unwrap_or(""))).collect();
            let opts: Vec<(&str, &str)> = words.iter().zip(icons).map(|(w, i)| (w.as_str(), i)).collect();
            s.switch_at(at, &opts, chosen(d, v));
        }
        _ => {}
    }
}

/// A zones setting: the whole light or some zones, the zones as chips (each
/// by its place in the light's list, faded on the whole light; `hot` the one
/// the pointer is over on the picture), and a word on what lights.
fn zones_drawn(p: &mut Stack, at: i32, light: &Value, z: u64, look: &Look) {
    let (list, whole_bit, all) = zones(light);
    let whole = z & whole_bit != 0;
    let whole_words = list.iter().find(|z| z.2).map_or("Whole".to_string(), |z| z.1.clone());
    p.switch_at(at, &[(whole_words.as_str(), ""), ("Some zones", "")], if whole { 0 } else { 1 });
    // (the switch's choices told apart from the chips: 100 the whole light, 101 some zones)
    for c in p.cells.iter_mut().rev().take(2) {
        c.index += 100;
    }
    p.gap(14.0);
    let picks = if whole { 0 } else { z & all };
    let chips: Vec<(usize, &str, bool)> = list.iter().enumerate().filter(|(_, z)| !z.2).map(|(i, z)| (i, z.1.as_str(), picks & z.0 != 0)).collect();
    p.chips_at(at, &chips, whole, look.hot);
    p.gap(8.0);
    p.note(&if whole {
        "Every key lights. Pick a zone to light only that.".to_string()
    } else if picks != 0 {
        format!("Pick zones here or on the {}.", look.kind)
    } else {
        "Nothing lights until you pick a zone.".to_string()
    });
}

/// Painting key by key: the brush's colors (the ready ones, the one chosen
/// ringed, and any other from the picker), filling a zone, scattering colors
/// or clearing every key, and the effect's note.
fn brush(light: &Value, effect: &Value, look: &Look) -> Boxes {
    use crate::lighting::PRESETS;
    let mut s = Stack::new(SET_W);
    s.group("Color");
    let mut cols: Vec<[u8; 3]> = PRESETS.to_vec();
    let chosen = PRESETS.iter().position(|c| *c == look.paint);
    if chosen.is_none() {
        cols.push(look.paint);
    }
    let at = chosen.unwrap_or(PRESETS.len());
    let rgb_of = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
    // (a little closer than the effects' colors, so the ready ones and "+" share a line)
    let (mut x, mut y) = (16.0f32, s.y);
    for (j, col) in cols.iter().enumerate() {
        if x > 16.0 && x + 28.0 > SET_W - 16.0 {
            x = 16.0;
            y += 36.0;
        }
        s.cells.push(Cell { color: rgb_of(*col), on: j == at, id: crate::lighting::id(Ctl::Preset), index: j as i32, ..crate::lighting::cell(kind::COLOR_DOT, x, y, 28.0, 28.0) });
        x += 33.0;
    }
    // any other color, from the picker
    if x + 28.0 > SET_W - 16.0 {
        x = 16.0;
        y += 36.0;
    }
    s.cells.push(Cell { style: 1, on: look.painting, id: crate::lighting::id(Ctl::Paint), ..crate::lighting::cell(kind::COLOR_DOT, x, y, 28.0, 28.0) });
    s.y = y + 28.0;
    s.group("Fill a zone");
    let list = zones(light).0;
    let chips: Vec<(usize, &str, bool)> = list.iter().enumerate().map(|(i, z)| (i, z.1.as_str(), false)).collect();
    s.chips(Ctl::Fill, &chips, false, None);
    s.gap(10.0);
    let row = s.y;
    let w1 = crate::text::width(500, 12.5, "Scatter colors") + 18.0;
    s.cells.push(Cell { text: "Scatter colors".into(), id: crate::lighting::id(Ctl::Scatter), ..crate::lighting::cell(kind::QUIET, 8.0, row, w1, 24.0) });
    s.cells.push(Cell { text: "Clear every key".into(), id: crate::lighting::id(Ctl::Clear), ..crate::lighting::cell(kind::QUIET, 8.0 + w1 + 4.0, row, crate::text::width(500, 12.5, "Clear every key") + 18.0, 24.0) });
    s.y = row + 24.0 + 12.0;
    s.note(&words(effect["note"].as_str().unwrap_or(""), look));
    Boxes { kind: 2, title: "Brush".into(), set_h: s.height(), set: s.cells, place: Vec::new(), place_h: 0.0, place_title: String::new() }
}

/// The small picture and motion of an effect's tile, by its id: the
/// keyboard's own pictures, a mouse's color shift as Wave's.
pub fn tile(id: &str) -> (&'static str, i32) {
    match id {
        "off" => ("off", 0),
        "breathing" => ("breathing", 1),
        "pulse" => ("pulse", 2),
        "spiral" => ("spiral", 0),
        "wave" => ("wave", 3),
        "colorshift" => ("wave", 3),
        "stars" => ("stars", 4),
        "trigger" => ("trigger", 5),
        "custom" => ("custom", 0),
        _ => ("static", 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse() -> Value {
        json!({
            "kind": "mouse",
            "capabilities": { "settings": { "items": [
                { "id": "cpi1", "type": "int", "min": 100, "max": 12000, "step": 100, "unit": "CPI", "label": "First sensitivity", "short": "First" },
                { "id": "cpi2", "type": "int", "min": 100, "max": 12000, "step": 100, "unit": "CPI", "label": "Second sensitivity", "short": "Second" },
                { "id": "polling", "type": "choice", "options": [125, 250, 500, 1000], "unit": "Hz", "label": "Polling rate" },
                { "id": "lift", "type": "choice", "options": ["low", "high"], "label": "Lift-off" },
            ] } },
            "layout": { "device": [
                { "section": "Sensitivity", "show": ["cpi1", "cpi2"], "note": "Two of them." },
                { "section": "Polling rate", "show": ["polling", { "id": "nothing", "when": { "polling": 125 } }] },
            ] },
        })
    }

    /// The keyboard's lighting as its core describes it, each effect with
    /// its layout (as the keyboard's page reads them), and its light.
    fn keyboard() -> (Value, Value) {
        let mut l = evga_z10::shared::capabilities()["lighting"].clone();
        let layout = evga_z10::shared::lighting_layout();
        for e in l["effects"].as_array_mut().unwrap() {
            if let Some(g) = layout.get(e["id"].as_str().unwrap()) {
                e["layout"] = g.clone();
            }
        }
        let light = l["lights"].as_array().unwrap().iter().find(|x| x["id"] == "keys").unwrap().clone();
        (l, light)
    }
    fn effect(l: &Value, id: &str) -> Value {
        l["effects"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap().clone()
    }
    fn setting(e: &Value, id: &str) -> Value {
        params(e).into_iter().find(|(k, _)| k == id).unwrap().1
    }

    /// Every one of the keyboard's effects reads and draws from what the
    /// keyboard keeps (its own settings, as it read them): each setting its
    /// layout shows is described, kept where its `at` says, and drawn; Off
    /// says the keys stay dark on the profile, by its name.
    #[test]
    fn the_keyboards_effects_draw_from_what_it_keeps() {
        let (l, light) = keyboard();
        let c = |r: u8, g: u8, b: u8| json!({ "brightness": 255, "color": [r, g, b] });
        let seven = json!([c(255, 0, 0), c(255, 128, 0), c(255, 255, 0), c(0, 255, 0), c(0, 0, 255), c(128, 0, 255), c(255, 0, 255), c(255, 255, 255)]);
        let kept_by_keyboard = json!({
            "static": { "zone": 128, "start": 0, "stop": 0, "brightness": 255, "color": [255, 0, 0] },
            "breathing": { "zone": 128, "start": 0, "stop": 0, "random": 0, "cycle": 1000, "colors": [{ "hold": 10, "brightness": 255, "color": [255, 0, 0] }, { "hold": 10, "brightness": 255, "color": [0, 255, 0] }] },
            "pulse": { "zone": 128, "start": 0, "stop": 0, "random": 0, "cycle": 6000, "count": 7, "on_hold": 10, "off_hold": 10, "background": { "brightness": 255, "color": [0, 0, 0] }, "colors": seven },
            "spiral": { "zone": 128, "start": 0, "stop": 0, "direction": 0, "cycle": 3000, "count": 7, "colors": seven },
            "wave": { "zone": 128, "start": 0, "stop": 0, "random": 0, "direction": 2, "cycle": 1000, "count": 7, "colors": seven },
            "trigger": { "zone": 128, "start": 0, "stop": 0, "random": 0, "submode": 0, "shift": 100, "count": 7, "colors": seven },
            "stars": { "zone": 128, "start": 0, "stop": 0, "cycle": 500, "brightness": 255 },
        });
        let look = Look { kind: "keyboard", profile: "Studio", open: None, painting: false, menu: None, hot: None, demo: false, paint: [255, 8, 0] };
        let ids: Vec<&str> = l["effects"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["off", "static", "breathing", "pulse", "spiral", "wave", "stars", "trigger", "custom"]);
        for e in l["effects"].as_array().unwrap() {
            let id = e["id"].as_str().unwrap();
            let stored = &kept_by_keyboard[id];
            for want in shown(e) {
                let (k, d) = params(e).into_iter().find(|(k, _)| *k == want).unwrap_or_else(|| panic!("{id} shows {want}, which it doesn't describe"));
                if !d["fields"].is_array() && d["type"] != "zones" {
                    assert!(read(stored, &steps(d["at"].as_str().unwrap_or(&format!("/{k}")))).is_some(), "{id}'s {k} is kept where its at says");
                }
            }
            let b = boxes(&light, e, stored, &look);
            match id {
                "off" => assert_eq!((b.kind, b.set.iter().any(|c| c.text.contains("Studio"))), (1, true)),
                "custom" => assert_eq!((b.kind, b.title.as_str()), (2, "Brush")),
                _ => {
                    assert_eq!((b.kind, b.place_title.as_str()), (0, "Zones and timing"), "{id}");
                    assert!(b.set.iter().any(|c| c.text == "Brightness"), "{id} draws its brightness");
                }
            }
        }
        // what each shows of them: the brightness as a percent, a speed's time, a direction
        let pct = |e: &str, v: u64| {
            let ef = effect(&l, e);
            let mut st = kept_by_keyboard[e].clone();
            keep(&mut st, "brightness", &setting(&ef, "brightness"), &json!(v));
            boxes(&light, &ef, &st, &look).set.iter().find(|c| c.text.ends_with('%')).unwrap().text.to_string()
        };
        assert_eq!(pct("static", 128), "50%");
        assert_eq!(pct("wave", 255), "100%");
        let wave = effect(&l, "wave");
        let b = boxes(&light, &wave, &kept_by_keyboard["wave"], &look);
        assert!(b.set.iter().any(|c| c.text == "1.0 s"));
        assert!(b.set.iter().any(|c| c.text == "Right" && c.on), "kept 2, shown Right");
    }

    /// A brightness set on an effect of several colors lights every one of
    /// them and the color between pulses, as the keyboard's page always has;
    /// a speed is kept as the time a cycle takes, the slider running slow to
    /// fast; a color set in the picker is kept as the keyboard keeps colors.
    #[test]
    fn a_setting_is_kept_where_the_keyboard_keeps_it() {
        let (l, _) = keyboard();
        let pulse = effect(&l, "pulse");
        let mut e = json!({ "colors": [{ "brightness": 255, "color": [1, 2, 3] }, { "brightness": 255, "color": [4, 5, 6] }], "background": { "brightness": 255, "color": [0, 0, 0] }, "cycle": 6000 });
        let bri = setting(&pulse, "brightness");
        keep(&mut e, "brightness", &bri, &slid_kept("brightness", &bri, 0.5));
        assert_eq!((e["colors"][0]["brightness"].clone(), e["colors"][1]["brightness"].clone(), e["background"]["brightness"].clone()), (json!(128), json!(128), json!(128)));
        let speed = setting(&pulse, "speed");
        keep(&mut e, "speed", &speed, &slid_kept("speed", &speed, 1.0));
        assert_eq!(e["cycle"], 6000, "right is fastest: the shortest cycle");
        keep(&mut e, "speed", &speed, &slid_kept("speed", &speed, 0.25));
        assert_eq!(e["cycle"], 9000);
        keep_color(&mut e, "colors", &setting(&pulse, "colors"), 1, [9, 8, 7]);
        assert_eq!(e["colors"][1]["color"], json!([9, 8, 7]));
        // (a static color, and Stars' own brightness, where there's no background)
        let stars = effect(&l, "stars");
        let mut s = json!({ "brightness": 255, "cycle": 500 });
        keep(&mut s, "brightness", &setting(&stars, "brightness"), &json!(9));
        assert_eq!(s, json!({ "brightness": 9, "cycle": 500 }));
        // a checkbox kept as a number
        let random = setting(&effect(&l, "wave"), "random");
        assert_eq!(picked(&random, &json!(0), 1), Some(json!(1)));
    }

    /// Plays is the keyboard's start and stop together, both ways: each of
    /// the six its pair, as EVGA's LedStartMode and LedStopMode name them,
    /// with a rule between those that start with the profile and those that
    /// start on a key press; a pair it has no name for is said as it is,
    /// last and ticked, and choosing it changes nothing.
    #[test]
    fn plays_is_the_keyboards_start_and_stop_both_ways() {
        let (l, _) = keyboard();
        let plays = setting(&effect(&l, "spiral"), "plays");
        let want = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 2), (1, 1)];
        let rows = menu_rows(&plays, &json!([0, 0]));
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [0, 0, 0, 2, 0, 0, 0]);
        for (row, (start, stop)) in [0, 1, 2, 4, 5, 6].into_iter().zip(want) {
            let mut e = json!({ "zone": 128 });
            let v = menu_pick(&plays, row).unwrap();
            keep(&mut e, "plays", &plays, &v);
            assert_eq!((e["start"].as_u64(), e["stop"].as_u64()), (Some(start), Some(stop)), "{}", rows[row].1);
            assert_eq!(chosen_label(&plays, &kept(&e, "plays", &plays)), rows[row].1);
        }
        assert_eq!(menu_pick(&plays, 3), None, "the rule");
        assert_eq!(chosen_label(&plays, &kept(&json!({}), "plays", &plays)), "All the time", "none kept: with the profile, never stopping");
        let odd = json!([3, 0]);
        assert_eq!(chosen_label(&plays, &odd), "Something else (start 3, stop 0)");
        let rows = menu_rows(&plays, &odd);
        assert_eq!(rows[rows.len() - 1], (0, "Something else (start 3, stop 0)".to_string(), true));
        assert_eq!(menu_pick(&plays, rows.len() - 1), None);
    }

    /// Some zones remembers its zones and never snaps back to the whole
    /// keyboard: the first time, every zone; each taken out, nothing lit
    /// (not the whole keyboard); two put back, the whole keyboard and back
    /// again brings the two.
    #[test]
    fn some_zones_remembers_its_zones_and_never_snaps_back() {
        let (_, light) = keyboard();
        let all = all_zones(&light);
        let mut areas = all;
        let mut z = zones_picked(&light, 0x80, 101, &mut areas);
        assert_eq!(z, all);
        for i in 1..6 {
            z = zones_picked(&light, z, i, &mut areas);
        }
        assert_eq!(z, 0, "no zones chosen lights nothing; it is not the whole keyboard");
        z = zones_picked(&light, z, 1, &mut areas);
        z = zones_picked(&light, z, 3, &mut areas);
        let two = zone_bit(&light, 1).unwrap() | zone_bit(&light, 3).unwrap();
        assert_eq!(z, two);
        z = zones_picked(&light, z, 100, &mut areas);
        assert!(whole(&light, z));
        z = zones_picked(&light, z, 101, &mut areas);
        assert_eq!(z, two);
    }

    /// A zone picked on the whole keyboard is the only one lit; then
    /// another added, and the first taken out again; the whole keyboard and
    /// back keeps it.
    #[test]
    fn a_zone_picked_on_the_whole_keyboard_is_the_only_one_lit() {
        let (_, light) = keyboard();
        let mut areas = all_zones(&light);
        let wasd = zone_bit(&light, 3).unwrap();
        let fkeys = zone_bit(&light, 1).unwrap();
        let mut z = zones_picked(&light, 0x80, 3, &mut areas);
        assert_eq!(z, wasd, "WASD alone, and Some zones");
        z = zones_picked(&light, z, 1, &mut areas);
        assert_eq!(z, wasd | fkeys);
        z = zones_picked(&light, z, 3, &mut areas);
        assert_eq!(z, fkeys);
        z = zones_picked(&light, z, 100, &mut areas);
        z = zones_picked(&light, z, 101, &mut areas);
        assert_eq!(z, fkeys);
    }

    /// A device's settings go in its layout's sections, in order; one no
    /// section shows comes last; a section shows only what it holds.
    #[test]
    fn settings_go_in_their_sections() {
        let s = sections(&mouse(), &json!({ "polling": 1000 }));
        assert_eq!(s.iter().map(|s| (s.title.as_str(), s.items.clone())).collect::<Vec<_>>(), [("Sensitivity", vec![0, 1]), ("Polling rate", vec![2]), ("More", vec![3])]);
        assert_eq!(s[0].note.as_deref(), Some("Two of them."));
    }

    /// A slider stands where its value is and gives back the value it
    /// stands for, in its steps; a speed runs from slow to fast.
    #[test]
    fn sliders_stand_for_their_values() {
        let cpi = json!({ "type": "int", "min": 100, "max": 12000, "step": 100 });
        assert_eq!(slid("cpi1", &cpi, frac("cpi1", &cpi, &json!(800))), json!(800));
        assert_eq!(slid("cpi1", &cpi, 1.0), json!(12000));
        assert_eq!(slid("cpi1", &cpi, 0.5004), json!(6100));
        let speed = json!({ "type": "duration", "min": 500, "max": 30000 });
        assert!(frac("speed", &speed, &json!(500)) > 0.99, "the quickest at the right");
        assert_eq!(slid("speed", &speed, 0.0), json!(30000));
        assert_eq!(slid("speed", &speed, frac("speed", &speed, &json!(3000))), json!(3000));
    }

    /// An effect's settings: colors first; a missing one plays its default;
    /// a choice picked takes its option; a color set lands in its place.
    #[test]
    fn an_effects_settings_come_from_its_description() {
        let e = json!({ "id": "colorshift", "params": {
            "speed": { "type": "duration", "min": 500, "max": 30000, "default": 5000 },
            "colors": { "type": "colors", "max": 7, "default": ["#FF0000", "#00FF00"] },
        }, "start": { "colors": ["#FF0000", "#00FF00", "#0000FF"], "speed": 1000 } });
        let p = params(&e);
        assert_eq!(p.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["colors", "speed"]);
        assert_eq!(value_of(&json!({}), "speed", &p[1].1), &json!(5000));
        let set = with_color(&p[0].1, &json!(["#FF0000", "#00FF00"]), 1, [0, 0, 255]);
        assert_eq!(set, json!(["#FF0000", "#0000FF"]));
        let dir = json!({ "type": "choice", "options": ["up", "down"] });
        assert_eq!(picked(&dir, &json!("up"), 1), Some(json!("down")));
        // chosen for the first time: what it starts as; again: what it had
        assert_eq!(with_effect(&json!({ "effect": "static", "params": { "color": "#123456" } }), &e)["params"], json!({ "color": "#123456", "colors": ["#FF0000", "#00FF00", "#0000FF"], "speed": 1000 }));
        assert_eq!(with_effect(&json!({ "effect": "static", "params": { "colors": ["#FFFFFF"], "speed": 2000 } }), &e)["params"], json!({ "colors": ["#FFFFFF"], "speed": 2000 }));
    }
}
