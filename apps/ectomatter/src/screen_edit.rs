//! The screen layout editor's model: the layout open, the block selected,
//! the box of the block's settings under the screen (one for each kind of
//! block, Align first, two settings to a row), what each of its controls does
//! to the layout, and where a new or moved block goes. The window itself is
//! layout_page.rs.

use ectomatter_sensors::list::{Catalog, Part, What};
use crate::lcd_page::{DateFormat, Kind, Page, Piece, Readings, Size, Source};
use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::tokens::{font, line, size, space};
use crate::Cell;

/// What is being edited: which page of the Library, the block chosen, and
/// whether the screen shows its grid of cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub page: usize,
    pub chosen: Option<usize>,
    pub grid: bool,
}

/// What a Data block shows, as the menu lists it: a reading, or the time
/// (which a page keeps as a clock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shows {
    Reading(Source),
    Time,
    /// "Add more sensors in Settings": Settings, at its Sensor Feed box
    MoreSensors,
}

/// The kind of block a piece is, as the editor names it: 1 Text, 2 Data, 3 Bar, 4 Effect
/// (0 is nothing selected). The time is a Data block.
pub fn block_kind(k: &Kind) -> i32 {
    match k {
        Kind::Text(_) => 1,
        Kind::Data(..) | Kind::Clock(_) => 2,
        Kind::Bar(_) => 3,
        Kind::Effect { .. } => 4,
    }
}

/// Whether a piece is an effect, which lies behind the other blocks: effects take cells
/// only from each other, and every other block can sit on one.
pub fn behind(p: &Piece) -> bool {
    matches!(p.kind, Kind::Effect { .. })
}

/// How tall the effects' tiles are in the Effect box, four to a row, for the groups under them.
pub const TILES_H: f32 = TILE_ROWS * crate::tokens::size::TILE + (TILE_ROWS - 1.0) * crate::tokens::space::INLINE;
const TILE_ROWS: f32 = crate::lcd_fx::FXS.len().div_ceil(4) as f32;
/// What the Effect box says under its tiles.
const UNDER: &str = "Blocks placed on the effect cover the effect in their own cells. Drag the handle on the effect’s corner to change how much of the screen the effect fills.";

/// The box under the screen: its kind (as [block_kind]), its title, its
/// cells in its own terms, and its height.
pub struct BlockBox {
    pub kind: i32,
    pub title: String,
    pub cells: Vec<Cell>,
    pub h: f32,
}

/// Where the chosen block's reading comes from, for its From line, as [from_line] says it:
/// the line, and the words in it that link to the Sensor Feed box in Settings. None says what the
/// reading's source says of itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct From {
    pub line: Option<(String, &'static str)>,
}

/// A label of a driver's own that says nothing a person reads: temp1, fan2.
fn bare(label: &str) -> bool {
    let l = label.trim_end_matches(|c: char| c.is_ascii_digit());
    l.len() < label.len() && ["temp", "fan", "in", "power", "curr", "energy", "freq"].contains(&l)
}

/// The From line of a block showing one of the computer's readings: where it comes from
/// and the driver's own name for it, ending in a link to the Sensor Feed box in Settings (to add more
/// sensors, or to change the main card); or, for one unchecked there, that it is. None for a
/// reading not of the computer, or one this computer doesn't have.
pub fn from_line(src: Source, cat: &Catalog, setup: &crate::sensors::Setup) -> Option<(String, &'static str)> {
    let more = "Add more sensors";
    let main = setup.main_card(cat);
    // the sensor it reads, by id
    let id = match src {
        Source::CpuTemp => cat.cpu_temp().map(|s| s.id.clone()),
        Source::CpuLoad => Some("cpu/load".to_string()),
        Source::CpuClock => Some("cpu/clock/average".to_string()),
        Source::Memory => Some("memory/used".to_string()),
        Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => {
            let k = ["temperature", "load", "clock", "video-memory"][[Source::GpuTemp, Source::GpuLoad, Source::GpuClock, Source::VideoMemory].iter().position(|s| *s == src).unwrap()];
            let d = main?;
            // (a reading the main card's driver doesn't report: the card's note says why)
            if cat.sensor(&format!("{}/{k}", d.key)).is_none() {
                return Some((format!("The main card, now the {}, doesn’t report this. Change the main card in Settings.", d.card.as_ref()?.name), "Change the main card"));
            }
            Some(format!("{}/{k}", d.key))
        }
        Source::Sensor(n) => Some(crate::sensors::id_of(n)),
        _ => None,
    }?;
    let (d, s) = cat.sensor(&id)?;
    if !setup.checked_in(cat).contains(&id) {
        return Some(("Unchecked in Settings, so the block says “unavailable”. Check it in Settings.".into(), "Check it"));
    }
    if matches!(src, Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory) {
        return Some((format!("The main card, now the {}. Change the main card in Settings.", d.card.as_ref()?.name), "Change the main card"));
    }
    // the driver's own name for it, where it has one worth saying
    let label = s.orig.clone().or_else(|| id.rsplit('/').next().filter(|l| id.matches('/').count() >= 2 && !bare(l) && id.contains('@')).map(String::from));
    let with = |w: String| match &label {
        Some(l) => format!("{w}, {l}."),
        None => format!("{w}."),
    };
    let line = match d.part {
        Part::Processor if s.what == What::Temperature => with("The processor’s own sensor".into()),
        Part::Processor => "The processor, from Linux.".into(),
        Part::Graphics if label.is_some() => with(format!("The {}’s own sensor", d.card.as_ref().map_or(d.name.as_str(), |c| c.name.as_str()))),
        Part::Graphics => format!("The {}, {}.", d.card.as_ref().map_or(d.name.as_str(), |c| c.name.as_str()), lower_first(&d.src)),
        Part::Memory if s.what == What::InUse => "This computer’s memory, from Linux.".into(),
        Part::Memory => format!("{}’s own sensor.", s.name),
        Part::Motherboard => with("The motherboard’s sensor chip".into()),
        Part::Drives | Part::Network => with(format!("The {}’s own sensor", d.name)),
        Part::Battery => "The battery.".into(),
        Part::Laptop => with("The laptop’s own controller".into()),
        _ => with(format!("The {}", d.name)),
    };
    Some((format!("{line} {more} in Settings."), more))
}

/// A sensor as a question about it says it: the readings the ready-made layouts show by
/// what they are ("the processor’s temperature", "the main card’s load"), any other by its
/// name in the editor.
pub fn sensor_phrase(id: &str, cat: &Catalog, setup: &crate::sensors::Setup) -> String {
    if cat.cpu_temp().is_some_and(|s| s.id == id) {
        return "the processor’s temperature".into();
    }
    match id {
        "cpu/load" => return "the processor’s load".into(),
        "cpu/clock/average" => return "the processor’s clock".into(),
        "memory/used" => return "the memory in use".into(),
        _ => {}
    }
    if let Some(m) = setup.main_card(cat) {
        for (k, w) in [("temperature", "temperature"), ("load", "load"), ("clock", "clock"), ("video-memory", "video memory")] {
            if id == format!("{}/{k}", m.key) {
                return format!("the main card’s {w}");
            }
        }
    }
    cat.sensor(id).map_or_else(|| id.to_string(), |(_, s)| s.menu_name(setup.names.get(id).map(String::as_str)))
}

/// "From NVIDIA’s driver" as it runs on in a sentence: "from NVIDIA’s driver".
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_lowercase().collect::<String>() + c.as_str())
}

