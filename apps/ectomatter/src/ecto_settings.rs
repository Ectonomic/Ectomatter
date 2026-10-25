//! Ectomatter's own Settings, as the web app had them (shell.js at cd9ea82):
//! each device with its core: whether Ectomatter controls it, whether it may
//! open it, what else has it open; the plugins, each with what it can do,
//! its version and whether it's on, then the sections the plugins describe
//! of themselves (the weather's place and units), drawn here; the sensors the
//! Sensor Feed reads, and which the screens show; the HDR interface; and
//! how often the window draws a lighting effect.

use serde_json::{json, Value};

use std::collections::{BTreeSet, HashMap};

use crate::form::{self, Card, Form};
use crate::tokens::{font, ink, size};
use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use ectomatter_sensors::list::{Catalog, Device, Part, Run, Sensor, Val};
use crate::text;
use crate::Cell;

/// the page's margin (the form, src/form.rs, starts there), a box's padding, and where
/// a row's controls start from a box's inner left (after the label's column)
const X0: f32 = crate::tokens::space::MARGIN;
const PAD_X: f32 = crate::tokens::space::PAD;
const CTRL: f32 = crate::tokens::size::LABEL + crate::tokens::space::PAD;

/// What the Sensor Feed box shows: the computer's sensors as last listed, each one's reading
/// now, the choices, the tab open, the folded runs opened (by device and run), and the
/// sensor being renamed with what is typed so far.
#[derive(Clone)]
pub struct Sensors {
    pub cat: Catalog,
    pub setup: crate::sensors::Setup,
    pub values: HashMap<String, Val>,
    pub tab: Part,
    pub open: BTreeSet<(String, Run)>,
    pub renaming: Option<(String, String)>,
}

/// What the Sensor Feed box's pieces stand for, by their places: each tab's part, each
/// checkbox's sensor, each folded run's device and run, each Make main's card; and where
/// the box's heading is on the page.
#[derive(Clone, Debug, Default)]
pub struct SensorMap {
    pub tabs: Vec<Part>,
    pub chips: Vec<String>,
    pub runs: Vec<(String, Run)>,
    pub mains: Vec<String>,
    pub top: f32,
}

/// A package as Settings' Cores or Plugins lists it: its name, what it's for
/// in its own words, what it can do in Ectomatter's, its version and where that
/// stands, whether it's on, what it says is wrong (a server it can't reach),
/// and its place among the packages (for its glyph and its controls).
#[derive(Clone, Debug, PartialEq)]
pub struct PackageRow {
    pub name: String,
    pub description: String,
    pub can: Vec<String>,
    pub version: String,
    pub status: String,
    pub on: bool,
    pub warning: Option<String>,
    pub glyph: usize,
    /// added, rather than come with Ectomatter: it can be taken out
    pub removable: bool,
    /// the systems it works on, as people say them ("Linux"), each a tag beside its name
    pub systems: Vec<String>,
}

/// A device as Settings' Devices shows it, with its core: how the device is
/// now (plugged in, opened, who else has it open), what it's called in words
/// about it ("keyboard"), and the core's row (its switch is the device's).
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRow {
    pub core: PackageRow,
    pub kind: String,
    pub present: bool,
    /// whether the system lets Ectomatter open it (`None` where it isn't asked)
    pub reachable: Option<bool>,
    pub holders: Vec<String>,
    /// what another program having it open means for it
    pub holders_note: String,
}

/// A plugin's own Settings section, as it last described it (settings/section):
/// whose it is, the section, the values Ectomatter keeps for it, and what is
/// being typed into one of its boxes (that setting's id, and the words so far).
#[derive(Clone, Debug, Default)]
pub struct PluginSection {
    pub plugin: String,
    pub section: Value,
    pub values: Value,
    pub typed: Option<(String, String)>,
}

/// What a plugin section's pieces stand for, by their places: each box's
/// plugin and setting, and each choice's plugin, setting and value.
#[derive(Clone, Debug, Default)]
pub struct PluginMap {
    pub fields: Vec<(String, String)>,
    pub choices: Vec<(String, String, Value)>,
}

pub struct State {
    pub sensors: Option<Sensors>,
    pub devices: Vec<DeviceRow>,
    pub plugins: Vec<PackageRow>,
    pub sections: Vec<PluginSection>,
    pub settings: Value,
    /// the Peak nits slider's two ends: the brightest screen's peak, and the
    /// white of the screen the window is on
    pub top_peak: f64,
    pub reference: f64,
}

/// A switch in a row's controls' column, its options' words and values; how wide it is.
fn seg(c: &mut Card, y: f32, ctl: Ctl, opts: &[(&str, &str)], current: &str) -> f32 {
    let names: Vec<(&str, &str)> = opts.iter().map(|(_, n)| (*n, "")).collect();
    let chosen = opts.iter().position(|(v, _)| *v == current);
    let x = c.control_x();
    form::switch(&mut c.cells, x, y, None, &names, chosen, |j| (id(ctl), j as i32, 0))
}

/// A row's state in its controls' column: its dot and its words, level with the label.
fn state(c: &mut Card, y: f32, tone: i32, s: &str) {
    let x = c.control_x();
    c.status(x, y, form::LINE, tone, s);
}

/// A section: its heading, then its box under it, the box's rows laid by `fill`.
fn section(f: &mut Form, title: &str, fill: impl FnOnce(&mut Card)) {
    f.section(title, "", fill);
}

