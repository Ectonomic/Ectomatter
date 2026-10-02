//! The Lighting lab: a developer's window for tuning how the Z10's keys are
//! lit. The keys' light is simulated (see ectodeck_draw's sim): each LED
//! followed through the keyboard's shape. The lab's sliders are the
//! keyboard's physical make, each in the units that mean something: where
//! the LED sits and how its light leaves the switch, the caps' heights and
//! how much light their plastic gives back or lets through, the wells' depth
//! and finish; and the light's brightness, which everything else follows.
//! The keyboard in the main window takes every change at once, its light
//! clearing as the simulation runs. Save writes the tuning into the Z10
//! core's own file, which Ectodeck reads when it runs from its source.

mod legends;

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{json, Value};
use slint::{Model, ModelRc, VecModel};

use crate::front::WindowAsk;
use crate::lighting::{cell, kind, style, text_cell};
use crate::text;
use crate::{Cell, LabWindow, Plot, Sketch};
use ectodeck_draw::hdr::sim::{on_cap_top, Physics};

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
    /// a height or a width, in millimetres
    Mm,
    /// how far something carries, in millimetres, on a scale of ratios like a length's
    Reach,
    /// a part of the whole, as a percentage
    Fraction,
    /// a plain number, as a beam's power
    Number,
}

/// One value of the core's "light": where it is, what it is called, its
/// slider's range and step. A brightness's range is 0 to Peak nits, whatever
/// `lo` and `hi` say, since nothing above Peak nits shows, and the LED's runs
/// to its switch's edges, since it sits inside the switch. A brightness and a
/// length keep three significant figures from their sliders, whatever the
/// step says, so a small value moves in small steps.
struct Param {
    path: (&'static str, &'static str),
    label: &'static str,
    lo: f64,
    hi: f64,
    step: f64,
    unit: Unit,
    /// what the keyboard takes when the core gives none
    def: f64,
    note: &'static str,
}

