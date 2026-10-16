//! A page of the Z10's screen as pieces: words, readings, bars and a clock,
//! each in a row of the 20 by 4 screen at a column and a width. The pages are
//! the Library, every screen there is (devices.z10.screen.pages). Playlists
//! play some of them in turn, each for its own time and coming in its own
//! way (devices.z10.screen.playlists), and each profile plays one of them
//! (devices.z10.screen.plays). All of it is kept in Ectodeck's settings.
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

use crate::lcd_stats::Gpu;
use crate::sensor_list::{Val, What};

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
    /// what the keyboard knows about itself: the active profile's number, its mode
    /// (PC, game or LCD) and its key rollover (the last two read from firmware build 5 on)
    ProfileNumber,
    GameMode,
    KeyRollover,
    OutsideTemp,
    Sky,
    /// one of the computer's sensors checked in Settings, by the number [crate::sensors]
    /// knows it by (kept in a layout by its id)
    Sensor(u16),
}

pub const SOURCES: [Source; 15] = [
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
    Source::ProfileNumber,
    Source::GameMode,
    Source::KeyRollover,
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
            Source::ProfileNumber => "profile-number",
            Source::GameMode => "game-mode",
            Source::KeyRollover => "key-rollover",
            Source::OutsideTemp => "outside-temp",
            Source::Sky => "sky",
            Source::Sensor(_) => "sensor",
        }
    }

    fn from_key(k: &str) -> Option<Source> {
        SOURCES.iter().copied().find(|s| s.key() == k)
    }

    /// What a saved block shows: a reading by its key, or a sensor by its id.
    fn of_json(p: &Value) -> Option<Source> {
        match p["source"].as_str()? {
            "sensor" => Some(Source::Sensor(crate::sensors::number(p["sensor"].as_str()?))),
            k => Source::from_key(k),
        }
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
            Source::ProfileNumber => "Profile number",
            Source::GameMode => "Game mode",
            Source::KeyRollover => "Key rollover",
            Source::OutsideTemp => "Outside temperature",
            Source::Sky => "Weather",
            Source::Sensor(_) => "Sensor",
        }
    }

    /// Its name as the editor says it: a sensor's as given in Settings, or its own.
    pub fn title(self) -> String {
        match self {
            Source::Sensor(n) => crate::sensors::known(n).map_or_else(|| sensor_label(n), |k| k.name),
            s => s.name().to_string(),
        }
    }

    /// What it is a reading of, as a card's line on a page says it.
    pub fn phrase(self) -> String {
        if let Source::Sensor(n) = self {
            return crate::sensors::known(n).map_or_else(|| sensor_label(n), |k| k.name);
        }
        match self {
            Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "CPU",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock => "GPU",
            Source::Memory => "RAM",
            Source::VideoMemory => "VRAM",
            Source::Date => "the date",
            Source::Profile => "the active profile",
            Source::ProfileNumber => "the profile’s number",
            Source::GameMode => "game mode",
            Source::KeyRollover => "key rollover",
            Source::OutsideTemp => "the temperature outside",
            Source::Sky => "the sky outside",
            Source::Sensor(_) => "a sensor",
        }
        .to_string()
    }

    /// The group it sits in on the menu of readings.
    pub fn group(self) -> &'static str {
        match self {
            Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "Processor",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => "Graphics",
            Source::Memory => "Memory",
            Source::Profile | Source::ProfileNumber | Source::GameMode | Source::KeyRollover => "Keyboard",
            Source::Date => "Time and date",
            Source::OutsideTemp | Source::Sky => "More",
            Source::Sensor(_) => "Sensors",
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
            Source::Profile | Source::ProfileNumber => "The profile the keyboard is using",
            Source::GameMode | Source::KeyRollover => "The keyboard, with Ectodeck’s firmware build 5 or later",
            Source::OutsideTemp | Source::Sky => "Open-Meteo, for the place in Ectodeck’s settings",
            Source::Sensor(_) => "One of this computer’s sensors",
        }
    }

    /// Whether a temperature, which can be shown in °C or °F.
    /// Whether the reading is words (the weather, the active profile's name), which
    /// carry on to the next row, scroll or are cut off, as Text does.
    pub fn is_words(self) -> bool {
        matches!(self, Source::Sky | Source::Profile)
    }

    pub fn is_temperature(self) -> bool {
        match self {
            Source::Sensor(n) => crate::sensors::known(n).is_some_and(|k| k.what == What::Temperature),
            s => matches!(s, Source::CpuTemp | Source::GpuTemp | Source::OutsideTemp),
        }
    }

    /// Whether a new block of this reading shows it in °F: the weather, as Americans read
    /// it; the computer's temperatures stay in °C, as their makers and every reading of them
    /// give them.
    pub fn starts_in_fahrenheit(self) -> bool {
        self == Source::OutsideTemp
    }

    /// Whether a bar can show it: a share of something.
    pub fn barred(self) -> bool {
        match self {
            Source::Sensor(n) => crate::sensors::known(n).is_some_and(|k| matches!(k.what, What::Temperature | What::Load | What::Charge | What::FanShare | What::InUse)),
            s => matches!(s, Source::CpuTemp | Source::CpuLoad | Source::GpuTemp | Source::GpuLoad | Source::Memory | Source::VideoMemory),
        }
    }

    /// What it reads from: when that can't be read, every reading from it
    /// is unavailable, and said to be once.
    fn reads_from(self) -> Option<&'static str> {
        match self {
            Source::CpuTemp | Source::CpuClock => Some("cpu"),
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => Some("gpu"),
            Source::Memory => Some("memory"),
            Source::OutsideTemp | Source::Sky => Some("weather"),
            Source::GameMode | Source::KeyRollover => Some("keyboard"),
            // (a sensor's part: its readings said unavailable once)
            Source::Sensor(n) => Some(crate::sensors::known(n).map_or("Sensor", |k| k.part.short())),
            Source::CpuLoad | Source::Date | Source::Profile | Source::ProfileNumber => None,
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
            Source::Profile | Source::ProfileNumber => "Profile",
            Source::GameMode => "Mode",
            Source::KeyRollover => "Rollover",
            Source::OutsideTemp | Source::Sky => "Weather",
            Source::Sensor(n) => crate::sensors::known(n).map_or("Sensor", |k| k.part.short()),
        }
    }
}

/// A sensor not on this computer, or not listed yet, by the label its id ends with ("Pump Fan").
fn sensor_label(n: u16) -> String {
    let id = crate::sensors::id_of(n);
    id.rsplit('/').next().unwrap_or("Sensor").to_string()
}

pub use crate::lcd_fx::Fx;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text(String),
    /// a reading; in °F rather than °C, for a temperature
    Data(Source, bool),
    Bar(Source),
    /// the time; on a 12-hour clock rather than a 24-hour one
    Clock(bool),
    /// a moving pattern behind the other blocks (see [crate::lcd_fx]): which, how fast (0
    /// slow to 100 fast), and the word Bounce bounces
    Effect { fx: Fx, speed: u8, word: String },
}

/// How a Data block showing the date writes it: written out, from the weekday alone to the
/// whole date, or set in numbers, US or day first. Short (Tue Oct 13) is how the date read
/// before there were formats, so a new date block starts with it and the layouts made
/// before don't change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFormat {
    /// Tue Oct 13
    #[default]
    Short,
    /// Tuesday, October 13
    Long,
    /// October 13, 2026
    Full,
    /// Oct 13
    MonthDay,
    /// 13 Oct 2026
    DayMonthYear,
    /// Tuesday
    Weekday,
    /// 10/13/2026
    Numbers,
    /// 10/13
    NumbersShort,
    /// 13/10/2026
    DayFirst,
    /// 2026-10-13
    Iso,
}

/// The date's formats as its menu lists them: written out, then in numbers.
pub const DATE_FORMATS: [DateFormat; 10] = [
    DateFormat::Short,
    DateFormat::Long,
    DateFormat::Full,
    DateFormat::MonthDay,
    DateFormat::DayMonthYear,
    DateFormat::Weekday,
    DateFormat::Numbers,
    DateFormat::NumbersShort,
    DateFormat::DayFirst,
    DateFormat::Iso,
];

impl DateFormat {
    pub fn key(self) -> &'static str {
        match self {
            DateFormat::Short => "short",
            DateFormat::Long => "long",
            DateFormat::Full => "full",
            DateFormat::MonthDay => "month-day",
            DateFormat::DayMonthYear => "day-month-year",
            DateFormat::Weekday => "weekday",
            DateFormat::Numbers => "numbers",
            DateFormat::NumbersShort => "numbers-short",
            DateFormat::DayFirst => "day-first",
            DateFormat::Iso => "iso",
        }
    }

    pub fn from_key(k: &str) -> Option<DateFormat> {
        DATE_FORMATS.into_iter().find(|f| f.key() == k)
    }

    /// Whether it is set in numbers rather than written out.
    pub fn in_numbers(self) -> bool {
        matches!(self, DateFormat::Numbers | DateFormat::NumbersShort | DateFormat::DayFirst | DateFormat::Iso)
    }

    /// The date at `m`, written this way.
    pub fn write(self, m: &Moment) -> String {
        const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let (weekday, month) = (DAYS[m.weekday as usize % 7], MONTHS[m.month as usize % 12]);
        let short = |s: &str| s.chars().take(3).collect::<String>();
        let (d, n, y) = (m.day, m.month % 12 + 1, m.year);
        match self {
            DateFormat::Short => format!("{} {} {d}", short(weekday), short(month)),
            DateFormat::Long => format!("{weekday}, {month} {d}"),
            DateFormat::Full => format!("{month} {d}, {y}"),
            DateFormat::MonthDay => format!("{} {d}", short(month)),
            DateFormat::DayMonthYear => format!("{d} {} {y}", short(month)),
            DateFormat::Weekday => weekday.to_string(),
            DateFormat::Numbers => format!("{n}/{d}/{y}"),
            DateFormat::NumbersShort => format!("{n}/{d}"),
            DateFormat::DayFirst => format!("{d}/{n}/{y}"),
            DateFormat::Iso => format!("{y}-{n:02}-{d:02}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    pub row: u8,
    pub col: u8,
    pub width: u8,
    /// set against its right edge rather than its left
    pub right: bool,
    /// set in the middle of its width (before `right`)
    pub center: bool,
    /// the spaces kept after the piece before it, when that piece runs long
    pub gap: u8,
    /// how many rows it takes, from `row` down: words and readings carry on to the next
    /// row where a row ends, and a bar is drawn thicker (see [block_rows])
    pub height: u8,
    /// words that don't fit even in all its rows: scrolled through it (left on one row,
    /// up a row at a time on more), or else cut off at its end
    pub scroll: bool,
    /// how a block showing the date writes it
    pub date_format: DateFormat,
}

/// How a page comes onto the screen after the one before it: a column of #
/// sweeping across (as a game's launch begins), its cells changing in a
/// random order, its words typed in, or at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Wipe,
    Dissolve,
    Type,
    Cut,
}

pub const TRANSITIONS: [Transition; 4] = [Transition::Wipe, Transition::Dissolve, Transition::Type, Transition::Cut];

impl Transition {
    pub fn key(self) -> &'static str {
        match self {
            Transition::Wipe => "wipe",
            Transition::Dissolve => "dissolve",
            Transition::Type => "type",
            Transition::Cut => "cut",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Transition::Wipe => "Wipe",
            Transition::Dissolve => "Dissolve",
            Transition::Type => "Type",
            Transition::Cut => "Cut",
        }
    }

    fn from_key(k: &str) -> Option<Transition> {
        TRANSITIONS.iter().copied().find(|t| t.key() == k)
    }
}

/// How long a page shows, in seconds: the least, the most, and a new page's.
pub const SECS: (u32, u32, u32) = (3, 60, 8);

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub id: String,
    pub name: String,
    /// the screen's light while this page shows; None, the profile's
    pub light: Option<[u8; 3]>,
    /// the page's own light, kept while the page follows the profile's, for
    /// when it has its own again
    pub kept_light: Option<[u8; 3]>,
    pub pieces: Vec<Piece>,
    /// as played: how long it shows before the next page, or whether it stays
    /// until the layouts are switched, and how it comes onto the screen after
    /// the page before it (each turn of the playlist says, see [Entry]; a page
    /// in the Library alone keeps a new page's)
    pub secs: u32,
    pub stays: bool,
    pub comes_in: Transition,
}

fn words(row: u8, col: u8, t: &str) -> Piece {
    Piece { kind: Kind::Text(t.into()), row, col, width: t.chars().count() as u8, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short }
}

fn reading(row: u8, col: u8, width: u8, s: Source, gap: u8) -> Piece {
    Piece { kind: Kind::Data(s, false), row, col, width, right: true, center: false, gap, height: 1, scroll: true, date_format: DateFormat::Short }
}

/// A piece across the whole of a row, in its middle.
fn middle(row: u8, kind: Kind) -> Piece {
    Piece { kind, row, col: 0, width: COLS, right: false, center: true, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short }
}

/// A piece set at the left of its width, a row high.
fn block(row: u8, col: u8, width: u8, kind: Kind) -> Piece {
    Piece { kind, row, col, width, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short }
}

/// An effect across whole rows, `height` of them from `row` down, at `speed`, with the word
/// a new Effect block has.
fn behind(row: u8, height: u8, fx: Fx, speed: u8) -> Piece {
    Piece { height, ..block(row, 0, COLS, Kind::Effect { fx, speed, word: "EVGA".into() }) }
}