/// The computer's readings a block can show, as the Shows menu lists them: each device with
/// a sensor checked in Settings, under its heading (the main card says it is), each of its
/// checked sensors by its name; the readings the ready-made layouts show as they show them,
/// so a block of the main card's follows whichever card is main.
pub fn computer_groups(cat: &Catalog, setup: &crate::sensors::Setup) -> Vec<(String, Vec<(String, Source)>)> {
    let checked = setup.checked_in(cat);
    let main = setup.main_card(cat).map(|d| d.key.clone());
    let usual_cpu = cat.cpu_temp().map(|s| s.id.clone());
    let mut devices: Vec<&ectomatter_sensors::list::Device> = cat.devices.iter().collect();
    // (in the tabs' order, the main card first among the cards)
    devices.sort_by_key(|d| (d.part, Some(&d.key) != main.as_ref()));
    let mut out = Vec::new();
    for d in devices {
        let is_main = main.as_ref() == Some(&d.key);
        let items: Vec<(String, Source)> = d
            .sensors
            .iter()
            .filter(|s| checked.contains(&s.id))
            .map(|s| {
                let k = s.id.rsplit('/').next().unwrap_or("");
                let usual = match s.id.as_str() {
                    _ if Some(&s.id) == usual_cpu.as_ref() => Some(Source::CpuTemp),
                    "cpu/load" => Some(Source::CpuLoad),
                    "cpu/clock/average" => Some(Source::CpuClock),
                    "memory/used" => Some(Source::Memory),
                    _ if is_main && s.usual => [("temperature", Source::GpuTemp), ("load", Source::GpuLoad), ("clock", Source::GpuClock), ("video-memory", Source::VideoMemory)].into_iter().find(|(x, _)| *x == k).map(|(_, v)| v),
                    _ => None,
                };
                match usual {
                    Some(Source::CpuLoad) => ("Load".to_string(), Source::CpuLoad),
                    Some(Source::CpuClock) => ("Clock".to_string(), Source::CpuClock),
                    Some(Source::Memory) => ("Memory in use".to_string(), Source::Memory),
                    Some(u) => (s.name.clone(), u),
                    None => (s.menu_name(setup.names.get(&s.id).map(String::as_str)), Source::Sensor(crate::sensors::number(&s.id))),
                }
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let head = match d.part {
            // (the processor by its model, its maker left off, as the cards are)
            Part::Processor => d.name.strip_prefix("AMD ").or_else(|| d.name.strip_prefix("Intel ")).unwrap_or(&d.name).to_string(),
            Part::Memory => "Memory".into(),
            _ if is_main => format!("{}, the main card", d.name),
            _ => d.name.clone(),
        };
        out.push((head, items));
    }
    out
}

/// What a piece looks like now, for its box: words and readings whole, however its block
/// shows them.
fn looks(p: &Piece, r: &Readings) -> String {
    // (words and readings whole, however many rows they take; a bar as its row draws it)
    if !matches!(p.kind, Kind::Bar(_)) {
        return crate::lcd_page::says(p, r).map_or("Unavailable".into(), |s| s.trim().to_string());
    }
    let page = Page { pieces: vec![Piece { row: 0, col: 0, gap: 0, height: 1, ..p.clone() }], ..Page::stats() };
    let row = crate::lcd_page::render(&page, r).into_iter().next().unwrap_or_default();
    if row.ends_with(" unavailable") || row.is_empty() { "Unavailable".into() } else { row.trim().to_string() }
}

/// The box's groups, laid out two to a row, Space.pad between the columns and between
/// the rows: each a setting's name (with its value in a pill, for Looks like) Space.label
/// over its control, or a note alone. A row is a control's height and more where a note
/// runs longer, so the two groups in a row line up.
struct Grid {
    w: f32,
    y: f32,
    col: usize,
    row_h: f32,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(w: f32) -> Grid {
        // under the title's line
        Grid { w, y: size::BOX_HEAD, col: 0, row_h: 0.0, cells: Vec::new() }
    }
    fn col_w(&self) -> f32 {
        (self.w - 3.0 * space::PAD) / 2.0
    }
    fn x(&self) -> f32 {
        space::PAD + self.col as f32 * (self.col_w() + space::PAD)
    }
    /// the group ends: the next goes beside it, or on a new row
    fn done(&mut self, h: f32) {
        self.row_h = self.row_h.max(h);
        if self.col == 0 {
            self.col = 1;
        } else {
            self.col = 0;
            self.y += self.row_h + space::PAD;
            self.row_h = 0.0;
        }
    }
    fn head(&mut self, label: &str, value: Option<&str>) -> f32 {
        let (x, w) = (self.x(), self.col_w());
        self.cells.push(text_cell(style::PROP, x, self.y, w, line::TEXT, label));
        if let Some(v) = value {
            // (never over the setting's name: at most what it leaves, less Space.row)
            let vw = crate::form::pill_width(0, v).min(w - crate::text::width(font::REGULAR, font::NOTE, label) - space::ROW);
            self.cells.push(Cell { text: v.into(), ..cell(kind::PILL, x + w - vw, self.y + (line::TEXT - size::PILL) / 2.0, vw, size::PILL) });
        }
        line::TEXT + space::LABEL
    }
    fn switch(&mut self, label: &str, c: Ctl, opts: &[&str], chosen: usize) {
        let top = self.head(label, None);
        let (x, w, y) = (self.x(), self.col_w(), self.y + top);
        let opts: Vec<(&str, &str)> = opts.iter().map(|o| (*o, "")).collect();
        crate::form::switch(&mut self.cells, x, y, Some(w), &opts, Some(chosen), |j| (id(c), j as i32, 0));
        self.done(top + size::CONTROL);
    }
    fn dropdown(&mut self, label: &str, c: Ctl, shown: &str, open: bool) {
        let top = self.head(label, None);
        self.cells.push(Cell { text: shown.into(), on: open, id: id(c), ..cell(kind::DROP, self.x(), self.y + top, self.col_w(), size::CONTROL) });
        self.done(top + size::CONTROL);
    }
    fn field(&mut self, label: &str, c: Ctl, words: &str) {
        let top = self.head(label, None);
        self.cells.push(Cell { text: words.into(), style: 10, id: id(c), ..cell(kind::FIELD, self.x(), self.y + top, self.col_w(), size::CONTROL) });
        self.done(top + size::CONTROL);
    }
    /// a slider, as the Lighting tab's speed is, filled to `frac`, the words for its two
    /// ends beside it (Space.inline either side), at the middle of a control's height so it
    /// lines up with the control beside it, and what it is set to in a pill at its name's
    /// right; `notch`, a notch at its middle, its default, which holds the knob as it passes
    fn slider(&mut self, label: &str, c: Ctl, frac: f32, ends: (&str, &str), value: Option<&str>, notch: bool) {
        let top = self.head(label, value);
        let (x, w, y) = (self.x(), self.col_w(), self.y + top + (size::CONTROL - size::SLIDER) / 2.0);
        let (lw, rw) = (crate::text::width(font::REGULAR, font::SMALL, ends.0).ceil(), crate::text::width(font::REGULAR, font::SMALL, ends.1).ceil());
        self.cells.push(text_cell(style::END_L, x, y, lw, size::SLIDER, ends.0));
        self.cells.push(Cell { value: frac, color: crate::lighting::run(), id: id(c), style: if notch { 4 } else { 0 }, ..cell(kind::FILLED, x + lw + space::INLINE, y, w - lw - rw - 2.0 * space::INLINE, size::SLIDER) });
        self.cells.push(text_cell(style::END_R, x + w - rw, y, rw, size::SLIDER, ends.1));
        self.done(top + size::CONTROL);
    }
    /// a note naming where a reading comes from, ending in a link to where that is chosen:
    /// `s` holds `link` once, its words kept on one line
    fn note_link(&mut self, label: Option<&str>, s: &str, link: &str, c: Ctl) {
        let top = label.map_or(0.0, |l| self.head(l, None));
        let (x, w) = (self.x(), self.col_w());
        let held = link.replace(' ', "\u{a0}");
        let lines = crate::text::wrap(font::REGULAR, font::NOTE, &s.replace(link, &held), w);
        let lines: Vec<String> = lines.iter().map(|l| l.replace(&held, link)).collect();
        for (k, l) in lines.iter().enumerate() {
            let y = self.y + top + k as f32 * line::NOTE;
            match l.find(link) {
                Some(at) => {
                    let (before, after) = (&l[..at], &l[at + link.len()..]);
                    let (bw, lw) = (crate::text::width(font::REGULAR, font::NOTE, before), crate::text::width(font::REGULAR, font::NOTE, link).ceil() + 1.0);
                    self.cells.push(text_cell(style::NOTE, x, y, bw + 1.0, line::NOTE, before));
                    self.cells.push(Cell { text: link.into(), id: id(c), ..cell(kind::INLINE_LINK, x + bw, y, lw, line::NOTE) });
                    self.cells.push(text_cell(style::NOTE, x + bw + lw, y, (w - bw - lw).max(1.0), line::NOTE, after));
                }
                None => self.cells.push(text_cell(style::NOTE, x, y, w, line::NOTE, l)),
            }
        }
        self.done(top + lines.len() as f32 * line::NOTE);
    }
    fn value(&mut self, label: &str, v: &str) {
        self.head(label, Some(v));
        self.done(line::TEXT);
    }
    fn note(&mut self, label: Option<&str>, s: &str) {
        let top = label.map_or(0.0, |l| self.head(l, None));
        let (x, w) = (self.x(), self.col_w());
        let end = crate::form::note_lines(&mut self.cells, x, self.y + top, w, style::NOTE, s);
        self.done(end - self.y);
    }
    /// the box's pieces, and how tall it is: Space.pad under its last row
    fn height(mut self) -> (Vec<Cell>, f32) {
        if self.col == 1 {
            self.y += self.row_h + space::PAD;
        }
        (self.cells, self.y)
    }
}

/// How a block is dragged, at the foot of every box with blocks on the screen (the handle
/// changes a one-row block's width, and a taller one's rows too).
const DRAG: &str = "Drag the block on the screen to move the block, or drag the handle on the block’s corner to change the block’s size.";
/// What words do that don't fit their block, as the Text box and a word reading's box set it.
const FIT: [&str; 2] = ["Cut off", "Scroll"];

/// The box of the block chosen, `w` wide: for Text, Data and Bar their own
/// settings; with nothing chosen, the window draws what each block is for.
/// `avail` is what this computer can read; `open`, the dropdown whose menu is open (Shows,
/// or the date's Format); `from`, where the readings with a choice of source come from now
/// (the CPU sensor's name, how the clock is counted, the graphics card's name), which the
/// From line names and links to Settings for.
pub fn block_box(page: &Page, chosen: Option<usize>, r: &Readings, avail: &[Source], open: Option<Ctl>, from: &From, w: f32) -> BlockBox {
    let Some(p) = chosen.and_then(|i| page.pieces.get(i)) else {
        // (its legend, as the window draws it: the line on selecting, once there are blocks,
        // Space.row over the three kinds of block, a note's line each, Space.row apart)
        let lead = if page.pieces.is_empty() { 0.0 } else { line::NOTE + space::ROW };
        let h = size::BOX_HEAD + lead + 3.0 * line::NOTE + 2.0 * space::ROW + space::PAD;
        return BlockBox { kind: 0, title: "Nothing selected".into(), cells: Vec::new(), h };
    };
    let mut g = Grid::new(w);
    let align = if p.center { 1 } else if p.right { 2 } else { 0 };
    match &p.kind {
        Kind::Text(t) => {
            g.switch("Align", Ctl::ScrAlign, &["Left", "Center", "Right"], align);
            g.switch("When the words don't fit", Ctl::ScrFit, &FIT, p.scroll as usize);
            g.field("Says", Ctl::ScrText, t);
            g.note(None, "Letters, numbers and most symbols show as typed. Other characters show as a question mark.");
            g.note(None, DRAG);
        }
        Kind::Data(src, f) => {
            g.switch("Align", Ctl::ScrAlign, &["Left", "Center", "Right"], align);
            if src.is_temperature() {
                g.switch("Units", Ctl::ScrUnits, &["°C", "°F"], if *f { 1 } else { 0 });
            } else if src.is_words() {
                g.switch("When the words don't fit", Ctl::ScrFit, &FIT, p.scroll as usize);
            } else if *src == Source::Date {
                // (where the time has Hours: the format, as the date reads today)
                g.dropdown("Format", Ctl::ScrFormat, &p.date_format.write(&today(r)), open == Some(Ctl::ScrFormat));
            } else {
                g.done(0.0);
            }
            g.dropdown("Shows", Ctl::ScrSource, &src.title(), open == Some(Ctl::ScrSource));
            g.value("Looks like", &looks(p, r));
            match &from.line {
                Some((line, link)) => g.note_link(Some("From"), line, link, Ctl::OpenSettings),
                None if avail.contains(src) => g.note(Some("From"), src.from()),
                None => g.note(Some("From"), &format!("Ectomatter can’t read the {} on this computer, so the block says “unavailable”.", src.title().to_lowercase())),
            }
            g.note(None, DRAG);
        }
        Kind::Clock(twelve) => {
            g.switch("Align", Ctl::ScrAlign, &["Left", "Center", "Right"], align);
            g.switch("Hours", Ctl::ScrHours, &["24-hour", "12-hour"], if *twelve { 1 } else { 0 });
            g.dropdown("Shows", Ctl::ScrSource, "Time", open == Some(Ctl::ScrSource));
            g.value("Looks like", &looks(p, r));
            g.note(Some("From"), "This computer’s clock");
            g.note(None, DRAG);
        }
        Kind::Effect { fx, speed, word } => {
            // (under the effects' tiles, which the window draws)
            g.y += TILES_H + space::PAD;
            if *fx == crate::lcd_page::Fx::Bounce {
                g.field("Word", Ctl::ScrText, word);
            }
            // (what a step takes, to find on the keyboard the fastest its screen shows)
            g.slider("Speed", Ctl::ScrSpeed, *speed as f32 / 100.0, ("Slow", "Fast"), Some(&format!("{:.2} s a step", crate::lcd_fx::period(*speed))), true);
            g.note(None, UNDER);
        }
        Kind::Bar(src) => {
            g.dropdown("Shows", Ctl::ScrSource, &src.title(), open == Some(Ctl::ScrSource));
            g.value("Looks like", &looks(p, r));
            g.note(None, "The bar fills as the reading rises. A wider bar shows smaller changes.");
            g.note(None, DRAG);
        }
    }
    let (cells, h) = g.height();
    let title = ["", "Text", "Data", "Bar", "Effect"][block_kind(&p.kind) as usize].to_string();
    BlockBox { kind: block_kind(&p.kind), title, cells, h }
}

/// Today, as this computer's clock reads it (or, before it has, Tuesday, October 13, 2026),
/// for showing a format as the date reads now.
fn today(r: &Readings) -> crate::lcd_page::Moment {
    r.now.unwrap_or(crate::lcd_page::Moment { hour: 12, minute: 0, weekday: 2, day: 13, month: 9, year: 2026 })
}

/// The date's Format menu: each format as the date reads today, under Written out and In
/// numbers, the block's own ticked. The rows, and the format each row stands for.
pub fn format_menu(current: DateFormat, r: &Readings) -> Vec<(i32, String, String, bool, Option<DateFormat>)> {
    let now = today(r);
    let mut rows = vec![(1, "Written out".to_string(), String::new(), false, None)];
    for f in crate::lcd_page::DATE_FORMATS {
        if f == DateFormat::Numbers {
            rows.push((2, String::new(), String::new(), false, None));
            rows.push((1, "In numbers".to_string(), String::new(), false, None));
        }
        rows.push((0, f.write(&now), String::new(), f == current, Some(f)));
    }
    rows
}

/// The menu of what a block can show: first the computer's readings, the sensors checked
/// in Settings under the devices they read (`groups`, see [computer_groups]); then the
/// others this computer can read (`avail`) under their groups' headings; each with what it
/// reads now; last, the way to Settings for more. A bar shows only what fills one; a Data
/// block can show the time too, in the group the date is in (the date as the block writes
/// it, `date`). The rows, and what each row stands for.
pub fn source_menu(current: Shows, bars: bool, date: DateFormat, groups: &[(String, Vec<(String, Source)>)], avail: &[Source], r: &Readings) -> Vec<(i32, String, String, bool, Option<Shows>)> {
    let mut rows = Vec::new();
    let piece = |s: Shows| match s {
        Shows::Reading(src) => Piece { kind: if bars { Kind::Bar(src) } else { Kind::Data(src, src.starts_in_fahrenheit()) }, row: 0, col: 0, width: if bars { 10 } else { 1 }, right: false, center: false, gap: 0, height: 1, scroll: true, date_format: date },
        _ => Piece { kind: Kind::Clock(true), row: 0, col: 0, width: 8, right: false, center: false, gap: 0, height: 1, scroll: true, date_format: Default::default() },
    };
    let head = |rows: &mut Vec<(i32, String, String, bool, Option<Shows>)>, h: &str| {
        if !rows.is_empty() {
            rows.push((2, String::new(), String::new(), false, None));
        }
        rows.push((1, h.to_string(), String::new(), false, None));
    };
    for (h, items) in groups {
        let items: Vec<&(String, Source)> = items.iter().filter(|(_, s)| !bars || s.barred()).collect();
        if items.is_empty() {
            continue;
        }
        head(&mut rows, h);
        for (name, src) in items {
            let s = Shows::Reading(*src);
            rows.push((0, name.clone(), reading_now(*src, r).unwrap_or_else(|| "Unavailable".into()), s == current, Some(s)));
        }
    }
    // the others, each group once, in the menu's order
    let order = ["Time and date", "Keyboard", "Game", "More"];
    let mut listed: Vec<Shows> = avail.iter().copied().filter(|s| order.contains(&s.group()) && (!bars || s.barred())).map(Shows::Reading).collect();
    if !bars {
        listed.push(Shows::Time);
    }
    let group_of = |s: &Shows| match s {
        Shows::Reading(s) => s.group(),
        _ => "Time and date",
    };
    // (the time before the date)
    listed.sort_by_key(|s| (order.iter().position(|g| *g == group_of(s)), !matches!(s, Shows::Time)));
    let mut group = "";
    for s in listed {
        if group_of(&s) != group {
            group = group_of(&s);
            head(&mut rows, group);
        }
        let name = match s {
            Shows::Reading(src) => src.title(),
            _ => "Time".to_string(),
        };
        rows.push((0, name, looks(&piece(s), r), s == current, Some(s)));
    }
    rows.push((2, String::new(), String::new(), false, None));
    rows.push((7, "Add more sensors in Settings".into(), String::new(), false, Some(Shows::MoreSensors)));
    rows
}

/// One of the computer's readings now, as the Sensor Feed box in Settings writes it, for the Shows
/// menu: "66°", "4.7 GHz", "1882 MHz", "33.4 / 60.2 GB", "2,836 RPM".
fn reading_now(src: Source, r: &Readings) -> Option<String> {
    use ectomatter_sensors::list::Val;
    let shown = |what: What, part: Part, v: Val| ectomatter_sensors::read::shown(what, part, v);
    let g = r.gpu;
    Some(match src {
        Source::CpuTemp => shown(What::Temperature, Part::Processor, Val::One(r.cpu_temp?)),
        Source::CpuLoad => shown(What::Load, Part::Processor, Val::One(r.cpu_load?)),
        Source::CpuClock => shown(What::Clock, Part::Processor, Val::One(r.cpu_ghz? * 1000.0)),
        Source::Memory => shown(What::InUse, Part::Memory, Val::Of(r.ram?.0, r.ram?.1)),
        Source::GpuTemp => shown(What::Temperature, Part::Graphics, Val::One(g?.temp? as f64)),
        Source::GpuLoad => shown(What::Load, Part::Graphics, Val::One(g?.load? as f64)),
        Source::GpuClock => shown(What::Clock, Part::Graphics, Val::One(g?.clock? as f64)),
        Source::VideoMemory => shown(What::InUse, Part::Graphics, Val::Of(g?.vram?.0, g?.vram?.1)),
        Source::Sensor(n) => {
            let (what, v) = r.sensors.get(&n)?;
            shown(*what, crate::sensors::known(n).map_or(Part::Other, |k| k.part), *v)
        }
        _ => return None,
    })
}

/// What the chosen piece shows, as the menu lists it.
pub fn shows_of(p: &Piece) -> Option<Shows> {
    match p.kind {
        Kind::Data(s, _) | Kind::Bar(s) => Some(Shows::Reading(s)),
        Kind::Clock(_) => Some(Shows::Time),
        Kind::Text(_) | Kind::Effect { .. } => None,
    }
}

/// Use a control of the box on the page, on a screen of `size`.
pub fn act(page: &mut Page, ed: &mut Editor, c: Ctl, index: usize, words: Option<&str>, (cols, rows): Size) {
    let Some(i) = ed.chosen.filter(|i| *i < page.pieces.len()) else { return };
    if c == Ctl::ScrRemove {
        page.pieces.remove(i);
        ed.chosen = None;
        return;
    }
    let p = &mut page.pieces[i];
    match c {
        Ctl::ScrText => match (&mut p.kind, words) {
            // (Bounce's word, as wide as the screen)
            (Kind::Effect { word, .. }, Some(t)) => *word = t.chars().take(cols).collect(),
            // (a whole screen's worth, since words that don't fit can scroll)
            (_, Some(t)) => p.kind = Kind::Text(t.chars().take(cols * rows).collect()),
            _ => {}
        },
        Ctl::ScrFit => p.scroll = index == 1,
        Ctl::ScrUnits => {
            if let Kind::Data(s, _) = p.kind {
                p.kind = Kind::Data(s, index == 1);
            }
        }
        Ctl::ScrHours => p.kind = Kind::Clock(index == 1),
        Ctl::ScrAlign => {
            p.center = index == 1;
            p.right = index == 2;
        }
        _ => {}
    }
}

/// The speed a new effect starts at: the slider's middle, where its notch is, a step every
/// 0.32 s, just inside what the keyboard's screen shows words clearly at (Grant: 0.3 s is the
/// very edge, 2026-10-13).
pub const SPEED: u8 = 50;

/// The chosen effect's speed, from its slider (0 slow to 1 fast); near the notch, the notch.
pub fn set_speed(page: &mut Page, ed: &Editor, v: f32) {
    if let Some(Kind::Effect { speed, .. }) = ed.chosen.and_then(|i| page.pieces.get_mut(i)).map(|p| &mut p.kind) {
        let s = (v.clamp(0.0, 1.0) * 100.0).round() as u8;
        *speed = if s.abs_diff(SPEED) <= 4 { SPEED } else { s };
    }
}

/// The chosen effect becomes another, from its tile; its speed and word stay.
pub fn set_fx(page: &mut Page, ed: &Editor, to: crate::lcd_page::Fx) {
    if let Some(Kind::Effect { fx, .. }) = ed.chosen.and_then(|i| page.pieces.get_mut(i)).map(|p| &mut p.kind) {
        *fx = to;
    }
}

/// What the chosen piece shows, chosen from the menu: a Data block can turn
/// into the time (on a 12-hour clock) and back. A temperature keeps the unit chosen for it
/// when it turns into another of its kind (the CPU's into the GPU's); any other reading
/// starts in its own (see [Source::starts_in_fahrenheit]).
pub fn set_shows(page: &mut Page, ed: &Editor, s: Shows) {
    let Some(p) = ed.chosen.and_then(|i| page.pieces.get_mut(i)) else { return };
    p.kind = match (&p.kind, s) {
        (Kind::Bar(_), Shows::Reading(src)) => Kind::Bar(src),
        (Kind::Data(was, f), Shows::Reading(src)) => {
            let alike = was.is_temperature() && src.is_temperature() && was.starts_in_fahrenheit() == src.starts_in_fahrenheit();
            Kind::Data(src, if alike { *f } else { src.starts_in_fahrenheit() })
        }
        (Kind::Clock(_), Shows::Reading(src)) => Kind::Data(src, src.starts_in_fahrenheit()),
        (Kind::Data(..), Shows::Time) => Kind::Clock(true),
        (k, _) => k.clone(),
    };
}

/// Whether piece `q` takes a cell of `row`.
fn covers(q: &Piece, row: u8) -> bool {
    q.row <= row && (row as usize) < q.row as usize + q.height.max(1) as usize
}

/// The screen's size in cells, as the editor counts them, its columns and rows (a block's
/// place is kept in a byte, as [crate::lcd_page::size_of] bounds a screen).
fn cells_of((cols, rows): Size) -> (u8, u8) {
    (cols.min(u8::MAX as usize) as u8, rows.min(u8::MAX as usize) as u8)
}

/// A piece moved on the screen (of `size`) to start at a cell: into the free cells there in
/// every row it takes (see [fit]), as wide as it was or as they let it be; where it doesn't
/// fit, it stays. `least` is the fewest cells it shows its whole self in (see
/// [crate::lcd_page::least]). A block of a layout made for a larger screen, moved, comes
/// onto this one.
pub fn move_to(page: &mut Page, i: usize, row: i32, col: i32, least: u8, size: Size) {
    let (cols, rows) = cells_of(size);
    let Some(p) = page.pieces.get(i).filter(|_| cols > 0 && rows > 0) else { return };
    let h = p.height.clamp(1, rows);
    let (row, col) = (row.clamp(0, (rows - h) as i32) as u8, col.clamp(0, cols as i32 - 1) as u8);
    if let Some((row, col, width)) = fit(page, row, col, p.width, h, least.min(p.width), Some(i), size) {
        let p = &mut page.pieces[i];
        (p.row, p.col, p.width, p.height) = (row, col, width, h);
    }
}

/// A piece's corner moved: `width` cells wide and `height` rows tall from its first cell,
/// as far as the free cells to its right and below it let it (see [room]), and no
/// narrower than `least` gives for that height (see [crate::lcd_page::least_at]).
pub fn size_to(page: &mut Page, i: usize, width: i32, height: i32, least: impl Fn(u8) -> u8, size: Size) {
    let ends = room(page, i, &least, size);
    let Some(p) = page.pieces.get(i) else { return };
    let tallest = ends.iter().take_while(|e| e.is_some()).count().max(1);
    let h = height.clamp(1, tallest as i32) as usize;
    let Some(end) = ends.get(h - 1).copied().flatten() else { return };
    let most = end as i32 - p.col as i32;
    let w = width.clamp((floor(p, h as u8, &least) as i32).min(most), most);
    let p = &mut page.pieces[i];
    (p.width, p.height) = (w as u8, h as u8);
}

/// The narrowest piece `p` can be at `height` rows: `least` gives it, and on one row (or
/// at its own height) never more than it is now, so the size it has can always be kept,
/// even by a reading already longer than its block.
pub fn floor(p: &Piece, height: u8, least: impl Fn(u8) -> u8) -> u8 {
    let l = least(height).max(1);
    if height == 1 || height == p.height { l.min(p.width.max(1)) } else { l }
}

/// How far piece `i`'s corner can reach on a screen of `size`: for each height from one row
/// to as many as the screen has, the column after the last free cell to its right in every
/// row that height takes; None from the first height that can't be had (the screen's foot,
/// or a block below it leaving fewer cells than the piece can be at that height, `least`).
/// A block starting past the screen's edge or its foot (a layout made for a larger screen)
/// can't be had at any height there.
pub fn room(page: &Page, i: usize, least: impl Fn(u8) -> u8, size: Size) -> Vec<Option<u8>> {
    let (cols, rows) = cells_of(size);
    let mut ends = vec![None; rows as usize];
    let Some(p) = page.pieces.get(i).filter(|p| p.row < rows && p.col < cols) else { return ends };
    let mut end = cols;
    for h in 1..=rows - p.row {
        // the free cells from its first column on, in the row this height adds
        let Some((_, hi)) = run_at(page, p.row + h - 1, p.col, Some(i), size) else { break };
        end = end.min(hi);
        if end < p.col + floor(p, h, &least).min(cols - p.col) {
            break;
        }
        ends[h as usize - 1] = Some(end);
    }
    ends
}

/// The fewest free cells a new block starts in: a bar's two ends and a cell to fill, and
/// room for words to start. (A block on the screen can be made narrower.)
pub const NEW_LEAST: u8 = 3;

/// A new block: `what` 0 Text, 1 Data, 2 Bar, 3 Effect, at `at` (its row, first column,
/// width and height, from [fit] or [effect_spot]), or else in the first free cells it fits
/// on a screen of `size` (see [first_room]; an effect, the whole screen or what other effects
/// leave of it); chosen. `first` is a reading this computer has, which a new Data block
/// shows. Whether there was room for it.
#[allow(clippy::too_many_arguments)]
pub fn add(page: &mut Page, ed: &mut Editor, what: usize, first: Source, r: &Readings, at: Option<(u8, u8, u8, u8)>, size: Size) -> bool {
    let (kind, width, right) = new_block(what, first, size);
    let mut p = Piece { kind, row: 0, col: 0, width, right, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() };
    let at = if what == 3 {
        at.or_else(|| effect_spot(page, None, size))
    } else {
        let least = crate::lcd_page::least(&p, r).max(NEW_LEAST).min(width);
        at.or_else(|| first_room(page, width, least, size).map(|(row, col, w)| (row, col, w, 1)))
    };
    let Some((row, col, width, height)) = at else { return false };
    (p.row, p.col, p.width, p.height) = (row, col, width, height);
    page.pieces.push(p);
    ed.chosen = Some(page.pieces.len() - 1);
    true
}

/// A new block of a kind, as it starts on a screen of `size`: its kind, width, and whether
/// set against its right edge. An effect starts as Rain, across the screen.
pub fn new_block(what: usize, first: Source, size: Size) -> (Kind, u8, bool) {
    match what {
        0 => (Kind::Text("TEXT".into()), 4, false),
        1 => (Kind::Data(first, first.starts_in_fahrenheit()), 7, true),
        2 => (Kind::Bar(if first.barred() { first } else { Source::CpuLoad }), 10, false),
        _ => (Kind::Effect { fx: crate::lcd_page::Fx::Rain, speed: SPEED, word: "EVGA".into() }, cells_of(size).0, false),
    }
}

/// Where a new effect goes on a screen of `size`, put at cell (`row`, `col`), or added with no
/// cell (None): the whole screen while no other effect is on it; else the cells round that one
/// (or the first free) that no effect takes, as wide as their run in its row and as many rows
/// as that run stays free, as its row, first column, width and height. None on another
/// effect, when effects fill the screen, or with no screen.
pub fn effect_spot(page: &Page, at: Option<(u8, u8)>, size: Size) -> Option<(u8, u8, u8, u8)> {
    let (cols, rows) = cells_of(size);
    if cols == 0 || rows == 0 {
        return None;
    }
    let effects: Vec<&Piece> = page.pieces.iter().filter(|p| behind(p)).collect();
    if effects.is_empty() {
        return Some((0, 0, cols, rows));
    }
    let free = |row: u8, col: u8| !effects.iter().any(|q| covers(q, row) && col >= q.col && (col as usize) < q.col as usize + q.width as usize);
    let (row, col) = match at {
        Some(rc) => Some(rc).filter(|&(r, c)| r < rows && c < cols && free(r, c))?,
        None => (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).find(|&(r, c)| free(r, c))?,
    };
    let lo = (0..=col).rev().take_while(|&c| free(row, c)).last()?;
    let hi = (col..cols).take_while(|&c| free(row, c)).last()? + 1;
    let whole = |r: u8| (lo..hi).all(|c| free(r, c));
    let top = (0..=row).rev().take_while(|&r| whole(r)).last()?;
    let bottom = (row..rows).take_while(|&r| whole(r)).last()?;
    Some((top, lo, hi - lo, bottom - top + 1))
}

/// The free cells round cell `col` of `row`, on a screen of `size`, which no block but `skip`
/// (one being moved) takes: the first column and the column after the last, the screen's edge
/// at most. None where a block takes the cell. Effects lie behind the other blocks: for an
/// effect (`skip` one) only effects take cells, and for any other block only the other
/// blocks do.
pub fn run_at(page: &Page, row: u8, col: u8, skip: Option<usize>, size: Size) -> Option<(u8, u8)> {
    let cols = cells_of(size).0;
    let layer = skip.and_then(|i| page.pieces.get(i)).is_some_and(behind);
    let end = |p: &Piece| p.col as usize + p.width as usize;
    let others = page.pieces.iter().enumerate().filter(|(k, p)| Some(*k) != skip && behind(p) == layer && covers(p, row));
    if others.clone().any(|(_, p)| col >= p.col && (col as usize) < end(p)) {
        return None;
    }
    let lo = others.clone().filter(|(_, p)| end(p) <= col as usize).map(|(_, p)| end(p) as u8).max().unwrap_or(0);
    let hi = others.filter(|(_, p)| p.col > col).map(|(_, p)| p.col).min().unwrap_or(cols).min(cols);
    Some((lo, hi))
}

/// Where a block `width` cells wide and `height` rows tall, that shows its whole self in
/// `least`, lands when it is put at cell (`row`, `col`) of a screen of `size`: in the cells
/// free round that cell in every row it takes, starting at that cell or as near it as they let
/// it fit, and no wider than they are, as its row, first column and width. None over another
/// block, past the screen's foot or its edge, or where the free cells are fewer than `least`:
/// blocks never cover each other.
#[allow(clippy::too_many_arguments)]
pub fn fit(page: &Page, row: u8, col: u8, width: u8, height: u8, least: u8, skip: Option<usize>, size: Size) -> Option<(u8, u8, u8)> {
    let (cols, rows) = cells_of(size);
    if col >= cols {
        return None;
    }
    let (mut lo, mut hi) = (0, cols);
    for r in row as usize..row as usize + height.max(1) as usize {
        if r >= rows as usize {
            return None;
        }
        let (a, b) = run_at(page, r as u8, col, skip, size)?;
        (lo, hi) = (lo.max(a), hi.min(b));
    }
    let room = hi.saturating_sub(lo);
    let least = least.max(1);
    if room < least {
        return None;
    }
    let width = width.clamp(least, room);
    Some((row, col.min(hi - width).max(lo), width))
}

/// The first free cells a new block fits in on a screen of `size`, row by row: a run as wide
/// as `width`, or else the first with at least `least` cells, the block narrowed to it. None
/// when no run has room.
pub fn first_room(page: &Page, width: u8, least: u8, size: Size) -> Option<(u8, u8, u8)> {
    let (cols, rows) = cells_of(size);
    let runs: Vec<(u8, u8, u8)> = (0..rows)
        .flat_map(|row| {
            let mut runs = Vec::new();
            let mut col = 0;
            while col < cols {
                match run_at(page, row, col, None, size) {
                    Some((lo, hi)) => {
                        runs.push((row, lo, hi));
                        col = hi;
                    }
                    None => col += 1,
                }
            }
            runs
        })
        .collect();
    let fits = |w: u8| runs.iter().find(|(_, lo, hi)| hi - lo >= w).map(|&(row, lo, hi)| (row, lo, width.min(hi - lo)));
    fits(width).or_else(|| fits(least.max(1)))
}

/// Whether any row of a screen of `size` has `cells` free cells in a run, room for another block.
pub fn has_room(page: &Page, cells: u8, size: Size) -> bool {
    first_room(page, cells, cells, size).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z10's screen.
    const Z10: Size = (20, 4);

    /// The editor's grid is the screen's, whatever its size: a new block lands within it, a
    /// block's corner reaches its edge and its foot, no further, and a layout made for a larger
    /// screen keeps its blocks past the edge, which can be moved onto it.
    #[test]
    fn the_editor_s_grid_is_the_screen_s_whatever_its_size() {
        let r = Readings { cpu_temp: Some(45.0), ..Default::default() };
        let mut ed = Editor { page: 0, chosen: None, grid: true };
        let small = (16, 2);
        // two rows of sixteen: a new effect takes the whole screen, a block lands on it
        let mut p = Page::blank();
        assert!(add(&mut p, &mut ed, 3, Source::CpuTemp, &r, None, small));
        assert_eq!((p.pieces[0].row, p.pieces[0].col, p.pieces[0].width, p.pieces[0].height), (0, 0, 16, 2));
        assert!(add(&mut p, &mut ed, 1, Source::CpuTemp, &r, None, small));
        assert_eq!((p.pieces[1].row, p.pieces[1].col, p.pieces[1].width), (0, 0, 7));
        assert_eq!(fit(&p, 1, 14, 7, 1, 3, None, small), Some((1, 9, 7)), "as near the edge as it fits");
        assert_eq!(fit(&p, 1, 0, 4, 2, 3, None, small), None, "past the foot of two rows");
        assert_eq!(fit(&p, 0, 16, 4, 1, 3, None, small), None, "past the edge");
        assert_eq!(room(&p, 1, |_| 3, small), [Some(16), Some(16)]);
        assert_eq!(room(&p, 0, |_| 1, small), [Some(16), Some(16)], "the effect, to the screen's corner");
        size_to(&mut p, 1, 40, 9, |_| 3, small);
        assert_eq!((p.pieces[1].width, p.pieces[1].height), (16, 2));
        // forty by four: twice the cells to a row
        let mut wide = Page::blank();
        assert!(add(&mut wide, &mut ed, 3, Source::CpuTemp, &r, None, (40, 4)));
        assert_eq!((wide.pieces[0].width, wide.pieces[0].height), (40, 4));
        assert_eq!(first_room(&wide, 30, 3, (40, 4)), Some((0, 0, 30)));
        assert_eq!(fit(&wide, 3, 39, 7, 1, 3, None, (40, 4)), Some((3, 33, 7)));
        assert!(has_room(&Page::stats(), 3, (40, 4)), "Stats leaves twenty columns free on forty");
        // the Stats layout, made for the Z10, on two rows of sixteen: CPU's clock (13 to 19)
        // reaches only the screen's edge; the rows below the foot can't be had at any height
        let stats = Page::stats();
        assert_eq!(room(&stats, 3, |_| 3, small), [Some(16), None]);
        assert_eq!(room(&stats, 9, |_| 3, small), [None, None], "RAM's reading, on the third row");
        assert!(!has_room(&stats, 3, small));
        // a block past the foot and the edge, moved, comes onto the screen, as tall as it can be there
        let mut past = Page { pieces: vec![Piece { kind: Kind::Text("FAR".into()), row: 3, col: 18, width: 6, right: false, center: false, gap: 1, height: 3, scroll: true, date_format: Default::default() }], ..Page::blank() };
        assert_eq!(room(&past, 0, |_| 1, small), [None, None]);
        size_to(&mut past, 0, 2, 1, |_| 1, small);
        assert_eq!((past.pieces[0].width, past.pieces[0].height), (6, 3), "not sized where it can't be had");
        move_to(&mut past, 0, 3, 18, 1, small);
        assert_eq!((past.pieces[0].row, past.pieces[0].col, past.pieces[0].width, past.pieces[0].height), (0, 10, 6, 2));
        // and no screen at all: nothing goes on it, nothing moves, nothing panics
        let none = (0, 0);
        let mut q = Page::stats();
        assert!(!add(&mut q, &mut ed, 0, Source::CpuTemp, &r, None, none) && !add(&mut q, &mut ed, 3, Source::CpuTemp, &r, None, none));
        assert!(room(&q, 1, |_| 3, none).is_empty() && !has_room(&q, 1, none));
        assert_eq!((fit(&q, 0, 0, 4, 1, 1, None, none), effect_spot(&q, None, none), first_room(&q, 4, 1, none)), (None, None, None));
        move_to(&mut q, 1, 0, 0, 3, none);
        size_to(&mut q, 1, 9, 9, |_| 3, none);
        assert_eq!(q, Page::stats());
    }

    /// The Shows menu has a Game group, after the keyboard's readings and before the
    /// weather: the game's name, the session, the time played, when last played, what's
    /// changing and the changes done; on a Bar block, only the changes done.
    #[test]
    fn the_shows_menu_has_the_games_readings() {
        let r = Readings { game: Some(crate::lcd_page::Game { name: "ARC Raiders".into(), since: 1, minutes: 90, last: 1, ..Default::default() }), ..Default::default() };
        let avail = crate::lcd_page::available(&r, true);
        let rows = source_menu(Shows::Time, false, DateFormat::Short, &[], &avail, &r);
        let heads: Vec<&str> = rows.iter().filter(|r| r.0 == 1).map(|r| r.1.as_str()).collect();
        assert_eq!(heads, ["Time and date", "Keyboard", "Game", "More"]);
        let at = rows.iter().position(|r| r.0 == 1 && r.1 == "Game").unwrap();
        let game: Vec<&str> = rows[at + 1..].iter().take_while(|r| r.0 == 0).map(|r| r.1.as_str()).collect();
        assert_eq!(game, ["Game name", "Session time", "Time played", "Last played", "Changing now", "Changes done"]);
        assert_eq!(rows[at + 1].2, "ARC RAIDERS", "each with how it reads now");
        let bars = source_menu(Shows::Time, true, DateFormat::Short, &[], &avail, &r);
        let at = bars.iter().position(|r| r.0 == 1 && r.1 == "Game").unwrap();
        assert_eq!(bars[at + 1].1, "Changes done");
    }

    #[test]
    fn the_keyboard_s_own_readings_have_a_group_of_their_own() {
        use crate::lcd_page::{available, Readings, Source};
        let r = Readings { profile: Some("DEFAULT 1".into()), profile_number: Some(1), ..Default::default() };
        let names_in = |rows: &[(i32, String, String, bool, Option<Shows>)], group: &str| -> Vec<String> {
            let at = rows.iter().position(|row| row.0 == 1 && row.1 == group).expect(group);
            rows[at + 1..].iter().take_while(|row| row.0 == 0).map(|row| row.1.clone()).collect()
        };
        let rows = source_menu(Shows::Reading(Source::Profile), false, DateFormat::Short, &[], &available(&r, false), &r);
        assert_eq!(names_in(&rows, "Keyboard"), ["Active profile", "Profile number"]);
        // the time is a reading, in a group with the date
        assert_eq!(names_in(&rows, "Time and date"), ["Time", "Date"]);
        // game mode and key rollover join the keyboard's once the keyboard can say them (build 5)
        let r = Readings { mode: Some("PC MODE".into()), rollover: Some("6 KEY".into()), ..r };
        let rows = source_menu(Shows::Reading(Source::Profile), false, DateFormat::Short, &[], &available(&r, false), &r);
        assert_eq!(names_in(&rows, "Keyboard"), ["Active profile", "Profile number", "Game mode", "Key rollover"]);
        // a bar shows only what fills one: no time
        let bars = source_menu(Shows::Reading(Source::CpuLoad), true, DateFormat::Short, &[], &available(&r, false), &r);
        assert!(!bars.iter().any(|row| row.4 == Some(Shows::Time)));
    }

    #[test]
    fn a_new_block_goes_into_free_cells_and_never_over_another() {
        let r = Readings { cpu_temp: Some(45.0), ..Default::default() };
        let mut p = Page::stats();
        let mut ed = Editor { page: 0, chosen: None, grid: true };
        // Stats has no three free cells in a row: no block fits
        assert!(!add(&mut p, &mut ed, 0, Source::CpuTemp, &r, None, Z10));
        assert_eq!((p.pieces.len(), ed.chosen), (12, None));
        assert!(!has_room(&p, 3, Z10) && has_room(&p, 1, Z10));
        let mut empty = Page { pieces: Vec::new(), ..Page::stats() };
        assert!(add(&mut empty, &mut ed, 1, Source::GpuTemp, &r, None, Z10));
        assert_eq!((empty.pieces[0].row, empty.pieces[0].col, empty.pieces[0].width), (0, 0, 7));
        assert!(add(&mut empty, &mut ed, 0, Source::GpuTemp, &r, None, Z10));
        assert_eq!((empty.pieces[1].row, empty.pieces[1].col), (0, 7));
        // put at a cell: from that cell, or as near it as the free cells let it fit
        assert_eq!(fit(&empty, 2, 15, 10, 1, 3, None, Z10), Some((2, 10, 10)));
        assert_eq!(fit(&empty, 0, 12, 7, 1, 3, None, Z10), Some((0, 12, 7)));
        // narrowed to the free cells, but never past what it shows; over a block, nowhere
        assert_eq!(fit(&empty, 0, 11, 10, 1, 3, None, Z10), Some((0, 11, 9)));
        assert_eq!(fit(&Page::stats(), 0, 3, 7, 1, 3, None, Z10), None, "one free cell between CPU and its temperature");
        assert_eq!(fit(&empty, 0, 2, 7, 1, 3, None, Z10), None);
        // a block two rows tall needs the cells free in both: the Data block on row 0 is in the way
        assert_eq!(fit(&empty, 0, 12, 7, 2, 3, None, Z10), Some((0, 12, 7)));
        assert_eq!(fit(&empty, 0, 3, 4, 2, 3, None, Z10), None);
        assert_eq!(fit(&empty, 3, 0, 4, 2, 3, None, Z10), None, "past the screen's foot");
        // the first room a block can have: a run as wide as it, or else narrowed to one
        let mut row = Page { pieces: Vec::new(), ..Page::stats() };
        for r in 0..4 {
            row.pieces.push(Piece { kind: Kind::Text("X".into()), row: r, col: 0, width: 16, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() });
        }
        assert_eq!(first_room(&row, 7, 3, Z10), Some((0, 16, 4)));
        assert_eq!(first_room(&row, 7, 5, Z10), None);
    }

    #[test]
    fn effects_lie_behind_the_blocks_and_never_overlap_each_other() {
        let r = Readings::default();
        let mut p = Page::stats();
        let mut ed = Editor { page: 0, chosen: None, grid: true };
        // dropped on a full screen of blocks, an effect fills the whole screen behind them
        assert!(add(&mut p, &mut ed, 3, Source::CpuTemp, &r, None, Z10));
        let fx = p.pieces.len() - 1;
        assert_eq!((p.pieces[fx].row, p.pieces[fx].col, p.pieces[fx].width, p.pieces[fx].height), (0, 0, 20, 4));
        // another can't go on it
        assert_eq!(effect_spot(&p, None, Z10), None);
        assert!(!add(&mut p, &mut ed, 3, Source::CpuTemp, &r, None, Z10));
        // cut to the left half, it leaves the right half to a second effect
        ed.chosen = Some(fx);
        size_to(&mut p, fx, 10, 4, |_| 1, Z10);
        assert_eq!((p.pieces[fx].width, p.pieces[fx].height), (10, 4), "the blocks on it don't stop its corner");
        assert_eq!(effect_spot(&p, Some((2, 15)), Z10), Some((0, 10, 10, 4)));
        assert_eq!(effect_spot(&p, Some((2, 5)), Z10), None, "on the first effect");
        // and the blocks on an effect still take their cells only from each other
        assert_eq!(run_at(&p, 0, 3, None, Z10), Some((3, 4)), "the cell between CPU and its temperature");
    }

    #[test]
    fn a_block_s_corner_stops_at_its_neighbors() {
        let p = Page::stats();
        // CPU's temperature, cells 4 to 6: its load starts at 8, and the GPU's temperature
        // under it keeps it one row
        assert_eq!(room(&p, 1, |_| 3, Z10), [Some(8), None, None, None]);
        let mut q = p.clone();
        size_to(&mut q, 1, 20, 4, |_| 3, Z10);
        assert_eq!((q.pieces[1].col, q.pieces[1].width, q.pieces[1].height), (4, 4, 1));
        // on one row no narrower than what it shows: 45° takes three cells
        size_to(&mut q, 1, 1, 1, |_| 3, Z10);
        assert_eq!(q.pieces[1].width, 3);
        // words take more rows, as far as the free cells below them go
        let mut w = Page { pieces: vec![Piece { kind: Kind::Data(Source::Sky, false), row: 0, col: 0, width: 6, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() }], ..Page::stats() };
        w.pieces.push(Piece { kind: Kind::Data(Source::CpuTemp, false), row: 2, col: 3, width: 3, right: true, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() });
        assert_eq!(room(&w, 0, |_| 1, Z10), [Some(20), Some(20), Some(3), Some(3)]);
        size_to(&mut w, 0, 10, 4, |_| 1, Z10);
        assert_eq!((w.pieces[0].width, w.pieces[0].height), (3, 4), "four rows tall, so only as wide as the cells beside the reading below");
        size_to(&mut w, 0, 10, 2, |_| 1, Z10);
        assert_eq!((w.pieces[0].width, w.pieces[0].height), (10, 2));
        // a reading takes more rows too, to the screen's foot, and on more rows can be
        // narrower than it reads (what doesn't fit stops after its last row)
        let mut two = w.clone();
        let r = Readings { cpu_temp: Some(45.0), ..Readings::default() };
        let q = two.pieces[1].clone();
        size_to(&mut two, 1, 1, 3, |h| crate::lcd_page::least_at(&q, &r, h), Z10);
        assert_eq!((two.pieces[1].width, two.pieces[1].height), (1, 2));
        let q = two.pieces[1].clone();
        size_to(&mut two, 1, 1, 1, |h| crate::lcd_page::least_at(&q, &r, h), Z10);
        assert_eq!((two.pieces[1].width, two.pieces[1].height), (1, 1), "its size kept: back on one row it runs long");
        // a reading chosen for words keeps their rows
        let mut ed = Editor { page: 0, chosen: Some(0), grid: true };
        set_shows(&mut w, &ed, Shows::Reading(Source::CpuLoad));
        assert_eq!(w.pieces[0].height, 2);
        // Text holds a whole screen's worth, and keeps what its words do when they don't fit
        let mut t = Page { pieces: vec![Piece { kind: Kind::Text(String::new()), row: 0, col: 0, width: 4, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() }], ..Page::stats() };
        ed.chosen = Some(0);
        act(&mut t, &mut ed, Ctl::ScrText, 0, Some(&"x".repeat(100)), Z10);
        assert_eq!(t.pieces[0].kind, Kind::Text("x".repeat(80)));
        act(&mut t, &mut ed, Ctl::ScrFit, 0, None, Z10);
        assert!(!t.pieces[0].scroll, "Cut off");
    }

    #[test]
    fn the_controls_change_the_chosen_block() {
        let mut p = Page::stats();
        let mut ed = Editor { page: 0, chosen: Some(1), grid: true };
        act(&mut p, &mut ed, Ctl::ScrUnits, 1, None, Z10);
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::CpuTemp, true));
        act(&mut p, &mut ed, Ctl::ScrAlign, 1, None, Z10);
        assert!(p.pieces[1].center && !p.pieces[1].right);
        // a temperature keeps its unit as another of its kind; the weather starts in °F, and
        // a reading that isn't a temperature drops it
        set_shows(&mut p, &ed, Shows::Reading(Source::GpuTemp));
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::GpuTemp, true));
        act(&mut p, &mut ed, Ctl::ScrUnits, 0, None, Z10);
        set_shows(&mut p, &ed, Shows::Reading(Source::OutsideTemp));
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::OutsideTemp, true), "the weather in °F");
        set_shows(&mut p, &ed, Shows::Reading(Source::CpuTemp));
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::CpuTemp, false), "the CPU in °C");
        set_shows(&mut p, &ed, Shows::Reading(Source::GpuLoad));
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::GpuLoad, false), "°F goes with a reading that isn't a temperature");
        // a Data block turns into the time, on a 12-hour clock, and back
        set_shows(&mut p, &ed, Shows::Time);
        assert_eq!(p.pieces[1].kind, Kind::Clock(true));
        act(&mut p, &mut ed, Ctl::ScrHours, 0, None, Z10);
        assert_eq!(p.pieces[1].kind, Kind::Clock(false), "24-hour");
        set_shows(&mut p, &ed, Shows::Reading(Source::Date));
        assert_eq!(p.pieces[1].kind, Kind::Data(Source::Date, false));
        // moved into free cells; onto another block, it stays
        move_to(&mut p, 1, 0, 3, 3, Z10);
        assert_eq!((p.pieces[1].row, p.pieces[1].col, p.pieces[1].width), (0, 3, 3));
        move_to(&mut p, 1, 1, 0, 3, Z10);
        assert_eq!((p.pieces[1].row, p.pieces[1].col), (0, 3), "the word GPU takes that cell");
        act(&mut p, &mut ed, Ctl::ScrRemove, 0, None, Z10);
        assert_eq!((p.pieces.len(), ed.chosen), (11, None));
    }

    #[test]
    fn each_kind_of_block_has_its_own_box() {
        let r = Readings::default();
        let p = Page::stats();
        let b = block_box(&p, Some(1), &r, &[Source::CpuTemp], None, &From::default(), 551.0);
        assert_eq!((b.kind, b.title.as_str()), (2, "Data"));
        // Align first, at the top left; then Units beside it
        let first = b.cells.iter().find(|c| c.kind == kind::CHOICE).unwrap();
        assert_eq!(first.text.as_str(), "Left");
        assert!(b.cells.iter().any(|c| c.text.as_str() == "Units"));
        let b = block_box(&p, Some(0), &r, &[], None, &From::default(), 551.0);
        assert_eq!(b.title, "Text");
        assert!(b.cells.iter().any(|c| c.kind == kind::FIELD && c.text.as_str() == "CPU"));
        let none = block_box(&p, None, &r, &[], None, &From::default(), 551.0);
        assert_eq!((none.kind, none.title.as_str()), (0, "Nothing selected"));
    }
}