/// The rows on a tab, in their order: on a tab of one device, its readings by kind; the
/// kinds a part's readings are set out in.
fn row_order(p: Part) -> &'static [&'static str] {
    match p {
        Part::Memory => &["In use", "Temperature"],
        Part::Motherboard => &["Temperature", "Fan", "Voltage", "Power", "Current"],
        Part::Battery => &["Charge", "Power"],
        Part::Laptop => &["Fan", "Temperature", "Voltage", "Power", "Current"],
        _ => &["Temperature", "Load", "Clock", "In use", "Fan", "Voltage", "Power", "Current", "Charge"],
    }
}

/// The row a run of like readings folds into, on a tab of one device.
fn run_row(r: Run) -> &'static str {
    match r {
        Run::Volts => "Voltage",
        Run::Temps => "Temperature",
        // (each core's load or clock, by what its readings are)
        Run::Cores => "",
    }
}

/// A row on a tab: its name (a kind of reading, or a device), what sits under the name (a
/// card's Main card or Make main, the word for it), its sensors, its folded runs, and its
/// lines under it.
struct Row<'a> {
    name: String,
    card: Option<(&'a Device, bool)>,
    inside: Option<String>,
    items: Vec<&'a Sensor>,
    runs: Vec<(Run, Vec<&'a Sensor>)>,
    notes: Vec<String>,
    /// the device its runs are kept open by
    key: String,
    /// what the row's name says under the pointer, for a device: its whole name where the
    /// row's is shorter, and where its readings come from
    tip: Option<(String, String)>,
}

/// The Sensor Feed box in Ectomatter's Settings: a tab for each part of the computer (the Screen tab's
/// own tabs), the open one's device and where its readings come from, then its rows, each
/// sensor a checkbox with its name, its driver's name and its reading, three to a line; long
/// runs of like readings folded into one line that opens; and at its foot what checking does.
fn sensors_box(f: &mut Form, v: &Sensors, map: &mut SensorMap) {
    f.heading("Sensor Feed", "");
    map.top = f.y - 18.0 - crate::tokens::space::LABEL;
    let (x0, w0) = (f.x, f.w);
    let line = slint::Color::from_rgb_u8(ink::LINE[0], ink::LINE[1], ink::LINE[2]);
    let parts = v.cat.parts();
    let tab = if parts.contains(&v.tab) { v.tab } else { parts.first().copied().unwrap_or(Part::Processor) };
    // the tabs, each as wide as its name; an amber dot on a part whose readings can't come in
    let mut tx = x0;
    for (i, p) in parts.iter().enumerate() {
        let bad = v.cat.missing.iter().any(|(m, _)| m == p);
        let w = text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, p.name()).ceil() + 36.0 + if bad { 13.0 } else { 0.0 };
        f.cells.push(Cell { text: p.name().into(), on: *p == tab, style: (i == 0) as i32 + if bad { 2 } else { 0 }, id: id(Ctl::SensorTab), index: i as i32, ..cell(kind::TAB, tx, f.y, w, 36.0) });
        map.tabs.push(*p);
        tx += w + 4.0;
    }
    f.y += 36.0;
    let top = f.y;
    let (bx, bw) = (x0 + PAD_X, w0 - 2.0 * PAD_X);
    let (cx, cw) = (bx + CTRL, bw - CTRL);
    let mut out: Vec<Cell> = Vec::new();
    let mut cy = top + 16.0;
    let main = v.setup.main_card(&v.cat).map(|d| d.key.clone());
    let checked = v.setup.checked_in(&v.cat);
    let mut devices: Vec<&Device> = v.cat.devices.iter().filter(|d| d.part == tab).collect();
    // (the main card first)
    devices.sort_by_key(|d| Some(&d.key) != main.as_ref());
    // the device the tab shows, and where its readings come from
    if !tab.by_device() {
        if let Some(d) = devices.first() {
            let nw = text::width(crate::tokens::font::MEDIUM, crate::tokens::font::TEXT, &d.name).ceil() + 2.0;
            out.push(text_cell(39, bx, cy, nw, 22.0, &d.name));
            if !d.src.is_empty() && !v.cat.missing.iter().any(|(p, _)| *p == tab) {
                out.push(Cell { y: cy + 1.0, ..text_cell(style::NOTE, bx + nw + 10.0, cy, bw - nw - 10.0, 22.0, &d.src) });
            }
            cy += 22.0 + 8.0;
        }
    }
    // a source that can't be read, said in amber
    if let Some((_, why)) = v.cat.missing.iter().find(|(p, _)| *p == tab) {
        cy += 6.0;
        let lines = text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, why, bw - 14.0);
        out.push(Cell { style: 3, ..cell(kind::DOT, bx, cy + 6.5, 6.0, 6.0) });
        for (k, l) in lines.iter().enumerate() {
            out.push(text_cell(style::FACT, bx + 14.0, cy + k as f32 * 19.0, bw - 14.0, 19.0, l));
        }
        cy += lines.len() as f32 * 19.0 + 10.0;
    }
    // the rows: on Graphics, Drives and Network each device, elsewhere each kind of reading
    let mut rows: Vec<Row> = Vec::new();
    let odd_notes = |items: &[&Sensor]| -> Vec<String> {
        items
            .iter()
            .filter_map(|s| {
                let x = v.values.get(&s.id)?;
                ectomatter_sensors::read::odd(s.what, *x).then(|| format!("{} reads {}: probably no sensor is connected to it.", v.setup.names.get(&s.id).unwrap_or(&s.name), ectomatter_sensors::read::shown(s.what, tab, *x)))
            })
            .collect()
    };
    fn runs_of<'a>(all: &[&'a Sensor]) -> Vec<(Run, Vec<&'a Sensor>)> {
        let mut runs: Vec<(Run, Vec<&Sensor>)> = Vec::new();
        for s in all {
            if let Some(r) = s.run {
                match runs.iter_mut().find(|(x, _)| *x == r) {
                    Some((_, v)) => v.push(s),
                    None => runs.push((r, vec![s])),
                }
            }
        }
        runs
    }
    if tab.by_device() {
        for d in &devices {
            let all: Vec<&Sensor> = d.sensors.iter().collect();
            let items: Vec<&Sensor> = all.iter().copied().filter(|s| s.run.is_none()).collect();
            let mut notes = odd_notes(&items);
            notes.extend(d.note.clone());
            let is_main = main.as_ref() == Some(&d.key);
            let tip = Some((if d.full != d.name { d.full.clone() } else { String::new() }, d.src.clone()));
            rows.push(Row { name: d.name.clone(), card: d.card.as_ref().map(|_| (*d, is_main)), inside: d.inside.clone(), runs: runs_of(&all), items, notes, key: d.key.clone(), tip });
        }
    } else if let Some(d) = devices.first() {
        for name in row_order(tab) {
            let items: Vec<&Sensor> = d.sensors.iter().filter(|s| s.run.is_none() && s.what.row() == *name).collect();
            // (each core's readings fold under the row of what they read; the others under their own)
            let runs: Vec<(Run, Vec<&Sensor>)> = runs_of(&d.sensors.iter().filter(|s| s.run.is_some_and(|r| if r == Run::Cores { s.what.row() == *name } else { run_row(r) == *name })).collect::<Vec<_>>());
            if items.is_empty() && runs.is_empty() {
                continue;
            }
            let mut notes = odd_notes(&items);
            for (_, rs) in &runs {
                notes.extend(odd_notes(rs));
            }
            // (each row's runs opened by themselves: the processor's loads apart from its clocks)
            rows.push(Row { name: name.to_string(), card: None, inside: None, items, runs, notes, key: format!("{}#{name}", d.key), tip: None });
        }
    }
    let col = (cw - 2.0 * 8.0) / 3.0;
    let sub_col = (cw - 12.0 - 3.0 * 6.0) / 4.0;
    for (k, r) in rows.iter().enumerate() {
        let ry = cy;
        if k > 0 {
            out.push(Cell { color: line, ..cell(kind::BOX, bx, ry, bw, 1.0) });
        }
        // the row's name, and under a card's its Main card or Make main (a model's code too
        // long for the column broken after a hyphen: WDS200T1X0E- / 00AFY0)
        let broken: Vec<String> = r.name.split(' ').map(|w| if text::width(crate::tokens::font::REGULAR, crate::tokens::font::TEXT, w) > CTRL - 12.0 { w.replacen('-', "- ", 1) } else { w.to_string() }).collect();
        let lines = text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::TEXT, &broken.join(" "), CTRL - 12.0);
        for (i, l) in lines.iter().enumerate() {
            out.push(text_cell(style::LABEL, bx, ry + 6.0 + i as f32 * 18.0, CTRL - 12.0, 18.0, l));
        }
        if let Some((full, src)) = &r.tip {
            let w = lines.iter().map(|l| text::width(crate::tokens::font::REGULAR, crate::tokens::font::TEXT, l).ceil()).fold(0.0f32, f32::max);
            out.push(Cell { text: full.into(), fill: src.into(), ..cell(kind::TIP, bx, ry + 6.0, w, lines.len() as f32 * 18.0) });
        }
        let mut left_h = 6.0 + lines.len() as f32 * 18.0;
        if r.card.is_some() || r.inside.is_some() {
            let sy = ry + left_h + 4.0;
            let mut sx = bx;
            if let Some((d, is_main)) = r.card {
                if is_main {
                    let w = text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "Main card").ceil() + 14.0;
                    out.push(Cell { text: "Main card".into(), style: 3, ..cell(kind::PILL, sx, sy + 1.0, w, 20.0) });
                    sx += w + 6.0;
                } else {
                    let w = text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "Make main").ceil() + 16.0;
                    out.push(Cell { text: "Make main".into(), style: 33, id: id(Ctl::MakeMain), index: map.mains.len() as i32, ..cell(kind::GHOST, sx, sy, w, 22.0) });
                    map.mains.push(d.key.clone());
                    sx += w + 6.0;
                }
            }
            if let Some(i) = &r.inside {
                out.push(text_cell(style::CREDIT, sx, sy, CTRL - (sx - bx), 22.0, i));
            }
            left_h += 4.0 + 22.0;
        }
        // its sensors, three to a line, then its folded runs; a renamed sensor, or one whose
        // name and driver's name run long together, takes two places
        let mut place = (0usize, 0usize);
        let mut at = |span: usize| -> (f32, f32, f32) {
            if place.1 + span > 3 {
                place = (place.0 + 1, 0);
            }
            let p = (cx + place.1 as f32 * (col + 8.0), ry + 6.0 + place.0 as f32 * 30.0, span as f32 * col + (span - 1) as f32 * 8.0);
            place.1 += span;
            p
        };
        for s in &r.items {
            let given = v.setup.names.get(&s.id);
            let wide = given.is_some() || s.orig.as_ref().is_some_and(|o| s.name.chars().count() + o.chars().count() > 20);
            let (x, sy, w) = at(if wide { 2 } else { 1 });
            chip(&mut out, v, &checked, tab, s, (x, sy, w), map);
        }
        for (run, items) in &r.runs {
            let (x, sy, w) = at(1);
            let open = v.open.contains(&(r.key.clone(), *run));
            let n = items.iter().filter(|s| checked.contains(&s.id)).count();
            let name = format!("{} ({})", run.name(), items.len());
            let said = if n > 0 { format!("{n} checked") } else { String::new() };
            out.push(Cell { text: name.into(), lines: said.into(), style: if open { 3 } else { 2 }, value: w, id: id(Ctl::SensorRun), index: map.runs.len() as i32, ..cell(kind::SENSOR, x, sy, w, 30.0) });
            map.runs.push((r.key.clone(), *run));
        }
        let used = if r.items.is_empty() && r.runs.is_empty() { 0 } else { place.0 + 1 };
        let mut right = 6.0 + used as f32 * 30.0;
        // a run opened: its readings four to a line in a well
        for (run, items) in &r.runs {
            if !v.open.contains(&(r.key.clone(), *run)) {
                continue;
            }
            let wy = ry + right + 2.0;
            let lines = items.len().div_ceil(4);
            let h = 12.0 + lines as f32 * 30.0;
            out.push(Cell { style: 1, ..cell(kind::WBOX, cx, wy, cw, h) });
            for (i, s) in items.iter().enumerate() {
                chip(&mut out, v, &checked, tab, s, (cx + 6.0 + (i % 4) as f32 * (sub_col + 6.0), wy + 6.0 + (i / 4) as f32 * 30.0, sub_col), map);
            }
            right += 2.0 + h + 2.0;
        }
        // the lines under it
        for n in &r.notes {
            right += 2.0;
            for l in text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, n, cw - 16.0) {
                out.push(text_cell(style::CREDIT, cx + 8.0, ry + right, cw - 16.0, 17.0, &l));
                right += 17.0;
            }
            right += 2.0;
        }
        cy = ry + left_h.max(right) + 6.0;
    }
    // the foot: what checking does, and how many are checked
    let all: Vec<&Sensor> = v.cat.devices.iter().flat_map(|d| d.sensors.iter()).collect();
    let on = all.iter().filter(|s| checked.contains(&s.id)).count();
    let count = format!("{on} of {} checked", all.len());
    let count_w = text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, &count).ceil() + 2.0;
    cy += 8.0;
    out.push(Cell { color: line, ..cell(kind::BOX, bx, cy, bw, 1.0) });
    cy += 12.0;
    let lines = text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, "The sensors checked here are the readings you can add to a screen layout. Ectomatter checked the usual ones for you.", bw - count_w - 16.0);
    for (i, l) in lines.iter().enumerate() {
        out.push(text_cell(style::NOTE, bx, cy + i as f32 * 18.0, bw - count_w - 16.0, 18.0, l));
    }
    out.push(text_cell(style::NOTE, bx + bw - count_w, cy, count_w, 18.0, &count));
    cy += lines.len() as f32 * 18.0 + 14.0;
    let h = cy - top;
    f.cells.push(Cell { style: if parts.first() == Some(&tab) { 2 } else { 0 }, ..cell(kind::TILE, x0, top, w0, h) });
    f.cells.extend(out);
    f.y = top + h;
}