const PARAMS: [Param; 23] = [
    Param { path: ("led", ""), label: "North of centre", lo: -9.66, hi: 9.66, step: 0.25, unit: Unit::Length, def: 7.0, note: "Where each key's LED sits under its cap, from the switch's centre toward the top of the key; below 0, toward the bottom, as on a keyboard whose LEDs face south. It goes as far as the switch's edge, so Switch width sets how far the slider reaches." },
    Param { path: ("sim", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, def: 600.0, note: "The brightest legend, with every key lit white at full, up to Peak nits (Settings › HDR). Everything else the LEDs light is as bright as the simulation finds it beside that. A colour gives off its own share, as an LED does: a red about a fifth." },
    Param { path: ("sim", "beam"), label: "Beam", lo: 1.0, hi: 200.0, step: 0.5, unit: Unit::Number, def: 24.0, note: "How tight the beam out of the top of the switch is. Higher keeps it to a small bright spot on the legend over the LED; lower spreads it across the cap's top." },
    Param { path: ("sim", "scatter"), label: "Scattered", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.7, note: "The share of the LED's light the switch's clear housing scatters, glowing with it all over, rather than sending up in the beam. More lights the legend more evenly, and the deck and the inside of the cap more; less leaves more for the spot over the LED." },
    Param { path: ("sim", "sides"), label: "Through its sides", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.25, note: "The share of the housing's glow that leaves through its sides rather than its top. Its sides light the deck round the switch, out under the skirt; its top lights the back of the legend evenly, its parts far from the LED as well." },
    Param { path: ("sim", "glow_spread"), label: "Glow spread", lo: 0.5, hi: 60.0, step: 0.1, unit: Unit::Reach, def: 4.0, note: "How far the housing's glow carries from the LED before it falls to about a third, as the light fades through the plastic. Short keeps it to the LED's side of the switch, so the deck and the legend are brightest there; long glows evenly all round." },
    Param { path: ("sim", "switch_width"), label: "Switch width", lo: 1.0, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 12.0, note: "How wide the switch's clear housing is: it glows with the scattered light over its top and its sides. Wider spreads the glow across the back of the legend; its sides light the deck round the switch, out under the skirt." },
    Param { path: ("sim", "switch_top"), label: "Switch top", lo: 0.1, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 6.5, note: "How high above the deck the switch's housing stands, where the beam leaves it. Nearer the cap's top makes the spot on the legend smaller and brighter." },
    Param { path: ("sim", "skirt_foot"), label: "Skirt's foot", lo: 0.0, hi: 12.0, step: 0.1, unit: Unit::Mm, def: 5.0, note: "How high above the deck the foot of each cap's skirt is: the gap the light gets out under it by." },
    Param { path: ("sim", "cap_top"), label: "Cap's top", lo: 1.0, hi: 25.0, step: 0.1, unit: Unit::Mm, def: 14.0, note: "How high above the deck each cap's top is, where the legend is." },
    Param { path: ("sim", "legends"), label: "Legends let through", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.3, note: "The share of the light on the back of a cap's top that its legend lets through. With Brightness fixing the legends, less makes everything else brighter beside them." },
    Param { path: ("sim", "cap_in"), label: "Inside", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.5, note: "The share of the light the inside of a cap gives back. More of it bounces about inside the cap, and more gets out under the skirt onto the deck round it." },
    Param { path: ("sim", "cap_out"), label: "Outside", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.18, note: "The share of the light the outside of a cap's skirt gives back: what the skirts show of the light round them." },
    Param { path: ("legend", "unlit"), label: "LED off", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.15, note: "How bright a legend looks with its LED off (the dimmer at 0%, or a key the lighting leaves dark), as a share of the grey the art prints it in. A backlit legend with no light behind it is dark." },
    Param { path: ("sim", "depth"), label: "Depth", lo: 0.0, hi: 12.0, step: 0.1, unit: Unit::Mm, def: 2.0, note: "How deep the wells are: how high the chassis round them stands above the deck. Deeper keeps more of the light in the wells." },
    Param { path: ("sim", "deck"), label: "Deck", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.22, note: "The share of the light the deck at the wells' floor gives back." },
    Param { path: ("sim", "walls"), label: "Walls", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.25, note: "The share of the light the wells' walls give back, into the well." },
    Param { path: ("sim", "chassis"), label: "Chassis", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.4, note: "The share of the light the chassis's top round the wells gives back." },
    Param { path: ("lcd", "nits"), label: "Lit dots", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, def: 324.0, note: "The display's lit dots with its backlight white at full. A colour gives off its own share of that, as an LED does." },
    Param { path: ("lcd", "unlit"), label: "Unlit dots", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, def: 4.1, note: "The dots that are off, which let a little of the backlight through, as a real display's do." },
    Param { path: ("lcd", "field"), label: "Background", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, def: 0.8, note: "The ground between the dots." },
    Param { path: ("lcd", "dot"), label: "Dot size", lo: 0.05, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.84, note: "How much of each cell a dot fills; the rest is the gap between dots." },
    Param { path: ("lcd", "glass"), label: "Glass sheen", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 1.0, note: "How strongly the glass over the display shows its reflection, as the art draws it at 100%." },
];

/// The page's sections: a title, what the part is, and its values.
const SECTIONS: [(&str, &str, &[usize]); 4] = [
    ("LED", "Each key's LED shines up through its switch: part of its light leaves the switch's top in a beam, onto the back of the legend; the rest the switch's clear housing scatters, glowing with it over its top and sides, which lights the legend evenly, the inside of the cap and the deck round the switch.", &[0, 1, 2, 3, 4, 5, 7, 6]),
    ("Keycaps", "Each cap stands over its switch, its skirt from its foot to its top. Light bounces about inside it; some gets out under the skirt, and some through the legend.", &[8, 9, 10, 11, 12, 13]),
    ("Wells", "The keys sit in wells in the chassis: the deck at their floor, walled round by the chassis. What each gives back of the light falling on it lights the rest.", &[14, 15, 16, 17]),
    ("Display", "The display's backlit dots, and the glass over them.", &[18, 19, 20, 21, 22]),
];

/// the art's units to the millimetre
const UNITS_PER_MM: f64 = 1.38;

/// How a slider spreads its values along it.
#[derive(Clone, Copy, PartialEq)]
enum Scale {
    /// in equal steps of how bright the light looks (PQ, as the screen is
    /// sent it), so a dim value has as much room as a bright one
    Seen,
    /// in equal steps of ratio, each step the same share longer, so a short
    /// length has as much room as a long one
    Ratio,
    /// evenly
    Even,
}

impl Param {
    fn scale(&self) -> Scale {
        match self.unit {
            Unit::Nits => Scale::Seen,
            Unit::Length | Unit::Number | Unit::Reach if self.lo > 0.0 => Scale::Ratio,
            _ => Scale::Even,
        }
    }
}

/// PQ's signal for nits, as the screen is sent it, and the nits for a signal.
const PQ: (f64, f64, f64, f64, f64) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0, 3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);

