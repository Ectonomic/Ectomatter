//! Ectomatter's own Settings, as the web app had them (shell.js at cd9ea82):
//! each device with its core: whether Ectomatter controls it, whether it may
//! open it, what else has it open; the plugins, each with what it can do,
//! its version and whether it's on, then the sections the plugins describe
//! of themselves (the weather's place and units), drawn here; the sensors the
//! Sensor Feed reads, and which the screens show; the HDR interface. About is
//! a page of its own (ui/pages/about.slint, front/about_page.rs).

use serde_json::{json, Value};

use std::collections::{BTreeSet, HashMap};

use crate::form::{self, Card, Form};
use crate::tokens::{font, ink, line, part, size, space};
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
    /// a newer version that can do more waits for the person to review it
    pub review: bool,
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
    /// whether the system lets Ectomatter open it (`None` where it isn't asked), and
    /// whether it has no rule that would, for Allow to write one (permission.rs)
    pub reachable: Option<bool>,
    pub allow: bool,
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
    /// what starts at login (autostart.rs)
    pub starts: crate::autostart::Starts,
}

/// A switch in a row's controls' column, its options' words and values; how wide it is.
fn seg(c: &mut Card, y: f32, ctl: Ctl, opts: &[(&str, &str)], current: &str) -> f32 {
    let names: Vec<(&str, &str)> = opts.iter().map(|(_, n)| (*n, "")).collect();
    let chosen = opts.iter().position(|(v, _)| *v == current);
    let x = c.control_x();
    form::switch(&mut c.cells, x, y, None, &names, chosen, |j| (id(ctl), j as i32, 0))
}