/// A sensor's checkbox: its name (or the one given it), its driver's name in gray, its
/// reading at its right (amber when it can't be right), a pencil under the pointer; or, as
/// it is renamed, a line to type its new name in.
fn chip(out: &mut Vec<Cell>, v: &Sensors, checked: &BTreeSet<String>, part: Part, s: &Sensor, (x, y, w): (f32, f32, f32), map: &mut SensorMap) {
    let given = v.setup.names.get(&s.id);
    let name = given.cloned().unwrap_or_else(|| s.name.clone());
    // (a renamed sensor keeps its own name beside the one given it)
    let orig = if given.is_some() { s.orig.clone().or(Some(s.name.clone())) } else { s.orig.clone() };
    let reading = v.values.get(&s.id).map(|x| ectomatter_sensors::read::shown(s.what, part, *x)).unwrap_or_default();
    let odd = v.values.get(&s.id).is_some_and(|x| ectomatter_sensors::read::odd(s.what, *x));
    let on = checked.contains(&s.id);
    let index = map.chips.len() as i32;
    map.chips.push(s.id.clone());
    let editing = v.renaming.as_ref().filter(|(id, _)| *id == s.id);
    // the name's room: the chip less its checkbox, its driver's name and its reading (the
    // window takes the pencil's place from it while the pencil shows)
    let orig_w = orig.as_ref().map_or(0.0, |o| text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, o).ceil() + 8.0);
    let room = (w - 8.0 - 15.0 - 8.0 - orig_w - 8.0 - text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, &reading).ceil() - 8.0).max(24.0);
    out.push(Cell {
        text: name.into(),
        fill: orig.unwrap_or_default().into(),
        lines: reading.into(),
        on,
        value: room,
        style: if editing.is_some() { 4 } else if odd { 1 } else { 0 },
        id: id(Ctl::Sensor),
        index,
        ..cell(kind::SENSOR, x, y, w, 30.0)
    });
    if let Some((_, typed)) = editing {
        out.push(Cell { text: typed.into(), style: 7, id: id(Ctl::SensorName), index, ..cell(kind::FIELD, x + 31.0, y + 3.0, w - 31.0 - 8.0, 24.0) });
    }
}

