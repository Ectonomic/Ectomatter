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
//!
//! The lab is landscape and shows one section at a time: its drawing of the
//! key in the middle (see drawing), its sliders round it, each beside the
//! part it changes and joined to it by a line. The line under the drawing
//! says what the slider under the pointer does; what is on the keyboard now
//! is read out by Save.

mod drawing;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::{Model, ModelRc, VecModel};

use crate::front::WindowAsk;
use crate::lighting::{cell, kind, style, text_cell};
use crate::text;
use crate::{Cell, LabRow, LabWindow, Sketch};
use ectodeck_draw::hdr::sim::{Caps, Physics, Switch};

#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Move,
    Minimise,
    Maximise,
    Close,
    Slide(i32, f32),
    Typed(i32, String),
    /// one of a section's choices, and the option chosen
    Choose(i32, i32),
    /// Hold to compare held down, or let go
    Compare(bool),
    /// a slider back to its saved value
    Reset(i32),
    Revert,
    Save,
    /// a section chosen
    Section(i32),
    /// a slider or a reading under the pointer, or no longer
    Hover(i32, bool),
    /// a slider's box typed in, or no longer
    Focus(i32, bool),
    /// the pointer has rested where it is
    Rest,
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

const PARAMS: [Param; 26] = [
    Param { path: ("led", ""), label: "North of centre", lo: -9.66, hi: 9.66, step: 0.25, unit: Unit::Length, def: 7.0, note: "Where each key's LED sits under its cap, from the switch's centre toward the top of the key; below 0, toward the bottom, as on a keyboard whose LEDs face south. It goes as far as the switch's edge, so Switch width sets how far the slider reaches." },
    Param { path: ("sim", "nits"), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, def: 600.0, note: "The brightest legend, with every key lit white at full, up to Peak nits (Settings › HDR). Everything else the LEDs light is as bright as the simulation finds it beside that. A colour gives off its own share, as an LED does: a red about a fifth." },
    Param { path: ("sim", "beam"), label: "Beam", lo: 1.0, hi: 200.0, step: 0.5, unit: Unit::Number, def: 24.0, note: "How tight the LED's beam out of the top of the switch is. Higher keeps it to a small bright spot on the legend over the LED; lower spreads it across the cap's top and onto the inside of its skirt." },
    Param { path: ("sim", "scatter"), label: "Scattered", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.7, note: "The share of the LED's light a clear housing scatters, glowing with it all over, rather than sending up in the beam. More lights the legend more evenly, and the deck and the inside of the cap more; less leaves more for the spot over the LED." },
    Param { path: ("sim", "sides"), label: "Through its sides", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.25, note: "The share of the housing's glow that leaves through its sides rather than its top. Its sides light the deck round the switch, out under the skirt; its top lights the back of the legend evenly, its parts far from the LED as well." },
    Param { path: ("sim", "glow_spread"), label: "Glow spread", lo: 0.5, hi: 60.0, step: 0.1, unit: Unit::Reach, def: 4.0, note: "How far the housing's glow carries from the LED before it falls to about a third, as the light fades through the plastic. Short keeps it to the LED's side of the switch, so the deck and the legend are brightest there; long glows evenly all round." },
    Param { path: ("sim", "switch_width"), label: "Switch width", lo: 1.0, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 12.0, note: "How wide the switch's housing is. A black one stands in the way of the light bouncing about inside the cap; a clear one glows with the scattered light over its top and its sides, spreading it across the back of the legend and, out under the skirt, onto the deck round the switch." },
    Param { path: ("sim", "switch_top"), label: "Switch top", lo: 0.1, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 6.5, note: "How high above the deck the switch's housing stands, where the beam leaves it: 5.7 mm on Cherry-style switches like the Z10's Kailhs. Nearer the cap's top makes the spot on the legend smaller and brighter." },
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
    Param { path: ("sim", "skirts"), label: "Plastic lets through", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "The share of the light on the inside of a cap that its translucent plastic lets through, glowing on its outside: its sides below the black on pudding caps, all of it but the legends on translucent ones. Plastic with no coating lets through about what a legend does." },
    Param { path: ("sim", "black"), label: "Black down the sides", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "How far down a cap's sides the black of its top comes, as a share of the skirt's height. A pudding cap's black comes part of the way, its milky plastic below; light gets out only below the black." },
    Param { path: ("sim", "milky"), label: "Milky", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "How milky the translucent plastic is. Milky plastic carries the light it takes some way through itself before it leaves, so it glows evenly round the cap, and looks white with its LED off; clear plastic lets the light out where it went in." },
];

/// One of a section's choices, at the top of its left column, which says
/// what the keyboard is as a whole: what it is called, what it does, and its options.
struct Choice {
    label: &'static str,
    note: &'static str,
    options: &'static [&'static str],
}

const CHOICES: [Choice; 2] = [
    Choice { label: "Switch", note: "What the switch's housing is: black with its LED through a slot in its top, as older switches like the Z10's Kailh Browns have it, so all the LED's light leaves in its beam; or clear, glowing with a share of it, over its top as most RGB switches have it, or all over. Each sets the switch's values for that kind; the sliders tune from there.", options: &["LED slot", "Clear top", "Clear housing"] },
    Choice { label: "Keycaps", note: "What the caps let light through: shine-through caps, as the Z10 came with, are opaque but for their legends; pudding caps have translucent sides below a black top whose black comes part of the way down them; translucent caps let light through all over. Each fills in Plastic lets through and Black down the sides where they are off; the sliders tune from there.", options: &["Shine-through", "Pudding", "Translucent"] },
];