/// A row's state in its controls' column: its dot and its words, on a line of words
/// level with the label (the row's label set with `label_text`).
fn state(c: &mut Card, y: f32, tone: i32, s: &str) {
    let x = c.control_x();
    c.status(x, y, line::TEXT, tone, s);
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

/// The Sensors box in Ectomatter's Settings (General): a tab for each part of the computer (the Screen tab's
/// own tabs), the open one's device and where its readings come from, then its rows, each
/// sensor a checkbox with its name, its driver's name and its reading, three to a line; long
/// runs of like readings folded into one line that opens; and at its foot what checking does.
fn sensors_box(f: &mut Form, v: &Sensors, map: &mut SensorMap) {
    // (Ectomatter's box for every reading a plugin gives, whichever plugin)
    f.heading("Sensors", "");
    map.top = f.y - line::NOTE - space::LABEL;
    let (x0, w0) = (f.x, f.w);
    let parts = v.cat.parts();
    let tab = if parts.contains(&v.tab) { v.tab } else { parts.first().copied().unwrap_or(Part::Processor) };
    // the tabs, each as wide as its name; an amber dot on a part whose readings can't come in
    let mut tx = x0;
    for (i, p) in parts.iter().enumerate() {
        let bad = v.cat.missing.iter().any(|(m, _)| m == p);
        let w = text::width(font::MEDIUM, font::CONTROL, p.name()).ceil() + 2.0 * part::TAB_PAD + if bad { part::GLYPH_GAP + size::DOT } else { 0.0 };
        f.cells.push(Cell { text: p.name().into(), on: *p == tab, style: (i == 0) as i32 + if bad { 2 } else { 0 }, id: id(Ctl::SensorTab), index: i as i32, ..cell(kind::TAB, tx, f.y, w, size::TAB) });
        map.tabs.push(*p);
        tx += w + space::TIGHT;
    }
    f.y += size::TAB;
    let top = f.y;
    let (bx, bw) = (x0 + PAD_X, w0 - 2.0 * PAD_X);
    let (cx, cw) = (bx + CTRL, bw - CTRL);
    let mut out: Vec<Cell> = Vec::new();
    let mut cy = top + space::PAD;
    let main = v.setup.main_card(&v.cat).map(|d| d.key.clone());
    let checked = v.setup.checked_in(&v.cat);
    let mut devices: Vec<&Device> = v.cat.devices.iter().filter(|d| d.part == tab).collect();
    // (the main card first)
    devices.sort_by_key(|d| Some(&d.key) != main.as_ref());
    // the device the tab shows, and where its readings come from
    if !tab.by_device() {
        if let Some(d) = devices.first() {
            let nw = text::width(font::MEDIUM, font::TEXT, &d.name).ceil() + 2.0;
            out.push(text_cell(39, bx, cy, nw, line::TEXT, &d.name));
            if !d.src.is_empty() && !v.cat.missing.iter().any(|(p, _)| *p == tab) {
                out.push(text_cell(style::NOTE, bx + nw + space::INLINE, cy, bw - nw - space::INLINE, line::TEXT, &d.src));
            }
            cy += line::TEXT + space::INLINE;
        }
    }
    // a source that can't be read, said in amber
    if let Some((_, why)) = v.cat.missing.iter().find(|(p, _)| *p == tab) {
        cy = form::warning(&mut out, bx, cy, bw, why) + space::INLINE;
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
    // (three chips to a line, Space.inline apart; an open run's four to a line in a well,
    // Space.inline in from it; a row Space.inline in from the rules round it, its name's
    // lines at the middle of its chips' first line)
    let gap = space::INLINE;
    let col = (cw - 2.0 * gap) / 3.0;
    let sub_col = (cw - 2.0 * gap - 3.0 * gap) / 4.0;
    let name_w = CTRL - space::ROW;
    let name_top = gap + (size::SENSOR - line::TEXT) / 2.0;
    for (k, r) in rows.iter().enumerate() {
        let ry = cy;
        if k > 0 {
            out.push(form::rule(bx, ry, bw));
        }
        // the row's name, and under a card's its Main card or Make main (a model's code too
        // long for the column broken after a hyphen: WDS200T1X0E- / 00AFY0)
        let broken: Vec<String> = r.name.split(' ').map(|w| if text::width(font::REGULAR, font::TEXT, w) > name_w { w.replacen('-', "- ", 1) } else { w.to_string() }).collect();
        let lines = text::wrap(font::REGULAR, font::TEXT, &broken.join(" "), name_w);
        for (i, l) in lines.iter().enumerate() {
            out.push(text_cell(style::LABEL, bx, ry + name_top + i as f32 * line::TEXT, name_w, line::TEXT, l));
        }
        if let Some((full, src)) = &r.tip {
            let w = lines.iter().map(|l| text::width(font::REGULAR, font::TEXT, l).ceil()).fold(0.0f32, f32::max);
            out.push(Cell { text: full.into(), fill: src.into(), ..cell(kind::TIP, bx, ry + name_top, w, lines.len() as f32 * line::TEXT) });
        }
        let mut left_h = name_top + lines.len() as f32 * line::TEXT;
        if r.card.is_some() || r.inside.is_some() {
            let sy = ry + left_h + space::TIGHT;
            let mut sx = bx;
            if let Some((d, is_main)) = r.card {
                if is_main {
                    sx += form::pill(&mut out, 3, sx, sy, "Main card") + space::INLINE;
                } else {
                    let w = form::small_button_width("Make main");
                    out.push(Cell { text: "Make main".into(), style: 33, id: id(Ctl::MakeMain), index: map.mains.len() as i32, ..cell(kind::GHOST, sx, sy + (line::TEXT - size::PILL) / 2.0, w, size::PILL) });
                    map.mains.push(d.key.clone());
                    sx += w + space::INLINE;
                }
            }
            if let Some(i) = &r.inside {
                out.push(text_cell(style::CREDIT, sx, sy, CTRL - (sx - bx), line::TEXT, i));
            }
            left_h += space::TIGHT + line::TEXT;
        }
        // its sensors, three to a line, then its folded runs; a renamed sensor, or one whose
        // name and driver's name run long together, takes two places
        let mut place = (0usize, 0usize);
        let mut at = |span: usize| -> (f32, f32, f32) {
            if place.1 + span > 3 {
                place = (place.0 + 1, 0);
            }
            let p = (cx + place.1 as f32 * (col + gap), ry + gap + place.0 as f32 * size::SENSOR, span as f32 * col + (span - 1) as f32 * gap);
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
            out.push(Cell { text: name.into(), lines: said.into(), style: if open { 3 } else { 2 }, id: id(Ctl::SensorRun), index: map.runs.len() as i32, ..cell(kind::SENSOR, x, sy, w, size::SENSOR) });
            map.runs.push((r.key.clone(), *run));
        }
        let used = if r.items.is_empty() && r.runs.is_empty() { 0 } else { place.0 + 1 };
        let mut right = gap + used as f32 * size::SENSOR;
        // a run opened: its readings four to a line in a well
        for (run, items) in &r.runs {
            if !v.open.contains(&(r.key.clone(), *run)) {
                continue;
            }
            let wy = ry + right + space::TIGHT;
            let lines = items.len().div_ceil(4);
            let h = 2.0 * gap + lines as f32 * size::SENSOR;
            out.push(Cell { style: 1, ..cell(kind::WBOX, cx, wy, cw, h) });
            for (i, s) in items.iter().enumerate() {
                chip(&mut out, v, &checked, tab, s, (cx + gap + (i % 4) as f32 * (sub_col + gap), wy + gap + (i / 4) as f32 * size::SENSOR, sub_col), map);
            }
            right += space::TIGHT + h + space::TIGHT;
        }
        // the lines under it, in from the chips' edges as their words are
        for n in &r.notes {
            right += space::TIGHT;
            let (nx, nw) = (cx + part::SENSOR_PAD, cw - 2.0 * part::SENSOR_PAD);
            for l in text::wrap(font::REGULAR, font::CAPTION, n, nw) {
                out.push(text_cell(style::CREDIT, nx, ry + right, nw, line::NOTE, &l));
                right += line::NOTE;
            }
        }
        cy = ry + left_h.max(right) + gap;
    }
    // the foot: what checking does, and how many are checked
    let all: Vec<&Sensor> = v.cat.devices.iter().flat_map(|d| d.sensors.iter()).collect();
    let on = all.iter().filter(|s| checked.contains(&s.id)).count();
    let count = format!("{on} of {} checked", all.len());
    let count_w = text::width(font::REGULAR, font::NOTE, &count).ceil() + 2.0;
    out.push(form::rule(bx, cy, bw));
    cy += 1.0 + space::ROW;
    let end = form::note_lines(&mut out, bx, cy, bw - count_w - space::PAD, style::NOTE, "The sensors checked here are the readings you can add to a screen layout. Ectomatter checked the usual ones for you.");
    out.push(text_cell(style::NOTE, bx + bw - count_w, cy, count_w, line::NOTE, &count));
    cy = end + space::PAD;
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
    // (the window reckons the name's room: the chip less its checkbox, its driver's name,
    // its reading and, while it shows, the pencil)
    out.push(Cell {
        text: name.into(),
        fill: orig.unwrap_or_default().into(),
        lines: reading.into(),
        on,
        style: if editing.is_some() { 4 } else if odd { 1 } else { 0 },
        id: id(Ctl::Sensor),
        index,
        ..cell(kind::SENSOR, x, y, w, size::SENSOR)
    });
    if let Some((_, typed)) = editing {
        // the line to type its new name in, where its name is, Part.inset in from the chip
        let nx = part::SENSOR_PAD + size::CHECK + part::CHECK_GAP;
        out.push(Cell { text: typed.into(), style: 7, id: id(Ctl::SensorName), index, ..cell(kind::FIELD, x + nx, y + part::INSET, w - nx - part::SENSOR_PAD, size::SENSOR - 2.0 * part::INSET) });
    }
}

/// Settings' tabs, in the bar's order: Ectomatter's own settings and what plugins add to
/// them; a box for each device; a box for each plugin; About.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    General,
    Devices,
    Plugins,
    About,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::General, Tab::Devices, Tab::Plugins, Tab::About];

    /// The tab at a place in the bar (the first where there's none there).
    pub fn at(i: i32) -> Tab {
        Tab::ALL.get(i.max(0) as usize).copied().unwrap_or_default()
    }

    /// The tab's name, as the bar and a saved place have it.
    pub fn name(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Devices => "Devices",
            Tab::Plugins => "Plugins",
            Tab::About => "About",
        }
    }

    /// The tab a name names, in any case.
    pub fn named(n: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(n))
    }
}