/// The page, a form (src/form.rs) `room` across under the page's bar, Space.margin
/// down (Settings has no hint over it).
pub fn page(s: &State, room: f32) -> (Vec<Cell>, f32, SensorMap, PluginMap) {
    let mut f = Form::new(X0, crate::tokens::space::MARGIN, room - 2.0 * X0);

    // each device with its core, then Add a core; then the plugins
    devices_section(&mut f, &s.devices);
    packages_section(&mut f, "Plugins", 1, &s.plugins, "Add a plugin", "Ectomatter checks each plugin’s GitHub releases for updates once a day.");

    // the sections the plugins describe of themselves
    let mut pmap = PluginMap::default();
    for p in &s.sections {
        plugin_section(&mut f, p, &mut pmap);
    }

    // every sensor on the computer, a tab for each part, and which the screen's layouts can show
    let mut map = SensorMap::default();
    if let Some(v) = &s.sensors {
        sensors_box(&mut f, v, &mut map);
    }

    // the HDR interface
    let hdr = &s.settings["hdr"];
    let on = hdr["lighting"].as_str() != Some("off");
    section(&mut f, "HDR", |c| {
        c.row();
        c.label("HDR interface");
        let top = c.y;
        seg(c, top, Ctl::HdrMode, &[("auto", "On"), ("off", "Off")], if on { "auto" } else { "off" });
        let end = c.note(top + form::LINE, "Lit keys glow brighter than the desktop’s white on screens with room for it, up to Peak nits. At the left, they keep the design’s brightness.");
        c.next(top, end - top);
        c.row();
        c.label("Peak nits");
        let top = c.y;
        let (lo, hi) = (s.reference.round(), s.top_peak.max(s.reference + 1.0).round());
        let nits = hdr["glow_limit"].as_f64().map_or(lo, |v| v.clamp(lo, hi));
        let frac = ((nits - lo) / (hi - lo)) as f32;
        let (cx, cw) = (c.control_x(), c.control_w());
        let vw = 56.0;
        c.cells.push(Cell { value: frac, id: id(Ctl::PeakNits), ..cell(kind::SLIDER, cx + 2.0, top + 9.0, cw - vw - 14.0 - 4.0, 16.0) });
        c.cells.push(text_cell(style::VALUE, cx + cw - vw, top + 8.0, vw, 18.0, &format!("{} nits", nits.round())));
        c.next(top, form::LINE);
    });

    let (cells, h) = f.end();
    (cells, h, map, pmap)
}

