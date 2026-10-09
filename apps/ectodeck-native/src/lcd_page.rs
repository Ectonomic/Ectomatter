//! A page of the Z10's screen as pieces: words, readings, bars and a clock,
//! each in a row of the 20 by 4 screen at a column and a width. Pages are the
//! same on every profile and kept in Ectodeck's settings
//! (devices.z10.screen.pages).
//!
//! A row is laid out as the z10-lcd service's format strings laid out the
//! Stats page, so the Stats page made of pieces shows what it always showed,
//! character for character:
//!
//! - each piece starts at its column, or, when the piece before it ran long,
//!   its gap (one space, or none) after that piece's end, so a long reading
//!   pushes the rest of its row along rather than over it;
//! - a piece is padded to its width, to the left or the right;
//! - a reading that can't be had makes its row the row's first words and
//!   "unavailable" ("GPU unavailable"), said once for each source: a later
//!   row that only has readings from a source already said to be unavailable
//!   is left empty, as the service left VRAM's row when the graphics card
//!   couldn't be read.

use serde_json::{json, Value};

use crate::lcd_stats::GpuReading;

pub const COLS: u8 = 20;
pub const ROWS: u8 = 4;

/// What a reading or a bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    CpuTemp,
    CpuLoad,
    CpuClock,
    GpuTemp,
    GpuLoad,
    GpuClock,
    Memory,
    VideoMemory,
    Date,
    Profile,
    OutsideTemp,
    Sky,
}

pub const SOURCES: [Source; 12] = [
    Source::CpuTemp,
    Source::CpuLoad,
    Source::CpuClock,
    Source::GpuTemp,
    Source::GpuLoad,
    Source::GpuClock,
    Source::Memory,
    Source::VideoMemory,
    Source::Date,
    Source::Profile,
    Source::OutsideTemp,
    Source::Sky,
];