fn pq(nits: f64) -> f64 {
    let (m1, m2, c1, c2, c3) = PQ;
    let y = (nits.max(0.0) / 10000.0).powf(m1);
    ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
}

fn pq_nits(signal: f64) -> f64 {
    let (m1, m2, c1, c2, c3) = PQ;
    let e = signal.clamp(0.0, 1.0).powf(1.0 / m2);
    10000.0 * ((e - c1).max(0.0) / (c2 - c3 * e)).powf(1.0 / m1)
}

/// A value to three significant figures.
fn sig3(v: f64) -> f64 {
    if v <= 0.0 {
        return 0.0;
    }
    let m = 10f64.powf(v.log10().floor() - 2.0);
    (v / m).round() * m
}

/// A value as its box shows it, and its unit after the box.
fn reads(p: &Param, v: f64) -> (String, String) {
    let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    match p.unit {
        // to three significant figures, as its slider keeps it
        Unit::Nits => (trim(if v >= 100.0 { format!("{v:.0}") } else if v >= 10.0 { format!("{v:.1}") } else if v >= 1.0 { format!("{v:.2}") } else { format!("{v:.3}") }), "nits".into()),
        Unit::Length => (trim(format!("{v:.2}")), format!("({:.1} mm)", v / UNITS_PER_MM)),
        Unit::Mm => (trim(format!("{v:.1}")), "mm".into()),
        Unit::Reach => (trim(format!("{v:.2}")), "mm".into()),
        Unit::Fraction => (trim(format!("{:.1}", v * 100.0)), "%".into()),
        Unit::Number => (trim(format!("{v:.2}")), String::new()),
    }
}

/// A value as a legend names it: its number and unit, as its box and the
/// unit after it read ("6.5 mm", "25%"), and a length's millimetres apart.
fn says(i: usize, v: f64) -> (String, String) {
    let (n, u) = reads(&PARAMS[i], v);
    match PARAMS[i].unit {
        Unit::Length => (n, u),
        Unit::Fraction => (format!("{n}{u}"), String::new()),
        Unit::Nits if n == "1" => ("1 nit".into(), String::new()),
        _ if u.is_empty() => (n, String::new()),
        _ => (format!("{n} {u}"), String::new()),
    }
}

/// What is on the keyboard now, in nits, and how far its light's simulation has got.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Readout {
    /// the brightest legend, and how bright the lit keys' colour shows before
    /// the screen clips one of its channels
    pub legend: f64,
    pub reach: f64,
    pub deck: f64,
    pub skirt: f64,
    pub chassis: f64,
    pub lcd: f64,
    /// Peak nits, from Settings › HDR, and the desktop's white
    pub peak: f64,
    pub white: f64,
    /// the photons the simulation has followed from each LED, and whether that is all it follows
    pub photons: usize,
    pub settled: bool,
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
    /// the sections' legends, and their words over them
    sketches: Rc<VecModel<Sketch>>,
    over: Rc<VecModel<Cell>>,
    /// each legend as last drawn, and what they were drawn from: the light,
    /// the window's width and its scale
    drawn: Vec<(legends::Drawing, slint::Image)>,
    drawn_of: Option<(Value, f32, f32)>,
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