/// A plugin's own section, drawn from what it says of itself: under its
/// title, each part of its layout in order. `show` rows its settings (a
/// `search`, a box to type in whose matches the plugin finds; a `choice`,
/// its options side by side), the part's note under the last; `card`, a box
/// with a reading in large and words beside it and facts two to a line (or
/// what it's waiting for); `status`, a dot and words, its note under them;
/// `credit`, the small words saying whose the data is.
fn plugin_section(f: &mut Form, p: &PluginSection, map: &mut PluginMap) {
    let sec = &p.section;
    let title = sec["label"].as_str().unwrap_or(&p.plugin).to_string();
    let empty = Vec::new();
    let settings = sec["settings"].as_array().unwrap_or(&empty);
    let words = |v: &Value| v.as_str().unwrap_or("").to_string();
    section(f, &title, |c| {
        let cx = c.control_x();
        // (a field's, a card's and a credit's width: the column's, at most a reading's)
        let field_w = c.control_w().min(380.0);
        for part in sec["layout"].as_array().unwrap_or(&empty) {
            if let Some(ids) = part["show"].as_array() {
                for (k, sid) in ids.iter().enumerate() {
                    let Some(st) = settings.iter().find(|x| x["id"] == *sid) else { continue };
                    let sid = words(sid);
                    c.row();
                    c.label(&words(&st["label"]));
                    let top = c.y;
                    let value = p.values.get(&sid).cloned().filter(|v| !v.is_null()).unwrap_or_else(|| st["default"].clone());
                    if st["type"] == "search" {
                        let text = match &p.typed {
                            Some((t, typed)) if *t == sid => typed.clone(),
                            _ => words(&st["text"]),
                        };
                        map.fields.push((p.plugin.clone(), sid.clone()));
                        c.cells.push(Cell { text: text.into(), id: id(Ctl::PluginField), index: map.fields.len() as i32 - 1, style: 1, ..cell(kind::FIELD, cx, top, field_w, form::LINE) });
                    } else {
                        let opts: Vec<(String, Value)> = st["options"].as_array().unwrap_or(&empty).iter().map(|o| (words(&o["label"]), o["value"].clone())).collect();
                        let names: Vec<(&str, &str)> = opts.iter().map(|(n, _)| (n.as_str(), "")).collect();
                        let first = map.choices.len() as i32;
                        for (_, v) in &opts {
                            map.choices.push((p.plugin.clone(), sid.clone(), v.clone()));
                        }
                        let chosen = opts.iter().position(|(_, v)| *v == value);
                        form::switch(&mut c.cells, cx, top, None, &names, chosen, |j| (id(Ctl::PluginChoice), first + j as i32, 0));
                    }
                    // (the part's note under its last row)
                    let mut end = top + form::LINE;
                    if let Some(note) = part["note"].as_str().filter(|n| !n.is_empty() && k + 1 == ids.len()) {
                        end = c.note(end, note);
                    }
                    c.next(top, end - top);
                }
            } else if let Some(card) = part.get("card") {
                c.row();
                c.label(&words(&card["label"]));
                let (bx, by) = (cx, c.y);
                c.cells.push(cell(kind::WBOX, bx, by, field_w, 100.0));
                if let Some(w) = card["waiting"].as_str() {
                    c.cells.push(text_cell(style::LABEL, bx + 16.0, by + 14.0, field_w - 32.0, 26.0, w));
                } else {
                    let big = words(&card["big"]);
                    let tw = text::width(crate::tokens::font::MEDIUM, crate::tokens::font::DISPLAY, &big) - 0.44 * big.chars().count() as f32;
                    c.cells.push(text_cell(style::BIG, bx + 16.0, by + 14.0, tw + 4.0, 26.0, &big));
                    c.cells.push(text_cell(style::LABEL_SOFT, bx + 16.0 + tw + 12.0, by + 22.0, 200.0, 16.0, &words(&card["beside"])));
                    let facts: Vec<(String, String)> = card["facts"].as_array().unwrap_or(&empty).iter().map(|f| (words(&f[0]), words(&f[1]))).collect();
                    // two to a line, the first column as wide as its widest
                    let col0 = facts.iter().step_by(2).map(|(k, v)| text::width(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, &format!("{k} ")) + text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, v)).fold(0.0f32, f32::max);
                    for (i, (k, v)) in facts.iter().enumerate() {
                        let fx = bx + 16.0 + if i % 2 == 1 { col0 + 28.0 } else { 0.0 };
                        let fy = by + 46.0 + (i / 2) as f32 * 22.0;
                        let kw = text::width(crate::tokens::font::REGULAR, crate::tokens::font::CONTROL, &format!("{k} "));
                        c.cells.push(text_cell(style::FACT, fx, fy, kw + 2.0, 18.0, k));
                        c.cells.push(text_cell(style::FACT_VALUE, fx + kw, fy, 120.0, 18.0, v));
                    }
                }
                c.next(by, 100.0);
            } else if let Some(st) = part.get("status") {
                c.row();
                c.label(&words(&st["label"]));
                let top = c.y;
                let tone = match st["tone"].as_str() {
                    Some("good") => 1,
                    Some("bad") => 2,
                    Some("warn") => 3,
                    _ => 0,
                };
                state(c, top, tone, &words(&st["words"]));
                let mut end = top + form::LINE;
                if let Some(note) = st["note"].as_str().filter(|n| !n.is_empty()) {
                    end = c.note(end, note);
                }
                c.next(top, end - top);
            } else if let Some(credit) = part["credit"].as_str() {
                c.row();
                let top = c.y;
                let w = c.control_w().min(640.0);
                let mut cy = top;
                for line in text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, credit, w) {
                    c.cells.push(text_cell(style::CREDIT, cx, cy, w, 18.0, &line));
                    cy += 18.0;
                }
                c.next(top, cy - top);
            }
        }
    });
}