impl Source {
    pub fn key(self) -> &'static str {
        match self {
            Source::CpuTemp => "cpu-temp",
            Source::CpuLoad => "cpu-load",
            Source::CpuClock => "cpu-clock",
            Source::GpuTemp => "gpu-temp",
            Source::GpuLoad => "gpu-load",
            Source::GpuClock => "gpu-clock",
            Source::Memory => "memory",
            Source::VideoMemory => "video-memory",
            Source::Date => "date",
            Source::Profile => "profile",
            Source::OutsideTemp => "outside-temp",
            Source::Sky => "sky",
        }
    }

    fn from_key(k: &str) -> Option<Source> {
        SOURCES.iter().copied().find(|s| s.key() == k)
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::CpuTemp => "CPU temperature",
            Source::CpuLoad => "CPU load",
            Source::CpuClock => "CPU clock",
            Source::GpuTemp => "GPU temperature",
            Source::GpuLoad => "GPU load",
            Source::GpuClock => "GPU clock",
            Source::Memory => "Memory used",
            Source::VideoMemory => "Video memory used",
            Source::Date => "Date",
            Source::Profile => "Active profile",
            Source::OutsideTemp => "Outside temperature",
            Source::Sky => "Weather",
        }
    }

    /// The group it sits in on the menu of readings.
    pub fn group(self) -> &'static str {
        match self {
            Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "Processor",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => "Graphics",
            Source::Memory => "Memory",
            Source::Date | Source::Profile | Source::OutsideTemp | Source::Sky => "More",
        }
    }

    /// Where the reading comes from, as the piece's box says it.
    pub fn from(self) -> &'static str {
        match self {
            Source::CpuTemp => "This computer’s temperature sensor (k10temp)",
            Source::CpuLoad | Source::Memory => "This computer (/proc)",
            Source::CpuClock => "This computer (/sys)",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => "NVIDIA’s driver (NVML)",
            Source::Date => "This computer’s clock",
            Source::Profile => "The profile the keyboard is using",
            Source::OutsideTemp | Source::Sky => "Open-Meteo, for the place in Ectodeck’s settings",
        }
    }

    /// Whether a temperature, which can be shown in °C or °F.
    pub fn is_temperature(self) -> bool {
        matches!(self, Source::CpuTemp | Source::GpuTemp | Source::OutsideTemp)
    }

    /// Whether a bar can show it: a share of something.
    pub fn barred(self) -> bool {
        matches!(self, Source::CpuTemp | Source::CpuLoad | Source::GpuTemp | Source::GpuLoad | Source::Memory | Source::VideoMemory)
    }

    /// What it reads from: when that can't be read, every reading from it
    /// is unavailable, and said to be once.
    fn reads_from(self) -> Option<&'static str> {
        match self {
            Source::CpuTemp | Source::CpuClock => Some("cpu"),
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => Some("gpu"),
            Source::Memory => Some("memory"),
            Source::OutsideTemp | Source::Sky => Some("weather"),
            Source::CpuLoad | Source::Date | Source::Profile => None,
        }
    }

    /// The short name a row says is unavailable when it has no words of its own.
    fn short(self) -> &'static str {
        match self {
            Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "CPU",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock => "GPU",
            Source::Memory => "RAM",
            Source::VideoMemory => "VRAM",
            Source::Date => "Date",
            Source::Profile => "Profile",
            Source::OutsideTemp | Source::Sky => "Weather",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text(String),
    /// a reading; in °F rather than °C, for a temperature
    Data(Source, bool),
    Bar(Source),
    /// the time; on a 12-hour clock rather than a 24-hour one
    Clock(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    pub row: u8,
    pub col: u8,
    pub width: u8,
    /// set against its right edge rather than its left
    pub right: bool,
    /// the spaces kept after the piece before it, when that piece runs long
    pub gap: u8,
}

impl Piece {
    pub fn name(&self) -> String {
        match &self.kind {
            Kind::Text(t) => if t.trim().is_empty() { "Words".into() } else { format!("“{}”", t.trim()) },
            Kind::Data(s, _) => s.name().into(),
            Kind::Bar(s) => format!("{} bar", s.name()),
            Kind::Clock(_) => "Clock".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub id: String,
    pub name: String,
    /// the screen's light while this page shows; None, the profile's
    pub light: Option<[u8; 3]>,
    pub pieces: Vec<Piece>,
}

fn words(row: u8, col: u8, t: &str) -> Piece {
    Piece { kind: Kind::Text(t.into()), row, col, width: t.chars().count() as u8, right: false, gap: 1 }
}

fn reading(row: u8, col: u8, width: u8, s: Source, gap: u8) -> Piece {
    Piece { kind: Kind::Data(s, false), row, col, width, right: true, gap }
}

impl Page {
    /// The Stats page as Grant laid it out for the z10-lcd service.
    pub fn stats() -> Page {
        Page {
            id: "stats".into(),
            name: "Stats".into(),
            light: None,
            pieces: vec![
                words(0, 0, "CPU"),
                reading(0, 4, 3, Source::CpuTemp, 1),
                reading(0, 8, 4, Source::CpuLoad, 1),
                reading(0, 13, 7, Source::CpuClock, 1),
                words(1, 0, "GPU"),
                reading(1, 4, 3, Source::GpuTemp, 1),
                reading(1, 8, 4, Source::GpuLoad, 1),
                reading(1, 13, 7, Source::GpuClock, 1),
                words(2, 0, "RAM"),
                reading(2, 4, 16, Source::Memory, 1),
                words(3, 0, "VRAM"),
                reading(3, 4, 16, Source::VideoMemory, 0),
            ],
        }
    }

    pub fn to_json(&self) -> Value {
        let pieces: Vec<Value> = self
            .pieces
            .iter()
            .map(|p| {
                let mut v = json!({ "row": p.row, "col": p.col, "width": p.width, "right": p.right, "gap": p.gap });
                match &p.kind {
                    Kind::Text(t) => {
                        v["type"] = json!("text");
                        v["text"] = json!(t);
                    }
                    Kind::Data(s, f) => {
                        v["type"] = json!("data");
                        v["source"] = json!(s.key());
                        v["fahrenheit"] = json!(f);
                    }
                    Kind::Bar(s) => {
                        v["type"] = json!("bar");
                        v["source"] = json!(s.key());
                    }
                    Kind::Clock(twelve) => {
                        v["type"] = json!("clock");
                        v["twelve"] = json!(twelve);
                    }
                }
                v
            })
            .collect();
        let mut v = json!({ "id": self.id, "name": self.name, "pieces": pieces });
        if let Some(l) = self.light {
            v["light"] = json!(l);
        }
        v
    }

    pub fn from_json(v: &Value) -> Option<Page> {
        let b = |v: &Value| v.as_u64().map(|n| n.min(255) as u8);
        let pieces = v["pieces"]
            .as_array()?
            .iter()
            .filter_map(|p| {
                let kind = match p["type"].as_str()? {
                    "text" => Kind::Text(p["text"].as_str().unwrap_or("").into()),
                    "data" => Kind::Data(Source::from_key(p["source"].as_str()?)?, p["fahrenheit"].as_bool().unwrap_or(false)),
                    "bar" => Kind::Bar(Source::from_key(p["source"].as_str()?)?),
                    "clock" => Kind::Clock(p["twelve"].as_bool().unwrap_or(false)),
                    _ => return None,
                };
                Some(Piece {
                    kind,
                    row: b(&p["row"])?.min(ROWS - 1),
                    col: b(&p["col"])?.min(COLS - 1),
                    width: b(&p["width"])?.clamp(1, COLS),
                    right: p["right"].as_bool().unwrap_or(false),
                    gap: b(&p["gap"]).unwrap_or(1).min(1),
                })
            })
            .collect();
        let light = v["light"].as_array().filter(|a| a.len() == 3).map(|a| std::array::from_fn(|i| b(&a[i]).unwrap_or(0)));
        Some(Page { id: v["id"].as_str()?.into(), name: v["name"].as_str().unwrap_or("Page").into(), light, pieces })
    }
}

/// The pages kept in Ectodeck's settings, or Stats as it ships.
pub fn pages_of(v: Option<&Value>) -> Vec<Page> {
    let pages: Vec<Page> = v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(Page::from_json).collect()).unwrap_or_default();
    if pages.is_empty() { vec![Page::stats()] } else { pages }
}

/// The local time, for the clock and the date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    pub hour: u32,
    pub minute: u32,
    /// 0 is Sunday
    pub weekday: u32,
    pub day: u32,
    /// 0 is January
    pub month: u32,
}

impl Moment {
    pub fn now() -> Moment {
        let mut t: libc::tm = unsafe { std::mem::zeroed() };
        let secs = unsafe { libc::time(std::ptr::null_mut()) };
        unsafe { libc::localtime_r(&secs, &mut t) };
        Moment { hour: t.tm_hour as u32, minute: t.tm_min as u32, weekday: t.tm_wday as u32, day: t.tm_mday as u32, month: t.tm_mon as u32 }
    }
}

/// Everything a page can show, read once for each frame.
#[derive(Clone, Debug, Default)]
pub struct Readings {
    /// °C
    pub cpu_temp: Option<f64>,
    /// percent
    pub cpu_load: f64,
    pub cpu_ghz: Option<f64>,
    /// used and in all, GiB
    pub ram: Option<(f64, f64)>,
    pub gpu: Option<GpuReading>,
    pub now: Option<Moment>,
    pub profile: Option<String>,
    /// °C, and the sky in words
    pub outside: Option<(f64, String)>,
}

fn fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// A reading as the piece shows it, or None when it can't be had.
fn text_of(s: Source, f: bool, r: &Readings) -> Option<String> {
    let deg = |c: f64| if f { fahrenheit(c) } else { c };
    Some(match s {
        Source::CpuTemp => format!("{:>2.0}°", deg(r.cpu_temp?)),
        Source::CpuLoad => format!("{:>3.0}%", r.cpu_load),
        Source::CpuClock => format!("{:>4.0}MHz", r.cpu_ghz? * 1000.0),
        Source::GpuTemp => {
            let t = r.gpu?.0;
            if f { format!("{:>2.0}°", fahrenheit(t as f64)) } else { format!("{t:>2}°") }
        }
        Source::GpuLoad => format!("{:>3}%", r.gpu?.1),
        Source::GpuClock => format!("{:>4}MHz", r.gpu?.2),
        Source::Memory => {
            let (used, total) = r.ram?;
            format!("{used:.1} / {total:.1} GB")
        }
        Source::VideoMemory => {
            let g = r.gpu?;
            format!("{:.1} / {:.1} GB", g.3, g.4)
        }
        Source::Date => {
            let m = r.now?;
            const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            format!("{} {} {}", DAYS[m.weekday as usize % 7], MONTHS[m.month as usize % 12], m.day)
        }
        Source::Profile => ectotype_core::lcd::lcd_text(r.profile.as_deref()?),
        Source::OutsideTemp => format!("{:.0}°", deg(r.outside.as_ref()?.0)),
        Source::Sky => ectotype_core::lcd::lcd_text(&r.outside.as_ref()?.1),
    })
}

/// A share, 0 to 1, for a bar; None when it can't be had.
fn share_of(s: Source, r: &Readings) -> Option<f64> {
    Some(match s {
        Source::CpuTemp => r.cpu_temp? / 100.0,
        Source::CpuLoad => r.cpu_load / 100.0,
        Source::GpuTemp => r.gpu?.0 as f64 / 100.0,
        Source::GpuLoad => r.gpu?.1 as f64 / 100.0,
        Source::Memory => r.ram.map(|(u, t)| if t > 0.0 { u / t } else { 0.0 })?,
        Source::VideoMemory => r.gpu.map(|g| if g.4 > 0.0 { g.3 / g.4 } else { 0.0 })?,
        _ => return None,
    }
    .clamp(0.0, 1.0))
}

/// A bar `width` wide, as the launch sequence draws its bar: [###.....]
pub fn bar(share: f64, width: u8) -> String {
    let inner = width.saturating_sub(2) as usize;
    let fill = ((inner as f64) * share).round() as usize;
    format!("[{}{}]", "#".repeat(fill.min(inner)), ".".repeat(inner - fill.min(inner)))
}

/// What one piece shows; Err with what it reads from when that can't be had.
fn show(p: &Piece, r: &Readings) -> Result<String, &'static str> {
    let fail = |s: Source| s.reads_from().unwrap_or("");
    match &p.kind {
        Kind::Text(t) => Ok(ectotype_core::lcd::lcd_text(t)),
        Kind::Data(s, f) => text_of(*s, *f, r).ok_or(fail(*s)),
        Kind::Bar(s) => share_of(*s, r).map(|v| bar(v, p.width)).ok_or(fail(*s)),
        Kind::Clock(twelve) => {
            let m = r.now.ok_or("")?;
            Ok(if *twelve {
                let h = if m.hour % 12 == 0 { 12 } else { m.hour % 12 };
                format!("{h}:{:02} {}", m.minute, if m.hour < 12 { "AM" } else { "PM" })
            } else {
                format!("{:02}:{:02}", m.hour, m.minute)
            })
        }
    }
}

