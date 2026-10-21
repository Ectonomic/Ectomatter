//! A device's settings drawn from what its core says of them, in the shared
//! settings language (the core protocol's section 6): an effect's settings,
//! and the device's own by its layout's sections, each with the app's own
//! controls, so a device whose core describes them needs no code of its own
//! here. The mouse's Lighting and Performance boxes are built this way.
//!
//! Not drawn yet: an effect's own layout (its settings go colors first, then
//! choices, then amounts and speeds, then switches), and a layout's rows,
//! whose settings go one under another.

use serde_json::{Value, json};

use crate::lighting::{Stack, hex_of, style, text_cell};

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
/// kind in the order the core lists them.
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
    v
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
/// to fast, as the keyboard's do, its value the time a cycle takes.
fn is_speed(id: &str) -> bool {
    id == "speed"
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

/// A choice's options, in words: with its unit after each.
fn options(d: &Value) -> Vec<String> {
    let unit = d["unit"].as_str();
    d["options"]
        .as_array()
        .map(|o| {
            o.iter()
                .map(|v| {
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
    o.iter().position(|x| x == v).or_else(|| o.iter().position(|x| *x == d["default"])).unwrap_or(o.len().saturating_sub(1))
}

/// A piece picked at `index`: the setting's new value, for a choice or a
/// switch. A color isn't changed here: it opens the picker.
pub fn picked(d: &Value, v: &Value, index: usize) -> Option<Value> {
    match d["type"].as_str()? {
        "choice" => d["options"].get(index).cloned(),
        // (a checkbox tells the state it is turned to)
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
    let code = json!(hex_of(c).to_uppercase());
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
                s.note(&format!("The {kind} can’t tell Ectodeck its {name}. Choose one to set it."));
            } else {
                s.note(&format!("The {kind} can’t tell Ectodeck its {name}. Until you set it here, this shows the factory’s."));
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
