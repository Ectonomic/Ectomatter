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
    /// what the keyboard knows about itself: the active profile's number, its mode
    /// (PC, game or LCD) and its key rollover (the last two read from firmware build 5 on)
    ProfileNumber,
    GameMode,
    KeyRollover,
    OutsideTemp,
    Sky,
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
            Source::ProfileNumber => "Profile number",
            Source::GameMode => "Game mode",
            Source::KeyRollover => "Key rollover",
            Source::OutsideTemp => "Outside temperature",
            Source::Sky => "Weather",
        }
    }

    /// What it is a reading of, as a card's line on a page says it.
    pub fn phrase(self) -> &'static str {
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
        }
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
        }
    }

    /// Whether a temperature, which can be shown in °C or °F.
    /// Whether the reading is words (the weather, the active profile's name), which
    /// carry on to the next row, scroll or are cut off, as Text does.
    pub fn is_words(self) -> bool {
        matches!(self, Source::Sky | Source::Profile)
    }

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
            Source::GameMode | Source::KeyRollover => Some("keyboard"),
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
    /// set in the middle of its width (before `right`)
    pub center: bool,
    /// the spaces kept after the piece before it, when that piece runs long
    pub gap: u8,
    /// how many rows it takes, from `row` down: more than one only for words (see
    /// [is_words]), which carry on to the next row where a row ends
    pub height: u8,
    /// words that don't fit even in all its rows: scrolled through it (left on one row,
    /// up a row at a time on more), or else cut off at its end
    pub scroll: bool,
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
    /// as played: how long it shows before the next page, and how it comes
    /// onto the screen after the page before it (each turn of the playlist
    /// says, see [Entry]; a page in the Library alone keeps a new page's)
    pub secs: u32,
    pub comes_in: Transition,
}

fn words(row: u8, col: u8, t: &str) -> Piece {
    Piece { kind: Kind::Text(t.into()), row, col, width: t.chars().count() as u8, right: false, center: false, gap: 1, height: 1, scroll: true }
}

fn reading(row: u8, col: u8, width: u8, s: Source, gap: u8) -> Piece {
    Piece { kind: Kind::Data(s, false), row, col, width, right: true, center: false, gap, height: 1, scroll: true }
}

/// A piece across the whole of a row, in its middle.
fn middle(row: u8, kind: Kind) -> Piece {
    Piece { kind, row, col: 0, width: COLS, right: false, center: true, gap: 1, height: 1, scroll: true }
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
        Page { id: "custom".into(), name: "Custom".into(), light: None, kept_light: None, secs: SECS.2, comes_in: Transition::Wipe, pieces: Vec::new() }
    }

    /// The time on the second row and the date on the third, in the middle.
    pub fn clock() -> Page {
        Page {
            id: "clock".into(),
            name: "Clock".into(),
            light: None,
            kept_light: None,
            secs: SECS.2,
            comes_in: Transition::Wipe,
            pieces: vec![middle(1, Kind::Clock(false)), middle(2, Kind::Data(Source::Date, false))],
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
            comes_in: Transition::Wipe,
            pieces: vec![middle(1, Kind::Data(Source::OutsideTemp, false)), middle(2, Kind::Data(Source::Sky, false))],
        }
    }

    /// A layout as Ectodeck ships it (stats, clock, weather, or blank), by its
    /// key, with an id no layout in `taken` has (a second Clock is clock-2).
    pub fn template(key: &str, taken: &[Page]) -> Option<Page> {
        let mut p = match key {
            "blank" => Page::blank(),
            "stats" => Page::stats(),
            "clock" => Page::clock(),
            "weather" => Page::weather(),
            _ => return None,
        };
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
        if Page::template(base, &[]).is_some_and(|t| t.pieces == self.pieces) {
            match base {
                "stats" => return "CPU, GPU, RAM and VRAM, every second.".into(),
                "clock" => return "The time and the date.".into(),
                "weather" => return "The temperature and the sky outside.".into(),
                _ => {}
            }
        }
        let said: Vec<&str> = self.pieces.iter().filter_map(|p| if let Kind::Text(t) = &p.kind { Some(t.trim()).filter(|t| !t.is_empty()) } else { None }).collect();
        let mut shown: Vec<String> = Vec::new();
        for p in &self.pieces {
            let what = match &p.kind {
                Kind::Text(_) => continue,
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
        match (said.is_empty(), list.is_empty()) {
            (true, true) => "An empty screen layout.".into(),
            (false, true) => format!("{words}."),
            (false, false) if own => format!("{words}, with {list}."),
            _ => {
                let mut c = list.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect::<String>() + ".")
            }
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
                    "data" => Kind::Data(Source::from_key(p["source"].as_str()?)?, p["fahrenheit"].as_bool().unwrap_or(false)),
                    "bar" => Kind::Bar(Source::from_key(p["source"].as_str()?)?),
                    "clock" => Kind::Clock(p["twelve"].as_bool().unwrap_or(false)),
                    _ => return None,
                };
                let row = b(&p["row"])?.min(ROWS - 1);
                // (only words take more than one row, and never past the screen's foot)
                let height = if is_words(&kind) { b(&p["height"]).unwrap_or(1).clamp(1, ROWS - row) } else { 1 };
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
                })
            })
            .collect();
        let rgb_of = |k: &str| v[k].as_array().filter(|a| a.len() == 3).map(|a| std::array::from_fn(|i| b(&a[i]).unwrap_or(0)));
        let (light, kept_light) = (rgb_of("light"), rgb_of("kept_light"));
        let secs = v["secs"].as_u64().map_or(SECS.2, |n| (n as u32).clamp(SECS.0, SECS.1));
        let comes_in = v["comes_in"].as_str().and_then(Transition::from_key).unwrap_or(Transition::Wipe);
        Some(Page { id: v["id"].as_str()?.into(), name: v["name"].as_str().unwrap_or("Page").into(), light, kept_light, pieces, secs, comes_in })
    }
}