/// The page's four rows, with these readings.
pub fn render(page: &Page, r: &Readings) -> Vec<String> {
    lay_out(page, r).0
}

/// Where each piece lands, in the page's order: its row, the cell it starts
/// at and how many it takes (a long reading runs past its width and pushes
/// the rest of its row along). A row said to be unavailable keeps its
/// pieces where they were put.
pub fn spans(page: &Page, r: &Readings) -> Vec<(u8, usize, usize)> {
    lay_out(page, r).1
}

fn lay_out(page: &Page, r: &Readings) -> (Vec<String>, Vec<(u8, usize, usize)>) {
    let mut said: Vec<&'static str> = Vec::new();
    let mut spans: Vec<(u8, usize, usize)> = page.pieces.iter().map(|p| (p.row, p.col as usize, p.width as usize)).collect();
    let rows = (0..ROWS)
        .map(|row| {
            let mut pieces: Vec<(usize, &Piece)> = page.pieces.iter().enumerate().filter(|(_, p)| p.row == row).collect();
            pieces.sort_by_key(|(_, p)| p.col);
            let shown: Vec<Result<String, &'static str>> = pieces.iter().map(|(_, p)| show(p, r)).collect();
            let failed: Vec<&'static str> = shown.iter().filter_map(|s| s.as_ref().err().copied()).collect();
            if !failed.is_empty() {
                let new = failed.iter().any(|f| !said.contains(f));
                for f in &failed {
                    if !said.contains(f) {
                        said.push(f);
                    }
                }
                if !new {
                    return String::new();
                }
                // the row's own first words, or what it read
                let label = pieces.iter().find_map(|(_, p)| match &p.kind {
                    Kind::Text(t) if !t.trim().is_empty() => Some(ectotype_core::lcd::lcd_text(t.trim())),
                    _ => None,
                });
                let label = label.unwrap_or_else(|| {
                    pieces
                        .iter()
                        .find_map(|(_, p)| match &p.kind {
                            Kind::Data(s, _) | Kind::Bar(s) => Some(s.short().to_string()),
                            _ => None,
                        })
                        .unwrap_or_default()
                });
                return format!("{label} unavailable");
            }
            let mut out = String::new();
            let mut cursor = 0usize;
            for (k, ((i, p), s)) in pieces.iter().zip(&shown).enumerate() {
                let s = s.as_ref().unwrap();
                let w = p.width as usize;
                let start = if k == 0 { p.col as usize } else { (p.col as usize).max(cursor + p.gap as usize) };
                out.push_str(&" ".repeat(start.saturating_sub(cursor)));
                let text = if p.right { format!("{s:>w$}") } else { format!("{s:<w$}") };
                let len = text.chars().count();
                spans[*i] = (row, start, len);
                cursor = start + len;
                out.push_str(&text);
            }
            out.trim_end().to_string()
        })
        .collect();
    (rows, spans)
}