/// The layouts Ectodeck ships, by key, in the order a new Library has them: the three
/// Playlist 1 plays, the time with the weather, the computer's readings, the keyboard's
/// profile, then the effects to look at. Each is set as Americans read them: the time on a
/// 12-hour clock and the weather in °F (the computer's temperatures stay in °C, as their
/// makers and every reading of them give them).
pub const BUILT_INS: [&str; 12] = ["stats", "clock", "weather", "today", "gaming", "heartbeat", "profile", "rain", "starfield", "campfire", "aquarium", "music"];

/// What a layout Ectodeck ships shows, for its card while it is as it shipped: in two lines
/// of a card's words at the window's first size (1440 wide), each line kept short to fit.
fn shipped_line(key: &str) -> Option<&'static str> {
    Some(match key {
        "stats" => "CPU, GPU, RAM and VRAM, every second.",
        "clock" => "The time and the date.",
        "weather" => "The temperature and the sky outside.",
        "today" => "The date, the time and the weather outside.",
        "gaming" => "GPU and CPU temperature and load, VRAM, the time.",
        "heartbeat" => "A heartbeat that races as the CPU works harder.",
        "profile" => "The profile in use, by name and number.",
        "rain" => "Rain falling, splashing where it lands.",
        "starfield" => "Stars streaming past, the nearest ones fastest.",
        "campfire" => "Flames flickering up the screen.",
        "aquarium" => "Fish swimming by, with bubbles rising.",
        "music" => "Bars that rise and fall with the computer’s sound.",
        _ => return None,
    })
}

impl Page {
    /// The Stats page as Grant laid it out for the z10-lcd service.
    pub fn stats() -> Page {
        Page {
            id: "stats".into(),
            name: "Stats".into(),
            light: None,
            kept_light: None,
            secs: SECS.2,
            stays: false,
            comes_in: Transition::Wipe,
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

    /// A page with nothing on it yet, to be made in the editor.
    pub fn blank() -> Page {
        Page { id: "custom".into(), name: "Custom".into(), light: None, kept_light: None, secs: SECS.2, stays: false, comes_in: Transition::Wipe, pieces: Vec::new() }
    }

    /// The time on the second row and the date on the third, in the middle.
    pub fn clock() -> Page {
        Page {
            id: "clock".into(),
            name: "Clock".into(),
            light: None,
            kept_light: None,
            secs: SECS.2,
            stays: false,
            comes_in: Transition::Wipe,
            pieces: vec![middle(1, Kind::Clock(true)), middle(2, Kind::Data(Source::Date, false))],
        }
    }

    /// The temperature outside on the second row and the sky on the third.
    pub fn weather() -> Page {
        Page {
            id: "weather".into(),
            name: "Weather".into(),
            light: None,
            kept_light: None,
            secs: SECS.2,
            stays: false,
            comes_in: Transition::Wipe,
            pieces: vec![middle(1, Kind::Data(Source::OutsideTemp, true)), middle(2, Kind::Data(Source::Sky, false))],
        }
    }

    /// A layout Ectodeck ships (see [BUILT_INS]), by its key, as it ships.
    pub fn built_in(key: &str) -> Option<Page> {
        let (name, pieces) = match key {
            "stats" => return Some(Page::stats()),
            "clock" => return Some(Page::clock()),
            "weather" => return Some(Page::weather()),
            // (the date written short, Wed Oct 14, as the longest dates written out run past
            // the screen's 20 cells; the sky in 15, as wide as every sky but the three longest,
            // Freezing drizzle, Heavy snow showers and Thunderstorm with hail, which scroll)
            "today" => (
                "Today",
                vec![middle(0, Kind::Data(Source::Date, false)), middle(1, Kind::Clock(true)), block(3, 0, 4, Kind::Data(Source::OutsideTemp, true)), reading(3, 5, 15, Source::Sky, 1)],
            ),
            "gaming" => (
                "Gaming",
                vec![
                    words(0, 0, "GPU"),
                    block(0, 4, 3, Kind::Data(Source::GpuTemp, false)),
                    block(0, 8, 12, Kind::Bar(Source::GpuLoad)),
                    words(1, 0, "CPU"),
                    block(1, 4, 3, Kind::Data(Source::CpuTemp, false)),
                    block(1, 8, 12, Kind::Bar(Source::CpuLoad)),
                    words(2, 0, "VRAM"),
                    reading(2, 5, 15, Source::VideoMemory, 1),
                    // (as wide as 12:59 PM)
                    block(3, 0, 8, Kind::Clock(true)),
                    reading(3, 10, 10, Source::Date, 1),
                ],
            ),
            "heartbeat" => (
                "Heartbeat",
                vec![words(0, 0, "CPU"), reading(0, 15, 5, Source::CpuTemp, 1), behind(1, 2, Fx::Heartbeat, 50), words(3, 0, "Load"), reading(3, 14, 6, Source::CpuLoad, 1)],
            ),
            "profile" => ("Profile", vec![middle(1, Kind::Data(Source::Profile, false)), words(2, 5, "Profile"), block(2, 13, 2, Kind::Data(Source::ProfileNumber, false))]),
            // (the effects alone, across the whole screen)
            "rain" => ("Rain", vec![behind(0, ROWS, Fx::Rain, 40)]),
            "starfield" => ("Starfield", vec![behind(0, ROWS, Fx::Stars, 40)]),
            "campfire" => ("Campfire", vec![behind(0, ROWS, Fx::Fire, 100)]),
            "aquarium" => ("Aquarium", vec![behind(0, ROWS, Fx::Aquarium, 40)]),
            "music" => ("Music", vec![behind(0, ROWS, Fx::Spectrum, 100)]),
            _ => return None,
        };
        Some(Page { id: key.into(), name: name.into(), pieces, ..Page::blank() })
    }

    /// A layout as Ectodeck ships it (one of [BUILT_INS], or blank), by its key, with an id
    /// no layout in `taken` has (a second Clock is clock-2).
    pub fn template(key: &str, taken: &[Page]) -> Option<Page> {
        let mut p = if key == "blank" { Page::blank() } else { Page::built_in(key)? };
        let base = p.id.clone();
        let mut n = 1;
        while taken.iter().any(|q| q.id == p.id) {
            n += 1;
            p.id = format!("{base}-{n}");
        }
        // pages made from blank are told apart by name too: Custom, Custom 2
        if key == "blank" && n > 1 {
            p.name = format!("Custom {n}");
        }
        Some(p)
    }

    /// What the page shows, in a line for its card in the Library. A page as
    /// its kind makes it says what that kind shows; a page of one's own says
    /// its words, then what else it shows; any other page changed since,
    /// what it reads now, by what each reading is of (CPU, GPU, RAM, VRAM).
    pub fn about(&self) -> String {
        let base = self.id.rsplit_once('-').filter(|(_, n)| n.parse::<u32>().is_ok()).map_or(self.id.as_str(), |(b, _)| b);
        let own = base == "custom";
        if let Some(line) = shipped_line(base).filter(|_| Page::built_in(base).is_some_and(|t| t.pieces == self.pieces)) {
            return line.into();
        }
        let said: Vec<&str> = self.pieces.iter().filter_map(|p| if let Kind::Text(t) = &p.kind { Some(t.trim()).filter(|t| !t.is_empty()) } else { None }).collect();
        let mut shown: Vec<String> = Vec::new();
        for p in &self.pieces {
            let what = match &p.kind {
                Kind::Text(_) | Kind::Effect { .. } => continue,
                Kind::Data(s, _) | Kind::Bar(s) => s.phrase().to_string(),
                Kind::Clock(_) => "the time".to_string(),
            };
            if !shown.contains(&what) {
                shown.push(what);
            }
        }
        let list = match shown.len() {
            0 => String::new(),
            1 => shown[0].clone(),
            n => format!("{} and {}", shown[..n - 1].join(", "), shown[n - 1]),
        };
        let words = format!("Says “{}”", said.join(" "));
        // (the computer's readings come every second)
        let computer = !shown.is_empty() && shown.iter().all(|w| ["CPU", "GPU", "RAM", "VRAM"].contains(&w.as_str()));
        let list = if computer { format!("{list}, every second") } else { list };
        // (and the effects it shows behind them: "The time, over rain.")
        let mut effects: Vec<String> = Vec::new();
        for p in &self.pieces {
            if let Kind::Effect { fx, .. } = &p.kind {
                let name = fx.said().to_string();
                if !effects.contains(&name) {
                    effects.push(name);
                }
            }
        }
        let capital = |s: &str| {
            let mut c = s.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect::<String>())
        };
        let said = match (said.is_empty(), list.is_empty()) {
            (true, true) => String::new(),
            (false, true) => words,
            (false, false) if own => format!("{words}, with {list}"),
            _ => capital(&list),
        };
        match (said.is_empty(), effects.is_empty()) {
            (true, true) => "An empty screen layout.".into(),
            (true, false) => format!("{}.", capital(&and_list(&effects))),
            (false, true) => format!("{said}."),
            (false, false) => format!("{said}, over {}.", and_list(&effects)),
        }
    }

    /// The page as the Library keeps it: what it shows, not how it plays.
    pub fn to_json(&self) -> Value {
        let pieces: Vec<Value> = self
            .pieces
            .iter()
            .map(|p| {
                let mut v = json!({ "row": p.row, "col": p.col, "width": p.width, "right": p.right, "gap": p.gap });
                if p.center {
                    v["center"] = json!(true);
                }
                if p.height > 1 {
                    v["height"] = json!(p.height);
                }
                if !p.scroll {
                    v["scroll"] = json!(false);
                }
                match &p.kind {
                    Kind::Text(t) => {
                        v["type"] = json!("text");
                        v["text"] = json!(t);
                    }
                    Kind::Data(s, f) => {
                        v["type"] = json!("data");
                        v["source"] = json!(s.key());
                        if let Source::Sensor(n) = s {
                            v["sensor"] = json!(crate::sensors::id_of(*n));
                        }
                        v["fahrenheit"] = json!(f);
                        if *s == Source::Date && p.date_format != DateFormat::Short {
                            v["format"] = json!(p.date_format.key());
                        }
                    }
                    Kind::Bar(s) => {
                        v["type"] = json!("bar");
                        v["source"] = json!(s.key());
                        if let Source::Sensor(n) = s {
                            v["sensor"] = json!(crate::sensors::id_of(*n));
                        }
                    }
                    Kind::Clock(twelve) => {
                        v["type"] = json!("clock");
                        v["twelve"] = json!(twelve);
                    }
                    Kind::Effect { fx, speed, word } => {
                        v["type"] = json!("effect");
                        v["effect"] = json!(fx.key());
                        v["speed"] = json!(speed);
                        v["word"] = json!(word);
                    }
                }
                v
            })
            .collect();
        let mut v = json!({ "id": self.id, "name": self.name, "pieces": pieces });
        if let Some(l) = self.light {
            v["light"] = json!(l);
        }
        if let Some(l) = self.kept_light {
            v["kept_light"] = json!(l);
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
                    "data" => Kind::Data(Source::of_json(p)?, p["fahrenheit"].as_bool().unwrap_or(false)),
                    "bar" => Kind::Bar(Source::of_json(p)?),
                    "clock" => Kind::Clock(p["twelve"].as_bool().unwrap_or(false)),
                    // (an effect no longer offered, as Life, reads as Rain: the block keeps its place)
                    "effect" => Kind::Effect {
                        fx: Fx::from_key(p["effect"].as_str().unwrap_or("")).unwrap_or(Fx::Rain),
                        speed: b(&p["speed"]).unwrap_or(50).min(100),
                        word: p["word"].as_str().unwrap_or("EVGA").chars().take(COLS as usize).collect(),
                    },
                    _ => return None,
                };
                let row = b(&p["row"])?.min(ROWS - 1);
                // (never past the screen's foot)
                let height = b(&p["height"]).unwrap_or(1).clamp(1, ROWS - row);
                Some(Piece {
                    kind,
                    row,
                    col: b(&p["col"])?.min(COLS - 1),
                    width: b(&p["width"])?.clamp(1, COLS),
                    right: p["right"].as_bool().unwrap_or(false),
                    center: p["center"].as_bool().unwrap_or(false),
                    gap: b(&p["gap"]).unwrap_or(1).min(1),
                    height,
                    scroll: p["scroll"].as_bool().unwrap_or(true),
                    date_format: p["format"].as_str().and_then(DateFormat::from_key).unwrap_or_default(),
                })
            })
            .collect();
        let rgb_of = |k: &str| v[k].as_array().filter(|a| a.len() == 3).map(|a| std::array::from_fn(|i| b(&a[i]).unwrap_or(0)));
        let (light, kept_light) = (rgb_of("light"), rgb_of("kept_light"));
        let secs = v["secs"].as_u64().map_or(SECS.2, |n| (n as u32).clamp(SECS.0, SECS.1));
        let comes_in = v["comes_in"].as_str().and_then(Transition::from_key).unwrap_or(Transition::Wipe);
        Some(Page { id: v["id"].as_str()?.into(), name: v["name"].as_str().unwrap_or("Page").into(), light, kept_light, pieces, secs, stays: false, comes_in })
    }
}

/// The pages kept in Ectodeck's settings, or every layout Ectodeck ships when none were
/// ever kept (a Library emptied by deleting its layouts stays empty).
pub fn pages_of(v: Option<&Value>) -> Vec<Page> {
    match v.and_then(|v| v.as_array()) {
        Some(a) => a.iter().filter_map(Page::from_json).collect(),
        None => BUILT_INS.iter().filter_map(|k| Page::built_in(k)).collect(),
    }
}

/// The layouts Ectodeck ships that aren't in `library` (deleted), by key, in the order a
/// new Library has them.
pub fn missing_built_ins(library: &[Page]) -> Vec<&'static str> {
    BUILT_INS.iter().copied().filter(|k| !library.iter().any(|p| p.id == *k)).collect()
}

