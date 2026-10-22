//! Ectodeck's own Settings, as the web app had them (shell.js at cd9ea82):
//! each device, whether Ectodeck controls it, whether it may open it and
//! what else has it open; the cores, and then the plugins, each with what it
//! can do, its version and whether it's on; the weather
//! every device shares, its place and its units; the HDR interface; and how
//! often the window draws a lighting effect.

use serde_json::{json, Value};

use std::collections::{BTreeSet, HashMap};

use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::sensor_list::{Catalog, Device, Part, Run, Sensor, Val};
use crate::text;
use crate::Cell;

const WMO: [(i32, &str); 28] = [
    (0, "Clear"), (1, "Mostly clear"), (2, "Partly cloudy"), (3, "Overcast"), (45, "Fog"), (48, "Freezing fog"),
    (51, "Light drizzle"), (53, "Drizzle"), (55, "Heavy drizzle"), (56, "Freezing drizzle"), (57, "Freezing drizzle"),
    (61, "Light rain"), (63, "Rain"), (65, "Heavy rain"), (66, "Freezing rain"), (67, "Freezing rain"),
    (71, "Light snow"), (73, "Snow"), (75, "Heavy snow"), (77, "Snow grains"), (80, "Light showers"), (81, "Showers"), (82, "Heavy showers"),
    (85, "Snow showers"), (86, "Heavy snow showers"), (95, "Thunderstorm"), (96, "Thunderstorm with hail"), (99, "Thunderstorm with hail"),
];
/// The sky in words, from the weather's code.
pub fn sky(code: i32) -> String {
    WMO.iter().find(|(k, _)| *k == code).map_or(format!("Weather code {code}"), |(_, n)| n.to_string())
}
const ALERTS: [(i32, &str); 3] = [(1, "Tropical storm"), (2, "Hurricane"), (3, "Tornado")];

/// the page's column, inside its padding, and a card's rows
const X0: f32 = 40.0;
const W: f32 = 860.0;
const PAD_X: f32 = 24.0;
const LABEL_W: f32 = 132.0;
const CTRL: f32 = 148.0;

/// What the Sensors box shows: the computer's sensors as last listed, each one's reading
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

/// What the Sensors box's pieces stand for, by their places: each tab's part, each
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
/// in its own words, what it can do in Ectodeck's, its version and where that
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
    /// added, rather than come with Ectodeck: it can be taken out
    pub removable: bool,
}

pub struct State<'a> {
    pub sensors: Option<Sensors>,
    pub cores: Vec<PackageRow>,
    pub plugins: Vec<PackageRow>,
    pub status: Value,
    pub control: bool,
    pub settings: Value,
    pub report: Option<ectodeck_weather::Report>,
    pub locale: &'a str,
    pub top_peak: f64,
    pub reference: f64,
    pub place_text: &'a str,
    /// the mouse, once Ectodeck has met it: what the system says of it, and whether it is plugged in
    pub mouse: Option<(Value, bool)>,
}

struct Card {
    y: f32,
    cells: Vec<Cell>,
}

impl Card {
    fn label(&mut self, s: &str) {
        self.cells.push(text_cell(style::LABEL, X0 + PAD_X, self.y, LABEL_W, 34.0, s));
    }
    fn note(&mut self, y: &mut f32, s: &str, w: f32) {
        for line in text::wrap(400, 12.5, s, w) {
            self.cells.push(text_cell(style::NOTE, X0 + PAD_X + CTRL, *y, w, 18.0, &line));
            *y += 18.125;
        }
    }
    /// A switch in the row's control column.
    fn seg(&mut self, y: f32, c: Ctl, opts: &[(&str, &str)], current: &str) -> f32 {
        let cx = X0 + PAD_X + CTRL;
        let mut bx = cx + 3.0;
        let mut buttons = Vec::new();
        for (j, (v, name)) in opts.iter().enumerate() {
            let w = text::width(400, 13.0, name) + 24.0;
            buttons.push(Cell { text: (*name).into(), on: *v == current, id: id(c), index: j as i32, ..cell(kind::SEG_BUTTON, bx, y + 3.0, w, 30.0) });
            bx += w + 2.0;
        }
        self.cells.push(cell(kind::SEG, cx, y, bx - 2.0 + 3.0 - cx, 36.0));
        self.cells.extend(buttons);
        36.0
    }
    fn state(&mut self, y: f32, tone: i32, s: &str) {
        let cx = X0 + PAD_X + CTRL;
        self.cells.push(Cell { style: tone, ..cell(kind::DOT, cx, y + 13.5, 7.0, 7.0) });
        self.cells.push(text_cell(style::LABEL, cx + 15.0, y, 600.0, 34.0, s));
    }
    fn next(&mut self, h: f32) {
        self.y += h.max(34.0) + 14.0;
    }
}