/// The page's sections: a title, what the part is, its values, and its choices.
const SECTIONS: [(&str, &str, &[usize], &[usize]); 4] = [
    ("LED", "Each key's LED shines up out of its switch in a beam, onto the back of the legend. In a black housing that is all its light; a clear housing scatters a share of it instead, glowing with it over its top and sides, which lights the legend evenly, the inside of the cap and the deck round the switch.", &[0, 1, 2, 3, 4, 5, 7, 6], &[0]),
    ("Keycaps", "Each cap stands over its switch, its skirt from its foot to its top. Light bounces about inside it; some gets out under the skirt, some through the legend, and some through the plastic where it is translucent and not coated black.", &[8, 9, 10, 23, 24, 25, 11, 12, 13], &[1]),
    ("Wells", "The keys sit in wells in the chassis: the deck at their floor, walled round by the chassis. What each gives back of the light falling on it lights the rest.", &[14, 15, 16, 17], &[]),
    ("Display", "The display's backlit dots, and the glass over them.", &[18, 19, 20, 21, 22], &[]),
];

/// A section's sliders that tune this light: a black housing does not
/// glow, so its glow's sliders are not shown with it.
pub(crate) fn sliders_of(section: usize, light: &Value) -> Vec<usize> {
    let slot = Physics::from(light).switch == Switch::Slot;
    // shine-through caps show their plastic only through their legends; translucent ones have no black
    let caps = Physics::from(light).caps;
    SECTIONS[section].2.iter().copied().filter(|&i| {
        let k = PARAMS[i].path.1;
        !(slot && matches!(k, "scatter" | "sides" | "glow_spread")) && !(caps == Caps::ShineThrough && matches!(k, "skirts" | "milky" | "black")) && !(caps == Caps::Translucent && k == "black")
    }).collect()
}

/// The option of a choice the light has.
fn chosen(choice: usize, light: &Value) -> usize {
    let p = Physics::from(light);
    match choice {
        0 => match p.switch {
            Switch::Slot => 0,
            Switch::ClearTop => 1,
            Switch::Clear => 2,
        },
        _ => match p.caps {
            Caps::ShineThrough => 0,
            Caps::Pudding => 1,
            Caps::Translucent => 2,
        },
    }
}

/// The light with an option of a choice taken, and the values it sets for it.
fn choose(light: &mut Value, choice: usize, option: usize) {
    match choice {
        0 => {
            let switch = [Switch::Slot, Switch::ClearTop, Switch::Clear][option.min(2)];
            light["sim"]["switch"] = json!(switch.name());
            // a Cherry-style housing's top, and a clear one's glow as such switches have it
            set(light, ("sim", "switch_top"), 5.7);
            match switch {
                // all the LED's light in its beam: as wide as a diffused LED's, to light the whole cap
                Switch::Slot => set(light, ("sim", "beam"), 3.0),
                Switch::ClearTop => {
                    set(light, ("sim", "beam"), 24.0);
                    set(light, ("sim", "scatter"), 0.7);
                    set(light, ("sim", "sides"), 0.25);
                    set(light, ("sim", "glow_spread"), 4.0);
                }
                Switch::Clear => {
                    set(light, ("sim", "beam"), 24.0);
                    set(light, ("sim", "scatter"), 0.85);
                    set(light, ("sim", "sides"), 0.5);
                    set(light, ("sim", "glow_spread"), 6.0);
                }
            }
        }
        _ => {
            // The caps' kind, and what it needs where it is off: plastic that
            // lets light through (about what a legend does, uncoated), and for
            // pudding caps black some way down the sides. Values already tuned
            // stay, so going from one kind to another and back keeps them.
            let caps = [Caps::ShineThrough, Caps::Pudding, Caps::Translucent][option.min(2)];
            light["sim"]["caps"] = json!(caps.name());
            let plastic = get(light, ("sim", "legends")).unwrap_or(0.3);
            if caps != Caps::ShineThrough && get(light, ("sim", "skirts")).unwrap_or(0.0) <= 0.0 {
                set(light, ("sim", "skirts"), plastic);
            }
            let black = get(light, ("sim", "black")).unwrap_or(0.0);
            if caps == Caps::Pudding && !(black > 0.0 && black < 1.0) {
                set(light, ("sim", "black"), 0.4);
            }
        }
    }
}

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
        // the LED's place in millimetres from the centre, and which way
        Unit::Length if p.path == ("led", "") => (trim(format!("{:.1}", v.abs() / UNITS_PER_MM)), if v.abs() < 0.005 { "centred".into() } else if v > 0.0 { "mm north".into() } else { "mm south".into() }),
        Unit::Length => (trim(format!("{v:.2}")), format!("({:.1} mm)", v / UNITS_PER_MM)),
        Unit::Mm => (trim(format!("{v:.1}")), "mm".into()),
        Unit::Reach => (trim(format!("{v:.2}")), "mm".into()),
        Unit::Fraction => (trim(format!("{:.1}", v * 100.0)), "%".into()),
        Unit::Number => (trim(format!("{v:.2}")), String::new()),
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