/// One turn of the playlist: a screen of the Library, how long it shows,
/// and how it comes in after the one before it. The same screen can have
/// more than one turn. A turn that `stays` shows until the layouts are
/// switched, and keeps its seconds for when it is timed again. `key` tells
/// turns apart while Ectodeck runs (it is not kept), so a turn's block on
/// the strip stays the same block when others move.
#[derive(Clone, Debug)]
pub struct Entry {
    pub page: String,
    pub secs: u32,
    pub stays: bool,
    pub comes_in: Transition,
    pub key: u64,
}

impl PartialEq for Entry {
    fn eq(&self, o: &Entry) -> bool {
        (&self.page, self.secs, self.stays, self.comes_in) == (&o.page, o.secs, o.stays, o.comes_in)
    }
}

/// A name as typed, kept: trimmed, at most 40 characters, and no space left at the end
/// where the 40 cut it.
fn named(typed: &str) -> String {
    typed.trim().chars().take(40).collect::<String>().trim_end().to_string()
}

/// Names in a sentence, as the macros' profiles are: "A", "A and B", "A, B and C".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A key no other turn has.
fn new_key() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Entry {
    pub fn new(page: &str, secs: u32, comes_in: Transition) -> Entry {
        Entry { page: page.into(), secs: secs.clamp(SECS.0, SECS.1), stays: false, comes_in, key: new_key() }
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({ "page": self.page, "secs": self.secs, "comes_in": self.comes_in.key() });
        if self.stays {
            v["stays"] = json!(true);
        }
        v
    }

    pub fn from_json(v: &Value) -> Option<Entry> {
        let secs = v["secs"].as_u64().map_or(SECS.2, |n| (n as u32).clamp(SECS.0, SECS.1));
        let comes_in = v["comes_in"].as_str().and_then(Transition::from_key).unwrap_or(Transition::Wipe);
        Some(Entry { stays: v["stays"].as_bool().unwrap_or(false), ..Entry::new(v["page"].as_str()?, secs, comes_in) })
    }
}

/// A playlist: what it is called, and its turns, never none.
#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub turns: Vec<Entry>,
}

impl Playlist {
    fn to_json(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "turns": self.turns.iter().map(|e| e.to_json()).collect::<Vec<_>>() })
    }
}

/// The profiles there are, 1 to 5.
pub const PROFILES: [u8; 5] = [1, 2, 3, 4, 5];

/// The Library, the playlists, and the playlist each profile plays, as
/// Ectodeck keeps them. A profile not given one plays the first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screens {
    pub library: Vec<Page>,
    pub playlists: Vec<Playlist>,
    pub plays: std::collections::BTreeMap<u8, String>,
    /// the layout each profile keeps on the keyboard, its onboard layout, by its id, or
    /// "off" for the screen off; a profile given none keeps the Clock layout
    pub onboard: std::collections::BTreeMap<u8, String>,
}

