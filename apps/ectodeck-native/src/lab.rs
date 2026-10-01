//! The Lighting lab: a developer's window for tuning how the Z10's keys are
//! lit. Each part of the light (the legends, the deck's glow, the skirt's
//! foot, where the LED sits, the display) has its sliders, each in the units
//! that mean something, and a curve of the shape they give; the keyboard in
//! the main window takes every change at once. Save writes the tuning into
//! the Z10 core's own file, which Ectodeck reads when it runs from its source.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{json, Value};
use slint::{Model, ModelRc, VecModel};

use crate::front::WindowAsk;
use crate::lighting::{cell, kind, style, text_cell};
use crate::text;
use crate::{Cell, LabWindow, Plot};

#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Move,
    Minimise,
    Maximise,
    Close,
    Slide(i32, f32),
    Typed(i32, String),
    Revert,
    Save,
}

/// What a value is, which says how it reads.
#[derive(Clone, Copy, PartialEq)]
enum Unit {
    /// luminance, in nits
    Nits,
    /// a length in the art's units, read in millimetres too
    Length,
    /// a part of the whole, as a percentage
    Fraction,
}

/// One value of the core's "light": where it is, what it is called, its
/// slider's range and step. A brightness's range is 0 to Peak nits, whatever
/// `lo` and `hi` say, since nothing above Peak nits shows.
struct Param {
    path: (&'static str, &'static str),
    label: &'static str,
    lo: f64,
    hi: f64,
    step: f64,
    unit: Unit,
    note: &'static str,
}