/// A tab's page, a form (src/form.rs) `room` across under the page's bar, Space.margin
/// down (Settings has no hint over it).
pub fn page(s: &State, tab: Tab, room: f32) -> (Vec<Cell>, f32, SensorMap, PluginMap) {
    let mut f = Form::new(X0, crate::tokens::space::MARGIN, room - 2.0 * X0);
    let (mut map, mut pmap) = (SensorMap::default(), PluginMap::default());
    match tab {
        Tab::General => {
            ectomatter_section(&mut f, s);
            // the sections the plugins describe of themselves, each titled in its plugin's words
            for p in &s.sections {
                plugin_section(&mut f, p, &mut pmap);
            }
            // every sensor on the computer, a tab for each part, and which the screen's layouts can show
            if let Some(v) = &s.sensors {
                sensors_box(&mut f, v, &mut map);
            }
        }
        // each device with its core, a box each, then Add a device core
        Tab::Devices => devices_section(&mut f, &s.devices),
        // each plugin, a box each, then Add a plugin
        Tab::Plugins => packages_section(&mut f, "Plugins", 1, &s.plugins, "Add a plugin", "Ectomatter checks each plugin’s GitHub releases for updates once a day."),
        // (About is a page of its own, drawn by the window: ui/pages/about.slint, front/about_page.rs)
        Tab::About => {}
    }
    let (cells, h) = f.end();
    (cells, h, map, pmap)
}