/// A heading, then a card under it; the card's pieces placed by `fill`.
fn section(cells: &mut Vec<Cell>, y: &mut f32, title: &str, fill: impl FnOnce(&mut Card)) {
    *y += 20.0;
    cells.push(text_cell(style::HEAD, X0, *y, W, 18.0, title));
    *y += 18.0;
    let mut card = Card { y: *y + 20.0, cells: Vec::new() };
    fill(&mut card);
    let h = card.y - 14.0 + 20.0 - *y;
    cells.push(cell(kind::TILE, X0, *y, W, h));
    cells.extend(card.cells);
    *y += h + 14.0;
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

/// Ectodeck's Settings' Sensors box: a tab for each part of the computer (the Screen tab's
/// own tabs), the open one's device and where its readings come from, then its rows, each
/// sensor a checkbox with its name, its driver's name and its reading, three to a line; long
/// runs of like readings folded into one line that opens; and at its foot what checking does.
fn sensors_box(cells: &mut Vec<Cell>, y: &mut f32, v: &Sensors, map: &mut SensorMap) {
    *y += 20.0;
    map.top = *y;
    cells.push(text_cell(style::HEAD, X0, *y, W, 18.0, "Sensors"));
    *y += 18.0 + 8.0;
    let parts = v.cat.parts();
    let tab = if parts.contains(&v.tab) { v.tab } else { parts.first().copied().unwrap_or(Part::Processor) };
    // the tabs, each as wide as its name; an amber dot on a part whose readings can't come in
    let mut tx = X0;
    for (i, p) in parts.iter().enumerate() {
        let bad = v.cat.missing.iter().any(|(m, _)| m == p);
        let w = text::width(500, 13.0, p.name()).ceil() + 36.0 + if bad { 13.0 } else { 0.0 };
        cells.push(Cell { text: p.name().into(), on: *p == tab, style: (i == 0) as i32 + if bad { 2 } else { 0 }, id: id(Ctl::SensorTab), index: i as i32, ..cell(kind::TAB, tx, *y, w, 36.0) });
        map.tabs.push(*p);
        tx += w + 4.0;
    }
    *y += 36.0;
    let top = *y;
    let (bx, bw) = (X0 + PAD_X, W - 2.0 * PAD_X);
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
            let nw = text::width(500, 13.5, &d.name).ceil() + 2.0;
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
        let lines = text::wrap(400, 13.0, why, bw - 14.0);
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
                crate::sensor_read::odd(s.what, *x).then(|| format!("{} reads {}: probably no sensor is connected to it.", v.setup.names.get(&s.id).unwrap_or(&s.name), crate::sensor_read::shown(s.what, tab, *x)))
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
            out.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, bx, ry, bw, 1.0) });
        }
        // the row's name, and under a card's its Main card or Make main (a model's code too
        // long for the column broken after a hyphen: WDS200T1X0E- / 00AFY0)
        let broken: Vec<String> = r.name.split(' ').map(|w| if text::width(400, 13.5, w) > CTRL - 12.0 { w.replacen('-', "- ", 1) } else { w.to_string() }).collect();
        let lines = text::wrap(400, 13.5, &broken.join(" "), CTRL - 12.0);
        for (i, l) in lines.iter().enumerate() {
            out.push(text_cell(style::LABEL, bx, ry + 6.0 + i as f32 * 18.0, CTRL - 12.0, 18.0, l));
        }
        if let Some((full, src)) = &r.tip {
            let w = lines.iter().map(|l| text::width(400, 13.5, l).ceil()).fold(0.0f32, f32::max);
            out.push(Cell { text: full.into(), fill: src.into(), ..cell(kind::TIP, bx, ry + 6.0, w, lines.len() as f32 * 18.0) });
        }
        let mut left_h = 6.0 + lines.len() as f32 * 18.0;
        if r.card.is_some() || r.inside.is_some() {
            let sy = ry + left_h + 4.0;
            let mut sx = bx;
            if let Some((d, is_main)) = r.card {
                if is_main {
                    let w = text::width(400, 11.5, "Main card").ceil() + 14.0;
                    out.push(Cell { text: "Main card".into(), style: 3, ..cell(kind::PILL, sx, sy + 1.0, w, 20.0) });
                    sx += w + 6.0;
                } else {
                    let w = text::width(400, 12.0, "Make main").ceil() + 16.0;
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
            for l in text::wrap(400, 12.0, n, cw - 16.0) {
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
    let count_w = text::width(400, 12.5, &count).ceil() + 2.0;
    cy += 8.0;
    out.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, bx, cy, bw, 1.0) });
    cy += 12.0;
    let lines = text::wrap(400, 12.5, "The sensors checked here are the readings you can add to a screen layout. Ectodeck checked the usual ones for you.", bw - count_w - 16.0);
    for (i, l) in lines.iter().enumerate() {
        out.push(text_cell(style::NOTE, bx, cy + i as f32 * 18.0, bw - count_w - 16.0, 18.0, l));
    }
    out.push(text_cell(style::NOTE, bx + bw - count_w, cy, count_w, 18.0, &count));
    cy += lines.len() as f32 * 18.0 + 14.0;
    let h = cy - top;
    cells.push(Cell { style: if parts.first() == Some(&tab) { 2 } else { 0 }, ..cell(kind::TILE, X0, top, W, h) });
    cells.extend(out);
    *y = top + h + 14.0;
}