/// Settings' Devices: each device with its core, one after another in one
/// box: its glyph, its name and how it is; then Control (the core's switch,
/// on unless turned off), Permission, the programs that also have it open,
/// and its Core (the version, where it stands, what it's for and what it can
/// do); then Add a core, and how updates come.
fn devices_section(f: &mut Form, devices: &[DeviceRow]) {
    section(f, "Devices", |c| {
        for d in devices {
            let r = &d.core;
            // the device: its glyph, its name and how it is
            let line = if !r.on {
                "Control is off".to_string()
            } else if !d.present {
                "Not connected".to_string()
            } else if d.reachable == Some(false) {
                format!("Ectomatter can’t open this {}", d.kind)
            } else {
                "Connected".to_string()
            };
            c.row();
            let (x, top, w) = (c.inner_x(), c.y, c.inner_w());
            c.cells.push(Cell { style: 2 + r.glyph as i32, ..cell(kind::DEVICE, x, top, 48.0, 48.0) });
            c.cells.push(text_cell(style::NAME, x + 62.0, top + 3.0, w - 62.0, 21.0, &r.name));
            c.cells.push(text_cell(style::SMALL, x + 62.0, top + 27.0, w - 62.0, 17.0, &line));
            c.next(top, 48.0);
            // (its switch is its core's)
            c.row();
            c.label("Control");
            let top = c.y;
            let cx = c.control_x();
            form::switch(&mut c.cells, cx, top, None, &[("On", ""), ("Off", "")], Some(if r.on { 0 } else { 1 }), |j| (id(Ctl::PackageOn), (r.glyph * 2 + j) as i32, 0));
            let end = c.note(top + form::LINE, &format!("When off, Ectomatter leaves the {} to other programs.", d.kind));
            c.next(top, end - top);
            c.row();
            c.label("Permission");
            let top = c.y;
            match d.reachable {
                None => state(c, top, 0, "Not needed on this system"),
                Some(_) if !d.present => state(c, top, 0, &format!("Checked when the {} is connected", d.kind)),
                Some(true) => state(c, top, 1, "Granted"),
                Some(false) => state(c, top, 2, "Not granted"),
            }
            c.next(top, form::LINE);
            if !d.holders.is_empty() {
                c.row();
                c.label("Also open in");
                let top = c.y;
                c.cells.push(text_cell(style::VALUE_BRIGHT, cx, top, c.control_w(), form::LINE, &d.holders.join(", ")));
                let end = c.note(top + form::LINE, &d.holders_note);
                c.next(top, end - top);
            }
            // its core: the version and where it stands, then its own words and Ectomatter's
            c.row();
            c.label("Core");
            let top = c.y;
            let v = format!("{} · {}", r.version, r.status);
            let cw = c.control_w();
            c.cells.push(text_cell(style::VALUE_BRIGHT, cx, top, cw, form::LINE, &v));
            if r.removable {
                let vx = cx + text::width(font::REGULAR, font::TEXT, &v).ceil() + 10.0;
                c.cells.push(Cell { text: "Remove".into(), id: id(Ctl::PackageRemove), index: r.glyph as i32, ..cell(kind::LINK, vx, top + 4.0, crate::lighting::quiet_width("Remove", false), 26.0) });
            }
            let mut end = c.note(top + form::LINE, &r.description);
            for words in &r.can {
                end = c.note_at(cx, end, cw, words);
            }
            if let Some(w) = &r.warning {
                end += 6.0;
                let a = ink::WARN;
                c.cells.push(Cell { style: 3, ..cell(kind::DOT, cx, end + 6.0, size::DOT, size::DOT) });
                for l in text::wrap(font::REGULAR, font::NOTE, w, cw - 15.0) {
                    c.cells.push(Cell { color: slint::Color::from_rgb_u8(a[0], a[1], a[2]), ..text_cell(style::NOTE_IN, cx + 15.0, end, cw - 15.0, form::NOTE_LINE, &l) });
                    end += form::NOTE_LINE;
                }
            }
            c.next(top, end - top);
        }
        // Add a core, and how updates come
        c.row();
        let (x, top, w) = (c.inner_x(), c.y, c.inner_w());
        let aw = form::button_width("Add a core", true);
        c.cells.push(Cell { text: "Add a core".into(), lines: "M8 3v10M3 8h10".into(), style: 35, id: id(Ctl::PackageAdd), index: 0, ..cell(kind::GHOST, x, top, aw, size::BUTTON) });
        c.cells.push(text_cell(style::NOTE, x + aw + 14.0, top + 6.0, w - aw - 14.0, form::NOTE_LINE, "Ectomatter checks each core’s GitHub releases for updates once a day."));
        c.next(top, size::BUTTON);
    });
}