/// The pages kept in Ectodeck's settings, or Stats as it ships when none were
/// ever kept (a Library emptied by deleting its layouts stays empty).
pub fn pages_of(v: Option<&Value>) -> Vec<Page> {
    match v.and_then(|v| v.as_array()) {
        Some(a) => a.iter().filter_map(Page::from_json).collect(),
        None => vec![Page::stats()],
    }
}

/// One turn of the playlist: a screen of the Library, how long it shows,
/// and how it comes in after the one before it. The same screen can have
/// more than one turn. `key` tells turns apart while Ectodeck runs (it is
/// not kept), so a turn's block on the strip stays the same block when
/// others move.
#[derive(Clone, Debug)]
pub struct Entry {
    pub page: String,
    pub secs: u32,
    pub comes_in: Transition,
    pub key: u64,
}

impl PartialEq for Entry {
    fn eq(&self, o: &Entry) -> bool {
        (&self.page, self.secs, self.comes_in) == (&o.page, o.secs, o.comes_in)
    }
}

/// A key no other turn has.
fn new_key() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Entry {
    pub fn new(page: &str, secs: u32, comes_in: Transition) -> Entry {
        Entry { page: page.into(), secs: secs.clamp(SECS.0, SECS.1), comes_in, key: new_key() }
    }

    pub fn to_json(&self) -> Value {
        json!({ "page": self.page, "secs": self.secs, "comes_in": self.comes_in.key() })
    }

    pub fn from_json(v: &Value) -> Option<Entry> {
        let secs = v["secs"].as_u64().map_or(SECS.2, |n| (n as u32).clamp(SECS.0, SECS.1));
        let comes_in = v["comes_in"].as_str().and_then(Transition::from_key).unwrap_or(Transition::Wipe);
        Some(Entry::new(v["page"].as_str()?, secs, comes_in))
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
    /// its own time and way in; either becomes Default, played by every profile.
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
            if turns.is_empty() {
                let old: Vec<Value> = pages.and_then(|v| v.as_array()).cloned().unwrap_or_default();
                turns = library
                    .iter()
                    .map(|p| {
                        let o = old.iter().find(|v| v["id"].as_str() == Some(p.id.as_str()));
                        Entry::from_json(&json!({ "page": p.id, "secs": o.map(|o| o["secs"].clone()), "comes_in": o.map(|o| o["comes_in"].clone()) })).expect("a page's turn")
                    })
                    .collect();
            }
            lists.push(Playlist { id: "default".into(), name: "Default".into(), turns });
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
        self.turns(p).iter().filter_map(|e| self.page_of(e).map(|pg| Page { secs: e.secs, comes_in: e.comes_in, ..pg.clone() })).collect()
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

    /// Layout `i` renamed: trimmed, at most 40 characters, never to nothing.
    pub fn rename_page(&mut self, i: usize, name: &str) {
        let name: String = name.trim().chars().take(40).collect();
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

    pub fn rename(&mut self, id: &str, name: &str) {
        let name: String = name.trim().chars().take(40).collect();
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
    /// the active profile's number; the keyboard's mode and key rollover, in its own words
    /// ("PC MODE", "6 KEY"), once its firmware can say them
    pub profile_number: Option<u8>,
    pub mode: Option<String>,
    pub rollover: Option<String>,
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
        Source::ProfileNumber => r.profile_number?.to_string(),
        Source::GameMode => r.mode.clone()?,
        Source::KeyRollover => r.rollover.clone()?,
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

/// The fewest cells a piece shows its whole self in now: its words or its reading, as
/// long as they are (a reading that can't be had, one), and a bar three, its ends and a
/// cell to fill.
pub fn least(p: &Piece, r: &Readings) -> u8 {
    match &p.kind {
        // (words fit any width: they carry on, scroll or are cut off)
        k if is_words(k) => 1,
        Kind::Bar(_) => 3,
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

/// The page's four rows, with these readings, as the page comes in (words that scroll
/// at their start).
pub fn render(page: &Page, r: &Readings) -> Vec<String> {
    render_at(page, r, 0.0)
}

/// The page's four rows `t` seconds after it came in: words that scroll have moved on.
pub fn render_at(page: &Page, r: &Readings, t: f64) -> Vec<String> {
    lay_out(page, r, t).0
}

/// Whether the page has words that scroll now (more than fit their block, with Scroll
/// chosen), so its rows change between readings.
pub fn moving(page: &Page, r: &Readings) -> bool {
    page.pieces.iter().filter(|p| is_words(&p.kind) && p.scroll).any(|p| {
        show(p, r).is_ok_and(|s| if p.height <= 1 { s.chars().count() > p.width as usize } else { wrap(&s, p.width as usize).len() > p.height as usize })
    })
}

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
    let (gap, hold, step) = (3usize, 1.2, 0.25);
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

/// Whether the keyboard can draw a piece by itself, with no program on the computer to
/// send it (with Ectodeck's firmware build 5): words, the time and the date, and what the
/// keyboard knows about itself. The computer's readings, and bars of them, it can't.
pub fn onboard_can(kind: &Kind) -> bool {
    match kind {
        Kind::Text(_) | Kind::Clock(_) => true,
        Kind::Data(s, _) => matches!(s, Source::Date | Source::Profile | Source::ProfileNumber | Source::GameMode | Source::KeyRollover),
        Kind::Bar(_) => false,
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
}

impl Gaps {
    pub fn of(page: &Page) -> Gaps {
        let mut g = Gaps::default();
        for p in &page.pieces {
            match &p.kind {
                Kind::Bar(_) => g.bars = true,
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
        let what = match self.readings.len() {
            0 => String::new(),
            1 => self.readings[0].to_string(),
            n => format!("{} and {}", self.readings[..n - 1].join(", "), self.readings[n - 1]),
        };
        let parts = match (what.is_empty(), self.bars) {
            (true, false) => return None,
            (true, true) => "Bars".to_string(),
            (false, false) => format!("{what} readings"),
            (false, true) => format!("{what} readings and bars"),
        };
        // (a sentence starts with a capital: "Weather readings", "CPU readings")
        let mut c = parts.chars();
        let parts = c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect());
        Some(format!("{parts} need Ectodeck, so the keyboard leaves them blank."))
    }

    /// The page's note in the menu of onboard layouts, before it is chosen.
    pub fn tag(&self) -> &'static str {
        match (self.readings.is_empty(), self.bars) {
            (true, false) => "",
            (true, true) => "Bars stay blank",
            (false, false) => "Readings stay blank",
            (false, true) => "Readings and bars stay blank",
        }
    }
}

/// Where each piece lands, in the page's order: its row, the cell it starts
/// at and how many it takes (a long reading runs past its width and pushes
/// the rest of its row along). A row said to be unavailable keeps its
/// pieces where they were put, as words always are.
pub fn spans(page: &Page, r: &Readings) -> Vec<(u8, usize, usize)> {
    lay_out(page, r, 0.0).1
}

/// The rows, and where each piece lands, `t` seconds in. A row holds its one-row pieces,
/// left to right, a reading that runs long pushing the ones after it along; and the rows
/// of any taller piece of words passing through it, which keeps its cells (a reading
/// pushed up to one stops at its edge).
fn lay_out(page: &Page, r: &Readings, t: f64) -> (Vec<String>, Vec<(u8, usize, usize)>) {
    let mut said: Vec<&'static str> = Vec::new();
    let mut spans: Vec<(u8, usize, usize)> = page.pieces.iter().map(|p| (p.row, p.col as usize, p.width as usize)).collect();
    let tall = |p: &Piece| is_words(&p.kind) && p.height > 1;
    // the taller pieces' own rows, or what they read from when they can't be read
    let walls: Vec<(&Piece, Result<Vec<String>, &'static str>)> = page.pieces.iter().filter(|p| tall(p)).map(|p| (p, show(p, r).map(|s| word_rows(p, &s, t)))).collect();
    let rows = (0..ROWS)
        .map(|row| {
            let mut pieces: Vec<(usize, &Piece)> = page.pieces.iter().enumerate().filter(|(_, p)| p.row == row && !tall(p)).collect();
            pieces.sort_by_key(|(_, p)| p.col);
            let through: Vec<&(&Piece, Result<Vec<String>, &'static str>)> = walls.iter().filter(|(p, _)| p.row <= row && row < p.row + p.height).collect();
            let shown: Vec<Result<String, &'static str>> = pieces.iter().map(|(_, p)| show(p, r)).collect();
            // (a taller piece that can't be read says so on its top row)
            let failed: Vec<&'static str> = shown.iter().filter_map(|s| s.as_ref().err().copied()).chain(through.iter().filter(|(p, _)| p.row == row).filter_map(|(_, w)| w.as_ref().err().copied())).collect();
            let mut out = String::new();
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
                    out = format!("{label} unavailable");
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
                    out.push_str(&" ".repeat(start.saturating_sub(cursor)));
                    let len = text.chars().count();
                    spans[*i] = (row, start, len);
                    cursor = start + len;
                    out.push_str(&text);
                }
            }
            // the taller pieces' rows, over the cells they keep
            let mut cells: Vec<char> = out.chars().collect();
            for (p, w) in &through {
                if let Ok(rows) = w {
                    let a = p.col as usize;
                    let line: Vec<char> = rows[(row - p.row) as usize].chars().collect();
                    if cells.len() < a + line.len() {
                        cells.resize(a + line.len(), ' ');
                    }
                    cells[a..a + line.len()].copy_from_slice(&line);
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
            Source::CpuLoad | Source::Date | Source::Profile => true,
            Source::ProfileNumber => r.profile_number.is_some(),
            Source::GameMode => r.mode.is_some(),
            Source::KeyRollover => r.rollover.is_some(),
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock | Source::VideoMemory => r.gpu.is_some(),
            Source::Memory => r.ram.is_some(),
            Source::OutsideTemp | Source::Sky => weather,
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
        // one row, Scroll: still for a moment, then a cell to the left every quarter second
        let one = w20(block(0, 1, true));
        assert_eq!(render_at(&one, &r, 1.0)[0], "Raid at 9 tonight. B");
        assert_eq!(render_at(&one, &r, 1.2 + 0.25 * 5.0 + 0.01)[0], "at 9 tonight. Bring");
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
        // a reading is never more than one row
        let mut v = q.to_json();
        v["pieces"][0]["height"] = json!(3);
        assert_eq!(Page::from_json(&v).unwrap().pieces[0].height, 1);
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
        assert_eq!((Gaps::of(&clock).line(), Gaps::of(&clock).tag()), (None, ""));
        // the weather, and a bar: named as what they are
        assert_eq!(Gaps::of(&Page::weather()).line().unwrap(), "Weather readings need Ectodeck, so the keyboard leaves them blank.");
        let mut barred = Page::blank();
        barred.pieces.push(Piece { kind: Kind::Bar(Source::CpuLoad), row: 0, col: 0, width: 10, right: false, center: false, gap: 1, height: 1, scroll: true });
        assert_eq!(Gaps::of(&barred).line().unwrap(), "Bars need Ectodeck, so the keyboard leaves them blank.");
        assert_eq!(Gaps::of(&barred).tag(), "Bars stay blank");
        // what the keyboard knows of itself: until it can say them, its mode and rollover as it starts up
        let mut own = Page::blank();
        own.pieces.push(Piece { kind: Kind::Data(Source::GameMode, false), row: 0, col: 0, width: 9, right: false, center: false, gap: 1, height: 1, scroll: true });
        own.pieces.push(Piece { kind: Kind::Data(Source::KeyRollover, false), row: 1, col: 0, width: 5, right: false, center: false, gap: 1, height: 1, scroll: true });
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
            cpu_load: 15.0,
            cpu_ghz: Some(4.36875),
            ram: Some((25.1, 60.2)),
            gpu: Some((45, 12, 1837, 13.13, 31.84)),
            now: Some(Moment { hour: 21, minute: 5, weekday: 5, day: 9, month: 9 }),
            profile: Some("DEFAULT 1".into()),
            profile_number: Some(1),
            mode: None,
            rollover: None,
            outside: None,
        }
    }

    #[test]
    fn the_stats_page_keeps_its_place_through_json() {
        let p = Page::stats();
        assert_eq!(Page::from_json(&p.to_json()), Some(p.clone()));
        assert_eq!(pages_of(None), vec![p.clone()]);
        assert_eq!(pages_of(Some(&json!([p.to_json()]))), vec![p]);
        assert!(pages_of(Some(&json!([]))).is_empty(), "a Library emptied by its owner stays empty");
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
            kept_light: None,
            secs: SECS.2,
            comes_in: Transition::Wipe,
            pieces: vec![
                Piece { kind: Kind::Clock(false), row: 0, col: 0, width: 5, right: false, center: false, gap: 1, height: 1, scroll: true },
                Piece { kind: Kind::Data(Source::Date, false), row: 0, col: 9, width: 11, right: true, center: false, gap: 1, height: 1, scroll: true },
                Piece { kind: Kind::Bar(Source::CpuLoad), row: 1, col: 0, width: 12, right: false, center: false, gap: 1, height: 1, scroll: true },
                Piece { kind: Kind::Data(Source::CpuTemp, true), row: 2, col: 0, width: 4, right: false, center: false, gap: 1, height: 1, scroll: true },
                Piece { kind: Kind::Clock(true), row: 3, col: 0, width: 8, right: false, center: false, gap: 1, height: 1, scroll: true },
                Piece { kind: Kind::Data(Source::Profile, false), row: 3, col: 10, width: 10, right: true, center: false, gap: 1, height: 1, scroll: true },
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
        assert_eq!(render(&Page::clock(), &r), vec!["", "       21:05", "     Fri Oct 9", ""]);
        r.outside = Some((21.6, "Partly cloudy".into()));
        assert_eq!(render(&Page::weather(), &r), vec!["", "        22°", "   Partly cloudy", ""]);
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
        // a page in the Library keeps what it shows; its turns keep how it plays
        assert!(p.to_json().get("secs").is_none());
        // and a second Clock is told apart from the first
        let q = Page::template("clock", &[p]).unwrap();
        assert_eq!(q.id, "clock-2");
        assert!(Page::template("rain", &[]).is_none());
    }

    #[test]
    fn what_was_kept_before_becomes_default_for_every_profile() {
        // pages kept before the Library: each with its own time and way in
        let mut old = Page::stats().to_json();
        old["secs"] = json!(12);
        old["comes_in"] = json!("type");
        let s = Screens::of(Some(&json!([old, Page::clock().to_json()])), None, None, None);
        assert_eq!(s.library.len(), 2);
        assert_eq!((s.playlists.len(), s.playlists[0].name.as_str()), (1, "Default"));
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
}