/// A sensor's checkbox: its name (or the one given it), its driver's name in gray, its
/// reading at its right (amber when it can't be right), a pencil under the pointer; or, as
/// it is renamed, a line to type its new name in.
fn chip(out: &mut Vec<Cell>, v: &Sensors, checked: &BTreeSet<String>, part: Part, s: &Sensor, (x, y, w): (f32, f32, f32), map: &mut SensorMap) {
    let given = v.setup.names.get(&s.id);
    let name = given.cloned().unwrap_or_else(|| s.name.clone());
    // (a renamed sensor keeps its own name beside the one given it)
    let orig = if given.is_some() { s.orig.clone().or(Some(s.name.clone())) } else { s.orig.clone() };
    let reading = v.values.get(&s.id).map(|x| crate::sensor_read::shown(s.what, part, *x)).unwrap_or_default();
    let odd = v.values.get(&s.id).is_some_and(|x| crate::sensor_read::odd(s.what, *x));
    let on = checked.contains(&s.id);
    let index = map.chips.len() as i32;
    map.chips.push(s.id.clone());
    let editing = v.renaming.as_ref().filter(|(id, _)| *id == s.id);
    // the name's room: the chip less its checkbox, its driver's name and its reading (the
    // window takes the pencil's place from it while the pencil shows)
    let orig_w = orig.as_ref().map_or(0.0, |o| text::width(400, 11.5, o).ceil() + 8.0);
    let room = (w - 8.0 - 15.0 - 8.0 - orig_w - 8.0 - text::width(400, 12.5, &reading).ceil() - 8.0).max(24.0);
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

pub fn page(s: &State) -> (Vec<Cell>, f32, SensorMap) {
    let mut cells = vec![text_cell(style::TITLE, X0, 34.0, W, 29.0, "Settings")];
    let mut y = 34.0 + 29.0 + 6.0 - 14.0;
    y += 14.0;

    // each device: its glyph and name with how it is, then its settings
    let st = &s.status;
    let present = st["present"].as_bool().unwrap_or(false);
    let reachable = st["reachable"].as_bool();
    section(&mut cells, &mut y, "Devices", |c| {
        let line = if !present { "Not connected" } else if reachable == Some(false) { "Ectodeck can’t open this keyboard" } else if !s.control { "Control is off" } else { "Connected" };
        c.cells.push(cell(kind::DEVICE, X0 + PAD_X, c.y, 48.0, 48.0));
        c.cells.push(text_cell(style::NAME, X0 + PAD_X + 62.0, c.y + 3.0, 600.0, 21.0, "EVGA Z10 RGB"));
        c.cells.push(text_cell(style::SMALL, X0 + PAD_X + 62.0, c.y + 27.0, 600.0, 17.0, line));
        c.y += 48.0 + 18.0;
        c.label("Control");
        let mut ny = c.y + 36.0 + 7.0;
        c.seg(c.y, Ctl::Control, &[("on", "On"), ("off", "Off")], if s.control { "on" } else { "off" });
        c.note(&mut ny, "When off, Ectodeck leaves the keyboard to other programs.", 664.0);
        c.next(ny - c.y - 0.125);
        c.label("Permission");
        match reachable {
            None => c.state(c.y, 0, "Not needed on this system"),
            Some(_) if !present => c.state(c.y, 0, "Checked when the keyboard is connected"),
            Some(true) => c.state(c.y, 1, "Granted"),
            Some(false) => c.state(c.y, 2, "Not granted"),
        }
        c.next(34.0);
        let holders: Vec<String> = st["holders"].as_array().map(|a| a.iter().filter_map(|h| h["name"].as_str().map(String::from)).collect()).unwrap_or_default();
        if !holders.is_empty() {
            c.label("Also open in");
            c.cells.push(text_cell(style::VALUE_BRIGHT, X0 + PAD_X + CTRL, c.y, 600.0, 34.0, &holders.join(", ")));
            let mut ny = c.y + 34.0 + 7.0;
            c.note(&mut ny, "Two programs sending lighting at once make the keys flicker. Close the other program, or turn Live lighting off here.", 664.0);
            c.next(ny - c.y);
        }
        // the mouse, once met: how it is, and who else has it open
        if let Some((m, present)) = &s.mouse {
            c.y += 10.0;
            c.cells.push(Cell { color: slint::Color::from_rgb_u8(0x2b, 0x2b, 0x30), ..cell(kind::BOX, X0 + PAD_X, c.y, 664.0 + CTRL, 1.0) });
            c.y += 24.0;
            c.cells.push(Cell { style: 1, ..cell(kind::DEVICE, X0 + PAD_X, c.y, 48.0, 48.0) });
            c.cells.push(text_cell(style::NAME, X0 + PAD_X + 62.0, c.y + 3.0, 600.0, 21.0, "SteelSeries Sensei 310"));
            c.cells.push(text_cell(style::SMALL, X0 + PAD_X + 62.0, c.y + 27.0, 600.0, 17.0, if *present { "Connected" } else { "Not connected" }));
            c.y += 48.0 + 18.0;
            let holders: Vec<String> = m["holders"].as_array().map(|a| a.iter().filter_map(|h| h["name"].as_str().map(String::from)).collect()).unwrap_or_default();
            if !holders.is_empty() {
                c.label("Also open in");
                c.cells.push(text_cell(style::VALUE_BRIGHT, X0 + PAD_X + CTRL, c.y, 600.0, 34.0, &holders.join(", ")));
                let mut ny = c.y + 34.0 + 7.0;
                c.note(&mut ny, "Programs that both set the mouse undo each other's changes: whichever sets its lights or buttons last wins. Ectodeck changes the mouse only when you do, here.", 664.0);
                c.next(ny - c.y);
            }
        }
    });

    // the cores, each for a device or the devices another program drives; then the plugins
    packages_section(&mut cells, &mut y, "Cores", 0, &s.cores, "Add a core", "Ectodeck checks each core’s GitHub releases for updates once a day.");
    packages_section(&mut cells, &mut y, "Plugins", 1, &s.plugins, "Add a plugin", "Ectodeck checks each plugin’s GitHub releases for updates once a day.");

    // the weather every device shares
    let place = &s.settings["weather"]["place"];
    let has_place = place.is_object();
    let d = ectodeck_weather::units::units_for(s.locale);
    let chosen = &s.settings["weather"]["units"];
    let tu = chosen["temperature"].as_str().unwrap_or(d.temperature).to_string();
    let wu = chosen["wind"].as_str().unwrap_or(d.wind).to_string();
    section(&mut cells, &mut y, "Weather", |c| {
        c.label("Place");
        c.cells.push(Cell { text: s.place_text.into(), id: id(Ctl::Place), style: 1, ..cell(kind::FIELD, X0 + PAD_X + CTRL, c.y, 380.0, 35.56) });
        let mut ny = c.y + 35.56 + 7.0;
        let where_ = if has_place {
            [place["name"].as_str(), place["region"].as_str(), place["country"].as_str()].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ")
        } else {
            "Every device that shows weather uses this place.".into()
        };
        c.note(&mut ny, &where_, 664.0);
        c.next(ny - c.y - 0.125);
        if has_place {
            c.label("Now");
            let (bx, by) = (X0 + PAD_X + CTRL, c.y);
            c.cells.push(cell(kind::WBOX, bx, by, 380.0, 100.0));
            match &s.report {
                None => c.cells.push(text_cell(style::LABEL, bx + 16.0, by + 14.0, 340.0, 26.0, "Getting the weather…")),
                Some(r) => {
                    let t = if tu == "f" { r.temperature * 9.0 / 5.0 + 32.0 } else { r.temperature };
                    let temp = format!("{}°{}", t.round(), tu.to_uppercase());
                    let cond = WMO.iter().find(|(k, _)| *k == r.code).map_or(format!("Weather code {}", r.code), |(_, n)| n.to_string());
                    let tw = text::width(500, 22.0, &temp) - 0.44 * temp.chars().count() as f32;
                    c.cells.push(text_cell(style::BIG, bx + 16.0, by + 14.0, tw + 4.0, 26.0, &temp));
                    c.cells.push(text_cell(style::LABEL_SOFT, bx + 16.0 + tw + 12.0, by + 22.0, 200.0, 16.0, &cond));
                    let wind = |k: f32| match wu.as_str() {
                        "mph" => format!("{} mph", (k / 1.609).round()),
                        "ms" => format!("{} m/s", (k / 3.6).round()),
                        _ => format!("{} km/h", k.round()),
                    };
                    let clock = |secs: f32| crate::lcd_page::twelve_hour((secs / 3600.0).floor().clamp(0.0, 23.0) as u32, ((secs % 3600.0) / 60.0).floor() as u32);
                    let facts = [("Cloud", format!("{}%", (r.cloud * 100.0).round())), ("Wind", wind(r.wind_speed)), ("Sunrise", clock(r.sunrise)), ("Sunset", clock(r.sunset))];
                    // two to a line, the first column as wide as its widest
                    let col0 = facts.iter().step_by(2).map(|(k, v)| text::width(400, 13.0, &format!("{k} ")) + text::width(500, 13.0, v)).fold(0.0f32, f32::max);
                    for (i, (k, v)) in facts.iter().enumerate() {
                        let fx = bx + 16.0 + if i % 2 == 1 { col0 + 28.0 } else { 0.0 };
                        let fy = by + 46.0 + (i / 2) as f32 * 22.0;
                        let kw = text::width(400, 13.0, &format!("{k} "));
                        c.cells.push(text_cell(style::FACT, fx, fy, kw + 2.0, 18.0, k));
                        c.cells.push(text_cell(style::FACT_VALUE, fx + kw, fy, 120.0, 18.0, v));
                    }
                }
            }
            c.next(100.0);
        }
        c.label("Warnings");
        let country = place["country_code"].as_str().unwrap_or("");
        let official = ectodeck_weather::alerts::source(country).map(|x| x.name());
        match (has_place, official) {
            (false, _) => c.state(c.y, 0, "Chosen with the place"),
            (true, Some(n)) => c.state(c.y, 1, n),
            (true, None) => c.state(c.y, 3, "Read from the weather"),
        }
        let mut ny = c.y + 34.0 + 7.0;
        if has_place {
            let alert = s.report.as_ref().map_or(0, |r| r.alert);
            let in_force = ALERTS.iter().find(|(k, _)| *k == alert).map_or(String::new(), |(_, n)| format!("{n} {}. ", if official.is_some() { "warning in force" } else { "conditions now" }));
            let note = format!(
                "{in_force}{}",
                if official.is_some() {
                    "Official warnings for this place, checked every 3 minutes."
                } else {
                    "No warning service covers this country yet, so storm-force wind with rain stands in for an official warning. A tornado cannot be told from the weather alone."
                }
            );
            c.note(&mut ny, &note, 664.0);
        }
        c.next(ny - c.y - 0.125 - if has_place { 0.0 } else { 7.0 });
        c.label("Temperature");
        c.seg(c.y, Ctl::Temperature, &[("c", "Celsius"), ("f", "Fahrenheit")], &tu);
        c.next(36.0);
        c.label("Wind");
        c.seg(c.y, Ctl::Wind, &[("kmh", "km/h"), ("mph", "mph"), ("ms", "m/s")], &wu);
        let mut ny = c.y + 36.0 + 7.0;
        if chosen["temperature"].is_null() && chosen["wind"].is_null() {
            c.note(&mut ny, &format!("Set from this computer's region ({}). Change either and Ectodeck keeps your choice.", s.locale.replace('-', "_")), 664.0);
        } else {
            ny -= 7.0;
        }
        c.next(ny - c.y - 0.125);
        let credit = "Weather and air from Open-Meteo (open-meteo.com, CC BY 4.0), every 15 minutes. Places from Open-Meteo, and from Photon over OpenStreetMap (ODbL) for names Open-Meteo cannot match. Warnings every 3 minutes from the National Weather Service, Environment and Climate Change Canada, or MeteoAlarm (meteoalarm.org, CC BY 4.0).";
        let mut cy = c.y;
        for line in text::wrap(400, 12.0, credit, 640.0) {
            c.cells.push(text_cell(style::CREDIT, X0 + PAD_X + CTRL, cy, 640.0, 18.0, &line));
            cy += 18.0;
        }
        c.next(cy - c.y);
    });

    // every sensor on the computer, a tab for each part, and which the screen's layouts can show
    let mut map = SensorMap::default();
    if let Some(v) = &s.sensors {
        sensors_box(&mut cells, &mut y, v, &mut map);
    }

    // the HDR interface
    let hdr = &s.settings["hdr"];
    let on = hdr["lighting"].as_str() != Some("off");
    section(&mut cells, &mut y, "HDR", |c| {
        c.label("HDR interface");
        c.seg(c.y, Ctl::HdrMode, &[("auto", "On"), ("off", "Off")], if on { "auto" } else { "off" });
        let mut ny = c.y + 36.0 + 7.0;
        c.note(&mut ny, "Lit keys glow brighter than the desktop’s white on screens with room for it, up to Peak nits. At the left, they keep the design’s brightness.", 664.0);
        c.next(ny - c.y - 0.125);
        c.label("Peak nits");
        let (lo, hi) = (s.reference.round(), s.top_peak.max(s.reference + 1.0).round());
        let nits = hdr["glow_limit"].as_f64().map_or(lo, |v| v.clamp(lo, hi));
        let frac = ((nits - lo) / (hi - lo)) as f32;
        let cx = X0 + PAD_X + CTRL;
        let vw = 56.0;
        c.cells.push(Cell { value: frac, id: id(Ctl::PeakNits), ..cell(kind::SLIDER, cx + 2.0, c.y + 9.0, 664.0 - vw - 14.0 - 4.0, 16.0) });
        c.cells.push(text_cell(style::VALUE, cx + 664.0 - vw, c.y + 8.0, vw, 18.0, &format!("{} nits", nits.round())));
        c.next(34.0);
    });

    (cells, y - 14.0 + 40.0, map)
}

/// Settings' Cores or Plugins: a row for each package, its glyph on its tile,
/// its name, what it's for and what it can do (and what it says is wrong, in
/// amber), its version and where that stands, and On and Off; then Add (a
/// core, `add` 0, or a plugin, 1), and how updates come.
fn packages_section(cells: &mut Vec<Cell>, y: &mut f32, title: &str, add: i32, rows: &[PackageRow], add_words: &str, note: &str) {
    *y += 20.0;
    cells.push(text_cell(style::HEAD, X0, *y, W, 18.0, title));
    *y += 18.0;
    let top = *y + 2.0;
    let (bx, bw) = (X0 + PAD_X, W - 2.0 * PAD_X);
    let mut out: Vec<Cell> = Vec::new();
    let mut ry = top;
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.push(Cell { color: slint::Color::from_argb_u8(9, 255, 255, 255), ..cell(kind::BOX, bx, ry, bw, 1.0) });
        }
        ry += 16.0;
        // the switch, then the version, from the right
        let opts = [("on", "On"), ("off", "Off")];
        let bws: Vec<f32> = opts.iter().map(|(_, n)| text::width(400, 12.5, n) + 28.0).collect();
        let seg_w = bws.iter().sum::<f32>() + 2.0 + 6.0;
        let sx = bx + bw - seg_w;
        out.push(cell(kind::SEG, sx, ry, seg_w, 34.0));
        let mut x = sx + 3.0;
        for (j, ((_, n), w)) in opts.iter().zip(&bws).enumerate() {
            out.push(Cell { text: (*n).into(), on: (j == 0) == r.on, id: id(Ctl::PackageOn), index: (r.glyph * 2 + j) as i32, ..cell(kind::SEG_BUTTON, x, ry + 3.0, *w, 28.0) });
            x += w + 2.0;
        }
        // (each line to the right, against the switch)
        let right = sx - 16.0;
        let (w1, w2) = (text::width(400, 12.5, &r.version).ceil() + 2.0, text::width(400, 12.5, &r.status).ceil() + 2.0);
        let vx = right - w1.max(w2);
        out.push(text_cell(style::NOTE, right - w1, ry + 1.0, w1, 18.0, &r.version));
        out.push(Cell { color: slint::Color::from_rgb_u8(0xa1, 0xa1, 0xaa), ..text_cell(style::NOTE_IN, right - w2, ry + 19.0, w2, 18.0, &r.status) });
        if r.removable {
            let w3 = text::width(500, 12.5, "Remove").ceil() + 18.0;
            out.push(Cell { text: "Remove".into(), id: id(Ctl::PackageRemove), index: r.glyph as i32, ..cell(kind::LINK, right - w3 + 9.0, ry + 40.0, w3, 26.0) });
        }
        // its tile, and its words
        out.push(Cell { style: 2 + r.glyph as i32, ..cell(kind::DEVICE, bx, ry, 40.0, 40.0) });
        let tx = bx + 40.0 + 16.0;
        let tw = vx - 16.0 - tx;
        out.push(text_cell(style::NAME, tx, ry, tw, 20.0, &r.name));
        let mut ty = ry + 22.0;
        for l in text::wrap(400, 12.5, &r.description, tw) {
            out.push(Cell { color: slint::Color::from_rgb_u8(0xa1, 0xa1, 0xaa), ..text_cell(style::NOTE_IN, tx, ty, tw, 18.0, &l) });
            ty += 18.0;
        }
        for c in &r.can {
            for l in text::wrap(400, 12.5, c, tw) {
                out.push(text_cell(style::NOTE, tx, ty, tw, 18.0, &l));
                ty += 18.0;
            }
        }
        if let Some(w) = &r.warning {
            ty += 6.0;
            let lines = text::wrap(400, 12.5, w, tw - 15.0);
            out.push(Cell { style: 3, ..cell(kind::DOT, tx, ty + 6.0, 7.0, 7.0) });
            for l in &lines {
                out.push(Cell { color: slint::Color::from_rgb_u8(0xf0, 0xb3, 0x5a), ..text_cell(style::NOTE_IN, tx + 15.0, ty, tw - 15.0, 18.0, l) });
                ty += 18.0;
            }
        }
        ry = ty.max(ry + 40.0) + 16.0;
    }
    // Add a plugin, and how updates come
    out.push(Cell { color: slint::Color::from_argb_u8(9, 255, 255, 255), ..cell(kind::BOX, bx, ry, bw, 1.0) });
    ry += 16.0;
    let aw = text::width(500, 13.0, add_words).ceil() + 12.0 + 14.0 + 7.0 + 12.0;
    out.push(Cell { text: add_words.into(), lines: "M8 3v10M3 8h10".into(), style: 35, id: id(Ctl::PackageAdd), index: add, ..cell(kind::GHOST, bx, ry, aw, 30.0) });
    out.push(text_cell(style::NOTE, bx + aw + 14.0, ry + 6.0, bw - aw - 14.0, 18.0, note));
    ry += 30.0 + 18.0;
    let h = ry - *y;
    cells.push(cell(kind::TILE, X0, *y, W, h));
    cells.extend(out);
    *y += h + 14.0;
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
        Ctl::Temperature => ["c", "f"].get(index).map(|v| json!(v)),
        Ctl::Wind => ["kmh", "mph", "ms"].get(index).map(|v| json!(v)),
        Ctl::HdrMode => ["auto", "off"].get(index).map(|v| json!(v)),
        _ => None,
    }
}
