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

    /// What it is a reading of, as a card's line on a page says it.
    pub fn phrase(self) -> &'static str {
        match self {
            Source::CpuTemp | Source::CpuLoad | Source::CpuClock => "CPU",
            Source::GpuTemp | Source::GpuLoad | Source::GpuClock => "GPU",
            Source::Memory => "RAM",
            Source::VideoMemory => "VRAM",
            Source::Date => "the date",
            Source::Profile => "the active profile",
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
    /// set in the middle of its width (before `right`)
    pub center: bool,
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
    pub pieces: Vec<Piece>,
    /// as played: how long it shows before the next page, and how it comes
    /// onto the screen after the page before it (each turn of the playlist
    /// says, see [Entry]; a page in the Library alone keeps a new page's)
    pub secs: u32,
    pub comes_in: Transition,
}

fn words(row: u8, col: u8, t: &str) -> Piece {
    Piece { kind: Kind::Text(t.into()), row, col, width: t.chars().count() as u8, right: false, center: false, gap: 1 }
}

fn reading(row: u8, col: u8, width: u8, s: Source, gap: u8) -> Piece {
    Piece { kind: Kind::Data(s, false), row, col, width, right: true, center: false, gap }
}

/// A piece across the whole of a row, in its middle.
fn middle(row: u8, kind: Kind) -> Piece {
    Piece { kind, row, col: 0, width: COLS, right: false, center: true, gap: 1 }
}

/// The pages that can be added to the playlist: what each is called, and
/// what it shows.
pub const TEMPLATES: [(&str, &str, &str); 4] = [
    ("stats", "Stats", "CPU, GPU, RAM and VRAM, every second."),
    ("clock", "Clock", "The time and the date, in the middle of the screen."),
    ("weather", "Weather", "The temperature and the sky outside, for the place in Ectodeck’s Settings."),
    ("blank", "Blank", "An empty screen layout for your own text, readings and clocks."),
];

impl Page {
    /// The Stats page as Grant laid it out for the z10-lcd service.
    pub fn stats() -> Page {
        Page {
            id: "stats".into(),
            name: "Stats".into(),
            light: None,
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
        Page { id: "custom".into(), name: "Custom".into(), light: None, secs: SECS.2, comes_in: Transition::Wipe, pieces: Vec::new() }
    }

    /// The time on the second row and the date on the third, in the middle.
    pub fn clock() -> Page {
        Page {
            id: "clock".into(),
            name: "Clock".into(),
            light: None,
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
            secs: SECS.2,
            comes_in: Transition::Wipe,
            pieces: vec![middle(1, Kind::Data(Source::OutsideTemp, false)), middle(2, Kind::Data(Source::Sky, false))],
        }
    }

    /// A page of one of the TEMPLATES, by its key, with an id no page in
    /// `taken` has (a second Clock is clock-2).
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
                    center: p["center"].as_bool().unwrap_or(false),
                    gap: b(&p["gap"]).unwrap_or(1).min(1),
                })
            })
            .collect();
        let light = v["light"].as_array().filter(|a| a.len() == 3).map(|a| std::array::from_fn(|i| b(&a[i]).unwrap_or(0)));
        let secs = v["secs"].as_u64().map_or(SECS.2, |n| (n as u32).clamp(SECS.0, SECS.1));
        let comes_in = v["comes_in"].as_str().and_then(Transition::from_key).unwrap_or(Transition::Wipe);
        Some(Page { id: v["id"].as_str()?.into(), name: v["name"].as_str().unwrap_or("Page").into(), light, pieces, secs, comes_in })
    }
}

/// The pages kept in Ectodeck's settings, or Stats as it ships.
pub fn pages_of(v: Option<&Value>) -> Vec<Page> {
    let pages: Vec<Page> = v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(Page::from_json).collect()).unwrap_or_default();
    if pages.is_empty() { vec![Page::stats()] } else { pages }
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
                        let turns = turns_of(v.get("turns"));
                        let id = v["id"].as_str()?;
                        (!turns.is_empty()).then(|| Playlist { id: id.into(), name: v["name"].as_str().unwrap_or("Playlist").into(), turns })
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
        Screens { library, playlists: lists, plays }
    }

    /// All of it, as one value: for the settings, and for undoing a change.
    pub fn to_json(&self) -> Value {
        let plays: serde_json::Map<String, Value> = self.plays.iter().map(|(n, id)| (n.to_string(), json!(id))).collect();
        json!({
            "pages": self.library.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
            "playlists": self.playlists.iter().map(|l| l.to_json()).collect::<Vec<_>>(),
            "plays": plays,
        })
    }

    pub fn from_json(v: &Value) -> Screens {
        Screens::of(v.get("pages"), v.get("playlists"), v.get("plays"), v.get("playlist"))
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

    /// Turn `i` taken off the playlist, never the last.
    pub fn remove_turn(&mut self, p: u8, i: usize) -> bool {
        let turns = self.turns_mut(p);
        if turns.len() > 1 && i < turns.len() {
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
                let text = if p.center { format!("{s:^w$}") } else if p.right { format!("{s:>w$}") } else { format!("{s:<w$}") };
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

    #[test]
    fn a_blank_page_starts_empty_and_each_one_is_named_apart() {
        let one = Page::template("blank", &[]).unwrap();
        assert!(one.pieces.is_empty());
        assert_eq!((one.id.as_str(), one.name.as_str()), ("custom", "Custom"));
        let two = Page::template("blank", &[one.clone()]).unwrap();
        assert_eq!((two.id.as_str(), two.name.as_str()), ("custom-2", "Custom 2"));
        assert_eq!(TEMPLATES[TEMPLATES.len() - 1].0, "blank", "Blank last in Add a page");
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
            secs: SECS.2,
            comes_in: Transition::Wipe,
            pieces: vec![
                Piece { kind: Kind::Clock(false), row: 0, col: 0, width: 5, right: false, center: false, gap: 1 },
                Piece { kind: Kind::Data(Source::Date, false), row: 0, col: 9, width: 11, right: true, center: false, gap: 1 },
                Piece { kind: Kind::Bar(Source::CpuLoad), row: 1, col: 0, width: 12, right: false, center: false, gap: 1 },
                Piece { kind: Kind::Data(Source::CpuTemp, true), row: 2, col: 0, width: 4, right: false, center: false, gap: 1 },
                Piece { kind: Kind::Clock(true), row: 3, col: 0, width: 8, right: false, center: false, gap: 1 },
                Piece { kind: Kind::Data(Source::Profile, false), row: 3, col: 10, width: 10, right: true, center: false, gap: 1 },
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
        assert!(!s.remove_turn(1, 0), "never the last turn");
        assert_eq!(s.turns(1).len(), 1);
    }
}
