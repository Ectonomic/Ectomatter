//! Ectodeck's own Settings, as the web app had them (shell.js at cd9ea82):
//! each device, whether Ectodeck controls it, whether it may open it and
//! what else has it open; the weather every device shares, its place and its
//! units; the HDR interface; and how often the window draws a lighting effect.

use serde_json::{json, Value};

use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
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

/// The Sensors card: the sensors that read the processor, each with its reading now, the
/// processor's and the motherboard's names, its cores, the graphics cards with their
/// temperatures, and the choices.
#[derive(Clone)]
pub struct Sensors {
    pub cpu: Vec<(crate::sensors::CpuSensor, Option<f64>)>,
    pub cpu_name: String,
    pub board: Option<String>,
    pub cores: Option<u32>,
    pub cards: Vec<(crate::sensors::Card, Option<u32>)>,
    pub choice: crate::sensors::Choice,
}

impl Sensors {
    /// A sensor's name as the card and the editor say it: "Tctl, AMD Ryzen 9 9950X3D".
    pub fn name(&self, s: &crate::sensors::CpuSensor) -> String {
        format!("{}, {}", s.label, if s.own { self.cpu_name.clone() } else { self.board.clone().unwrap_or_else(|| "the motherboard".into()) })
    }

    /// The note under the CPU temperature: what the processor's own sensors are, or where
    /// the motherboard's reads.
    pub fn cpu_note(&self, chosen: &crate::sensors::CpuSensor) -> String {
        if !chosen.own {
            return format!("{} is the motherboard’s own reading, from its sensor chip ({}).", chosen.label, chosen.driver);
        }
        let dies: Vec<String> = self.cpu.iter().map(|(s, _)| s).filter(|s| s.own && s.label.starts_with("Tccd")).map(|s| s.label.clone()).collect();
        let count = ["one", "two", "three", "four", "five", "six", "seven", "eight"].get(dies.len().wrapping_sub(1)).copied().unwrap_or("its");
        match chosen.label.as_str() {
            "Tctl" if dies.is_empty() => "Tctl is the temperature the processor reports for cooling.".into(),
            "Tctl" if dies.len() == 1 => format!("Tctl is the temperature the processor reports for cooling. {} is its core die.", dies[0]),
            "Tctl" => format!("Tctl is the temperature the processor reports for cooling. {} are its {count} core dies.", crate::sensors::and_list(&dies)),
            l if l.starts_with("Tccd") => format!("{l} is one of the processor’s core dies. Tctl is the temperature it reports for cooling."),
            "Package id 0" => "Package id 0 is the temperature of the whole processor. Each Core is one of its cores.".into(),
            l if l.starts_with("Core") => format!("{l} is one of the processor’s cores. Package id 0 is the whole processor."),
            l => format!("{l} is read by the processor’s own driver ({}).", chosen.driver),
        }
    }
}

pub struct State<'a> {
    pub sensors: Option<Sensors>,
    pub status: Value,
    pub control: bool,
    pub settings: Value,
    pub report: Option<ectodeck_weather::Report>,
    pub locale: &'a str,
    pub top_peak: f64,
    pub reference: f64,
    pub place_text: &'a str,
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

pub fn page(s: &State) -> (Vec<Cell>, f32) {
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
    });

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
                    let clock = |secs: f32| format!("{:02}:{:02}", (secs / 3600.0).floor() as i32, ((secs % 3600.0) / 60.0).floor() as i32);
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

    // where the screen's readings come from, where there is more than one source
    if let Some(v) = &s.sensors {
        section(&mut cells, &mut y, "Sensors", |c| {
            let cx = X0 + PAD_X + CTRL;
            // a dropdown with what its choice reads now at its right, before its chevron
            let drop = |c: &mut Card, ctl: Ctl, words: &str, reading: Option<String>| {
                c.cells.push(Cell { text: words.into(), id: id(ctl), ..cell(kind::DROP, cx, c.y, 300.0, 34.0) });
                if let Some(r) = reading {
                    let w = text::width(400, 12.5, &r).ceil() + 2.0;
                    c.cells.push(Cell { color: slint::Color::from_rgb_u8(0xa1, 0xa1, 0xaa), ..text_cell(style::NOTE_IN, cx + 300.0 - 36.0 - w, c.y, w, 34.0, &r) });
                }
            };
            let none = |c: &mut Card| c.cells.push(Cell { text: "None found".into(), style: 2, ..cell(kind::FIELD, cx, c.y, 300.0, 34.0) });
            let deg = |t: f64| format!("{}°", t.round());
            c.label("CPU temperature");
            let mut ny = c.y + 34.0 + 7.0;
            match (v.choice.cpu_sensor(&v.cpu.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>()), &v.choice.cpu) {
                (Some(s), _) => {
                    let reading = v.cpu.iter().find(|(o, _)| o == s).and_then(|(_, t)| t.map(deg));
                    drop(c, Ctl::CpuSensor, &v.name(&s), reading);
                    c.note(&mut ny, &v.cpu_note(&s), 640.0);
                }
                (None, Some((_, label))) if !v.cpu.is_empty() => {
                    drop(c, Ctl::CpuSensor, &format!("{label}, not found"), None);
                    c.note(&mut ny, &format!("{label} is no longer on this computer, so screen layouts show the CPU temperature as unavailable until another sensor is chosen."), 640.0);
                }
                _ => {
                    none(c);
                    c.note(&mut ny, "Linux reports no temperature sensor for this processor, so screen layouts can’t show the CPU temperature.", 640.0);
                }
            }
            c.next(ny - c.y - 0.125);
            c.label("CPU clock");
            c.seg(c.y, Ctl::CpuClock, &[("average", "Average of all cores"), ("fastest", "Fastest core")], if v.choice.fastest { "fastest" } else { "average" });
            let mut ny = c.y + 36.0 + 7.0;
            match v.cores {
                Some(n) if n > 1 => c.note(&mut ny, &format!("Each of the processor’s {n} cores runs at its own clock."), 640.0),
                _ => c.note(&mut ny, "Each of the processor’s cores runs at its own clock.", 640.0),
            }
            c.next(ny - c.y - 0.125);
            c.label("Graphics card");
            let mut ny = c.y + 34.0 + 7.0;
            let cards: Vec<crate::sensors::Card> = v.cards.iter().map(|(k, _)| k.clone()).collect();
            match (v.choice.card(&cards), &v.choice.card) {
                (Some(k), _) => {
                    let temp = v.cards.iter().find(|(o, _)| o == k).and_then(|(_, t)| t.map(|t| format!("{t}°")));
                    drop(c, Ctl::GpuCard, &k.shown(), temp);
                    c.note(&mut ny, crate::sensors::reports(k), 640.0);
                }
                (None, Some((_, name))) if !cards.is_empty() => {
                    drop(c, Ctl::GpuCard, &format!("{name}, not found"), None);
                    c.note(&mut ny, &format!("The {name} is no longer in this computer, so screen layouts show GPU readings as unavailable until another card is chosen."), 640.0);
                }
                _ => {
                    none(c);
                    c.note(&mut ny, "Linux reports no graphics card Ectodeck can read, so screen layouts can’t show GPU readings.", 640.0);
                }
            }
            ny += 10.0;
            c.note(&mut ny, "Screen layouts show these readings. CPU load and memory in use come from Linux (/proc), and the weather from the Weather settings above.", 640.0);
            c.next(ny - c.y - 0.125);
        });
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

    (cells, y - 14.0 + 40.0)
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