/// The core's light as the simulation takes it, from a light the lab or the
/// core once had: its legends' brightness kept, the rest the simulation's.
fn simulated(mut light: Value) -> Value {
    if light["sim"].is_object() {
        return light;
    }
    let nits = light["legend"]["nits"].as_f64().unwrap_or(600.0);
    let mut sim = serde_json::Map::new();
    for p in PARAMS.iter().filter(|p| p.path.0 == "sim") {
        sim.insert(p.path.1.into(), json!(p.def));
    }
    sim.insert("nits".into(), json!(nits));
    light["sim"] = Value::Object(sim);
    if let Some(l) = light["legend"].as_object_mut() {
        l.retain(|k, _| k == "unlit");
    }
    if let Some(o) = light.as_object_mut() {
        for k in ["deck", "rim", "chassis", "under"] {
            o.remove(k);
        }
    }
    light
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
        let sketches = Rc::new(VecModel::default());
        let over = Rc::new(VecModel::default());
        ui.set_cells(ModelRc::from(cells.clone()));
        ui.set_plots(ModelRc::from(plots.clone()));
        ui.set_sketches(ModelRc::from(sketches.clone()));
        ui.set_over(ModelRc::from(over.clone()));
        // a core lit as it once was, before the light was simulated, is read
        // as the simulation takes it, and is saved so
        let light = simulated(light);
        let mut page = LabPage { ui, asks, saved: light.clone(), light, rev: 0, readout: Readout::default(), status: (String::new(), 0), cells, plots, sketches, over, drawn: Vec::new(), drawn_of: None };
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
                        let v = self.at_along(id.max(0) as usize, f as f64);
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
                                // back but what cannot be (a negative brightness, a part past the
                                // whole, a switch wider than fits inside a cap, an LED outside its switch)
                                let v = match p.unit {
                                    Unit::Fraction => (v / 100.0).clamp(p.lo.max(0.0), p.hi.min(1.0)),
                                    Unit::Mm if p.path.1 == "switch_width" => v.clamp(p.lo, p.hi),
                                    Unit::Nits | Unit::Mm => v.max(0.0),
                                    Unit::Reach => v.max(p.lo),
                                    Unit::Length if p.path.0 == "led" => {
                                        let (lo, hi) = self.range(id.max(0) as usize);
                                        v.clamp(lo, hi)
                                    }
                                    Unit::Length => v.max(0.01),
                                    Unit::Number => v.max(p.lo),
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
        self.draw_legends();
        let (cells, plots, (sketches, over), h) = self.layout();
        crate::front::sync_cells(&self.cells, cells);
        crate::front::sync_cells(&self.over, over);
        if self.sketches.row_count() == sketches.len() {
            for (i, s) in sketches.into_iter().enumerate() {
                if self.sketches.row_data(i).as_ref() != Some(&s) {
                    self.sketches.set_row_data(i, s);
                }
            }
        } else {
            self.sketches.set_vec(sketches);
        }
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

    /// Draw the sections' legends again, those whose drawing has changed,
    /// for the light as it is now, at the window's width and scale.
    fn draw_legends(&mut self) {
        let width = self.ui.get_window_width().max(560.0);
        let scale = slint::ComponentHandle::window(&self.ui).scale_factor();
        let of = (self.light.clone(), width, scale);
        if self.drawn_of.as_ref() == Some(&of) {
            return;
        }
        let v = legends::Values::of(&self.light);
        let w = width - 48.0 - 40.0;
        let fresh = [
            legends::slice(&v, legends::Lens::Led, w, legends::SLICE_H),
            legends::slice(&v, legends::Lens::Caps, w, legends::SLICE_H),
            legends::slice(&v, legends::Lens::Wells, w, legends::SLICE_H),
            legends::display(&v, w, legends::DISPLAY_H),
        ];
        let same_scale = self.drawn_of.as_ref().is_some_and(|o| o.2 == scale);
        for (i, d) in fresh.into_iter().enumerate() {
            if !(same_scale && self.drawn.get(i).is_some_and(|(old, _)| *old == d)) {
                let image = legends::image(&d, scale);
                if i < self.drawn.len() {
                    self.drawn[i] = (d, image);
                } else {
                    self.drawn.push((d, image));
                }
            }
        }
        self.drawn_of = Some(of);
    }

    /// A value's slider range: a brightness from 0 to Peak nits (Settings ›
    /// HDR), since nothing above it shows; the LED from one edge of its
    /// switch to the other, as wide as Switch width has it; the rest their own.
    fn range(&self, i: usize) -> (f64, f64) {
        let p = &PARAMS[i];
        if p.unit == Unit::Nits {
            (0.0, if self.readout.peak > 0.0 { self.readout.peak } else { 1000.0 })
        } else if p.path == ("led", "") {
            let edge = Physics::from(&self.light).switch_edge() as f64;
            (-edge, edge)
        } else {
            (p.lo, p.hi)
        }
    }

    /// Where a value sits along its slider, 0 to 1, as its scale spreads it.
    fn along(&self, i: usize, v: f64) -> f64 {
        let (lo, hi) = self.range(i);
        match PARAMS[i].scale() {
            Scale::Seen => (pq(v) - pq(lo)) / (pq(hi) - pq(lo)).max(1e-9),
            Scale::Ratio => (v.max(lo) / lo).ln() / (hi / lo).ln(),
            Scale::Even => (v - lo) / (hi - lo),
        }
        .clamp(0.0, 1.0)
    }

    /// The value at a place along its slider, 0 to 1: a brightness or a
    /// length to three significant figures, anything else to its step.
    fn at_along(&self, i: usize, f: f64) -> f64 {
        let (lo, hi) = self.range(i);
        let (p, f) = (&PARAMS[i], f.clamp(0.0, 1.0));
        match p.scale() {
            Scale::Seen => sig3(pq_nits(pq(lo) + f * (pq(hi) - pq(lo)))).clamp(lo, hi),
            Scale::Ratio => sig3(lo * (hi / lo).powf(f)).clamp(lo, hi),
            Scale::Even => (((lo + f * (hi - lo)) / p.step).round() * p.step).clamp(lo, hi),
        }
    }

    /// A value as the light has it; the LED's as the simulation holds it,
    /// inside its switch (a narrower switch pushes it in, and it comes back
    /// out as the switch widens again).
    fn value(&self, i: usize) -> f64 {
        let p = &PARAMS[i];
        let v = get(&self.light, p.path).unwrap_or(p.def);
        if p.path == ("led", "") {
            let (lo, hi) = self.range(i);
            v.clamp(lo, hi)
        } else {
            v
        }
    }

    /// A value as its box shows it, and its unit after the box.
    fn reads(&self, i: usize) -> (String, String) {
        reads(&PARAMS[i], self.value(i))
    }

    /// The page laid out: its pieces, its curves, its legends and their
    /// words, and its height.
    fn layout(&self) -> (Vec<Cell>, Vec<Plot>, (Vec<Sketch>, Vec<Cell>), f32) {
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
        let (mut sketches, mut over) = (Vec::new(), Vec::new());
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
        card(&mut cells, &mut y, "On the keyboard now", "Measured from the keys' colours and brightness as the keyboard shows them, every part as if each key were lit as the brightest is.", &mut |c, cy| {
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
            let note = if self.value(1) > r.peak + 0.5 {
                Some(format!("Held at Peak nits, {:.0} (Settings › HDR).", r.peak))
            } else if r.legend > r.reach + 0.5 {
                Some(format!("This colour shows cleanly up to {:.0} nits. Above that the screen clips its red, so the brightest parts turn yellower.", r.reach))
            } else {
                None
            };
            // the dimmer parts to a tenth of a nit, where a whole one hides them
            let nits = |v: f64| if v < 10.0 { format!("{v:.1} nits") } else { format!("{v:.0} nits") };
            row(c, cy, "Brightest legend", nits(r.legend), note);
            row(c, cy, "Brightest deck", nits(r.deck), None);
            row(c, cy, "Brightest skirt", nits(r.skirt), None);
            row(c, cy, "Brightest chassis", nits(r.chassis), None);
            row(c, cy, "Display's lit dots", nits(r.lcd), None);
            row(c, cy, "The desktop's white", nits(r.white), None);
            let sim = if r.photons == 0 {
                "Starting".to_string()
            } else if r.settled {
                format!("Settled: {} photons from each LED", text::thousands(r.photons as u64))
            } else {
                format!("Clearing: {} photons from each LED so far", text::thousands(r.photons as u64))
            };
            row(c, cy, "The simulation", sim, Some("The light straight from each LED is worked out exactly at once; the light that has bounced is followed photon by photon, and clears as more come in.".into()));
        });

        for (k, (title, about, values)) in SECTIONS.into_iter().enumerate() {
            card(&mut cells, &mut y, title, about, &mut |c, cy| {
                // the section's legend, the card's width, under what the part is;
                // on whole pixels, so its picture lands on the screen's
                if let Some((d, image)) = self.drawn.get(k) {
                    let (lx, ly) = (x0 + pad, cy.round());
                    sketches.push(Sketch { x: lx, y: ly, w: d.w, h: d.h, image: image.clone() });
                    over.extend(d.words.iter().map(|word| word_cell(lx, ly, word)));
                    *cy = ly + d.h + 12.0;
                }
                for &i in values {
                    let p = &PARAMS[i];
                    c.push(text_cell(style::LABEL, x0 + pad, *cy, label_w, 34.0, p.label));
                    let frac = self.along(i, self.value(i)) as f32;
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
                // The LED's own light across the back of a cap's top, which its
                // legend shows: the beam and the scattered light, as a part of
                // the brightest legend, before the light bounced inside the cap.
                if title == "LED" {
                    let pw = w - pad * 2.0;
                    *cy += 26.0;
                    let (north, nits) = (self.value(0), self.value(1));
                    let physics = Physics::from(&self.light);
                    let at = |r: f64| on_cap_top(&physics, north as f32, r as f32) as f64;
                    let most = at(0.0).max(1e-12);
                    let far = 20.0;
                    let pts: Vec<(f64, f64)> = (0..=96).map(|k| {
                        let r = far * k as f64 / 96.0;
                        (r, nits * at(r) / most)
                    }).collect();
                    // an upper legend's middle sits about 7.25 units above a 1u key's
                    // centre, a lower one's about 6 below it; the LED `north` above it
                    let (up, low) = ((7.25 - north).abs(), (6.0 + north).abs());
                    self.plot(c, &mut plots, x0 + pad, *cy, pw, 118.0, &pts, far, nits.max(1.0), &[(up, "upper legend"), (low, "lower legend")], "Down a cap's top from over its LED: the LED's own light, before what bounces inside the cap", "LED", &format!("{:.0} mm", far / UNITS_PER_MM));
                    *cy += 118.0 + 30.0;
                }
            });
        }
        (cells, plots, (sketches, over), y + 28.0)
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

/// A legend's word as the lab's text, its baseline where the legend has it,
/// the legend at `x`, `y`.
fn word_cell(x: f32, y: f32, word: &legends::Word) -> Cell {
    let (weight, size) = word.look.face();
    let style = match word.look {
        legends::Look::Name => style::SMALL,
        legends::Look::Value => style::LEGEND_VALUE,
        legends::Look::Extra => style::NOTE,
        legends::Look::Axis => style::LEGEND_AXIS,
    };
    // Geist's line is 1.3 times its size, its baseline 1.005 of its size
    // down it, and a word is centred down its box
    let h = 18.0;
    let top = y + word.y - 1.005 * size - (h - 1.3 * size) / 2.0;
    text_cell(style, x + word.x, top, text::width(weight, size, &word.text) + 2.0, h, &word.text)
}

/// The core's light section as its file has it: two spaces in, each part's
/// values on one line.
fn light_block(light: &Value) -> String {
    // as the file writes them: nits whole, other lengths and parts with a decimal point
    let num = |key: &str, v: &Value| {
        if let Some(t) = v.as_str() {
            return serde_json::to_string(t).unwrap_or_default();
        }
        let f = (v.as_f64().unwrap_or(0.0) * 1000.0).round() / 1000.0;
        if key == "nits" && f.fract() == 0.0 {
            format!("{f:.0}")
        } else if key == "nits" {
            format!("{}", (f * 10.0).round() / 10.0)
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
    s += &part("sim", &["nits", "beam", "scatter", "sides", "glow_spread", "switch_top", "switch_width", "skirt_foot", "cap_top", "legends", "cap_in", "cap_out", "depth", "deck", "walls", "chassis"]);
    s += &part("legend", &["unlit"]);
    s += &part("lcd", &["nits", "unlit", "field", "dot", "glass"]);
    s += &format!("  \"wells\": {}\n", serde_json::to_string(&light["wells"]).unwrap_or_else(|_| "\"mask_1\"".into()));
    s += " },\n";
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The light section Save writes reads back as the light it was given,
    /// in the core's own file, and a core lit as it once was is read as the
    /// simulation takes it.
    #[test]
    fn the_light_saves_as_it_is() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10/device.json")).unwrap();
        let core: Value = serde_json::from_str(&text).unwrap();
        let light = simulated(core["light"].clone());
        let a = text.find(" \"light\": {").unwrap();
        let b = text[a..].find("\n },\n").map(|i| a + i + "\n },\n".len()).unwrap();
        let out = format!("{}{}{}", &text[..a], light_block(&light), &text[b..]);
        let back: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(back["light"], light);
        for p in PARAMS.iter().filter(|p| p.path.0 == "sim") {
            assert!(back["light"]["sim"][p.path.1].is_number(), "{} is saved", p.path.1);
        }
        let old = json!({ "led": 7.0, "legend": { "nits": 705, "depth": 10.0, "floor": 0.35, "unlit": 0.15 }, "deck": { "nits": 58 }, "rim": { "nits": 42 }, "chassis": { "nits": 19 }, "lcd": { "nits": 324 }, "wells": "mask_1" });
        let new = simulated(old);
        assert_eq!(new["sim"]["nits"], json!(705.0));
        assert_eq!(new["legend"], json!({ "unlit": 0.15 }));
        assert!(new["deck"].is_null() && new["rim"].is_null() && new["chassis"].is_null());
    }
}