const PARAMS: [Param; 18] = [
    Param { path: ("legend", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, note: "The legend at its brightest, nearest the LED, for a key lit fully, up to Peak nits (Settings › HDR)." },
    Param { path: ("legend", "depth"), label: "Spread", lo: 1.0, hi: 60.0, step: 0.5, unit: Unit::Length, note: "How deep under the legend the LED sits. Deeper lights a legend more evenly; shallower keeps the light to the part nearest the LED." },
    Param { path: ("legend", "floor"), label: "Far parts", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, note: "The least any part of a legend is lit, however far from the LED: light bouncing inside the cap." },
    Param { path: ("deck", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, note: "The glow on the deck at its brightest, just outside the skirt on the LED's side, for a key lit fully." },
    Param { path: ("deck", "height"), label: "Round the skirt", lo: 0.5, hi: 60.0, step: 0.5, unit: Unit::Length, note: "How evenly the light gets out round the skirt. Lower keeps it to the LED's side of the key; higher spreads it all round." },
    Param { path: ("deck", "floor"), label: "Far side", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, note: "The least the skirt's far side lets out, as a part of its LED side." },
    Param { path: ("deck", "reach"), label: "Reach", lo: 0.1, hi: 30.0, step: 0.1, unit: Unit::Length, note: "How far across the deck the glow goes before it falls to a third." },
    Param { path: ("rim", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, note: "The glow the foot of each cap's skirt catches from the deck, at its brightest." },
    Param { path: ("rim", "width"), label: "Height", lo: 0.05, hi: 10.0, step: 0.05, unit: Unit::Length, note: "How far up the skirt it reaches." },
    Param { path: ("led", ""), label: "North of centre", lo: -12.0, hi: 12.0, step: 0.25, unit: Unit::Length, note: "Where each key's LED sits under its cap, from the switch's centre toward the top of the key; below 0, toward the bottom, as on a keyboard whose LEDs face south." },
    Param { path: ("chassis", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, note: "At its brightest, on the chassis just past a well's edge." },
    Param { path: ("chassis", "reach"), label: "Reach", lo: 0.1, hi: 40.0, step: 0.1, unit: Unit::Length, note: "How far across the chassis it goes before it falls to a third." },
    Param { path: ("lcd", "nits"), label: "Lit dots", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, note: "The display's lit dots, as bright as its backlight shows them." },
    Param { path: ("lcd", "unlit"), label: "Unlit dots", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, note: "The dots that are off, which let a little of the backlight through, as a real display's do." },
    Param { path: ("lcd", "field"), label: "Background", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, note: "The ground between the dots." },
    Param { path: ("lcd", "dot"), label: "Dot size", lo: 0.05, hi: 1.0, step: 0.01, unit: Unit::Fraction, note: "How much of each cell a dot fills; the rest is the gap between dots." },
    Param { path: ("lcd", "glass"), label: "Glass sheen", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, note: "How strongly the glass over the display shows its reflection, as the art draws it at 100%." },
    Param { path: ("legend", "unlit"), label: "LED off", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, note: "How bright a legend looks with its LED off (the dimmer at 0%, or a key the lighting leaves dark), as a share of the grey the art prints it in. A backlit legend with no light behind it is dark." },
];

/// The page's sections: a title, what the part is, and its values.
const SECTIONS: [(&str, &str, &[usize]); 6] = [
    ("LED", "Where each key's light comes from.", &[9]),
    ("Legends", "The LED shines up through the legend: the brightest thing on a key.", &[0, 1, 2, 17]),
    ("Deck glow", "Light escaping under the cap's skirt onto the deck of the well.", &[3, 4, 5, 6]),
    ("Skirt's foot", "The deck's glow caught by the foot of each cap.", &[7, 8]),
    ("Chassis spill", "Light out of the wells onto the chassis round them, from the keys nearest each well's edge.", &[10, 11]),
    ("Display", "The display's backlit dots, and the glass over them.", &[12, 13, 14, 15, 16]),
];

/// the art's units to the millimetre
const UNITS_PER_MM: f64 = 1.38;

/// What is on the keyboard now, in nits.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Readout {
    /// the brightest legend, and how bright the lit keys' colour shows before
    /// the screen clips one of its channels
    pub legend: f64,
    pub reach: f64,
    pub deck: f64,
    pub chassis: f64,
    pub lcd: f64,
    /// Peak nits, from Settings › HDR, and the desktop's white
    pub peak: f64,
    pub white: f64,
}

pub struct LabPage {
    pub ui: LabWindow,
    asks: Rc<RefCell<Vec<Ask>>>,
    /// the light being tuned, and as the core's file last had it
    pub light: Value,
    saved: Value,
    /// counts each change, for the keyboard to take it
    pub rev: u64,
    readout: Readout,
    status: (String, i32),
    cells: Rc<VecModel<Cell>>,
    plots: Rc<VecModel<Plot>>,
}

fn get(light: &Value, (a, b): (&str, &str)) -> Option<f64> {
    if b.is_empty() { light[a].as_f64() } else { light[a][b].as_f64() }
}

fn set(light: &mut Value, (a, b): (&str, &str), v: f64) {
    let v = json!((v * 1000.0).round() / 1000.0);
    if b.is_empty() {
        light[a] = v;
    } else {
        if !light[a].is_object() {
            light[a] = json!({});
        }
        light[a][b] = v;
    }
}

/// How much of a small source's light falls on a surface `h` below it, at `r`
/// along it, as the keyboard's light is worked out.
fn falloff(r: f64, h: f64) -> f64 {
    let q = h * h / (h * h + r * r);
    q * q.sqrt()
}

impl LabPage {
    pub fn new(ui: LabWindow, mark: slint::Image, light: Value) -> LabPage {
        let asks = Rc::new(RefCell::new(Vec::new()));
        macro_rules! on {
            ($set:ident, |$($p:ident),*| $ask:expr) => {{
                let a = asks.clone();
                ui.$set(move |$($p),*| a.borrow_mut().push($ask));
            }};
            ($set:ident, $ask:expr) => {{
                let a = asks.clone();
                ui.$set(move || a.borrow_mut().push($ask));
            }};
        }
        on!(on_drag, Ask::Move);
        on!(on_minimise, Ask::Minimise);
        on!(on_maximise, Ask::Maximise);
        on!(on_close_window, Ask::Close);
        on!(on_slide, |id, v| Ask::Slide(id, v));
        on!(on_entered, |id, t| Ask::Typed(id, t.to_string()));
        on!(on_revert, Ask::Revert);
        on!(on_save, Ask::Save);
        ui.set_mark(mark);
        let cells = Rc::new(VecModel::default());
        let plots = Rc::new(VecModel::default());
        ui.set_cells(ModelRc::from(cells.clone()));
        ui.set_plots(ModelRc::from(plots.clone()));
        let mut page = LabPage { ui, asks, saved: light.clone(), light, rev: 0, readout: Readout::default(), status: (String::new(), 0), cells, plots };
        page.refresh();
        page
    }

    /// Do what the page asked; what only its window can do is handed back.
    pub fn act(&mut self) -> Vec<WindowAsk> {
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        let mut window = Vec::new();
        if asks.is_empty() {
            return window;
        }
        for ask in asks {
            match ask {
                Ask::Move => window.push(WindowAsk::Move),
                Ask::Minimise => window.push(WindowAsk::Minimise),
                Ask::Maximise => window.push(WindowAsk::Maximise),
                Ask::Close => window.push(WindowAsk::Close),
                Ask::Slide(id, f) => {
                    if let Some(p) = PARAMS.get(id.max(0) as usize) {
                        let (lo, hi) = self.range(id.max(0) as usize);
                        let v = lo + f.clamp(0.0, 1.0) as f64 * (hi - lo);
                        let v = ((v / p.step).round() * p.step).clamp(lo, hi);
                        if get(&self.light, p.path) != Some(v) {
                            set(&mut self.light, p.path, v);
                            self.rev += 1;
                            self.status = (String::new(), 0);
                        }
                    }
                }
                Ask::Typed(id, t) => {
                    if let Some(p) = PARAMS.get(id.max(0) as usize) {
                        // the number as typed, whatever follows it ("220 nits", "35%")
                        let num: String = t.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                        match num.parse::<f64>() {
                            Ok(v) => {
                                // a part is typed as a percentage; nothing else holds a typed value
                                // back but what cannot be (a negative brightness, a part past the whole)
                                let v = match p.unit {
                                    Unit::Fraction => (v / 100.0).clamp(p.lo.max(0.0), 1.0),
                                    Unit::Nits => v.max(0.0),
                                    Unit::Length if p.path.0 == "led" => v,
                                    Unit::Length => v.max(0.01),
                                };
                                if get(&self.light, p.path) != Some(v) {
                                    set(&mut self.light, p.path, v);
                                    self.rev += 1;
                                    self.status = (String::new(), 0);
                                }
                            }
                            Err(_) => self.status = (format!("“{}” isn't a number for {}.", t.trim(), p.label.to_lowercase()), 2),
                        }
                    }
                }
                Ask::Revert => {
                    self.light = self.saved.clone();
                    self.rev += 1;
                    self.status = ("Back to the Z10 core's saved tuning.".into(), 0);
                }
                Ask::Save => self.save(),
            }
        }
        self.refresh();
        window
    }

    /// What is on the keyboard now, from the main window.
    pub fn set_readout(&mut self, r: Readout) {
        if r != self.readout {
            self.readout = r;
            self.refresh();
        }
    }

    /// Write the tuning into the Z10 core's file, keeping the file as it is round it.
    fn save(&mut self) {
        let path = crate::core_device_path();
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| {
            let a = s.find(" \"light\": {").ok_or("the core's file has no light section")?;
            let b = s[a..].find("\n },\n").map(|i| a + i + "\n },\n".len()).ok_or("the core's light section has no end")?;
            let out = format!("{}{}{}", &s[..a], light_block(&self.light), &s[b..]);
            // only a file that still reads as the core's is written
            serde_json::from_str::<Value>(&out).map_err(|e| format!("the tuning would not read back: {e}"))?;
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, &out).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
        });
        self.status = match result {
            Ok(()) => {
                self.saved = self.light.clone();
                ("Saved into the Z10 core's device.json.".into(), 1)
            }
            Err(e) => (format!("Not saved: {e}."), 2),
        };
    }

    // ------------------------------------------------------------ the page

    pub fn refresh(&mut self) {
        let (cells, plots, h) = self.layout();
        crate::front::sync_cells(&self.cells, cells);
        if self.plots.row_count() == plots.len() {
            for (i, p) in plots.into_iter().enumerate() {
                if self.plots.row_data(i).as_ref() != Some(&p) {
                    self.plots.set_row_data(i, p);
                }
            }
        } else {
            self.plots.set_vec(plots);
        }
        self.ui.set_page_h(h);
        self.ui.set_status_text(self.status.0.as_str().into());
        self.ui.set_status_kind(self.status.1);
        let changed = self.light != self.saved;
        self.ui.set_can_save(changed);
        self.ui.set_can_revert(changed);
        if changed && self.status.0.is_empty() {
            self.ui.set_status_text("Changed, not saved.".into());
        }
    }

    /// A value's slider range: a brightness from 0 to Peak nits (Settings ›
    /// HDR), since nothing above it shows; the rest their own.
    fn range(&self, i: usize) -> (f64, f64) {
        let p = &PARAMS[i];
        if p.unit == Unit::Nits {
            (0.0, if self.readout.peak > 0.0 { self.readout.peak } else { 1000.0 })
        } else {
            (p.lo, p.hi)
        }
    }

    fn value(&self, i: usize) -> f64 {
        let p = &PARAMS[i];
        get(&self.light, p.path).unwrap_or(p.lo)
    }

    /// A value as its box shows it, and its unit after the box.
    fn reads(&self, i: usize) -> (String, String) {
        let p = &PARAMS[i];
        let v = self.value(i);
        let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        match p.unit {
            Unit::Nits => (trim(format!("{v:.1}")), "nits".into()),
            Unit::Length => (trim(format!("{v:.2}")), format!("({:.1} mm)", v / UNITS_PER_MM)),
            Unit::Fraction => (trim(format!("{:.1}", v * 100.0)), "%".into()),
        }
    }

    /// The page laid out: its pieces, its curves, and its height.
    fn layout(&self) -> (Vec<Cell>, Vec<Plot>, f32) {
        let width = self.ui.get_window_width().max(560.0);
        let x0 = 24.0;
        let w = width - 48.0;
        let pad = 20.0;
        let label_w = 132.0;
        let value_w = 150.0;
        let cx = x0 + pad + label_w;
        let slider_w = w - pad * 2.0 - label_w - value_w - 12.0;
        let note_w = w - pad * 2.0 - label_w;
        let mut cells = vec![text_cell(style::TITLE, x0, 22.0, w, 29.0, "Lighting lab")];
        let mut plots = Vec::new();
        let mut y = 22.0 + 29.0 + 6.0;
        for line in text::wrap(400, 12.5, "Tune how the Z10's keys are lit. The keyboard in Ectodeck's window takes each change at once; Save writes them into the Z10 core.", w) {
            cells.push(text_cell(style::NOTE, x0, y, w, 18.0, &line));
            y += 18.125;
        }

        // a card: its heading, what the part is, then what `fill` puts in it
        let card = |cells: &mut Vec<Cell>, y: &mut f32, title: &str, about: &str, fill: &mut dyn FnMut(&mut Vec<Cell>, &mut f32)| {
            *y += 22.0;
            cells.push(text_cell(style::HEAD, x0, *y, w, 18.0, title));
            *y += 18.0;
            let top = *y;
            let mut inner = Vec::new();
            let mut cy = top + 16.0;
            for line in text::wrap(400, 12.5, about, w - pad * 2.0) {
                inner.push(text_cell(style::NOTE, x0 + pad, cy, w - pad * 2.0, 18.0, &line));
                cy += 18.125;
            }
            cy += 8.0;
            fill(&mut inner, &mut cy);
            let h = cy - top + 6.0;
            cells.push(cell(kind::TILE, x0, top, w, h));
            cells.extend(inner);
            *y = top + h;
        };

        // what is on the keyboard now
        let r = self.readout;
        card(&mut cells, &mut y, "On the keyboard now", "Measured from the keys' colours and brightness as the keyboard shows them.", &mut |c, cy| {
            let row = |c: &mut Vec<Cell>, cy: &mut f32, label: &str, value: String, note: Option<String>| {
                c.push(text_cell(style::LABEL, x0 + pad, *cy, label_w + 60.0, 26.0, label));
                c.push(text_cell(style::VALUE_BRIGHT, cx + 60.0, *cy, w - pad * 2.0 - label_w - 60.0, 26.0, &value));
                *cy += 26.0;
                if let Some(n) = note {
                    for line in text::wrap(400, 12.5, &n, note_w - 60.0) {
                        c.push(text_cell(style::NOTE, cx + 60.0, *cy, note_w - 60.0, 18.0, &line));
                        *cy += 18.125;
                    }
                }
                *cy += 4.0;
            };
            // what holds the legends back, if anything: only Peak nits does; the
            // colour's clean limit is only told
            let note = if self.value(0) > r.peak + 0.5 {
                Some(format!("Held at Peak nits, {:.0} (Settings › HDR).", r.peak))
            } else if r.legend > r.reach + 0.5 {
                Some(format!("This colour shows cleanly up to {:.0} nits. Above that the screen clips its red, so the brightest parts turn yellower.", r.reach))
            } else {
                None
            };
            row(c, cy, "Brightest legend", format!("{:.0} nits", r.legend), note);
            row(c, cy, "Brightest deck glow", format!("{:.0} nits", r.deck), None);
            row(c, cy, "Brightest chassis spill", format!("{:.0} nits", r.chassis), None);
            row(c, cy, "Display's lit dots", format!("{:.0} nits", r.lcd), None);
            row(c, cy, "The desktop's white", format!("{:.0} nits", r.white), None);
        });

        for (title, about, values) in SECTIONS {
            card(&mut cells, &mut y, title, about, &mut |c, cy| {
                for &i in values {
                    let p = &PARAMS[i];
                    c.push(text_cell(style::LABEL, x0 + pad, *cy, label_w, 34.0, p.label));
                    let (lo, hi) = self.range(i);
                    let frac = ((self.value(i) - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
                    c.push(Cell { value: frac, id: i as i32, ..cell(kind::SLIDER, cx + 2.0, *cy + 9.0, slider_w, 16.0) });
                    let (number, unit) = self.reads(i);
                    c.push(Cell { text: number.into(), id: i as i32, ..cell(kind::NUMBER, cx + slider_w + 12.0, *cy + 4.0, 64.0, 26.0) });
                    c.push(text_cell(style::FACT, cx + slider_w + 12.0 + 64.0 + 7.0, *cy + 8.0, value_w - 71.0, 18.0, &unit));
                    *cy += 34.0;
                    for line in text::wrap(400, 12.5, p.note, note_w) {
                        c.push(text_cell(style::NOTE, cx, *cy, note_w, 18.0, &line));
                        *cy += 18.125;
                    }
                    *cy += 12.0;
                }
                // the curves this part's values give, under their captions
                let pw = w - pad * 2.0;
                if matches!(title, "Legends" | "Deck glow" | "Chassis spill") {
                    *cy += 26.0;
                }
                match title {
                    "Legends" => {
                        let (nits, depth, floor, led) = (self.value(0), self.value(1), self.value(2), self.value(9));
                        let far = 20.0;
                        let pts: Vec<(f64, f64)> = (0..=80).map(|k| {
                            let r = far * k as f64 / 80.0;
                            (r, nits * (floor + (1.0 - floor) * falloff(r, depth)))
                        }).collect();
                        // an upper legend's middle sits about 7.25 units above a 1u key's
                        // centre, a lower one's about 6 below it; the LED `led` above it
                        let (up, low) = ((7.25 - led).abs(), (6.0 + led).abs());
                        let marks = [(up, "upper legend"), (low, "lower legend")];
                        self.plot(c, &mut plots, x0 + pad, *cy, pw, 118.0, &pts, far, nits.max(1.0), &marks, "Across a key: brightness by distance from the LED", "LED", &format!("{:.0} mm", far / UNITS_PER_MM));
                        *cy += 118.0 + 30.0;
                    }
                    "Deck glow" => {
                        let (peak, height, floor, reach, led) = (self.value(3), self.value(4), self.value(5), self.value(6), self.value(9));
                        let half = (pw - 12.0) / 2.0;
                        // round a 1u key's skirt, from the top edge's middle to the bottom edge's
                        // the LED's own light round the skirt, as a part of what reaches its LED side
                        // (where the outline is nearest the LED, whichever side that is), and the far
                        // side never less than its share of that, as the keyboard works it out
                        let direct = |x: f64, y: f64| falloff(((x).powi(2) + (y + led).powi(2)).sqrt(), height);
                        let raw: Vec<(f64, f64)> = (0..=96).map(|k| {
                            let s = 48.0 * k as f64 / 96.0;
                            let (x, y) = if s < 12.0 { (s, -12.0) } else if s < 36.0 { (12.0, -12.0 + (s - 12.0)) } else { (12.0 - (s - 36.0), 12.0) };
                            (s, direct(x, y))
                        }).collect();
                        let most = raw.iter().map(|p| p.1).fold(0.0f64, f64::max).max(1e-12);
                        let pts: Vec<(f64, f64)> = raw.iter().map(|&(s, v)| (s, peak * (floor + (1.0 - floor) * v / most))).collect();
                        self.plot(c, &mut plots, x0 + pad, *cy, half, 118.0, &pts, 48.0, peak.max(0.1), &[(12.0, "corner"), (36.0, "corner")], "Round a key's skirt", "top", "bottom");
                        let far = 8.0;
                        let pts: Vec<(f64, f64)> = (0..=80).map(|k| {
                            let d = far * k as f64 / 80.0;
                            (d, peak * (-d / reach).exp())
                        }).collect();
                        // the gap between two keys, about 2 units
                        self.plot(c, &mut plots, x0 + pad + half + 12.0, *cy, half, 118.0, &pts, far, peak.max(0.1), &[(2.0, "next key")], "Out from the skirt", "skirt", &format!("{:.0} mm", far / UNITS_PER_MM));
                        *cy += 118.0 + 30.0;
                    }
                    "Chassis spill" => {
                        let (peak, reach) = (self.value(10), self.value(11));
                        let far = (reach * 4.0).max(4.0);
                        // from the key's skirt; the well's edge is about 2.5 units out
                        let pts: Vec<(f64, f64)> = (0..=80).map(|k| {
                            let d = far * k as f64 / 80.0;
                            (d, if d < 2.5 { 0.0 } else { peak * (-(d - 2.5) / reach).exp() })
                        }).collect();
                        self.plot(c, &mut plots, x0 + pad, *cy, pw, 96.0, &pts, far, peak.max(0.1), &[(2.5, "well's edge")], "Out from a key at a well's edge, across the chassis", "skirt", &format!("{:.0} mm", far / UNITS_PER_MM));
                        *cy += 96.0 + 30.0;
                    }
                    _ => {}
                }
            });
        }
        (cells, plots, y + 28.0)
    }

    /// A curve: its points (x from 0 to `xmax`, y from 0 to `ymax`), marks
    /// at some x, a caption over it, and both ends of its x axis named.
    #[allow(clippy::too_many_arguments)]
    fn plot(&self, c: &mut Vec<Cell>, plots: &mut Vec<Plot>, x: f32, y: f32, w: f32, h: f32, pts: &[(f64, f64)], xmax: f64, ymax: f64, marks: &[(f64, &str)], caption: &str, origin: &str, end: &str) {
        let (l, r, t, b) = (10.0f64, 10.0f64, 12.0f64, 8.0f64);
        let (fw, fh) = (w as f64, h as f64);
        let px = |v: f64| l + (v / xmax).clamp(0.0, 1.0) * (fw - l - r);
        let py = |v: f64| fh - b - (v / ymax).clamp(0.0, 1.05) * (fh - t - b);
        let mut line = String::new();
        for (i, (vx, vy)) in pts.iter().enumerate() {
            line += &format!("{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, px(*vx), py(*vy));
        }
        let area = format!("{line}L{:.1} {:.1}L{:.1} {:.1}Z", px(pts.last().map_or(0.0, |p| p.0)), fh - b, px(0.0), fh - b);
        let grid = format!("M{l:.1} {:.1}H{:.1}M{l:.1} {:.1}H{:.1}M{l:.1} {:.1}H{:.1}", py(0.0), fw - r, py(ymax * 0.5), fw - r, py(ymax), fw - r);
        let mut mk = String::from("M0 0");
        for (mx, _) in marks {
            if *mx <= xmax {
                mk += &format!("M{:.1} {t:.1}V{:.1}", px(*mx), fh - b);
            }
        }
        plots.push(Plot { x, y, w, h, grid: grid.into(), marks: mk.into(), area: area.into(), line: line.into() });
        // the caption over it, the axis's ends and the marks' names under it
        let top = format!("top {:.0} nits", ymax);
        let tw = text::width(400, 12.5, &top) + 2.0;
        c.push(text_cell(style::SMALL, x, y - 20.0, w - tw - 12.0, 17.0, caption));
        c.push(text_cell(style::SMALL, x + w - tw, y - 20.0, tw, 17.0, &top));
        let ow = text::width(400, 12.5, origin) + 2.0;
        c.push(text_cell(style::NOTE, x + l as f32 - 1.0, y + h + 4.0, ow, 18.0, origin));
        let ew = text::width(400, 12.5, end) + 2.0;
        c.push(text_cell(style::NOTE, x + w - r as f32 - ew, y + h + 4.0, ew, 18.0, end));
        // each mark's name under it, kept clear of the axis's ends and of each other
        let mut free = x + l as f32 + ow + 10.0;
        for (mx, name) in marks {
            if *mx <= xmax {
                let nw = text::width(400, 12.5, name) + 2.0;
                let at = (x + px(*mx) as f32 - nw / 2.0).max(free).min(x + w - r as f32 - ew - nw - 10.0);
                c.push(text_cell(style::NOTE, at, y + h + 4.0, nw, 18.0, name));
                free = at + nw + 10.0;
            }
        }
    }
}

/// The core's light section as its file has it: two spaces in, each part's
/// values on one line.
fn light_block(light: &Value) -> String {
    // as the file writes them: nits whole, other lengths and parts with a decimal point
    let num = |key: &str, v: &Value| {
        let f = (v.as_f64().unwrap_or(0.0) * 1000.0).round() / 1000.0;
        if key == "nits" {
            format!("{f:.0}")
        } else if f.fract() == 0.0 {
            format!("{f:.1}")
        } else {
            format!("{f}")
        }
    };
    let part = |name: &str, keys: &[&str]| {
        let body: Vec<String> = keys.iter().filter_map(|k| light[name].get(*k).map(|v| format!("\"{k}\": {}", num(k, v)))).collect();
        format!("  \"{name}\": {{ {} }},\n", body.join(", "))
    };
    let mut s = String::from(" \"light\": {\n");
    s += &format!("  \"about\": {},\n", serde_json::to_string(&light["about"]).unwrap_or_else(|_| "\"\"".into()));
    s += &format!("  \"led\": {},\n", num("led", &light["led"]));
    s += &part("legend", &["nits", "depth", "floor", "unlit"]);
    s += &part("deck", &["nits", "height", "floor", "reach"]);
    s += &part("rim", &["nits", "width"]);
    s += &part("chassis", &["nits", "reach"]);
    s += &part("lcd", &["nits", "unlit", "field", "dot", "glass"]);
    s += &format!("  \"wells\": {}\n", serde_json::to_string(&light["wells"]).unwrap_or_else(|_| "\"mask_1\"".into()));
    s += " },\n";
    s
}