/// What the lab says of the page, under its title.
const INTRO: &str = "Tune how the Z10's keys are lit. The keyboard in Ectodeck's window takes each change at once; Save writes them into the Z10 core.";
/// What the readout's numbers measure, and the simulation's.
const READOUT_ABOUT: &str = "Measured from the keys' colours and brightness as the keyboard shows them, every part as if each key were lit as the brightest is.";
const WHITE_ABOUT: &str = "How bright this screen shows the desktop's white now; the keyboard's light is drawn beside it.";
const SIM_ABOUT: &str = "The light straight from each LED is worked out exactly at once; the light that has bounced is followed photon by photon, and clears as more come in.";
/// the readout's numbers' ids, after the sliders', the choices', and the sections' tabs'
const READING: i32 = 100;
const CHOICE: i32 = 800;
const TABS: i32 = 900;
/// how long the pointer rests on something before the line under the drawing tells of it
const REST: Duration = Duration::from_millis(250);
/// the page's rows below the title bar: its head, the stage after a gap, the
/// line under the stage, and the foot's 64
const HEAD_H: f32 = 70.0;
const HELP_H: f32 = 44.0;

pub struct LabPage {
    pub ui: LabWindow,
    asks: Rc<RefCell<Vec<Ask>>>,
    /// the light being tuned, and as the core's file last had it
    pub light: Value,
    saved: Value,
    /// counts each change, for the keyboard to take it
    pub rev: u64,
    /// Hold to compare held: the keyboard shows the saved light
    pub comparing: bool,
    /// the changes, for undoing them and doing them again
    history: History,
    readout: Readout,
    status: (String, i32),
    /// the section shown
    section: usize,
    /// what is under the pointer and since when, which box is being typed
    /// in, and what the line under the drawing tells of
    hovered: Option<i32>,
    hovered_at: Instant,
    focused: Option<i32>,
    told: Option<i32>,
    rest: slint::Timer,
    /// where the section's sliders stand, and for which section, size of stage and sliders
    places: Vec<drawing::Place>,
    places_of: Option<(usize, i32, i32, Vec<usize>)>,
    /// the stage as last drawn, and what it was drawn from: the light, the
    /// section, the stage's size, the window's scale, the slider drawn white
    drawn: Option<(drawing::Stage, slint::Image)>,
    drawn_of: Option<(Value, usize, i32, i32, f32, Option<usize>)>,
    cells: Rc<VecModel<Cell>>,
    sketches: Rc<VecModel<Sketch>>,
    over: Rc<VecModel<Cell>>,
    rows: Rc<VecModel<LabRow>>,
    foot: Rc<VecModel<Cell>>,
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
        // the caps' kind named, as its values had it, so it saves as it is
        // (a translucent cap's top takes what its plastic lets through)
        if light["sim"]["caps"].is_null() {
            let caps = Caps::of(&light["sim"]);
            light["sim"]["caps"] = json!(caps.name());
        }
        if let Some(sim) = light["sim"].as_object_mut() {
            sim.remove("tops");
        }
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
        on!(on_pick, |id, index| if id == TABS { Ask::Section(index) } else { Ask::Choose(id - CHOICE, index) });
        on!(on_hovered, |id, on| Ask::Hover(id, on));
        on!(on_compare, |on| Ask::Compare(on));
        on!(on_reset, |id| Ask::Reset(id));
        on!(on_focused, |id, on| Ask::Focus(id, on));
        ui.set_mark(mark);
        let (cells, sketches, over, rows, foot) = (Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()));
        ui.set_cells(ModelRc::from(cells.clone()));
        ui.set_sketches(ModelRc::from(sketches.clone()));
        ui.set_over(ModelRc::from(over.clone()));
        ui.set_rows(ModelRc::from(rows.clone()));
        ui.set_foot(ModelRc::from(foot.clone()));
        // a core lit as it once was, before the light was simulated, is read
        // as the simulation takes it, and is saved so
        let light = simulated(light);
        // the section shown when the lab was last open
        let section = crate::uistate::get("lab_section").and_then(|v| v.as_u64()).map_or(0, |n| (n as usize).min(SECTIONS.len() - 1));
        let mut page = LabPage {
            ui,
            asks,
            saved: light.clone(),
            light,
            rev: 0,
            comparing: false,
            history: History::default(),
            readout: Readout::default(),
            status: (String::new(), 0),
            section,
            hovered: None,
            hovered_at: Instant::now(),
            focused: None,
            told: None,
            rest: slint::Timer::default(),
            places: Vec::new(),
            places_of: None,
            drawn: None,
            drawn_of: None,
            cells,
            sketches,
            over,
            rows,
            foot,
        };
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
                        // North of centre's slider runs north to south, as the drawing does
                        let f = if p.path == ("led", "") { 1.0 - f } else { f };
                        let v = self.at_along(id.max(0) as usize, f as f64);
                        if get(&self.light, p.path) != Some(v) {
                            self.remember(id);
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
                                    // millimetres, on the side it is on unless typed below 0 or "south"/"north"
                                    Unit::Length if p.path.0 == "led" => {
                                        let (lo, hi) = self.range(id.max(0) as usize);
                                        let typed = t.to_lowercase();
                                        let south = if typed.contains("south") || num.starts_with('-') { true } else if typed.contains("north") { false } else { self.value(0) < 0.0 };
                                        (v.abs() * UNITS_PER_MM * if south { -1.0 } else { 1.0 }).clamp(lo, hi)
                                    }
                                    Unit::Length => v.max(0.01),
                                    Unit::Number => v.max(p.lo),
                                };
                                if get(&self.light, p.path) != Some(v) {
                                    self.remember(id);
                                    set(&mut self.light, p.path, v);
                                    self.rev += 1;
                                    self.status = (String::new(), 0);
                                }
                            }
                            Err(_) => self.status = (format!("“{}” isn't a number for {}.", t.trim(), p.label.to_lowercase()), 2),
                        }
                    }
                }
                Ask::Choose(choice, option) => {
                    let (choice, option) = (choice.max(0) as usize, option.max(0) as usize);
                    if choice < CHOICES.len() && chosen(choice, &self.light) != option {
                        self.remember(CHOICE + choice as i32);
                        choose(&mut self.light, choice, option);
                        self.rev += 1;
                        self.status = (String::new(), 0);
                    }
                }
                Ask::Compare(on) => {
                    if on != self.comparing {
                        self.comparing = on;
                        self.status = if on { ("Showing the saved tuning while held.".into(), 0) } else { (String::new(), 0) };
                    }
                }
                Ask::Reset(id) => {
                    let i = id.max(0) as usize;
                    if let Some(p) = PARAMS.get(i) {
                        let was = self.value_in(&self.saved, i);
                        if get(&self.light, p.path) != Some(was) {
                            self.remember(id);
                            set(&mut self.light, p.path, was);
                            self.rev += 1;
                            self.status = (format!("{} back to its saved value.", p.label), 0);
                        }
                    }
                }
                Ask::Revert => {
                    self.remember(-1);
                    self.light = self.saved.clone();
                    self.rev += 1;
                    self.status = ("Back to the Z10 core's saved tuning.".into(), 0);
                }
                Ask::Save => self.save(),
                Ask::Section(i) => {
                    let i = (i.max(0) as usize).min(SECTIONS.len() - 1);
                    if i != self.section {
                        self.section = i;
                        self.hovered = None;
                        self.told = None;
                        self.focused = None;
                        crate::uistate::set("lab_section", json!(i));
                    }
                }
                Ask::Hover(id, on) => {
                    if on {
                        self.hovered = Some(id);
                    } else if self.hovered == Some(id) {
                        self.hovered = None;
                    }
                    // the line under the drawing tells of it once the pointer rests
                    self.hovered_at = Instant::now();
                    let a = self.asks.clone();
                    self.rest.start(slint::TimerMode::SingleShot, REST, move || a.borrow_mut().push(Ask::Rest));
                }
                Ask::Focus(id, on) => {
                    if on {
                        self.focused = Some(id);
                    } else if self.focused == Some(id) {
                        self.focused = None;
                    }
                }
                Ask::Rest => {
                    if self.hovered_at.elapsed() + Duration::from_millis(20) >= REST {
                        self.told = self.hovered;
                    }
                }
            }
        }
        self.refresh();
        window
    }

    /// Remember the light before a change to `id`, for undoing it.
    fn remember(&mut self, id: i32) {
        self.history.remember(&self.light, id, Instant::now());
    }

    /// Undo the last change (Ctrl+Z), or do again the last undone (Ctrl+Shift+Z).
    pub fn undo(&mut self, again: bool) {
        if self.history.step(&mut self.light, again) {
            self.rev += 1;
            self.status = (if again { "Done again." } else { "Undone." }.into(), 0);
            self.refresh();
        }
    }

    /// Whether a value's box is being typed in (its own keys are its own).
    pub fn typing(&self) -> bool {
        self.focused.is_some()
    }

    /// The light the keyboard shows: the saved one while Hold to compare is held.
    pub fn shown(&self) -> &Value {
        if self.comparing { &self.saved } else { &self.light }
    }

    /// The light as the core's file last had it.
    pub fn saved(&self) -> &Value {
        &self.saved
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
        self.draw_stage();
        let (cells, (sketches, over, rows), foot) = self.layout();
        crate::front::sync_cells(&self.cells, cells);
        crate::front::sync_cells(&self.over, over);
        crate::front::sync_cells(&self.foot, foot);
        for (model, fresh) in [(&self.sketches, sketches)] {
            if model.row_count() == fresh.len() {
                for (i, s) in fresh.into_iter().enumerate() {
                    if model.row_data(i).as_ref() != Some(&s) {
                        model.set_row_data(i, s);
                    }
                }
            } else {
                model.set_vec(fresh);
            }
        }
        if self.rows.row_count() == rows.len() {
            for (i, r) in rows.into_iter().enumerate() {
                if self.rows.row_data(i).as_ref() != Some(&r) {
                    self.rows.set_row_data(i, r);
                }
            }
        } else {
            self.rows.set_vec(rows);
        }
        self.ui.set_status_text(self.status.0.as_str().into());
        self.ui.set_status_kind(self.status.1);
        let changed = self.light != self.saved;
        self.ui.set_can_save(changed);
        self.ui.set_can_revert(changed);
        if changed && self.status.0.is_empty() {
            self.ui.set_status_text("Changed, not saved.".into());
        }
    }

    /// The window's size, and the stage's in it.
    fn sizes(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (self.ui.get_window_width().max(900.0), self.ui.get_window_height().max(560.0));
        (w, h, w - 48.0, (h - 36.0 - HEAD_H - 10.0 - HELP_H - 4.0 - 64.0).max(300.0))
    }

    /// The slider drawn white: the one being typed in, or under the pointer.
    fn hot(&self) -> Option<usize> {
        self.focused.or(self.hovered).filter(|&i| (0..READING).contains(&i)).map(|i| i as usize).filter(|i| sliders_of(self.section, &self.light).contains(i))
    }

    /// Place the section's sliders when it opens or the stage changes size,
    /// and draw the stage again when what it shows has changed.
    fn draw_stage(&mut self) {
        let (_, _, sw, sh) = self.sizes();
        let lens = drawing::Lens::of(self.section);
        let v = drawing::Values::of(&self.light);
        let size = (self.section, sw.round() as i32, sh.round() as i32, sliders_of(self.section, &self.light));
        if self.places_of.as_ref() != Some(&size) {
            self.places = drawing::places(&v, lens, sw, sh, SECTIONS[self.section].3.len());
            self.places_of = Some(size.clone());
        }
        let scale = slint::ComponentHandle::window(&self.ui).scale_factor();
        let hot = self.hot();
        let of = (self.light.clone(), self.section, size.1, size.2, scale, hot);
        if self.drawn_of.as_ref() == Some(&of) {
            return;
        }
        let same_scale = self.drawn_of.as_ref().is_some_and(|o| o.4 == scale);
        let st = drawing::stage(&v, lens, sw, sh, &self.places, hot);
        if !(same_scale && self.drawn.as_ref().is_some_and(|(old, _)| *old == st)) {
            let image = slint::Image::from_rgba8_premultiplied(drawing::pixels(&st, scale));
            self.drawn = Some((st, image));
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

    /// Where a value's knob sits on its track: North of centre's runs north
    /// to south, as the drawing does.
    fn knob(&self, i: usize, v: f64) -> f64 {
        let f = self.along(i, v);
        if PARAMS[i].path == ("led", "") { 1.0 - f } else { f }
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
        self.value_in(&self.light, i)
    }

    fn value_in(&self, light: &Value, i: usize) -> f64 {
        let p = &PARAMS[i];
        let v = get(light, p.path).unwrap_or(p.def);
        if p.path == ("led", "") {
            let edge = Physics::from(light).switch_edge() as f64;
            v.clamp(-edge, edge)
        } else {
            v
        }
    }

    /// What the line under the drawing tells of, and what it says.
    fn told(&self) -> (String, String) {
        let r = self.readout;
        match self.focused.or(self.told) {
            Some(i) if (0..READING).contains(&i) && sliders_of(self.section, &self.light).contains(&(i as usize)) => (PARAMS[i as usize].label.into(), PARAMS[i as usize].note.into()),
            Some(i) if (CHOICE..CHOICE + CHOICES.len() as i32).contains(&i) => (CHOICES[(i - CHOICE) as usize].label.into(), CHOICES[(i - CHOICE) as usize].note.into()),
            Some(i) if i == READING => ("Brightest legend".into(), self.legend_note().unwrap_or_else(|| READOUT_ABOUT.into())),
            Some(i) if i == READING + 1 => ("Brightest deck".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 2 => ("Brightest skirt".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 3 => ("Brightest chassis".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 4 => ("Display's lit dots".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 5 => ("The desktop's white".into(), WHITE_ABOUT.into()),
            Some(i) if i == READING + 6 => ("The simulation".into(), if r.photons == 0 { SIM_ABOUT.into() } else { format!("{} photons from each LED so far. {SIM_ABOUT}", text::thousands(r.photons as u64)) }),
            _ => (SECTIONS[self.section].0.into(), SECTIONS[self.section].1.into()),
        }
    }

    /// What holds the legends back, if anything: only Peak nits does; the
    /// colour's clean limit is only told.
    fn legend_note(&self) -> Option<String> {
        let r = self.readout;
        if self.value(1) > r.peak + 0.5 && r.peak > 0.0 {
            Some(format!("Held at Peak nits, {:.0} (Settings › HDR).", r.peak))
        } else if r.legend > r.reach + 0.5 && r.reach > 0.0 {
            Some(format!("This colour shows cleanly up to {:.0} nits. Above that the screen clips its red, so the brightest parts turn yellower.", r.reach))
        } else {
            None
        }
    }

    /// The page laid out: its pieces; the stage, the words on it and the
    /// sliders round it; and the foot's readout.
    #[allow(clippy::type_complexity)]
    fn layout(&self) -> (Vec<Cell>, (Vec<Sketch>, Vec<Cell>, Vec<LabRow>), Vec<Cell>) {
        let (w, _, sw, sh) = self.sizes();
        let colour = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
        let in_colour = |st: i32, x: f32, y: f32, ww: f32, hh: f32, s: &str, c: [u8; 3]| Cell { color: colour(c), ..text_cell(st, x, y, ww, hh, s) };
        // the head: the title and what the lab is, and the sections
        let mut cells = vec![text_cell(style::TITLE, 24.0, 14.0, 300.0, 29.0, "Lighting lab")];
        let mut bx = 0.0;
        let mut tabs = Vec::new();
        for (i, (title, _, _, _)) in SECTIONS.iter().enumerate() {
            let bw = text::width(400, 13.0, title) + 24.0;
            tabs.push((i, title, bx, bw));
            bx += bw + 2.0;
        }
        let tabs_w = bx - 2.0 + 6.0;
        let tx = w - 24.0 - tabs_w;
        cells.push(text_cell(style::NOTE, 24.0, 44.0, (tx - 48.0).max(200.0), 18.0, INTRO));
        cells.push(cell(kind::SEG, tx, 17.0, tabs_w, 36.0));
        for (i, title, x, bw) in tabs {
            cells.push(Cell { text: (*title).into(), on: i == self.section, id: TABS, index: i as i32, ..cell(kind::SEG_BUTTON, tx + 3.0 + x, 20.0, bw, 30.0) });
        }
        // the stage, and the words on it
        let (sx, sy) = (24.0, HEAD_H);
        let mut sketches = Vec::new();
        let mut over = Vec::new();
        if let Some((st, image)) = &self.drawn {
            sketches.push(Sketch { x: sx, y: sy, w: st.w, h: st.h, image: image.clone() });
            for word in &st.words {
                // a word's baseline where the stage has it, in Geist's 11.5px line centred in its box
                let top = sy + word.y - 1.005 * 11.5 - (16.0 - 1.3 * 11.5) / 2.0;
                over.push(in_colour(style::SMALL_IN, sx + word.x, top, text::width(400, 11.5, &word.text) + 2.0, 16.0, &word.text, word.colour));
            }
        }
        // the section's choices, at the top of its left column, over the stage:
        // each its name over its options, all of it telling of it under the pointer
        let cx = sx + drawing::column_x(true, sw);
        for (k, &c) in SECTIONS[self.section].3.iter().enumerate() {
            let choice = &CHOICES[c];
            let y = sy + drawing::choice_y(k);
            let id = CHOICE + c as i32;
            over.push(Cell { id, ..cell(kind::ZONE, cx - 10.0, y - 7.0, drawing::CTRL_W + 20.0, drawing::CHOICE_H) });
            over.push(text_cell(style::LABEL, cx, y, drawing::CTRL_W, 26.0, choice.label));
            // its options as wide as their words, the box round them as wide as they are
            let widths: Vec<f32> = choice.options.iter().map(|name| text::width(400, 13.0, name) + 20.0).collect();
            let seg_w = widths.iter().sum::<f32>() + 2.0 * (widths.len() as f32 - 1.0) + 6.0;
            over.push(cell(kind::SEG, cx, y + 30.0, seg_w, 34.0));
            let on = chosen(c, &self.light);
            let mut bx = cx + 3.0;
            for (j, (name, bw)) in choice.options.iter().zip(widths).enumerate() {
                over.push(Cell { text: (*name).into(), on: j == on, id, index: j as i32, style: style::TELLS, ..cell(kind::SEG_BUTTON, bx, y + 32.0, bw, 30.0) });
                bx += bw + 2.0;
            }
        }
        // the sliders, each where the section placed it
        let hot = self.hot();
        let mut rows = Vec::new();
        let ids = sliders_of(self.section, &self.light);
        for place in &self.places {
            let i = place.i;
            if !ids.contains(&i) {
                continue;
            }
            let p = &PARAMS[i];
            let (number, mut unit) = reads(p, self.value(i));
            let mut derived = p.unit == Unit::Length && p.path.0 != "led";
            if let Some((_, extra)) = self.drawn.as_ref().and_then(|(st, _)| st.extras.iter().find(|e| e.0 == i)) {
                unit = extra.clone();
                derived = true;
            }
            let (at, was) = (self.knob(i, self.value(i)), self.knob(i, self.value_in(&self.saved, i)));
            rows.push(LabRow {
                x: sx + drawing::column_x(place.left, sw),
                y: sy + place.y - drawing::ATTACH,
                w: drawing::CTRL_W,
                id: i as i32,
                name: p.label.into(),
                hot: hot == Some(i),
                number: number.into(),
                unit: unit.into(),
                derived,
                at: at as f32,
                was: was as f32,
                moved: (at - was).abs() > 0.002,
            });
        }
        // the line under the stage: what the slider under the pointer does, or what the section is
        let (name, note) = self.told();
        let hy = sy + sh + 10.0;
        let nw = text::width(500, 12.5, &name) + 2.0;
        cells.push(in_colour(style::LABEL_IN, 24.0, hy, nw, 18.0, &name, [0xe5, 0xe5, 0xe5]));
        let first = (w - 48.0 - nw - 6.0).max(100.0);
        for (k, line) in wrap_after(&note, first, w - 48.0).iter().take(2).enumerate() {
            let x = if k == 0 { 24.0 + nw + 6.0 } else { 24.0 };
            cells.push(text_cell(style::NOTE, x, hy + 18.125 * k as f32, w - 48.0, 18.0, line));
        }
        (cells, (sketches, over, rows), self.readout_line())
    }

    /// The foot's readout of what is on the keyboard now, each number with
    /// a zone that tells of it under the pointer; the numbers dimmed while
    /// the simulation clears. In a narrow window it says less to fit beside
    /// what was last done and the buttons: the simulation without its count
    /// of photons, then without its heading (both still told under the pointer).
    fn readout_line(&self) -> Vec<Cell> {
        let (w, _, _, _) = self.sizes();
        let status = if self.status.0.is_empty() && self.light != self.saved { "Changed, not saved." } else { self.status.0.as_str() };
        let room = w - 24.0 - 176.0 - 10.0 - 68.0 - 10.0 - 132.0 - 10.0 - if status.is_empty() { 0.0 } else { text::width(400, 12.5, status) + 16.0 } - 24.0;
        for level in 0..3 {
            let (cells, end) = self.readout_at(level);
            if end - 24.0 <= room || level == 2 {
                return cells;
            }
        }
        unreachable!()
    }

    /// The readout laid out, saying less as `level` rises; and where it ends.
    fn readout_at(&self, level: u8) -> (Vec<Cell>, f32) {
        let r = self.readout;
        let colour = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
        let in_colour = |st: i32, x: f32, s: &str, c: [u8; 3]| {
            let wgt = if st == style::LABEL_IN { 500 } else { 400 };
            Cell { color: colour(c), ..text_cell(st, x, 23.0, text::width(wgt, 12.5, s) + 2.0, 18.0, s) }
        };
        let clearing = r.photons > 0 && !r.settled;
        let value_colour = if clearing { [0x7c, 0x7c, 0x7c] } else { [0xe5, 0xe5, 0xe5] };
        let nits = |v: f64| if v < 10.0 { format!("{v:.1}") } else { format!("{v:.0}") };
        let mut out = Vec::new();
        let mut x = 24.0;
        if level < 2 {
            let head = "Brightest now";
            out.push(in_colour(style::LABEL_IN, x, head, [0xa3, 0xa3, 0xa3]));
            x += text::width(500, 12.5, head) + 12.0;
        }
        let held = self.legend_note().is_some();
        let items: [(&str, String); 5] = [("Legend", nits(r.legend)), ("Deck", nits(r.deck)), ("Skirt", nits(r.skirt)), ("Chassis", nits(r.chassis)), ("Display", format!("{} nits", nits(r.lcd)))];
        let item = |out: &mut Vec<Cell>, x: &mut f32, id: i32, label: &str, value: &str, c: [u8; 3]| {
            let x0 = *x;
            out.push(text_cell(style::NOTE, *x, 23.0, text::width(400, 12.5, label) + 2.0, 18.0, label));
            *x += text::width(400, 12.5, label) + 6.0;
            out.push(in_colour(style::LABEL_IN, *x, value, c));
            *x += text::width(500, 12.5, value);
            out.insert(out.len() - 2, Cell { id, ..cell(kind::ZONE, x0 - 4.0, 16.0, *x - x0 + 8.0, 32.0) });
            *x += 14.0;
        };
        for (k, (label, value)) in items.iter().enumerate() {
            let c = if k == 0 && held { [0xf0, 0xb3, 0x5a] } else { value_colour };
            item(&mut out, &mut x, READING + k as i32, label, value, c);
        }
        let sep = |out: &mut Vec<Cell>, x: &mut f32| {
            *x += 2.0;
            out.push(Cell { color: colour([0x3a, 0x3a, 0x3a]), ..cell(kind::BOX, *x, 25.0, 1.0, 14.0) });
            *x += 15.0;
        };
        sep(&mut out, &mut x);
        item(&mut out, &mut x, READING + 5, "Desktop white", &format!("{} nits", nits(r.white)), value_colour);
        sep(&mut out, &mut x);
        let photons = if level == 0 { format!(", {} photons", text::thousands(r.photons as u64)) } else { String::new() };
        let sim = if r.photons == 0 {
            "Starting".to_string()
        } else if r.settled {
            format!("Settled{photons}")
        } else {
            format!("Clearing{photons}")
        };
        let sw = text::width(400, 12.5, &sim) + 2.0;
        out.push(Cell { id: READING + 6, ..cell(kind::ZONE, x - 4.0, 16.0, sw + 8.0, 32.0) });
        out.push(text_cell(style::NOTE, x, 23.0, sw, 18.0, &sim));
        (out, x + sw)
    }
}

/// The lab's changes, for undoing them and doing them again: the light as it
/// was before each change, and as it was before each undo. A change to the
/// same thing within a moment of the last joins it, so a drag undoes whole.
#[derive(Default)]
struct History {
    back: Vec<Value>,
    ahead: Vec<Value>,
    last: Option<(i32, Instant)>,
}

impl History {
    const MOST: usize = 200;
    const JOINS: Duration = Duration::from_millis(800);

    fn remember(&mut self, light: &Value, id: i32, now: Instant) {
        let joins = self.last.is_some_and(|(last, at)| last == id && now.duration_since(at) < History::JOINS);
        if !joins {
            self.back.push(light.clone());
            if self.back.len() > History::MOST {
                self.back.remove(0);
            }
        }
        self.ahead.clear();
        self.last = Some((id, now));
    }

    /// Back a change (or ahead again), into `light`; false if there is none.
    fn step(&mut self, light: &mut Value, again: bool) -> bool {
        let (from, to) = if again { (&mut self.ahead, &mut self.back) } else { (&mut self.back, &mut self.ahead) };
        match from.pop() {
            Some(l) => {
                to.push(std::mem::replace(light, l));
                self.last = None;
                true
            }
            None => false,
        }
    }
}

/// Lines of `s` broken at spaces, the first to fit `first` pixels, the rest `rest`.
fn wrap_after(s: &str, first: f32, rest: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split(' ') {
        let max = if lines.is_empty() { first } else { rest };
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && text::width(400, 12.5, &next) > max {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
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
    s += &part("sim", &["nits", "switch", "beam", "scatter", "sides", "glow_spread", "switch_top", "switch_width", "skirt_foot", "cap_top", "legends", "caps", "skirts", "black", "milky", "cap_in", "cap_out", "depth", "deck", "walls", "chassis"]);
    s += &part("legend", &["unlit"]);
    s += &part("lcd", &["nits", "unlit", "field", "dot", "glass"]);
    s += &format!("  \"wells\": {}\n", serde_json::to_string(&light["wells"]).unwrap_or_else(|_| "\"mask_1\"".into()));
    s += " },\n";
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drag undoes whole, other changes one at a time, and what is undone
    /// can be done again until something new is changed.
    #[test]
    fn changes_undo_and_do_again() {
        let mut h = History::default();
        let t = Instant::now();
        let at = |ms: u64| t + Duration::from_millis(ms);
        let mut light = json!({ "v": 0 });
        // a drag: many changes to one slider close together
        for (i, ms) in [(1, 0), (2, 16), (3, 32), (4, 48)] {
            h.remember(&light, 5, at(ms));
            light["v"] = json!(i);
        }
        // then another slider, and the first again after a pause
        h.remember(&light, 6, at(100));
        light["w"] = json!(1);
        h.remember(&light, 5, at(2000));
        light["v"] = json!(9);
        assert!(h.step(&mut light, false) && light["v"] == json!(4), "the last change undone");
        assert!(h.step(&mut light, false) && light["w"].is_null(), "then the other slider's");
        assert!(h.step(&mut light, false) && light["v"] == json!(0), "then the whole drag at once");
        assert!(!h.step(&mut light, false), "nothing before it");
        assert!(h.step(&mut light, true) && light["v"] == json!(4), "done again");
        h.remember(&light, 7, at(3000));
        light["x"] = json!(1);
        assert!(!h.step(&mut light, true), "a new change ends what could be done again");
    }

    /// Each choice reads back as chosen and sets its values; a black housing
    /// shows no glow's sliders, and a cap's kind only the sliders it has;
    /// going from one kind of cap to another and back keeps what was tuned.
    #[test]
    fn the_choices_set_the_light() {
        let mut light = json!({ "led": 4.5, "sim": { "nits": 600, "legends": 0.3 } });
        for (option, switch) in [(1, Switch::ClearTop), (2, Switch::Clear), (0, Switch::Slot)] {
            choose(&mut light, 0, option);
            assert_eq!(chosen(0, &light), option);
            assert_eq!(Physics::from(&light).switch, switch);
        }
        assert_eq!(get(&light, ("sim", "switch_top")), Some(5.7));
        assert_eq!(get(&light, ("sim", "beam")), Some(3.0));
        let shown = |light: &Value, section| sliders_of(section, light).iter().map(|&i| PARAMS[i].path.1).collect::<Vec<_>>();
        assert!(shown(&light, 0).iter().all(|k| !matches!(*k, "scatter" | "sides" | "glow_spread")), "a black housing has no glow to tune");
        choose(&mut light, 0, 1);
        assert_eq!(sliders_of(0, &light).len(), SECTIONS[0].2.len(), "a clear one shows them all");
        // the caps
        assert_eq!(chosen(1, &light), 0, "no plastic letting light through: shine-through");
        assert!(shown(&light, 1).iter().all(|k| !matches!(*k, "skirts" | "milky" | "black")), "shine-through caps have none of those");
        choose(&mut light, 1, 1);
        assert_eq!((chosen(1, &light), get(&light, ("sim", "skirts")), get(&light, ("sim", "black"))), (1, Some(0.3), Some(0.4)), "pudding: the plastic lets through what a legend does, below the black");
        assert!(["skirts", "milky", "black"].iter().all(|k| shown(&light, 1).contains(k)));
        set(&mut light, ("sim", "skirts"), 0.0);
        assert_eq!(chosen(1, &light), 1, "pudding caps stay pudding with their plastic dragged to nothing");
        set(&mut light, ("sim", "skirts"), 0.45);
        set(&mut light, ("sim", "black"), 0.3);
        choose(&mut light, 1, 2);
        let p = Physics::from(&light);
        assert_eq!((p.caps, p.skirts, p.tops, p.black), (Caps::Translucent, 0.45, 0.45, 0.0), "a translucent top lets through what the plastic does");
        assert!(!shown(&light, 1).contains(&"black"), "translucent caps have no black");
        choose(&mut light, 1, 0);
        assert_eq!(Physics::from(&light).skirts, 0.0);
        choose(&mut light, 1, 1);
        assert_eq!((get(&light, ("sim", "skirts")), get(&light, ("sim", "black"))), (Some(0.45), Some(0.3)), "back to pudding as it was tuned");
        // the LED's place in millimetres, and its side
        assert_eq!(reads(&PARAMS[0], 7.0), ("5.1".into(), "mm north".into()));
        assert_eq!(reads(&PARAMS[0], -2.76), ("2".into(), "mm south".into()));
    }

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