/// Settings' Plugins: a row for each package, its glyph on its tile,
/// its name, what it's for and what it can do (and what it says is wrong, in
/// amber), its version and where that stands, and On and Off; then Add (a
/// core, `add` 0, or a plugin, 1), and how updates come.
fn packages_section(f: &mut Form, title: &str, add: i32, rows: &[PackageRow], add_words: &str, note: &str) {
    f.heading(title, "");
    f.card(|c| {
        let (bx, bw) = (c.inner_x(), c.inner_w());
        let soft = ink::SOFT;
        for r in rows {
            c.row();
            let ry = c.y;
            // the switch, then the version, from the right
            let opts = [("On", ""), ("Off", "")];
            let sx = bx + bw - form::switch_width(&opts);
            form::switch(&mut c.cells, sx, ry, None, &opts, Some(if r.on { 0 } else { 1 }), |j| (id(Ctl::PackageOn), (r.glyph * 2 + j) as i32, 0));
            // (each line to the right, against the switch)
            let right = sx - 16.0;
            let (w1, w2) = (text::width(font::REGULAR, font::NOTE, &r.version).ceil() + 2.0, text::width(font::REGULAR, font::NOTE, &r.status).ceil() + 2.0);
            let vx = right - w1.max(w2);
            c.cells.push(text_cell(style::NOTE, right - w1, ry + 1.0, w1, form::NOTE_LINE, &r.version));
            c.cells.push(Cell { color: slint::Color::from_rgb_u8(soft[0], soft[1], soft[2]), ..text_cell(style::NOTE_IN, right - w2, ry + 19.0, w2, form::NOTE_LINE, &r.status) });
            if r.removable {
                let w3 = crate::lighting::quiet_width("Remove", false);
                c.cells.push(Cell { text: "Remove".into(), id: id(Ctl::PackageRemove), index: r.glyph as i32, ..cell(kind::LINK, right - w3 + 9.0, ry + 40.0, w3, 26.0) });
            }
            // its tile, and its words
            c.cells.push(Cell { style: 2 + r.glyph as i32, ..cell(kind::DEVICE, bx, ry, 40.0, 40.0) });
            let tx = bx + 40.0 + 16.0;
            let tw = vx - 16.0 - tx;
            c.cells.push(text_cell(style::NAME, tx, ry, tw, 20.0, &r.name));
            // (the systems it works on, a tag each, beside its name)
            let mut gx = tx + text::width(font::MEDIUM, font::TAB, &r.name).ceil() + 10.0;
            for sys in &r.systems {
                let pw = text::width(font::REGULAR, font::CAPTION, sys).ceil() + 14.0;
                c.cells.push(Cell { text: sys.as_str().into(), style: 4, ..cell(kind::PILL, gx, ry, pw, 20.0) });
                gx += pw + 6.0;
            }
            let mut ty = ry + 22.0;
            for l in text::wrap(font::REGULAR, font::NOTE, &r.description, tw) {
                c.cells.push(Cell { color: slint::Color::from_rgb_u8(soft[0], soft[1], soft[2]), ..text_cell(style::NOTE_IN, tx, ty, tw, form::NOTE_LINE, &l) });
                ty += form::NOTE_LINE;
            }
            for can in &r.can {
                ty = c.note_at(tx, ty, tw, can);
            }
            if let Some(w) = &r.warning {
                ty += 6.0;
                let a = ink::WARN;
                c.cells.push(Cell { style: 3, ..cell(kind::DOT, tx, ty + 6.0, size::DOT, size::DOT) });
                for l in text::wrap(font::REGULAR, font::NOTE, w, tw - 15.0) {
                    c.cells.push(Cell { color: slint::Color::from_rgb_u8(a[0], a[1], a[2]), ..text_cell(style::NOTE_IN, tx + 15.0, ty, tw - 15.0, form::NOTE_LINE, &l) });
                    ty += form::NOTE_LINE;
                }
            }
            c.next(ry, (ty - ry).max(40.0).max(if r.removable { 66.0 } else { 0.0 }));
        }
        // Add a plugin, and how updates come
        c.row();
        let top = c.y;
        let aw = form::button_width(add_words, true);
        c.cells.push(Cell { text: add_words.into(), lines: "M8 3v10M3 8h10".into(), style: 35, id: id(Ctl::PackageAdd), index: add, ..cell(kind::GHOST, bx, top, aw, size::BUTTON) });
        c.cells.push(text_cell(style::NOTE, bx + aw + 14.0, top + 6.0, bw - aw - 14.0, form::NOTE_LINE, note));
        c.next(top, size::BUTTON);
    });
}