/// Ectomatter's own settings, in one box: whether it starts at login, as the system keeps
/// it (this copy, On; none, Off; or, neither chosen, another copy, named under the note),
/// and the HDR interface with its Peak nits.
fn ectomatter_section(f: &mut Form, s: &State) {
    let hdr = &s.settings["hdr"];
    let on = hdr["lighting"].as_str() != Some("off");
    section(f, "Ectomatter", |c| {
        use crate::autostart::Starts;
        c.row();
        c.label("Start at login");
        let top = c.y;
        let now = match &s.starts {
            Starts::This => "on",
            Starts::No => "off",
            Starts::Other(_) => "",
        };
        seg(c, top, Ctl::StartAtLogin, &[("on", "On"), ("off", "Off")], now);
        let mut end = c.note(top + form::LINE, "Ectomatter starts in the tray when you log in. Off, it does nothing until you open it.");
        if let Starts::Other(p) = &s.starts {
            end = c.note(end, &format!("Another copy of Ectomatter starts at login now, from {}. On starts this copy instead.", p.display()));
        }
        c.next(top, end - top);
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
        // the app's slider, at the middle of the row's first line, and how bright it says at
        // its right, Space.row after it (as wide at every place as at its brightest, so the
        // slider never shifts under the pointer)
        let vw = text::width(font::REGULAR, font::CONTROL, &format!("{} nits", hi)).ceil() + 2.0;
        c.cells.push(Cell { value: frac, color: crate::lighting::run(), id: id(Ctl::PeakNits), ..cell(kind::FILLED, cx, top + (form::LINE - size::SLIDER) / 2.0, cw - vw - space::ROW, size::SLIDER) });
        c.cells.push(text_cell(style::VALUE, cx + cw - vw, top, vw, form::LINE, &format!("{} nits", nits.round())));
        c.next(top, form::LINE);
    });
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
        let field_w = c.control_w().min(FIELD_MOST);
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
                // the card: a well, its reading in large on its first line (Line.title's room
                // Part.title-top down, as a box's title), the words beside the reading
                // Space.row after it, then its facts two to a line, Space.tight apart, the
                // first column as wide as its widest and Space.margin before the second; the
                // card Space.pad in all round
                let (fx0, inner) = (bx + space::PAD, field_w - 2.0 * space::PAD);
                let first = by + part::TITLE_TOP;
                let facts: Vec<(String, String)> = card["facts"].as_array().unwrap_or(&empty).iter().map(|f| (words(&f[0]), words(&f[1]))).collect();
                let pitch = line::NOTE + space::TIGHT;
                let fy0 = first + line::TITLE + space::ROW;
                let h = if card["waiting"].is_string() { part::TITLE_TOP + line::TITLE + part::TITLE_TOP } else { fy0 - by + facts.len().div_ceil(2) as f32 * pitch - space::TIGHT + space::PAD };
                c.cells.push(cell(kind::WBOX, bx, by, field_w, h));
                if let Some(w) = card["waiting"].as_str() {
                    c.cells.push(text_cell(style::LABEL, fx0, first, inner, line::TITLE, w));
                } else {
                    let big = words(&card["big"]);
                    let tw = text::width(font::MEDIUM, font::DISPLAY, &big).ceil() + 2.0;
                    c.cells.push(text_cell(style::BIG, fx0, first, tw, line::TITLE, &big));
                    let bx2 = fx0 + tw + space::ROW;
                    c.cells.push(text_cell(style::LABEL_SOFT, bx2, first, (fx0 + inner - bx2).max(1.0), line::TITLE, &words(&card["beside"])));
                    let kw_of = |k: &str| text::width(font::REGULAR, font::CONTROL, &format!("{k} "));
                    let col0 = facts.iter().step_by(2).map(|(k, v)| kw_of(k) + text::width(font::MEDIUM, font::CONTROL, v)).fold(0.0f32, f32::max).ceil();
                    for (i, (k, v)) in facts.iter().enumerate() {
                        let fx = fx0 + if i % 2 == 1 { col0 + space::MARGIN } else { 0.0 };
                        let fy = fy0 + (i / 2) as f32 * pitch;
                        let kw = kw_of(k);
                        let vw = text::width(font::MEDIUM, font::CONTROL, v).ceil() + 2.0;
                        c.cells.push(text_cell(style::FACT, fx, fy, kw + 2.0, line::NOTE, k));
                        c.cells.push(text_cell(style::FACT_VALUE, fx + kw, fy, vw, line::NOTE, v));
                    }
                }
                c.next(by, h);
            } else if let Some(st) = part.get("status") {
                c.row();
                c.label_text(&words(&st["label"]));
                let top = c.y;
                let tone = match st["tone"].as_str() {
                    Some("good") => 1,
                    Some("bad") => 2,
                    Some("warn") => 3,
                    _ => 0,
                };
                state(c, top, tone, &words(&st["words"]));
                let mut end = top + line::TEXT;
                if let Some(note) = st["note"].as_str().filter(|n| !n.is_empty()) {
                    end = c.note(end, note);
                }
                c.next(top, end - top);
            } else if let Some(credit) = part["credit"].as_str() {
                c.row();
                let top = c.y;
                let w = c.control_w().min(READ_MOST);
                let mut cy = top;
                for l in text::wrap(font::REGULAR, font::CAPTION, credit, w) {
                    c.cells.push(text_cell(style::CREDIT, cx, cy, w, line::NOTE, &l));
                    cy += line::NOTE;
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
    f.heading("Devices", "");
    for (k, d) in devices.iter().enumerate() {
        // (each device's box Space.row under the one before)
        if k > 0 {
            f.gap(space::ROW);
        }
        f.card(|c| {
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
            let end = package_head(&mut c.cells, x, top, w, 2 + r.glyph as i32, &r.name);
            c.cells.push(text_cell(style::SMALL, x + size::ROW + space::PAD, end, w - size::ROW - space::PAD, line::NOTE, &line));
            c.next(top, size::ROW);
            // (its switch is its core's)
            c.row();
            c.label("Control");
            let top = c.y;
            let cx = c.control_x();
            form::switch(&mut c.cells, cx, top, None, &[("On", ""), ("Off", "")], Some(if r.on { 0 } else { 1 }), |j| (id(Ctl::PackageOn), (r.glyph * 2 + j) as i32, 0));
            let end = c.note(top + form::LINE, &format!("When off, Ectomatter leaves the {} to other programs.", d.kind));
            c.next(top, end - top);
            // (rows of words: each on a line of words, its note Space.tight under it, as a
            // control's is)
            c.row();
            c.label_text("Permission");
            let top = c.y;
            match d.reachable {
                // (no rule lets it be opened: Allow writes one, with the system's password prompt)
                _ if d.allow => {
                    state(c, top, 2, "Not granted");
                    let ax = cx + size::DOT + space::INLINE + text::width(font::REGULAR, font::TEXT, "Not granted").ceil() + space::ROW - part::QUIET_PAD;
                    form::link(&mut c.cells, ax, top, line::TEXT, "Allow", id(Ctl::PackageAllow), r.glyph as i32);
                }
                None => state(c, top, 0, "Not needed on this system"),
                Some(_) if !d.present => state(c, top, 0, &format!("Checked when the {} is connected", d.kind)),
                Some(true) => state(c, top, 1, "Granted"),
                Some(false) => state(c, top, 2, "Not granted"),
            }
            c.next(top, line::TEXT);
            if !d.holders.is_empty() {
                c.row();
                c.label_text("Also open in");
                let top = c.y;
                c.cells.push(text_cell(style::VALUE_BRIGHT, cx, top, c.control_w(), line::TEXT, &d.holders.join(", ")));
                let end = c.note(top + line::TEXT, &d.holders_note);
                c.next(top, end - top);
            }
            // its core: the version and where it stands, then its own words and Ectomatter's
            c.row();
            c.label_text("Core");
            let top = c.y;
            let v = if r.status.is_empty() { r.version.clone() } else { format!("{} · {}", r.version, r.status) };
            let cw = c.control_w();
            c.cells.push(text_cell(style::VALUE_BRIGHT, cx, top, cw, line::TEXT, &v));
            // (Review, then Remove, after the version, each its words Space.row after the last's)
            let mut vx = cx + text::width(font::REGULAR, font::TEXT, &v).ceil() + space::ROW - part::QUIET_PAD;
            if r.review {
                form::link(&mut c.cells, vx, top, line::TEXT, "Review", id(Ctl::PackageReview), r.glyph as i32);
                vx += crate::lighting::quiet_width("Review", false) + space::ROW - 2.0 * part::QUIET_PAD;
            }
            if r.removable {
                form::link(&mut c.cells, vx, top, line::TEXT, "Remove", id(Ctl::PackageRemove), r.glyph as i32);
            }
            let mut end = c.note(top + line::TEXT, &r.description);
            for words in &r.can {
                end = c.note_at(cx, end, cw, words);
            }
            if let Some(w) = &r.warning {
                end = form::warning(&mut c.cells, cx, end + space::TIGHT, cw, w);
            }
            c.next(top, end - top);
        });
    }
    // Add a device core, and how updates come, on the page's ground under the boxes
    f.add_row("Add a device core", "Ectomatter checks each core’s GitHub releases for updates once a day.", id(Ctl::PackageAdd), 0);
}

/// Settings' Plugins: a row for each package, its glyph on its tile,
/// its name, what it's for and what it can do (and what it says is wrong, in
/// amber), its version and where that stands, and On and Off; then Add (a
/// core, `add` 0, or a plugin, 1), and how updates come.
fn packages_section(f: &mut Form, title: &str, add: i32, rows: &[PackageRow], add_words: &str, note: &str) {
    f.heading(title, "");
    for (k, r) in rows.iter().enumerate() {
        // (each plugin's box Space.row under the one before)
        if k > 0 {
            f.gap(space::ROW);
        }
        f.card(|c| {
            let (bx, bw) = (c.inner_x(), c.inner_w());
            let soft = ink::SOFT;
            c.row();
            let ry = c.y;
            // the switch, then the version, from the right
            let opts = [("On", ""), ("Off", "")];
            let sx = bx + bw - form::switch_width(&opts);
            form::switch(&mut c.cells, sx, ry, None, &opts, Some(if r.on { 0 } else { 1 }), |j| (id(Ctl::PackageOn), (r.glyph * 2 + j) as i32, 0));
            // (each line to the right, Space.pad before the switch: the version, where it
            // stands under it, and Remove under that)
            let right = sx - space::PAD;
            let (w1, w2) = (text::width(font::REGULAR, font::NOTE, &r.version).ceil() + 2.0, text::width(font::REGULAR, font::NOTE, &r.status).ceil() + 2.0);
            let vx = right - w1.max(w2);
            c.cells.push(text_cell(style::NOTE, right - w1, ry, w1, line::NOTE, &r.version));
            c.cells.push(Cell { color: crate::tokens::color(soft), ..text_cell(style::NOTE_IN, right - w2, ry + line::NOTE, w2, line::NOTE, &r.status) });
            let mut right_h = 2.0 * line::NOTE;
            if r.review {
                let w3 = crate::lighting::quiet_width("Review", false);
                form::link(&mut c.cells, right - w3 + part::QUIET_PAD, ry + right_h, size::QUIET, "Review", id(Ctl::PackageReview), r.glyph as i32);
                right_h += size::QUIET;
            }
            if r.removable {
                let w3 = crate::lighting::quiet_width("Remove", false);
                form::link(&mut c.cells, right - w3 + part::QUIET_PAD, ry + right_h, size::QUIET, "Remove", id(Ctl::PackageRemove), r.glyph as i32);
                right_h += size::QUIET;
            }
            // its tile, and its words
            let tx = bx + size::ROW + space::PAD;
            let tw = vx - space::PAD - tx;
            let end = package_head(&mut c.cells, bx, ry, tw + size::ROW + space::PAD, 2 + r.glyph as i32, &r.name);
            // (the systems it works on, a tag each, Space.inline after its name)
            let mut gx = tx + text::width(font::MEDIUM, font::TAB, &r.name).ceil() + 2.0 + space::INLINE;
            for sys in &r.systems {
                gx += form::pill(&mut c.cells, 4, gx, end - line::TEXT, sys) + space::INLINE;
            }
            let mut ty = form::note_lines_in(&mut c.cells, tx, end, tw, soft, &r.description);
            for can in &r.can {
                ty = c.note_at(tx, ty, tw, can);
            }
            if let Some(w) = &r.warning {
                ty = form::warning(&mut c.cells, tx, ty + space::TIGHT, tw, w);
            }
            c.next(ry, (ty - ry).max(size::ROW).max(right_h));
        });
    }
    // Add a plugin, and how updates come, on the page's ground under the boxes
    f.add_row(add_words, note, id(Ctl::PackageAdd), add);
}

/// A package's head in Settings (a device's core, a plugin): its glyph on its tile, a row's
/// height square, and its name Space.pad after the tile, Space.tight down; where the line
/// under the name starts.
fn package_head(cells: &mut Vec<Cell>, x: f32, top: f32, w: f32, glyph: i32, name: &str) -> f32 {
    cells.push(Cell { style: glyph, ..cell(kind::DEVICE, x, top, size::ROW, size::ROW) });
    let tx = x + size::ROW + space::PAD;
    cells.push(text_cell(style::NAME, tx, top + space::TIGHT, w - (tx - x), line::TEXT, name));
    top + space::TIGHT + line::TEXT
}

/// the widest a plugin's field and card grow, a reading's
const FIELD_MOST: f32 = 380.0;
use form::READ_MOST;

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
        PackageRow { name: name.into(), description: format!("What the {name} is for."), can: vec![format!("Talks to the {name} over USB.")], version: "1.0.0".into(), status: status.into(), on, warning: None, glyph, removable, review: false, systems: Vec::new() }
    }

    /// The words on the page, each piece once.
    fn texts(devices: Vec<DeviceRow>) -> Vec<String> {
        let s = State { sensors: None, devices, plugins: Vec::new(), sections: Vec::new(), settings: Value::Null, top_peak: 1000.0, reference: 254.0, starts: crate::autostart::Starts::No };
        page(&s, Tab::Devices, 1440.0).0.into_iter().map(|c| c.text.to_string()).filter(|t| !t.is_empty()).collect()
    }

    /// A package whose newer version waits for the person offers Review, on
    /// a device's core and on a plugin alike, beside Remove where it has one.
    #[test]
    fn a_waiting_version_offers_review() {
        let waiting = |mut r: PackageRow| {
            r.review = true;
            r.status = "1.2.0 asks to do more: waiting for your OK".into();
            r
        };
        let core = DeviceRow { core: waiting(row("Logitech G502", 4, true, true, "")), kind: "mouse".into(), present: true, reachable: Some(true), allow: false, holders: Vec::new(), holders_note: String::new() };
        let s = State { sensors: None, devices: vec![core], plugins: vec![waiting(row("Weather Feed", 2, true, false, "")), row("Monitor Switcher", 3, true, true, "Up to date")], sections: Vec::new(), settings: Value::Null, top_peak: 1000.0, reference: 254.0, starts: crate::autostart::Starts::No };
        let cells: Vec<Cell> = [Tab::Devices, Tab::Plugins].into_iter().flat_map(|t| page(&s, t, 1440.0).0).collect();
        let t: Vec<String> = cells.iter().map(|c| c.text.to_string()).collect();
        assert_eq!(t.iter().filter(|x| *x == "Review").count(), 2, "on the core and the plugin waiting, not the one up to date");
        assert!(t.iter().any(|x| x == "1.0.0 · 1.2.0 asks to do more: waiting for your OK"), "the core's version and where it stands");
        assert!(t.iter().any(|x| x == "1.2.0 asks to do more: waiting for your OK"), "the plugin's");
        let review = cells.iter().filter(|c| c.text == "Review").map(|c| c.id).collect::<Vec<_>>();
        assert!(review.iter().all(|i| crate::lighting::ctl(*i) == Ctl::PackageReview), "a press opens its review");
    }

    /// Peak nits reaches the brightest screen's peak wherever the window is:
    /// on a dim screen (white 73, peak 200) a saved 1000 shows as 1000 at the
    /// slider's right end, and a press there keeps it at 1000.
    #[test]
    fn peak_nits_reaches_the_brightest_screens_peak_on_a_dim_screen() {
        let settings = json!({ "hdr": { "lighting": "auto", "glow_limit": 1000.0 } });
        let s = State { sensors: None, devices: Vec::new(), plugins: Vec::new(), sections: Vec::new(), settings, top_peak: 1000.0, reference: 73.0, starts: crate::autostart::Starts::No };
        let cells = page(&s, Tab::General, 1440.0).0;
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
        let dev = |core, kind: &str, present, reachable, holders: &[&str]| DeviceRow { core, kind: kind.into(), present, reachable, allow: false, holders: holders.iter().map(|h| h.to_string()).collect(), holders_note: "Another program has it open.".into() };
        let t = texts(vec![
            dev(row("EVGA Z10 RGB", 0, true, false, ""), "keyboard", true, Some(true), &[]),
            dev(row("SteelSeries Sensei 310", 1, true, false, ""), "mouse", false, Some(false), &["openrgb"]),
            dev(row("Logitech G502", 4, false, true, "Added from a file"), "mouse", true, Some(true), &[]),
        ]);
        let has = |s: &str| t.iter().any(|x| x == s);
        assert!(has("Connected") && has("Granted"), "the keyboard as it is");
        assert!(has("Not connected") && has("Checked when the mouse is connected"), "a mouse not plugged in: its permission waits for it");
        assert!(has("openrgb") && has("Another program has it open."), "who else has it open, and what that means");
        assert!(has("Control is off"), "a core turned off says so");
        assert!(has("1.0.0") && has("1.0.0 · Added from a file") && !t.iter().any(|x| x.contains("Comes with")), "each core's version, and where it came from only when added");
        assert_eq!(t.iter().filter(|x| *x == "Remove").count(), 1, "only an added core can be taken out");
        assert!(has("When off, Ectomatter leaves the keyboard to other programs.") && has("When off, Ectomatter leaves the mouse to other programs."));
        assert!(has("Add a device core") && !has("Cores"), "Add a device core under Devices, and no Cores section of its own");
    }

    /// Each tab holds its own: the devices on Devices, the plugins on Plugins, and on
    /// General Ectomatter's settings, each section a plugin describes, under its own title,
    /// and the sensors; nothing on two tabs. The bar names the tabs as Tab does.
    #[test]
    fn each_tab_holds_its_own() {
        let dev = DeviceRow { core: row("Made-up Mouse", 4, true, true, ""), kind: "mouse".into(), present: true, reachable: Some(true), allow: false, holders: Vec::new(), holders_note: String::new() };
        let section = PluginSection { plugin: "made-up-feed".into(), section: json!({ "label": "Made-up readings", "layout": [] }), values: Value::Null, typed: None };
        let s = State { sensors: None, devices: vec![dev], plugins: vec![row("Made-up Feed", 3, true, true, "")], sections: vec![section], settings: Value::Null, top_peak: 1000.0, reference: 254.0, starts: crate::autostart::Starts::No };
        let on = |t: Tab| page(&s, t, 1440.0).0.into_iter().map(|c| c.text.to_string()).collect::<Vec<_>>();
        let found = |w: &str| Tab::ALL.into_iter().filter(|t| on(*t).iter().any(|x| x == w)).collect::<Vec<_>>();
        assert_eq!(found("Made-up Mouse"), [Tab::Devices]);
        assert_eq!(found("Made-up Feed"), [Tab::Plugins]);
        assert_eq!(found("Made-up readings"), [Tab::General], "a plugin's section on General, titled as the plugin says");
        assert_eq!(found("Start at login"), [Tab::General]);
        assert_eq!(found("Peak nits"), [Tab::General]);
        assert_eq!(found("Add a device core"), [Tab::Devices]);
        assert_eq!(found("Add a plugin"), [Tab::Plugins]);
        // (the bar's words, ui/pages/settings.slint, are Tab's)
        let bar = include_str!("../ui/pages/settings.slint");
        let names: Vec<String> = Tab::ALL.iter().map(|t| format!("\"{}\"", t.name())).collect();
        assert!(bar.contains(&format!("tabs: [{}];", names.join(", "))), "the bar's tabs aren't Tab's");
        assert!(Tab::ALL.iter().all(|t| Tab::named(t.name()) == Some(*t) && Tab::named(&t.name().to_lowercase()) == Some(*t)));
    }
}