impl Screens {
    /// From Ectodeck's settings: the Library's pages, the playlists (their
    /// turns of pages that are there), and the playlist each profile plays.
    /// Kept before there were playlists, the playlist was one list of turns
    /// (`playlist`), and before that the pages were the playlist, each with
    /// its own time and way in; either becomes Playlist 1, played by every profile.
    /// (The first playlist was called Default until Grant found the word clashed with
    /// the default backlight beside it: one still so named takes Playlist 1's name.)
    /// With nothing kept, the Library is new: every layout Ectodeck ships, and Playlist 1
    /// playing Stats, Clock and Weather.
    pub fn of(pages: Option<&Value>, playlists: Option<&Value>, plays: Option<&Value>, playlist: Option<&Value>) -> Screens {
        let library = pages_of(pages);
        let turns_of = |v: Option<&Value>| -> Vec<Entry> {
            v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(Entry::from_json).filter(|e| library.iter().any(|p| p.id == e.page)).collect()).unwrap_or_default()
        };
        let mut lists: Vec<Playlist> = playlists
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| {
                        // (an empty playlist is kept: its profile's screen shows blank)
                        let turns = turns_of(v.get("turns"));
                        let id = v["id"].as_str()?;
                        Some(Playlist { id: id.into(), name: v["name"].as_str().unwrap_or("Playlist").into(), turns })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if lists.is_empty() {
            let mut turns = turns_of(playlist);
            if turns.is_empty() && pages.is_none() {
                // (a new Library: Playlist 1 plays Stats, Clock and Weather)
                turns = ["stats", "clock", "weather"].iter().map(|id| Entry::new(id, SECS.2, Transition::Wipe)).collect();
            } else if turns.is_empty() {
                let old: Vec<Value> = pages.and_then(|v| v.as_array()).cloned().unwrap_or_default();
                turns = library
                    .iter()
                    .map(|p| {
                        let o = old.iter().find(|v| v["id"].as_str() == Some(p.id.as_str()));
                        Entry::from_json(&json!({ "page": p.id, "secs": o.map(|o| o["secs"].clone()), "comes_in": o.map(|o| o["comes_in"].clone()) })).expect("a page's turn")
                    })
                    .collect();
            }
            lists.push(Playlist { id: "default".into(), name: "Playlist 1".into(), turns });
        }
        // (the first playlist still under its old name, Default: the first Playlist name free)
        if let Some(k) = lists.iter().position(|l| l.id == "default" && l.name == "Default") {
            let mut n = 1;
            while lists.iter().any(|l| l.name == format!("Playlist {n}")) {
                n += 1;
            }
            lists[k].name = format!("Playlist {n}");
        }
        let plays = plays
            .and_then(|v| v.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| {
                        let n: u8 = k.parse().ok()?;
                        let id = v.as_str()?;
                        (PROFILES.contains(&n) && lists.iter().any(|l| l.id == id)).then(|| (n, id.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Screens { library, playlists: lists, plays, onboard: Default::default() }
    }

    /// The onboard layouts, from Ectodeck's settings: each profile's, by its number.
    pub fn with_onboard(mut self, onboard: Option<&Value>) -> Screens {
        self.onboard = onboard
            .and_then(|v| v.as_object())
            .map(|o| o.iter().filter_map(|(k, v)| Some((k.parse::<u8>().ok().filter(|n| PROFILES.contains(n))?, v.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        self
    }

    /// The layout profile `p` keeps on the keyboard: None for the screen off, and for a
    /// layout no longer in the Library (deleted, its profile's screen goes off).
    pub fn onboard_of(&self, p: u8) -> Option<&Page> {
        let id = self.onboard.get(&p).map_or("clock", |id| id.as_str());
        self.library.iter().find(|pg| pg.id == id)
    }

    /// Profile `p` keeps layout `id` on the keyboard from now on, or (None) the screen off.
    pub fn keep_onboard(&mut self, p: u8, id: Option<&str>) {
        let id = id.filter(|i| self.library.iter().any(|pg| pg.id == *i)).unwrap_or("off");
        self.onboard.insert(p, id.to_string());
    }

    /// All of it, as one value: for the settings, and for undoing a change.
    pub fn to_json(&self) -> Value {
        let plays: serde_json::Map<String, Value> = self.plays.iter().map(|(n, id)| (n.to_string(), json!(id))).collect();
        let onboard: serde_json::Map<String, Value> = self.onboard.iter().map(|(n, id)| (n.to_string(), json!(id))).collect();
        json!({
            "pages": self.library.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
            "playlists": self.playlists.iter().map(|l| l.to_json()).collect::<Vec<_>>(),
            "plays": plays,
            "onboard": onboard,
        })
    }

    pub fn from_json(v: &Value) -> Screens {
        Screens::of(v.get("pages"), v.get("playlists"), v.get("plays"), v.get("playlist")).with_onboard(v.get("onboard"))
    }

    /// Which playlist profile `p` plays, by its place.
    pub fn of_profile(&self, p: u8) -> usize {
        self.plays.get(&p).and_then(|id| self.playlists.iter().position(|l| &l.id == id)).unwrap_or(0)
    }

    pub fn playlist(&self, p: u8) -> &Playlist {
        &self.playlists[self.of_profile(p)]
    }

    /// The turns of the playlist profile `p` plays.
    pub fn turns(&self, p: u8) -> &Vec<Entry> {
        &self.playlist(p).turns
    }

    pub fn turns_mut(&mut self, p: u8) -> &mut Vec<Entry> {
        let i = self.of_profile(p);
        &mut self.playlists[i].turns
    }

    /// The pages profile `p`'s playlist plays, in turn: each turn's page,
    /// with the turn's time and way in.
    pub fn played(&self, p: u8) -> Vec<Page> {
        self.turns(p).iter().filter_map(|e| self.page_of(e).map(|pg| Page { secs: e.secs, stays: e.stays, comes_in: e.comes_in, ..pg.clone() })).collect()
    }

    pub fn page_of(&self, e: &Entry) -> Option<&Page> {
        self.library.iter().find(|p| p.id == e.page)
    }

    /// Where in the Library a page is, by its id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.library.iter().position(|p| p.id == id)
    }

    /// A turn of page `id` put in profile `p`'s playlist at `at`, as a new
    /// turn plays: eight seconds, coming in with a wipe.
    pub fn add_turn(&mut self, p: u8, at: usize, id: &str) -> usize {
        let turns = self.turns_mut(p);
        let at = at.min(turns.len());
        turns.insert(at, Entry::new(id, SECS.2, Transition::Wipe));
        at
    }

    /// Turn `i` moved to `to` (counted among the turns that stay); where it is now.
    pub fn move_turn(&mut self, p: u8, i: usize, to: usize) -> Option<usize> {
        let turns = self.turns_mut(p);
        if i >= turns.len() {
            return None;
        }
        let e = turns.remove(i);
        let to = to.min(turns.len());
        turns.insert(to, e);
        Some(to)
    }

    /// Turn `i` taken off the playlist, the last one too: an empty playlist's
    /// screen shows blank until a layout is added.
    pub fn remove_turn(&mut self, p: u8, i: usize) -> bool {
        let turns = self.turns_mut(p);
        if i < turns.len() {
            turns.remove(i);
            return true;
        }
        false
    }

    /// The profiles that play playlist `id`.
    pub fn profiles_of(&self, id: &str) -> Vec<u8> {
        PROFILES.iter().copied().filter(|p| self.playlist(*p).id == id).collect()
    }

    /// Profile `p` plays playlist `id` from now on.
    pub fn choose(&mut self, p: u8, id: &str) {
        if self.playlists.iter().any(|l| l.id == id) {
            self.plays.insert(p, id.to_string());
        }
    }

    /// A new playlist, Playlist 2 and so on, for profile `p` to play: one
    /// turn of the Library's first page, so the screen always has one.
    pub fn new_playlist(&mut self, p: u8) -> String {
        let mut n = self.playlists.len() + 1;
        while self.playlists.iter().any(|l| l.name == format!("Playlist {n}")) {
            n += 1;
        }
        let mut k = n;
        while self.playlists.iter().any(|l| l.id == format!("playlist-{k}")) {
            k += 1;
        }
        let id = format!("playlist-{k}");
        let first = self.library.first().map_or("stats".to_string(), |pg| pg.id.clone());
        self.playlists.push(Playlist { id: id.clone(), name: format!("Playlist {n}"), turns: vec![Entry::new(&first, SECS.2, Transition::Wipe)] });
        self.plays.insert(p, id.clone());
        id
    }

    /// The layouts Ectodeck ships that this Library was never given, put at its end in the
    /// order a new Library has them, and how many: `given` is the keys of those it was given,
    /// as the settings keep them, so a layout deleted after it was given stays deleted (a
    /// Library kept before they were kept is given each one it lacks). A layout whose id the
    /// Library already has isn't put there again.
    pub fn give_built_ins(&mut self, given: Option<&Value>) -> usize {
        let given: Vec<&str> = given.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|k| k.as_str()).collect()).unwrap_or_default();
        let mut added = 0;
        for key in BUILT_INS.iter().filter(|k| !given.contains(k)) {
            if let Some(p) = Page::built_in(key).filter(|p| !self.library.iter().any(|q| q.id == p.id)) {
                self.library.push(p);
                added += 1;
            }
        }
        added
    }

    /// Layout `key`, one Ectodeck ships, put back at the Library's end as it shipped, and
    /// its place.
    pub fn add_built_in(&mut self, key: &str) -> Option<usize> {
        let p = Page::template(key, &self.library).filter(|_| key != "blank")?;
        self.library.push(p);
        Some(self.library.len() - 1)
    }

    /// A new blank layout at the Library's end, and its place: Custom, or
    /// Custom 2 and on while a layout has that name.
    pub fn new_page(&mut self) -> usize {
        let mut p = Page::template("blank", &self.library).expect("a blank layout");
        let mut k = 1;
        p.name = "Custom".into();
        while self.library.iter().any(|q| q.name == p.name) {
            k += 1;
            p.name = format!("Custom {k}");
        }
        self.library.push(p);
        self.library.len() - 1
    }

    /// Layout `i` copied, the copy after it and named with the next free
    /// number (Stats 2), and the copy's place.
    pub fn duplicate_page(&mut self, i: usize) -> Option<usize> {
        let from = self.library.get(i)?.clone();
        let base = from.id.rsplit_once('-').filter(|(_, n)| n.parse::<u32>().is_ok()).map_or(from.id.as_str(), |(b, _)| b).to_string();
        let mut n = 2;
        while self.library.iter().any(|q| q.id == format!("{base}-{n}")) {
            n += 1;
        }
        let mut k = 2;
        while self.library.iter().any(|q| q.name == format!("{} {k}", from.name)) {
            k += 1;
        }
        let copy = Page { id: format!("{base}-{n}"), name: format!("{} {k}", from.name), ..from };
        self.library.insert(i + 1, copy);
        Some(i + 1)
    }

    /// Layout `i` moved to place `to` in the Library (counted among the others), and its
    /// place now: the playlists and the onboard layouts keep it, since they name it by its id.
    pub fn move_page(&mut self, i: usize, to: usize) -> Option<usize> {
        if i >= self.library.len() {
            return None;
        }
        let p = self.library.remove(i);
        let to = to.min(self.library.len());
        self.library.insert(to, p);
        Some(to)
    }

    /// Layout `i` renamed: trimmed, at most 40 characters, never to nothing.
    pub fn rename_page(&mut self, i: usize, name: &str) {
        let name = named(name);
        if let (false, Some(p)) = (name.is_empty(), self.library.get_mut(i)) {
            p.name = name;
        }
    }

    /// Layout `i` deleted: it comes off every playlist, and a profile that
    /// kept it on the keyboard keeps the screen off. Whether it was there.
    pub fn delete_page(&mut self, i: usize) -> bool {
        if i >= self.library.len() {
            return false;
        }
        let id = self.library.remove(i).id;
        for l in &mut self.playlists {
            l.turns.retain(|e| e.page != id);
        }
        for kept in self.onboard.values_mut() {
            if *kept == id {
                *kept = "off".into();
            }
        }
        true
    }

    /// What deleting layout `i` takes with it, for the question asked first: the
    /// playlists that play it, by name, and (where the keyboard keeps a layout by itself,
    /// `onboard`) the profiles that keep it, named by `name`, whose onboard layout becomes
    /// Screen off; or that nothing plays it.
    pub fn delete_says(&self, i: usize, onboard: bool, name: impl Fn(u8) -> String) -> String {
        let Some(page) = self.library.get(i) else { return String::new() };
        let lists: Vec<String> = self.playlists.iter().filter(|l| l.turns.iter().any(|e| e.page == page.id)).map(|l| l.name.clone()).collect();
        let kept: Vec<u8> = PROFILES.into_iter().filter(|p| onboard && self.onboard_of(*p).is_some_and(|k| k.id == page.id)).collect();
        let mut says = Vec::new();
        if !lists.is_empty() {
            let count = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine"].get(lists.len() - 1).map_or_else(|| lists.len().to_string(), |w| w.to_string());
            says.push(format!("{} comes off {count} {}: {}.", page.name, if lists.len() == 1 { "playlist" } else { "playlists" }, and_list(&lists)));
        }
        match kept.len() {
            0 => {}
            1 => says.push(format!("The onboard layout for {} becomes Screen off.", name(kept[0]))),
            5 => says.push("The onboard layouts for all profiles become Screen off.".into()),
            _ => says.push(format!("The onboard layouts for {} become Screen off.", and_list(&kept.iter().map(|p| name(*p)).collect::<Vec<_>>()))),
        }
        if says.is_empty() {
            says.push(if onboard { format!("No playlist plays {0}, and no profile keeps {0} on the keyboard.", page.name) } else { format!("No playlist plays {}.", page.name) });
        }
        says.join(" ")
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        let name = named(name);
        if let (false, Some(l)) = (name.is_empty(), self.playlists.iter_mut().find(|l| l.id == id)) {
            l.name = name;
        }
    }

    /// Playlist `id` deleted, never the last: the profiles that played it
    /// play the first playlist left.
    pub fn delete(&mut self, id: &str) -> bool {
        if self.playlists.len() < 2 {
            return false;
        }
        let Some(i) = self.playlists.iter().position(|l| l.id == id) else { return false };
        self.playlists.remove(i);
        self.plays.retain(|_, v| v != id);
        true
    }
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
    pub year: i32,
}

impl Moment {
    pub fn now() -> Moment {
        let mut t: libc::tm = unsafe { std::mem::zeroed() };
        let secs = unsafe { libc::time(std::ptr::null_mut()) };
        unsafe { libc::localtime_r(&secs, &mut t) };
        Moment { hour: t.tm_hour as u32, minute: t.tm_min as u32, weekday: t.tm_wday as u32, day: t.tm_mday as u32, month: t.tm_mon as u32, year: t.tm_year + 1900 }
    }
}

/// Everything a page can show, read once for each frame.
#[derive(Clone, Debug, Default)]
pub struct Readings {
    /// °C
    pub cpu_temp: Option<f64>,
    /// percent
    pub cpu_load: Option<f64>,
    pub cpu_ghz: Option<f64>,
    /// used and in all, GiB
    pub ram: Option<(f64, f64)>,
    /// the main card's
    pub gpu: Option<Gpu>,
    pub now: Option<Moment>,
    pub profile: Option<String>,
    /// the active profile's number; the keyboard's mode and key rollover, in its own words
    /// ("PC MODE", "6 KEY"), once its firmware can say them
    pub profile_number: Option<u8>,
    pub mode: Option<String>,
    pub rollover: Option<String>,
    /// °C, and the sky in words
    pub outside: Option<(f64, String)>,
    /// each checked sensor's reading, by the number a layout knows it by, and what it reads
    pub sensors: std::collections::HashMap<u16, (What, Val)>,
}

fn fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// A reading as the piece shows it (the date as `date` writes it), or None when it can't be had.
fn text_of(s: Source, f: bool, date: DateFormat, r: &Readings) -> Option<String> {
    let deg = |c: f64| if f { fahrenheit(c) } else { c };
    Some(match s {
        Source::CpuTemp => format!("{:>2.0}°", deg(r.cpu_temp?)),
        Source::CpuLoad => format!("{:>3.0}%", r.cpu_load?),
        Source::CpuClock => format!("{:>4.0}MHz", r.cpu_ghz? * 1000.0),
        Source::GpuTemp => {
            let t = r.gpu?.temp?;
            if f { format!("{:>2.0}°", fahrenheit(t as f64)) } else { format!("{t:>2}°") }
        }
        Source::GpuLoad => format!("{:>3}%", r.gpu?.load?),
        Source::GpuClock => format!("{:>4}MHz", r.gpu?.clock?),
        Source::Memory => {
            let (used, total) = r.ram?;
            format!("{used:.1} / {total:.1} GB")
        }
        Source::VideoMemory => {
            let (used, total) = r.gpu?.vram.filter(|v| v.1 > 0.0)?;
            format!("{used:.1} / {total:.1} GB")
        }
        Source::Date => date.write(r.now.as_ref()?),
        Source::Profile => ectotype_core::lcd::lcd_text(r.profile.as_deref()?),
        Source::ProfileNumber => r.profile_number?.to_string(),
        Source::GameMode => r.mode.clone()?,
        Source::KeyRollover => r.rollover.clone()?,
        Source::OutsideTemp => format!("{:.0}°", deg(r.outside.as_ref()?.0)),
        Source::Sky => ectotype_core::lcd::lcd_text(&r.outside.as_ref()?.1),
        Source::Sensor(n) => {
            let (what, v) = r.sensors.get(&n)?;
            crate::sensor_read::on_screen(*what, *v, f)
        }
    })
}

/// A share, 0 to 1, for a bar; None when it can't be had.
fn share_of(s: Source, r: &Readings) -> Option<f64> {
    Some(match s {
        Source::CpuTemp => r.cpu_temp? / 100.0,
        Source::CpuLoad => r.cpu_load? / 100.0,
        Source::GpuTemp => r.gpu?.temp? as f64 / 100.0,
        Source::GpuLoad => r.gpu?.load? as f64 / 100.0,
        Source::Memory => r.ram.map(|(u, t)| if t > 0.0 { u / t } else { 0.0 })?,
        Source::VideoMemory => r.gpu?.vram.filter(|v| v.1 > 0.0).map(|(u, t)| u / t)?,
        Source::Sensor(n) => {
            let (what, v) = r.sensors.get(&n)?;
            crate::sensor_read::share(*what, *v)?
        }
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

/// The fewest cells a piece shows its whole self in now, at its own height (see [least_at]).
pub fn least(p: &Piece, r: &Readings) -> u8 {
    least_at(p, r, p.height)
}

/// The fewest cells a piece can be, `height` rows tall: words any width (they carry on,
/// scroll or are cut off), and an effect; a bar three, its ends and a cell to fill; a reading on one row
/// as long as it is (a reading that can't be had, one), and on more rows any width, as
/// small as its maker likes (it carries on to the next row, and what still doesn't fit
/// stops after the last).
pub fn least_at(p: &Piece, r: &Readings, height: u8) -> u8 {
    match &p.kind {
        k if is_words(k) => 1,
        Kind::Effect { .. } => 1,
        Kind::Bar(_) => 3,
        _ if height > 1 => 1,
        _ => show(p, r).map_or(1, |s| s.chars().count().clamp(1, COLS as usize) as u8),
    }
}

/// Whether a piece shows words, which carry on to the next row where a row ends, and
/// scroll or are cut off when they don't fit, rather than run long: Text, and the
/// readings that are words.
pub fn is_words(kind: &Kind) -> bool {
    match kind {
        Kind::Text(_) => true,
        Kind::Data(s, _) => s.is_words(),
        _ => false,
    }
}

/// What a piece says now, whole (its words, or its reading as it reads), or None when it
/// can't be read.
pub fn says(p: &Piece, r: &Readings) -> Option<String> {
    show(p, r).ok()
}

/// What one piece shows; Err with what it reads from when that can't be had.
fn show(p: &Piece, r: &Readings) -> Result<String, &'static str> {
    let fail = |s: Source| s.reads_from().unwrap_or("");
    match &p.kind {
        Kind::Text(t) => Ok(ectotype_core::lcd::lcd_text(t)),
        Kind::Data(s, f) => text_of(*s, *f, p.date_format, r).ok_or(fail(*s)),
        Kind::Bar(s) => share_of(*s, r).map(|v| bar(v, p.width)).ok_or(fail(*s)),
        // (an effect draws its pattern, not words: see [lay_out])
        Kind::Effect { .. } => Ok(String::new()),
        Kind::Clock(twelve) => {
            let m = r.now.ok_or("")?;
            Ok(if *twelve { twelve_hour(m.hour, m.minute) } else { format!("{:02}:{:02}", m.hour, m.minute) })
        }
    }
}

/// A time of day on a 12-hour clock, as Americans write it: 9:05 PM, 12:30 AM.
pub fn twelve_hour(hour: u32, minute: u32) -> String {
    let h = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{h}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
}

/// The page's four rows, with these readings, as the page comes in (words that scroll
/// at their start).
pub fn render(page: &Page, r: &Readings) -> Vec<String> {
    render_at(page, r, 0.0)
}

/// The page's four rows `t` seconds after it came in: words that scroll have moved on.
pub fn render_at(page: &Page, r: &Readings, t: f64) -> Vec<String> {
    lay_out(page, r, t, false).0
}

/// The page's four rows as its card in the Library pictures them: as the page comes in,
/// but each effect at the moment its tile pictures it (see [crate::lcd_fx::still]), fed the
/// tiles' sample sound and heartbeat, so the picture holds still and shows the effect well
/// (Highway with traffic on the road, not the empty road it starts on).
pub fn render_still(page: &Page, r: &Readings) -> Vec<String> {
    lay_out(page, r, 0.0, true).0
}

/// Whether the page's rows change between readings: an effect on it, or words that scroll
/// now (more than fit their block, with Scroll chosen).
pub fn moving(page: &Page, r: &Readings) -> bool {
    pace(page, r).is_some()
}

/// How often the page's picture changes between readings, in seconds: an effect's step (a
/// tenth of a second at most, its shortest step if shorter), or the step of words scrolling a
/// cell at a time (see [SCROLL_STEP]); None while it holds still.
pub fn pace(page: &Page, r: &Readings) -> Option<f64> {
    let steps = page.pieces.iter().filter_map(|p| if let Kind::Effect { speed, .. } = p.kind { Some(crate::lcd_fx::period(speed)) } else { None });
    if let Some(shortest) = steps.reduce(f64::min) {
        return Some(shortest.min(0.1));
    }
    page.pieces
        .iter()
        .filter(|p| is_words(&p.kind) && p.scroll)
        .any(|p| show(p, r).is_ok_and(|s| if p.height <= 1 { s.chars().count() > p.width as usize } else { wrap(&s, p.width as usize).len() > p.height as usize }))
        .then_some(SCROLL_STEP)
}

/// How long words scrolling sideways hold each cell, in seconds: a little longer than the
/// 0.3 at which Grant found words on the keyboard's screen still legible, its liquid crystal
/// being slow to change (2026-10-13).
pub const SCROLL_STEP: f64 = 0.35;

/// Words set in a width: at its left, in its middle (any odd space after) or at its right.
fn align(s: &str, w: usize, center: bool, right: bool) -> String {
    if center {
        format!("{s:^w$}")
    } else if right {
        format!("{s:>w$}")
    } else {
        format!("{s:<w$}")
    }
}

/// Words carried on to the next row where a row of `w` cells ends: each row breaks at the
/// last space that fits, and only a word longer than the whole width breaks at its edge.
fn wrap(s: &str, w: usize) -> Vec<String> {
    let w = w.max(1);
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        let mut word: Vec<char> = word.chars().collect();
        while word.len() > w {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(word.drain(..w).collect());
        }
        let word: String = word.into_iter().collect();
        if cur.is_empty() {
            cur = word;
        } else if cur.chars().count() + 1 + word.chars().count() <= w {
            cur.push(' ');
            cur.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut cur, word));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Words longer than a row of `w` cells, `t` seconds in: still at their start for a moment,
/// then moving left a cell at a time at a reading pace, three spaces before they start again.
fn sideways(s: &str, w: usize, t: f64) -> String {
    let (gap, hold, step) = (3usize, 1.2, SCROLL_STEP);
    let ch: Vec<char> = s.chars().collect();
    let pass = ch.len() + gap;
    let k = t.max(0.0) % (hold + pass as f64 * step);
    let off = if k < hold { 0 } else { (((k - hold) / step) as usize).min(pass - 1) };
    let tape: Vec<char> = ch.iter().copied().chain(std::iter::repeat_n(' ', gap)).chain(ch.iter().copied()).collect();
    let shown: String = tape.iter().skip(off).take(w).collect();
    format!("{shown:<w$}")
}

/// Rows of words too many for `h` rows, `t` seconds in: the first `h` for a moment, then
/// moving up a row at a time, one blank row before they start again.
fn rolled(lines: &[String], h: usize, t: f64) -> Vec<String> {
    let (gap, hold, step) = (1usize, 3.0, 2.0);
    let pass = lines.len() + gap;
    let k = t.max(0.0) % (hold + pass as f64 * step);
    let off = if k < hold { 0 } else { ((((k - hold) / step) as usize) + 1).min(pass) };
    lines.iter().cloned().chain(std::iter::repeat_n(String::new(), gap)).chain(lines.iter().cloned()).skip(off).take(h).collect()
}

/// A piece of words in its own cells, `t` seconds in: a row of exactly its width for each
/// of its rows. On one row, the words as aligned, or when longer, scrolled through it
/// (Scroll) or cut at its end (Cut off); on more, carried on to the next row where a row
/// ends, and when they still don't fit, moved up a row at a time (Scroll) or stopped
/// after its last row (Cut off).
fn word_rows(p: &Piece, s: &str, t: f64) -> Vec<String> {
    let (w, h) = (p.width as usize, p.height.max(1) as usize);
    if h == 1 {
        let n = s.chars().count();
        return vec![if n <= w {
            align(s, w, p.center, p.right)
        } else if p.scroll {
            sideways(s, w, t)
        } else {
            s.chars().take(w).collect()
        }];
    }
    let lines = wrap(s, w);
    let shown = if lines.len() <= h {
        lines
    } else if p.scroll {
        rolled(&lines, h, t)
    } else {
        lines[..h].to_vec()
    };
    (0..h).map(|k| align(shown.get(k).map_or("", |l| l.as_str()), w, p.center, p.right)).collect()
}

/// A taller piece in its own cells, `t` seconds in: a row of exactly its width for each of
/// its rows. Words as [word_rows] sets them; a bar the same bar on every row, drawn
/// thicker; a reading carried on to the next row where a row ends, at a space when it has
/// one and otherwise at the block's edge, and stopped after the last row (a reading never
/// scrolls), each row aligned as the block is.
fn block_rows(p: &Piece, s: &str, t: f64) -> Vec<String> {
    let (w, h) = (p.width as usize, p.height.max(1) as usize);
    match &p.kind {
        k if is_words(k) => word_rows(p, s, t),
        Kind::Bar(_) => vec![align(s, w, p.center, p.right); h],
        _ => {
            let lines = wrap(s.trim(), w);
            (0..h).map(|k| align(lines.get(k).map_or("", |l| l.as_str()), w, p.center, p.right)).collect()
        }
    }
}

/// Whether the keyboard can draw a piece by itself, with no program on the computer to
/// send it (with Ectodeck's firmware build 5): words, the time and the date, and what the
/// keyboard knows about itself. The computer's readings, and bars of them, it can't.
pub fn onboard_can(kind: &Kind) -> bool {
    match kind {
        Kind::Text(_) | Kind::Clock(_) => true,
        Kind::Data(s, _) => matches!(s, Source::Date | Source::Profile | Source::ProfileNumber | Source::GameMode | Source::KeyRollover),
        Kind::Bar(_) | Kind::Effect { .. } => false,
    }
}

/// The page as the keyboard draws it by itself, as its onboard layout: the pieces it
/// can't draw left out, their cells blank. Until the keyboard can say them, its mode and
/// key rollover show as it starts up.
pub fn render_onboard(page: &Page, r: &Readings) -> Vec<String> {
    let mut r = r.clone();
    r.mode.get_or_insert_with(|| "PC MODE".into());
    r.rollover.get_or_insert_with(|| "6 KEY".into());
    let kept = Page { pieces: page.pieces.iter().filter(|p| onboard_can(&p.kind)).cloned().collect(), ..page.clone() };
    render(&kept, &r)
}

/// What of a page the keyboard can't draw by itself: the readings, by what they read
/// ("CPU", "GPU", "memory", "weather"), and whether it has bars.
#[derive(Debug, Default, PartialEq)]
pub struct Gaps {
    pub readings: Vec<&'static str>,
    pub bars: bool,
    /// the effects, by name
    pub effects: Vec<&'static str>,
}

impl Gaps {
    pub fn of(page: &Page) -> Gaps {
        let mut g = Gaps::default();
        for p in &page.pieces {
            match &p.kind {
                Kind::Bar(_) => g.bars = true,
                Kind::Effect { fx, .. } => {
                    if !g.effects.contains(&fx.name()) {
                        g.effects.push(fx.name());
                    }
                }
                Kind::Data(s, _) if !onboard_can(&p.kind) => {
                    let what = match s {
                        Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "CPU",
                        Source::GpuTemp | Source::GpuLoad | Source::GpuClock => "GPU",
                        Source::Memory | Source::VideoMemory => "memory",
                        _ => "weather",
                    };
                    if !g.readings.contains(&what) {
                        g.readings.push(what);
                    }
                }
                _ => {}
            }
        }
        g
    }

    /// The line under the screen while the page is the onboard layout: what the keyboard
    /// leaves blank, and why. None when it leaves nothing out.
    pub fn line(&self) -> Option<String> {
        let (parts, one) = self.parts();
        if parts.is_empty() {
            return None;
        }
        // (a sentence starts with a capital: "Weather readings", "CPU readings")
        let mut c = and_list(&parts).chars().collect::<Vec<_>>().into_iter();
        let list: String = c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect());
        Some(if one { format!("{list} needs Ectodeck, so the keyboard leaves it blank.") } else { format!("{list} need Ectodeck, so the keyboard leaves them blank.") })
    }

    /// The page's note in the menu of onboard layouts, before it is chosen.
    pub fn tag(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.readings.is_empty() {
            parts.push("Readings".into());
        }
        if self.bars {
            parts.push(if parts.is_empty() { "Bars" } else { "bars" }.into());
        }
        parts.extend(self.effects.iter().map(|e| e.to_string()));
        match parts.len() {
            0 => String::new(),
            1 if self.readings.is_empty() && !self.bars => format!("{} stays blank", parts[0]),
            _ => format!("{} stay blank", and_list(&parts)),
        }
    }

    /// What the keyboard leaves blank, as a sentence names them ("CPU readings", "bars",
    /// "Rain"), and whether that is one thing (an effect, which takes "needs" and "it").
    fn parts(&self) -> (Vec<String>, bool) {
        let mut parts = Vec::new();
        if !self.readings.is_empty() {
            let what: Vec<String> = self.readings.iter().map(|r| r.to_string()).collect();
            parts.push(format!("{} readings", and_list(&what)));
        }
        if self.bars {
            parts.push("bars".to_string());
        }
        parts.extend(self.effects.iter().map(|e| e.to_string()));
        let one = parts.len() == 1 && self.readings.is_empty() && !self.bars;
        (parts, one)
    }
}

/// Where each piece lands, in the page's order: its row, the cell it starts
/// at and how many it takes (a long reading runs past its width and pushes
/// the rest of its row along). A row said to be unavailable keeps its
/// pieces where they were put, as words always are.
pub fn spans(page: &Page, r: &Readings) -> Vec<(u8, usize, usize)> {
    lay_out(page, r, 0.0, false).1
}

/// The rows, and where each piece lands, `t` seconds in. A row holds its one-row pieces,
/// left to right, a reading that runs long pushing the ones after it along; and the rows
/// of any taller piece of words passing through it, which keeps its cells (a reading
/// pushed up to one stops at its edge).
fn lay_out(page: &Page, r: &Readings, t: f64, still: bool) -> (Vec<String>, Vec<(u8, usize, usize)>) {
    let mut said: Vec<&'static str> = Vec::new();
    let mut spans: Vec<(u8, usize, usize)> = page.pieces.iter().map(|p| (p.row, p.col as usize, p.width as usize)).collect();
    let effect = |p: &Piece| matches!(p.kind, Kind::Effect { .. });
    let tall = |p: &Piece| p.height > 1 && !effect(p);
    // the taller pieces' own rows, or what they read from when they can't be read
    let walls: Vec<(&Piece, Result<Vec<String>, &'static str>)> = page.pieces.iter().filter(|p| tall(p)).map(|p| (p, show(p, r).map(|s| block_rows(p, &s, t)))).collect();
    // the effects, behind every other block: each fills its own cells from its pattern at
    // its moment, and the other blocks cover it in theirs
    let mut behind = vec![vec![' '; COLS as usize]; ROWS as usize];
    for p in page.pieces.iter().filter(|p| effect(p)) {
        if let Kind::Effect { fx, speed, word } = &p.kind {
            let (at, live) = if still { (crate::lcd_fx::still(*fx), crate::lcd_fx::Live::sample()) } else { (crate::lcd_fx::tenths(*fx, t, *speed), crate::lcd_fx::live(*fx, *speed, r.cpu_load.unwrap_or(0.0))) };
            let f = crate::lcd_fx::frame(*fx, at, &ectotype_core::lcd::lcd_text(word), &live);
            for row in p.row..(p.row + p.height.max(1)).min(ROWS) {
                for c in p.col..(p.col + p.width).min(COLS) {
                    behind[row as usize][c as usize] = f[row as usize][c as usize];
                }
            }
        }
    }
    let rows = (0..ROWS)
        .map(|row| {
            let mut pieces: Vec<(usize, &Piece)> = page.pieces.iter().enumerate().filter(|(_, p)| p.row == row && !tall(p) && !effect(p)).collect();
            pieces.sort_by_key(|(_, p)| p.col);
            let through: Vec<&(&Piece, Result<Vec<String>, &'static str>)> = walls.iter().filter(|(p, _)| p.row <= row && row < p.row + p.height).collect();
            let shown: Vec<Result<String, &'static str>> = pieces.iter().map(|(_, p)| show(p, r)).collect();
            // (a taller piece that can't be read says so on its top row)
            let failed: Vec<&'static str> = shown.iter().filter_map(|s| s.as_ref().err().copied()).chain(through.iter().filter(|(p, _)| p.row == row).filter_map(|(_, w)| w.as_ref().err().copied())).collect();
            // (each piece's words where they start in the row; between them the effects show)
            let mut drawn: Vec<(usize, String)> = Vec::new();
            if !failed.is_empty() {
                let new = failed.iter().any(|f| !said.contains(f));
                for f in &failed {
                    if !said.contains(f) {
                        said.push(f);
                    }
                }
                if new {
                    // the row's own first words, or what it read
                    let label = pieces.iter().find_map(|(_, p)| match &p.kind {
                        Kind::Text(t) if !t.trim().is_empty() => Some(ectotype_core::lcd::lcd_text(t.trim())),
                        _ => None,
                    });
                    let label = label.unwrap_or_else(|| {
                        pieces
                            .iter()
                            .map(|(_, p)| *p)
                            .chain(through.iter().filter(|(p, w)| p.row == row && w.is_err()).map(|(p, _)| *p))
                            .find_map(|p| match &p.kind {
                                Kind::Data(s, _) | Kind::Bar(s) => Some(s.short().to_string()),
                                _ => None,
                            })
                            .unwrap_or_default()
                    });
                    drawn.push((0, format!("{label} unavailable")));
                }
            } else {
                let mut cursor = 0usize;
                for (k, ((i, p), s)) in pieces.iter().zip(&shown).enumerate() {
                    let s = s.as_ref().unwrap();
                    let (c, w) = (p.col as usize, p.width as usize);
                    let text = if is_words(&p.kind) { word_rows(p, s, t).swap_remove(0) } else { align(s, w, p.center, p.right) };
                    let start = if k == 0 { c } else { c.max(cursor + p.gap as usize) };
                    // (the taller piece to its right, which it stops at)
                    let limit = through.iter().map(|(q, _)| q.col as usize).filter(|&a| a >= c).min().unwrap_or(usize::MAX);
                    if start >= limit {
                        continue;
                    }
                    let text: String = text.chars().take(limit - start).collect();
                    let len = text.chars().count();
                    spans[*i] = (row, start, len);
                    cursor = start + len;
                    drawn.push((start, text));
                }
            }
            // over the effects, the row's pieces, then the taller pieces' rows over the cells
            // they keep
            let mut cells: Vec<char> = behind[row as usize].clone();
            let mut put = |at: usize, line: &str| {
                let line: Vec<char> = line.chars().collect();
                if cells.len() < at + line.len() {
                    cells.resize(at + line.len(), ' ');
                }
                cells[at..at + line.len()].copy_from_slice(&line);
            };
            for (at, text) in &drawn {
                put(*at, text);
            }
            for (p, w) in &through {
                if let Ok(rows) = w {
                    put(p.col as usize, &rows[(row - p.row) as usize]);
                }
            }
            cells.into_iter().collect::<String>().trim_end().to_string()
        })
        .collect();
    (rows, spans)
}

/// The keyboard's own status screen, the native layout its firmware draws when no program
/// on the computer drives the screen (read from Z10_RGB_117.bin, its rows in the factory
/// order): the profile's number and up to ten characters of its name; key rollover at the
/// left ("6 KEY", or "N KEY") and the mode at the right ("PC MODE", or "GAME MODE"); an
/// empty row; then the date and time, while the computer sends them (`clock`, as
/// [clock_now] gives it): the keyboard keeps no time itself, and the row is blank until
/// one comes. `apps` is the profile's mask of what it shows (0x04 the profile, 0x01 the
/// rollover, 0x08 the mode, 0x20 the clock), and a part switched off is left blank, as the
/// keyboard leaves it. Ectodeck can read neither the rollover nor the mode, so they show
/// as the keyboard starts up.
pub fn status_rows(apps: u64, profile: u8, name: &str, clock: Option<(u16, [u8; 5])>) -> Vec<String> {
    let mut rows = vec![String::new(); 4];
    if apps & 0x04 != 0 {
        let name: String = ectotype_core::lcd::lcd_text(name).chars().take(10).collect();
        rows[0] = format!("PROFILE {profile}:{name}");
    }
    let rollover = if apps & 0x01 != 0 { "6 KEY" } else { "" };
    let mode = if apps & 0x08 != 0 { "PC MODE" } else { "" };
    if apps & 0x09 != 0 {
        rows[1] = format!("{rollover:<5}{mode:>15}").trim_end().to_string();
    }
    // (24-hour, as EVGA's software sends it, two digits to each part but the year)
    if let (true, Some((y, [mo, d, h, mi, s]))) = (apps & 0x20 != 0, clock) {
        rows[3] = format!("{y:04}/{mo:02}/{d:02}  {h:02}:{mi:02}:{s:02}");
    }
    rows
}


/// The readings this computer can give, for the menu of readings.
pub fn available(r: &Readings, weather: bool) -> Vec<Source> {
    SOURCES
        .iter()
        .copied()
        .filter(|s| match s {
            Source::CpuTemp => r.cpu_temp.is_some(),
            Source::CpuClock => r.cpu_ghz.is_some(),
            Source::CpuLoad => r.cpu_load.is_some(),
            Source::Date | Source::Profile => true,
            Source::ProfileNumber => r.profile_number.is_some(),
            Source::GameMode => r.mode.is_some(),
            Source::KeyRollover => r.rollover.is_some(),
            Source::GpuTemp => r.gpu.is_some_and(|g| g.temp.is_some()),
            Source::GpuLoad => r.gpu.is_some_and(|g| g.load.is_some()),
            Source::GpuClock => r.gpu.is_some_and(|g| g.clock.is_some()),
            // (Intel's driver doesn't report it)
            Source::VideoMemory => r.gpu.is_some_and(|g| g.vram.is_some_and(|v| v.1 > 0.0)),
            Source::Memory => r.ram.is_some(),
            Source::OutsideTemp | Source::Sky => weather,
            Source::Sensor(n) => r.sensors.contains_key(n),
        })
        .collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn words_carry_on_to_the_next_row_and_scroll_or_stop_when_they_still_dont_fit() {
        let r = readings();
        let raid = "Raid at 9 tonight. Bring potions and the good snacks.";
        let block = |row: u8, height: u8, scroll: bool| Page { pieces: vec![Piece { height, scroll, ..words(row, 0, raid) }], ..Page::blank() };
        let w20 = |mut p: Page| {
            p.pieces[0].width = 20;
            p
        };
        // a row breaks at the last space that fits; three rows hold all of these words
        assert_eq!(render(&w20(block(1, 3, true)), &r), ["", "Raid at 9 tonight.", "Bring potions and", "the good snacks."]);
        assert!(!moving(&w20(block(1, 3, true)), &r), "words that fit stay still");
        // two rows, Scroll: the first two rows for three seconds, then up a row every two
        let two = w20(block(1, 2, true));
        assert!(moving(&two, &r));
        assert_eq!(render_at(&two, &r, 2.9)[1..3], ["Raid at 9 tonight.", "Bring potions and"]);
        assert_eq!(render_at(&two, &r, 3.5)[1..3], ["Bring potions and", "the good snacks."]);
        assert_eq!(render_at(&two, &r, 5.5)[1..3], ["the good snacks.", ""]);
        assert_eq!(render_at(&two, &r, 7.5)[1..3], ["", "Raid at 9 tonight."]);
        // and round again, back where they started
        assert_eq!(render_at(&two, &r, 11.0 + 1.0)[1..3], ["Raid at 9 tonight.", "Bring potions and"]);
        // two rows, Cut off: the words stop after the last row
        assert_eq!(render_at(&w20(block(1, 2, false)), &r, 30.0)[1..3], ["Raid at 9 tonight.", "Bring potions and"]);
        // one row, Scroll: still for a moment, then a cell to the left every step
        let one = w20(block(0, 1, true));
        assert_eq!(render_at(&one, &r, 1.0)[0], "Raid at 9 tonight. B");
        assert_eq!(render_at(&one, &r, 1.2 + SCROLL_STEP * 5.0 + 0.01)[0], "at 9 tonight. Bring");
        // one row, Cut off: up to the block's end
        assert_eq!(render_at(&w20(block(0, 1, false)), &r, 9.0)[0], "Raid at 9 tonight. B");
        // a word longer than the whole block breaks at its edge
        let mut long = Page { pieces: vec![Piece { height: 2, ..words(0, 0, "Supercalifragilistic") }], ..Page::blank() };
        long.pieces[0].width = 8;
        assert_eq!(render(&long, &r)[..2], ["Supercal", "ifragili"]);
    }

    #[test]
    fn a_taller_block_keeps_its_cells_and_a_long_reading_stops_at_its_edge() {
        let mut r = readings();
        r.cpu_temp = Some(105.0);
        // the temperature, three cells for four characters, before a two-row block of words
        let mut p = Page { pieces: vec![reading(0, 0, 3, Source::CpuTemp, 1), Piece { height: 2, ..words(0, 3, "AB CD") }], ..Page::blank() };
        p.pieces[1].width = 2;
        let rows = render(&p, &r);
        assert_eq!(&rows[0], "105AB", "the reading stops where the taller block begins");
        assert_eq!(&rows[1], "   CD");
        assert_eq!(spans(&p, &r)[1], (0, 3, 2), "the taller block holds its place");
        // kept and read back with its rows and its choice for words that don't fit
        let mut q = p.clone();
        q.pieces[1].scroll = false;
        let back = Page::from_json(&q.to_json()).unwrap();
        assert_eq!((back.pieces[1].height, back.pieces[1].scroll), (2, false));
        // a reading takes more rows too, never past the screen's foot
        let mut v = q.to_json();
        v["pieces"][0]["height"] = json!(3);
        assert_eq!(Page::from_json(&v).unwrap().pieces[0].height, 3);
        v["pieces"][0]["row"] = json!(2);
        assert_eq!(Page::from_json(&v).unwrap().pieces[0].height, 2);
    }

    #[test]
    fn the_keyboard_draws_what_it_can_and_leaves_the_rest_blank() {
        let r = readings();
        // Stats: its words stay, its readings go
        assert_eq!(render_onboard(&Page::stats(), &r), vec!["CPU", "GPU", "RAM", "VRAM"]);
        let g = Gaps::of(&Page::stats());
        assert_eq!(g.line().unwrap(), "CPU, GPU and memory readings need Ectodeck, so the keyboard leaves them blank.");
        assert_eq!(g.tag(), "Readings stay blank");
        // the Clock layout, the time and the date, it draws whole
        let clock = Page::clock();
        assert_eq!(render_onboard(&clock, &r), render(&clock, &r));
        assert_eq!((Gaps::of(&clock).line(), Gaps::of(&clock).tag()), (None, String::new()));
        // the weather, and a bar: named as what they are
        assert_eq!(Gaps::of(&Page::weather()).line().unwrap(), "Weather readings need Ectodeck, so the keyboard leaves them blank.");
        let mut barred = Page::blank();
        barred.pieces.push(Piece { kind: Kind::Bar(Source::CpuLoad), row: 0, col: 0, width: 10, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short });
        assert_eq!(Gaps::of(&barred).line().unwrap(), "Bars need Ectodeck, so the keyboard leaves them blank.");
        assert_eq!(Gaps::of(&barred).tag(), "Bars stay blank");
        // what the keyboard knows of itself: until it can say them, its mode and rollover as it starts up
        let mut own = Page::blank();
        own.pieces.push(Piece { kind: Kind::Data(Source::GameMode, false), row: 0, col: 0, width: 9, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short });
        own.pieces.push(Piece { kind: Kind::Data(Source::KeyRollover, false), row: 1, col: 0, width: 5, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short });
        assert_eq!(render_onboard(&own, &r)[..2], ["PC MODE", "6 KEY"]);
    }

    #[test]
    fn layouts_are_made_copied_renamed_and_deleted() {
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, None);
        let i = s.new_page();
        assert_eq!((s.library[i].name.as_str(), s.library[i].pieces.len()), ("Custom", 0));
        let c = s.duplicate_page(0).unwrap();
        assert_eq!((c, s.library[c].id.as_str(), s.library[c].name.as_str()), (1, "stats-2", "Stats 2"));
        s.rename_page(c, "  My stats  ");
        assert_eq!(s.library[c].name, "My stats");
        s.rename_page(c, "   ");
        assert_eq!(s.library[c].name, "My stats");
        // cut at 40 characters, a name ends on its last word, not a space
        s.rename_page(c, "Stats for the long raid nights with the guild");
        assert_eq!(s.library[c].name, "Stats for the long raid nights with the");
        s.rename_page(c, "My stats");
        // Stats, deleted, comes off the playlist; a profile keeping Clock on the keyboard, once Clock goes, keeps the screen off
        s.add_turn(1, 0, "stats");
        s.keep_onboard(2, Some("clock"));
        let stats = s.index_of("stats").unwrap();
        assert!(s.delete_page(stats));
        assert!(s.turns(1).iter().all(|e| e.page != "stats"));
        let clock = s.index_of("clock").unwrap();
        assert!(s.delete_page(clock));
        assert_eq!(s.onboard.get(&2).map(|v| v.as_str()), Some("off"));
        assert!(s.onboard_of(1).is_none());
        assert!(!s.delete_page(99));
        // a new layout takes the first Custom name no layout has
        s.rename_page(s.index_of("custom").unwrap(), "Mine");
        let j = s.new_page();
        assert_eq!((s.library[j].id.as_str(), s.library[j].name.as_str()), ("custom-2", "Custom"));
        // every layout deleted, the Library stays empty when read back
        while s.delete_page(0) {}
        assert!(Screens::from_json(&s.to_json()).library.is_empty());
    }

    #[test]
    fn readings_and_bars_take_more_rows() {
        let at = |h: u32, m: u32| Moment { hour: h, minute: m, weekday: 2, day: 13, month: 9, year: 2026 };
        let r = Readings { cpu_ghz: Some(4.286), ram: Some((22.5, 60.2)), cpu_load: Some(60.0), now: Some(at(2, 36)), ..Readings::default() };
        let piece = |kind: Kind, row: u8, col: u8, width: u8, height: u8| Piece { kind, row, col, width, right: true, center: false, gap: 1, height, scroll: true, date_format: DateFormat::Short };
        // the CPU clock, which has no space, breaks at the block's edge; the memory at its space
        let p = Page { pieces: vec![piece(Kind::Data(Source::CpuClock, false), 0, 4, 4, 2), piece(Kind::Data(Source::Memory, false), 2, 4, 7, 2)], ..Page::blank() };
        assert_eq!(render(&p, &r), vec!["    4286", "     MHz", "     22.5 /", "    60.2 GB"]);
        // the date written out and the 12-hour time, on two rows each, centered
        let mut date = piece(Kind::Data(Source::Date, false), 1, 2, 10, 2);
        date.date_format = DateFormat::Long;
        let mut time = piece(Kind::Clock(true), 1, 14, 4, 2);
        (date.center, time.center) = (true, true);
        let p = Page { pieces: vec![date, time], ..Page::blank() };
        assert_eq!(render(&p, &r), vec!["", "   Tuesday,   2:36", "  October 13   AM", ""]);
        // a bar two rows tall is the same bar on each, drawn thicker
        let mut bar = piece(Kind::Bar(Source::CpuLoad), 0, 0, 7, 2);
        bar.right = false;
        let p = Page { pieces: vec![bar], ..Page::blank() };
        assert_eq!(render(&p, &r), vec!["[###..]", "[###..]", "", ""]);
        // a reading that doesn't fit even in all its rows stops after the last of them
        let p = Page { pieces: vec![piece(Kind::Data(Source::Memory, false), 0, 0, 4, 2)], ..Page::blank() };
        assert_eq!(render(&p, &r), vec!["22.5", "   /", "", ""]);
        // on one row, as narrow as its reading; on more, as narrow as its maker likes
        let clock = piece(Kind::Data(Source::CpuClock, false), 0, 0, 4, 1);
        assert_eq!((least_at(&clock, &r, 1), least_at(&clock, &r, 2)), (7, 1));
        // kept as it was made
        let mut m = Page { pieces: vec![piece(Kind::Data(Source::Memory, false), 1, 0, 7, 3), piece(Kind::Bar(Source::CpuLoad), 0, 8, 10, 2)], ..Page::blank() };
        m.pieces[0].date_format = DateFormat::Iso;
        let back = Page::from_json(&m.to_json()).unwrap();
        assert_eq!((back.pieces[0].height, back.pieces[1].height), (3, 2));
    }

    #[test]
    fn an_effect_lies_behind_the_other_blocks() {
        let r = Readings { now: Some(Moment { hour: 4, minute: 40, weekday: 2, day: 13, month: 9, year: 2026 }), ..Readings::default() };
        let rain = Piece { kind: Kind::Effect { fx: Fx::Rain, speed: 50, word: String::new() }, row: 0, col: 0, width: 20, right: false, center: false, gap: 1, height: 4, scroll: true, date_format: DateFormat::Short };
        let time = Piece { kind: Kind::Clock(false), row: 1, col: 7, width: 5, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short };
        let p = Page { pieces: vec![rain.clone(), time], ..Page::blank() };
        let rows = render_at(&p, &r, 4.0);
        let fx: Vec<String> = crate::lcd_fx::frame(Fx::Rain, crate::lcd_fx::tenths(Fx::Rain, 4.0, 50), "", &crate::lcd_fx::Live::quiet()).iter().map(|row| row.iter().collect()).collect();
        // the time covers the rain in its own five cells, and the rain falls round it
        assert_eq!(&rows[1].chars().skip(7).take(5).collect::<String>(), "04:40");
        for (k, row) in rows.iter().enumerate() {
            let mine: Vec<char> = format!("{row:<20}").chars().collect();
            let theirs: Vec<char> = fx[k].chars().collect();
            for c in 0..20 {
                if !(k == 1 && (7..12).contains(&c)) {
                    assert_eq!(mine[c], theirs[c], "row {k}, cell {c}");
                }
            }
        }
        // it moves, so the screen is drawn every tenth of a second, and it is kept as it was made
        assert_eq!(pace(&p, &r), Some(0.1));
        let back = Page::from_json(&p.to_json()).unwrap();
        assert_eq!(back.pieces[0], rain);
        // the keyboard can't draw it by itself, and says so by its name
        assert_eq!(Gaps::of(&p).line().unwrap(), "Rain needs Ectodeck, so the keyboard leaves it blank.");
        assert_eq!(Gaps::of(&p).tag(), "Rain stays blank");
        assert_eq!(p.about(), "The time, over rain.");
    }

    #[test]
    fn a_card_whose_driver_reports_no_video_memory_leaves_it_out() {
        // (Intel's xe driver: temperature, load and clock, and 0 of 0 for video memory)
        let r = Readings { gpu: Some(Gpu::of(56, 3, 0, 0.0, 0.0)), ..Readings::default() };
        let avail = available(&r, false);
        assert!(avail.contains(&Source::GpuTemp) && !avail.contains(&Source::VideoMemory));
        let vram = Piece { kind: Kind::Data(Source::VideoMemory, false), row: 0, col: 0, width: 14, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short };
        assert_eq!(says(&vram, &r), None);
        let nvidia = Readings { gpu: Some(Gpu::of(45, 12, 1837, 13.1, 31.8)), ..Readings::default() };
        assert!(available(&nvidia, false).contains(&Source::VideoMemory));
    }

    #[test]
    fn the_date_has_formats() {
        let m = Moment { hour: 2, minute: 36, weekday: 2, day: 13, month: 9, year: 2026 };
        let written: Vec<String> = DATE_FORMATS.iter().map(|f| f.write(&m)).collect();
        assert_eq!(written, ["Tue Oct 13", "Tuesday, October 13", "October 13, 2026", "Oct 13", "13 Oct 2026", "Tuesday", "10/13/2026", "10/13", "13/10/2026", "2026-10-13"]);
        assert_eq!(DateFormat::Iso.write(&Moment { day: 5, month: 0, ..m }), "2026-01-05");
        // a date block made before there were formats reads as it did; a chosen one is kept
        let mut p = Page { pieces: vec![Piece { kind: Kind::Data(Source::Date, false), row: 0, col: 0, width: 10, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short }], ..Page::blank() };
        assert!(p.to_json()["pieces"][0].get("format").is_none());
        p.pieces[0].date_format = DateFormat::DayFirst;
        assert_eq!(p.to_json()["pieces"][0]["format"], "day-first");
        assert_eq!(Page::from_json(&p.to_json()).unwrap().pieces[0].date_format, DateFormat::DayFirst);
        let r = Readings { now: Some(m), ..Readings::default() };
        assert_eq!(render(&p, &r)[0], "13/10/2026");
    }

    #[test]
    fn deleting_a_layout_says_what_goes_with_it() {
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, None);
        let name = |p: u8| ["Games", "Late night work", "Profile 3", "Profile 4", "Profile 5"][p as usize - 1].to_string();
        let (stats, clock) = (s.index_of("stats").unwrap(), s.index_of("clock").unwrap());
        // Playlist 1 plays both; a new playlist, named Games, plays Stats too
        let games = s.new_playlist(2);
        s.rename(&games, "Games");
        assert_eq!(s.delete_says(stats, false, name), "Stats comes off two playlists: Playlist 1 and Games.");
        assert_eq!(s.delete_says(clock, false, name), "Clock comes off one playlist: Playlist 1.");
        // where the keyboard keeps a layout by itself, each profile given none keeps Clock
        assert_eq!(s.delete_says(clock, true, name), "Clock comes off one playlist: Playlist 1. The onboard layouts for all profiles become Screen off.");
        s.keep_onboard(1, Some("stats"));
        s.keep_onboard(3, None);
        assert_eq!(s.delete_says(stats, true, name), "Stats comes off two playlists: Playlist 1 and Games. The onboard layout for Games becomes Screen off.");
        assert_eq!(s.delete_says(clock, true, name), "Clock comes off one playlist: Playlist 1. The onboard layouts for Late night work, Profile 4 and Profile 5 become Screen off.");
        // a layout nothing plays
        let i = s.new_page();
        assert_eq!(s.delete_says(i, false, name), "No playlist plays Custom.");
        assert_eq!(s.delete_says(i, true, name), "No playlist plays Custom, and no profile keeps Custom on the keyboard.");
        assert_eq!(s.delete_says(99, true, name), "");
    }

    #[test]
    fn each_profile_keeps_a_layout_on_the_keyboard() {
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, None);
        // given none, a profile keeps the Clock layout
        assert_eq!(s.onboard_of(1).map(|p| p.id.as_str()), Some("clock"));
        s.keep_onboard(2, Some("stats"));
        s.keep_onboard(3, None);
        // a layout not in the Library is the screen off
        s.keep_onboard(4, Some("gone"));
        assert_eq!(s.onboard_of(2).map(|p| p.id.as_str()), Some("stats"));
        assert!(s.onboard_of(3).is_none() && s.onboard_of(4).is_none());
        // kept with the rest of the screens, and read back
        let back = Screens::from_json(&s.to_json());
        assert_eq!(back.onboard, s.onboard);
        // a layout deleted from the Library takes its profiles' screens off
        let mut gone = back.clone();
        gone.library.retain(|p| p.id != "stats");
        assert!(gone.onboard_of(2).is_none());
    }

    #[test]
    fn the_status_screen_is_drawn_as_the_firmware_draws_it() {
        // everything the keyboard can show (0x2f), as it comes from the factory
        assert_eq!(status_rows(0x2f, 1, "DEFAULT 1", None), vec!["PROFILE 1:DEFAULT 1", "6 KEY        PC MODE", "", ""]);
        // the clock, once the computer sends the time
        let t = Some((2026, [10, 12, 0, 5, 9]));
        assert_eq!(status_rows(0x2f, 1, "DEFAULT 1", t)[3], "2026/10/12  00:05:09");
        // the mode alone keeps its place at the right; a name longer than ten is cut
        assert_eq!(status_rows(0x0c, 3, "Afternoon games", t), vec!["PROFILE 3:Afternoon ", "             PC MODE", "", ""]);
        assert_eq!(status_rows(0x01, 2, "x", None), vec!["", "6 KEY", "", ""]);
    }

    #[test]
    fn a_blank_page_starts_empty_and_each_one_is_named_apart() {
        let one = Page::template("blank", &[]).unwrap();
        assert!(one.pieces.is_empty());
        assert_eq!((one.id.as_str(), one.name.as_str()), ("custom", "Custom"));
        let two = Page::template("blank", &[one.clone()]).unwrap();
        assert_eq!((two.id.as_str(), two.name.as_str()), ("custom-2", "Custom 2"));
    }

    use super::*;

    fn readings() -> Readings {
        Readings {
            cpu_temp: Some(64.5),
            cpu_load: Some(15.0),
            cpu_ghz: Some(4.36875),
            ram: Some((25.1, 60.2)),
            gpu: Some(Gpu::of(45, 12, 1837, 13.13, 31.84)),
            now: Some(Moment { hour: 21, minute: 5, weekday: 5, day: 9, month: 9, year: 2026 }),
            profile: Some("DEFAULT 1".into()),
            profile_number: Some(1),
            mode: None,
            rollover: None,
            outside: None,
            sensors: Default::default(),
        }
    }

    #[test]
    fn the_stats_page_keeps_its_place_through_json() {
        let p = Page::stats();
        assert_eq!(Page::from_json(&p.to_json()), Some(p.clone()));
        assert_eq!(pages_of(None).first(), Some(&p), "a new Library starts with Stats");
        assert_eq!(pages_of(Some(&json!([p.to_json()]))), vec![p]);
        assert!(pages_of(Some(&json!([]))).is_empty(), "a Library emptied by its owner stays empty");
    }

    #[test]
    fn a_block_shows_a_sensor_kept_by_its_id() {
        use crate::sensor_list::{Val, What};
        let dir = crate::sensor_list::tests::temp_dir("page-sensor");
        crate::sensor_list::tests::computer(&dir);
        let cat = crate::sensor_list::catalog(&dir, &mut crate::sensor_list::tests::nvidia());
        crate::sensors::remember(&cat, &Default::default());
        let pump = Source::Sensor(crate::sensors::number("nct6687@nct6687.2592/fan/Pump Fan"));
        let piece = |kind: Kind, row: u8| Piece { kind, row, col: 0, width: 7, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() };
        let page = Page { pieces: vec![piece(Kind::Text("PUMP".into()), 0), piece(Kind::Data(pump, false), 1)], ..Page::stats() };
        // kept by its id, and read back as the same sensor
        let saved = page.to_json();
        assert_eq!(saved["pieces"][1]["sensor"], json!("nct6687@nct6687.2592/fan/Pump Fan"));
        assert_eq!(Page::from_json(&saved).map(|p| p.pieces), Some(page.pieces.clone()));
        // its reading as the screen writes it; unchecked or gone, its row says its part is unavailable
        let mut r = readings();
        if let Source::Sensor(n) = pump {
            r.sensors.insert(n, (What::Fan, Val::One(2836.0)));
        }
        assert_eq!(render(&page, &r)[1].trim_end(), "2836RPM");
        r.sensors.clear();
        assert_eq!(render(&page, &r)[1].trim_end(), "Board unavailable");
        assert_eq!(pump.title(), "Pump Fan");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_reading_pushes_its_row_along() {
        let mut r = readings();
        r.cpu_temp = Some(105.25);
        r.cpu_load = Some(100.0);
        r.gpu = Some(Gpu::of(101, 100, 210, 0.04999, 8.0));
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
            kept_light: None,
            secs: SECS.2,
            stays: false,
            comes_in: Transition::Wipe,
            pieces: vec![
                Piece { kind: Kind::Clock(false), row: 0, col: 0, width: 5, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
                Piece { kind: Kind::Data(Source::Date, false), row: 0, col: 9, width: 11, right: true, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
                Piece { kind: Kind::Bar(Source::CpuLoad), row: 1, col: 0, width: 12, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
                Piece { kind: Kind::Data(Source::CpuTemp, true), row: 2, col: 0, width: 4, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
                Piece { kind: Kind::Clock(true), row: 3, col: 0, width: 8, right: false, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
                Piece { kind: Kind::Data(Source::Profile, false), row: 3, col: 10, width: 10, right: true, center: false, gap: 1, height: 1, scroll: true, date_format: DateFormat::Short },
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

    #[test]
    fn the_clock_and_the_weather_sit_in_the_middle() {
        let mut r = readings();
        // (as Americans read them: a 12-hour clock, the weather in °F)
        assert_eq!(render(&Page::clock(), &r), vec!["", "      9:05 PM", "     Fri Oct 9", ""]);
        r.outside = Some((21.6, "Partly cloudy".into()));
        assert_eq!(render(&Page::weather(), &r), vec!["", "        71°", "   Partly cloudy", ""]);
        // with no weather to show, the screen says so plainly
        r.outside = None;
        assert_eq!(render(&Page::weather(), &r), vec!["", "Weather unavailable", "", ""]);
    }

    #[test]
    fn a_turn_keeps_its_time_and_how_it_comes_in() {
        let p = Page::clock();
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), p.to_json()])), None, None, Some(&json!([{ "page": "clock", "secs": 15, "comes_in": "dissolve" }])));
        s.add_turn(1, 0, "stats");
        let back = Screens::from_json(&s.to_json());
        assert_eq!(back, s);
        assert_eq!(back.played(1).iter().map(|p| (p.id.as_str(), p.secs, p.comes_in)).collect::<Vec<_>>(), vec![("stats", SECS.2, Transition::Wipe), ("clock", 15, Transition::Dissolve)]);
        // a turn that stays until the layouts are switched keeps that, and its seconds for later
        s.turns_mut(1)[1].stays = true;
        let back = Screens::from_json(&s.to_json());
        assert_eq!(back, s);
        assert_eq!(back.played(1).iter().map(|p| (p.stays, p.secs)).collect::<Vec<_>>(), vec![(false, SECS.2), (true, 15)]);
        assert!(back.to_json()["playlists"][0]["turns"][0].get("stays").is_none(), "a timed turn is kept as before");
        // a page in the Library keeps what it shows; its turns keep how it plays
        assert!(p.to_json().get("secs").is_none());
        // and a second Clock is told apart from the first
        let q = Page::template("clock", &[p]).unwrap();
        assert_eq!(q.id, "clock-2");
        assert!(Page::template("snow", &[]).is_none(), "an effect with no layout of its own");
    }

    #[test]
    fn what_was_kept_before_becomes_playlist_1_for_every_profile() {
        // pages kept before the Library: each with its own time and way in
        let mut old = Page::stats().to_json();
        old["secs"] = json!(12);
        old["comes_in"] = json!("type");
        let s = Screens::of(Some(&json!([old, Page::clock().to_json()])), None, None, None);
        assert_eq!(s.library.len(), 2);
        assert_eq!((s.playlists.len(), s.playlists[0].name.as_str()), (1, "Playlist 1"));
        // one kept as Default, Ectodeck's old name for it, takes the new one (a name of the
        // person's own stays); beside a Playlist 1 already, the next number free
        let kept = |name: &str, other: &str| {
            let lists = json!([{ "id": "default", "name": name, "turns": [] }, { "id": "playlist-2", "name": other, "turns": [] }]);
            Screens::of(Some(&json!([Page::stats().to_json()])), Some(&lists), None, None).playlists[0].name.clone()
        };
        assert_eq!(kept("Default", "Games"), "Playlist 1");
        assert_eq!(kept("Default", "Playlist 1"), "Playlist 2");
        assert_eq!(kept("Mornings", "Games"), "Mornings");
        assert_eq!(s.turns(3), &vec![Entry::new("stats", 12, Transition::Type), Entry::new("clock", SECS.2, Transition::Wipe)]);
        // the one playlist kept before there were several
        let s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, Some(&json!([{ "page": "clock", "secs": 5 }])));
        assert_eq!(s.turns(1), &vec![Entry::new("clock", 5, Transition::Wipe)]);
        // a turn of a page not in the Library is dropped; none left, every page plays
        let s = Screens::of(Some(&json!([Page::stats().to_json()])), None, None, Some(&json!([{ "page": "gone", "secs": 5 }])));
        assert_eq!(s.turns(2), &vec![Entry::new("stats", SECS.2, Transition::Wipe)]);
    }

    #[test]
    fn each_profile_plays_its_own_playlist() {
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, None);
        // Profile 2 gets a playlist of its own: one turn of the Library's first page
        let id = s.new_playlist(2);
        assert_eq!((s.playlist(2).name.as_str(), s.turns(2).len(), s.turns(2)[0].page.as_str()), ("Playlist 2", 1, "stats"));
        assert_eq!(s.playlist(1).id, "default");
        assert_eq!(s.profiles_of("default"), vec![1, 3, 4, 5]);
        assert_eq!(s.profiles_of(&id), vec![2]);
        // a change to one profile's playlist leaves the other's
        s.add_turn(2, 9, "clock");
        assert_eq!((s.turns(2).len(), s.turns(1).len()), (2, 2));
        // kept and read back, named and played as they were
        s.rename(&id, "  Gaming  ");
        let back = Screens::from_json(&s.to_json());
        assert_eq!(back, s);
        assert_eq!(back.playlist(2).name, "Gaming");
        // Profile 4 plays Gaming too; Gaming deleted, both back on Default
        s.choose(4, &id);
        assert_eq!(s.profiles_of(&id), vec![2, 4]);
        assert!(s.delete(&id));
        assert_eq!(s.profiles_of("default"), vec![1, 2, 3, 4, 5]);
        assert!(!s.delete("default"), "never the last playlist");
        // a playlist the settings name but don't hold is played as the first
        let s = Screens::of(Some(&json!([Page::stats().to_json()])), None, Some(&json!({ "3": "gone" })), None);
        assert_eq!(s.of_profile(3), 0);
    }

    #[test]
    fn an_empty_playlist_shows_a_blank_screen() {
        // (what the player is given for an empty playlist: every cell clear)
        assert!(render(&Page::blank(), &readings()).iter().all(|row| row.trim().is_empty()));
    }

    #[test]
    fn a_card_says_what_its_page_shows() {
        assert_eq!(Page::stats().about(), "CPU, GPU, RAM and VRAM, every second.");
        // a second Clock, as its kind made it
        assert_eq!(Page::template("clock", &[Page::clock()]).unwrap().about(), "The time and the date.");
        let mut own = Page::blank();
        assert_eq!(own.about(), "An empty screen layout.");
        own.pieces.push(words(1, 4, "NO MEETINGS"));
        own.pieces.push(words(2, 7, "TODAY"));
        assert_eq!(own.about(), "Says “NO MEETINGS TODAY”.");
        own.pieces.push(middle(3, Kind::Clock(false)));
        assert_eq!(own.about(), "Says “NO MEETINGS TODAY”, with the time.");
        // Stats changed: what it reads now, its labels left out
        let mut s = Page::stats();
        s.pieces[1].gap = 2;
        assert_eq!(s.about(), "CPU, GPU, RAM and VRAM, every second.");
        s.pieces.retain(|p| matches!(p.kind, Kind::Text(_) | Kind::Data(Source::CpuTemp | Source::GpuLoad, _)));
        assert_eq!(s.about(), "CPU and GPU, every second.");
        s.pieces.push(middle(3, Kind::Clock(false)));
        assert_eq!(s.about(), "CPU, GPU and the time.");
    }

    #[test]
    fn a_layout_moves_to_another_place_in_the_library_and_keeps_its_turns() {
        let mut s = Screens::of(None, None, None, None);
        let ids = |s: &Screens| s.library.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        // Weather to the end, Music to the start: the rest in their order
        assert_eq!(s.move_page(2, 99), Some(BUILT_INS.len() - 1));
        assert_eq!(s.move_page(BUILT_INS.len() - 2, 0), Some(0));
        let mut want: Vec<String> = BUILT_INS.iter().map(|k| k.to_string()).filter(|k| k != "weather" && k != "music").collect();
        want.insert(0, "music".into());
        want.push("weather".into());
        assert_eq!(ids(&s), want);
        // Playlist 1 still plays the same layouts, and the order is kept through a save
        assert_eq!(s.turns(1).iter().map(|e| e.page.as_str()).collect::<Vec<_>>(), ["stats", "clock", "weather"]);
        assert_eq!(ids(&Screens::from_json(&s.to_json())), want);
        assert_eq!(s.move_page(99, 0), None);
    }

    #[test]
    fn turns_are_added_moved_and_taken_off() {
        let mut s = Screens::of(Some(&json!([Page::stats().to_json(), Page::clock().to_json()])), None, None, None);
        let pages = |s: &Screens| s.turns(1).iter().map(|e| e.page.clone()).collect::<Vec<_>>();
        // the same page twice
        assert_eq!(s.add_turn(1, 9, "stats"), 2);
        assert_eq!(pages(&s), ["stats", "clock", "stats"]);
        // keys tell the two Stats apart
        assert_ne!(s.turns(1)[0].key, s.turns(1)[2].key);
        assert_eq!(s.move_turn(1, 2, 0), Some(0));
        assert_eq!(pages(&s), ["stats", "stats", "clock"]);
        assert!(s.remove_turn(1, 0) && s.remove_turn(1, 0));
        assert_eq!(s.turns(1).len(), 1);
        // the last one too, and the empty playlist kept through a save
        assert!(s.remove_turn(1, 0));
        assert!(s.turns(1).is_empty() && !s.remove_turn(1, 0));
        let v = s.to_json();
        let back = Screens::of(Some(&v["pages"]), Some(&v["playlists"]), Some(&v["plays"]), None);
        assert!(back.turns(1).is_empty() && back.playlists.len() == s.playlists.len());
    }

    #[test]
    fn each_layout_ectodeck_ships_fits_the_screen_keeps_through_json_and_says_what_it_shows() {
        let r = readings();
        for key in BUILT_INS {
            let p = Page::built_in(key).unwrap();
            assert_eq!((p.id.as_str(), p.light, p.secs, p.comes_in), (key, None, SECS.2, Transition::Wipe));
            assert_eq!(Page::from_json(&p.to_json()), Some(p.clone()), "{key}, kept and read back");
            assert_eq!(Page::template(key, &[]), Some(p.clone()));
            // every block inside the screen, and none over another (an effect is behind them)
            let cells = |q: &Piece| (q.row..q.row + q.height).flat_map(move |row| (q.col..q.col + q.width).map(move |c| (row, c))).collect::<Vec<_>>();
            let mut taken = Vec::new();
            for q in &p.pieces {
                assert!(q.row + q.height <= ROWS && q.col + q.width <= COLS, "{key}: a block past the screen");
                if !matches!(q.kind, Kind::Effect { .. }) {
                    for cell in cells(q) {
                        assert!(!taken.contains(&cell), "{key}: two blocks at {cell:?}");
                        taken.push(cell);
                    }
                }
            }
            // its card's line, the same for a copy, and once changed what it shows now
            assert_eq!(p.about(), shipped_line(key).unwrap());
            assert_eq!(Page::template(key, std::slice::from_ref(&p)).unwrap().about(), p.about());
            let mut changed = p.clone();
            changed.pieces.pop();
            assert_ne!(changed.about(), p.about(), "{key}, changed");
            // and it draws: no row past the screen's width
            assert!(render(&p, &r).iter().all(|row| row.chars().count() <= COLS as usize));
        }
        assert!(Page::built_in("blank").is_none() && Page::built_in("snow").is_none());
    }

    #[test]
    fn the_readings_layouts_read_as_designed() {
        let r = readings();
        let rows = |key: &str| render(&Page::built_in(key).unwrap(), &r);
        assert_eq!(rows("gaming"), ["GPU 45° [#.........]", "CPU 64° [##........]", "VRAM  13.1 / 31.8 GB", "9:05 PM    Fri Oct 9"]);
        // (no place for the weather yet: the row says so)
        assert_eq!(rows("today"), ["     Fri Oct 9", "      9:05 PM", "", "Weather unavailable"]);
        // and with one, in °F, the time on a 12-hour clock
        let mut out = r.clone();
        out.outside = Some((19.0, "Clear".into()));
        let rows = |key: &str| render(&Page::built_in(key).unwrap(), &out);
        assert_eq!(rows("today")[3], "66°            Clear");
        assert_eq!(rows("weather")[1..3], ["        66°", "       Clear"]);
        assert_eq!(rows("clock")[1], "      9:05 PM");
        assert_eq!(rows("profile"), ["", "     DEFAULT 1", "     Profile 1", ""]);
        // the heartbeat between the CPU's readings
        let beat = rows("heartbeat");
        assert_eq!([beat[0].as_str(), beat[3].as_str()], ["CPU              64°", "Load             15%"]);
        // the sky beside the temperature as wide as every sky but the three longest
        out.outside = Some((-1.0, "Partly cloudy".into()));
        assert_eq!(render(&Page::built_in("today").unwrap(), &out)[3], "30°    Partly cloudy");
    }

    #[test]
    fn a_card_pictures_an_effect_at_the_moment_its_tile_does() {
        let r = readings();
        for key in ["rain", "starfield", "campfire", "aquarium", "music"] {
            let p = Page::built_in(key).unwrap();
            let Kind::Effect { fx, .. } = p.pieces[0].kind else { panic!("{key} is an effect") };
            let tile: Vec<String> = crate::lcd_fx::frame(fx, crate::lcd_fx::still(fx), "EVGA", &crate::lcd_fx::Live::sample()).iter().map(|row| row.iter().collect::<String>().trim_end().to_string()).collect();
            assert_eq!(render_still(&p, &r), tile, "{key}");
        }
        // and words as the page comes in
        assert_eq!(render_still(&Page::stats(), &r), render(&Page::stats(), &r));
    }

    #[test]
    fn a_new_library_has_every_layout_and_a_kept_one_is_given_each_new_one_once() {
        // nothing kept: every layout Ectodeck ships, Playlist 1 playing Stats, Clock and Weather,
        // and each profile keeping Clock on the keyboard
        let s = Screens::of(None, None, None, None);
        assert_eq!(s.library.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), BUILT_INS);
        assert_eq!(s.turns(1).iter().map(|e| (e.page.as_str(), e.secs)).collect::<Vec<_>>(), [("stats", 8), ("clock", 8), ("weather", 8)]);
        assert_eq!(s.onboard_of(3).map(|p| p.id.as_str()), Some("clock"));
        let mut again = s.clone();
        assert_eq!(again.give_built_ins(None), 0);
        assert_eq!(again, s);
        // a Library kept before: its own layouts stay where they were, each one it lacks after
        // them, and its playlist as it was
        let mine = Page { id: "custom".into(), name: "Raid night".into(), pieces: vec![words(1, 0, "RAID AT 9")], ..Page::blank() };
        let kept = json!([Page::stats().to_json(), mine.to_json(), Page::clock().to_json()]);
        let mut old = Screens::of(Some(&kept), None, None, None);
        assert_eq!(old.give_built_ins(None), BUILT_INS.len() - 2);
        let ids: Vec<&str> = old.library.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids[..3], ["stats", "custom", "clock"]);
        assert_eq!(ids[3..], BUILT_INS.iter().copied().filter(|k| !["stats", "clock"].contains(k)).collect::<Vec<_>>());
        assert_eq!(old.turns(1).iter().map(|e| e.page.as_str()).collect::<Vec<_>>(), ["stats", "custom", "clock"]);
        // once given, a layout deleted stays deleted
        old.delete_page(old.index_of("rain").unwrap());
        old.delete_page(old.index_of("weather").unwrap());
        assert_eq!(old.give_built_ins(Some(&json!(BUILT_INS))), 0);
        assert_eq!(missing_built_ins(&old.library), ["weather", "rain"]);
        // and a layout Ectodeck ships later is given once too
        let mut later = Screens::of(Some(&kept), None, None, None);
        let had: Vec<&str> = BUILT_INS.iter().copied().filter(|k| *k != "music").collect();
        assert_eq!(later.give_built_ins(Some(&json!(had))), 1);
        assert_eq!(later.library.last().map(|p| p.id.as_str()), Some("music"));
        // a deleted one put back as it shipped, at the Library's end
        let at = old.add_built_in("rain").unwrap();
        assert_eq!((at, &old.library[at]), (old.library.len() - 1, &Page::built_in("rain").unwrap()));
        assert_eq!(missing_built_ins(&old.library), ["weather"]);
        assert!(old.add_built_in("blank").is_none() && old.add_built_in("snow").is_none());
    }
}