/// Nits for a place along the Peak nits slider; `None` at its left end, the design's brightness.
pub fn peak_at(frac: f32, reference: f64, top: f64) -> Option<f64> {
    let (lo, hi) = (reference.round(), top.max(reference + 1.0).round());
    let v = lo + (frac as f64) * (hi - lo);
    let v = (v / 10.0).round() * 10.0;
    if v <= lo + 5.0 { None } else { Some(v.min(hi)) }
}

/// The value a switch sets, by its place.
pub fn choice(c: Ctl, index: usize) -> Option<Value> {
    match c {
        Ctl::Control => ["on", "off"].get(index).map(|v| json!(*v == "on")),
        Ctl::HdrMode => ["auto", "off"].get(index).map(|v| json!(v)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, glyph: usize, on: bool, removable: bool, status: &str) -> PackageRow {
        PackageRow { name: name.into(), description: format!("What the {name} is for."), can: vec![format!("Talks to the {name} over USB.")], version: "1.0.0".into(), status: status.into(), on, warning: None, glyph, removable, systems: Vec::new() }
    }

    /// The words on the page, each piece once.
    fn texts(devices: Vec<DeviceRow>) -> Vec<String> {
        let s = State { sensors: None, devices, plugins: Vec::new(), sections: Vec::new(), settings: Value::Null, top_peak: 1000.0, reference: 254.0 };
        page(&s, 1440.0).0.into_iter().map(|c| c.text.to_string()).filter(|t| !t.is_empty()).collect()
    }

    /// Peak nits reaches the brightest screen's peak wherever the window is:
    /// on a dim screen (white 73, peak 200) a saved 1000 shows as 1000 at the
    /// slider's right end, and a press there keeps it at 1000.
    #[test]
    fn peak_nits_reaches_the_brightest_screens_peak_on_a_dim_screen() {
        let settings = json!({ "hdr": { "lighting": "auto", "glow_limit": 1000.0 } });
        let s = State { sensors: None, devices: Vec::new(), plugins: Vec::new(), sections: Vec::new(), settings, top_peak: 1000.0, reference: 73.0 };
        let cells = page(&s, 1440.0).0;
        let slider = cells.iter().find(|c| c.id == id(Ctl::PeakNits)).expect("the Peak nits slider");
        assert_eq!(slider.value, 1.0, "the saved 1000 at the right end");
        assert!(cells.iter().any(|c| c.text.as_str() == "1000 nits"));
        assert_eq!(peak_at(1.0, 73.0, 1000.0), Some(1000.0), "a press at the right end keeps 1000");
        assert_eq!(peak_at(0.0, 73.0, 1000.0), None, "the left end: the design's brightness");
    }

    /// Settings' Devices: each core's device with how it is (connected, not
    /// plugged in, turned off), its switch, its permission, the programs that
    /// also have it open, and its core, which can be taken out once added.
    #[test]
    fn each_device_shows_with_its_core() {
        let dev = |core, kind: &str, present, reachable, holders: &[&str]| DeviceRow { core, kind: kind.into(), present, reachable, holders: holders.iter().map(|h| h.to_string()).collect(), holders_note: "Another program has it open.".into() };
        let t = texts(vec![
            dev(row("EVGA Z10 RGB", 0, true, false, "Comes with Ectomatter"), "keyboard", true, Some(true), &[]),
            dev(row("SteelSeries Sensei 310", 1, true, false, "Comes with Ectomatter"), "mouse", false, Some(false), &["openrgb"]),
            dev(row("Logitech G502", 4, false, true, "Added from a file"), "mouse", true, Some(true), &[]),
        ]);
        let has = |s: &str| t.iter().any(|x| x == s);
        assert!(has("Connected") && has("Granted"), "the keyboard as it is");
        assert!(has("Not connected") && has("Checked when the mouse is connected"), "a mouse not plugged in: its permission waits for it");
        assert!(has("openrgb") && has("Another program has it open."), "who else has it open, and what that means");
        assert!(has("Control is off"), "a core turned off says so");
        assert!(has("1.0.0 · Comes with Ectomatter") && has("1.0.0 · Added from a file"), "each core's version and where it came from");
        assert_eq!(t.iter().filter(|x| *x == "Remove").count(), 1, "only an added core can be taken out");
        assert!(has("When off, Ectomatter leaves the keyboard to other programs.") && has("When off, Ectomatter leaves the mouse to other programs."));
        assert!(has("Add a core") && !has("Cores"), "Add a core under Devices, and no Cores section of its own");
    }
}