/// The readings this computer can give, for the menu of readings.
pub fn available(r: &Readings, weather: bool) -> Vec<Source> {
    SOURCES
        .iter()
        .copied()
        .filter(|s| match s {
            Source::CpuTemp => r.cpu_temp.is_some(),
            Source::CpuClock => r.cpu_ghz.is_some(),
            Source::CpuLoad | Source::Date | Source::Profile => true,
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => r.gpu.is_some(),
            Source::Memory => r.ram.is_some(),
            Source::OutsideTemp | Source::Sky => weather,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings() -> Readings {
        Readings {
            cpu_temp: Some(64.5),
            cpu_load: 15.0,
            cpu_ghz: Some(4.36875),
            ram: Some((25.1, 60.2)),
            gpu: Some((45, 12, 1837, 13.13, 31.84)),
            now: Some(Moment { hour: 21, minute: 5, weekday: 5, day: 9, month: 9 }),
            profile: Some("DEFAULT 1".into()),
            outside: None,
        }
    }

    #[test]
    fn the_stats_page_keeps_its_place_through_json() {
        let p = Page::stats();
        assert_eq!(Page::from_json(&p.to_json()), Some(p.clone()));
        assert_eq!(pages_of(None), vec![p.clone()]);
        assert_eq!(pages_of(Some(&json!([p.to_json()]))), vec![p]);
    }

    #[test]
    fn a_long_reading_pushes_its_row_along() {
        let mut r = readings();
        r.cpu_temp = Some(105.25);
        r.cpu_load = 100.0;
        r.gpu = Some((101, 100, 210, 0.04999, 8.0));
        let rows = render(&Page::stats(), &r);
        assert_eq!(rows[0], "CPU 105° 100% 4369MHz");
        assert_eq!(rows[1], "GPU 101° 100%  210MHz");
        assert_eq!(rows[3], "VRAM    0.0 / 8.0 GB");
    }

    #[test]
    fn an_unavailable_source_is_said_once() {
        let mut r = readings();
        r.gpu = None;
        let rows = render(&Page::stats(), &r);
        assert_eq!(rows[1], "GPU unavailable");
        assert_eq!(rows[3], "", "VRAM's row reads only from the graphics card, already said to be unavailable");
        // without the GPU row, VRAM's row says so itself
        let mut p = Page::stats();
        p.pieces.retain(|x| x.row != 1);
        assert_eq!(render(&p, &r)[3], "VRAM unavailable");
    }

    #[test]
    fn a_piece_that_runs_long_is_outlined_where_it_lands() {
        let mut r = readings();
        r.cpu_temp = Some(105.0);
        let sp = spans(&Page::stats(), &r);
        assert_eq!(sp[1], (0, 4, 4), "105° takes four cells from column 4");
        assert_eq!(sp[2], (0, 9, 4), "the load after it, pushed along by one");
        assert_eq!(sp[10], (3, 0, 4));
    }

    #[test]
    fn pieces_of_every_kind() {
        let r = readings();
        let p = Page {
            id: "x".into(),
            name: "X".into(),
            light: None,
            pieces: vec![
                Piece { kind: Kind::Clock(false), row: 0, col: 0, width: 5, right: false, gap: 1 },
                Piece { kind: Kind::Data(Source::Date, false), row: 0, col: 9, width: 11, right: true, gap: 1 },
                Piece { kind: Kind::Bar(Source::CpuLoad), row: 1, col: 0, width: 12, right: false, gap: 1 },
                Piece { kind: Kind::Data(Source::CpuTemp, true), row: 2, col: 0, width: 4, right: false, gap: 1 },
                Piece { kind: Kind::Clock(true), row: 3, col: 0, width: 8, right: false, gap: 1 },
                Piece { kind: Kind::Data(Source::Profile, false), row: 3, col: 10, width: 10, right: true, gap: 1 },
            ],
        };
        let rows = render(&p, &r);
        assert_eq!(rows[0], "21:05      Fri Oct 9");
        assert_eq!(rows[1], "[##........]");
        assert_eq!(rows[2], "148°");
        assert_eq!(rows[3], "9:05 PM    DEFAULT 1");
        assert_eq!(bar(1.0, 4), "[##]");
        assert_eq!(bar(0.0, 2), "[]");
    }
}
