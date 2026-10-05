//! The Light lab: a developer's window for tuning how a device is lit,
//! whichever device it is: it knows none, and works from what the device's
//! core describes (see LabDevice). The light is simulated (see
//! ectodeck_draw's sim): each LED followed through the device's shape. The
//! lab's sliders are the device's physical make, each in the units that
//! mean something: where the LED sits and how its light leaves the switch,
//! the caps' heights and how much light their plastic gives back or lets
//! through, the wells' depth and finish; and the light's brightness, which
//! everything else follows. The device in the main window takes every change
//! at once, its light clearing as the simulation runs. Save writes the
//! tuning into the device's core, which Ectodeck reads when it runs from its
//! source: each value that changed in its place in the core's file, the rest
//! of the file (its comments, its lines) as its author wrote it.
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
use ectodeck_draw::hdr::sim::{key_light, Caps, Physics, Switch};
use ectodeck_draw::hdr::{CapShape, KeyGeometry};
use ectodeck_draw::jsonc;

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
    /// the keys the page tunes: all of them (0), or one, whose map it opens (1)
    Scope(i32),
    /// a key chosen on the map, by its place among the caps
    Key(i32),
    /// a value of the chosen key's own given back, a slider's or a choice's:
    /// the key takes all keys' again
    Follow(i32),
    /// the switch pad pressed or dragged on, where on it, across and down
    Pad(f32, f32),
}

/// What a value is, which says how it reads.
#[derive(Clone, Copy, PartialEq)]
enum Unit {
    /// luminance, in nits
    Nits,
    /// how far north of its switch's middle the LED sits, in millimetres,
    /// below 0 south
    North,
    /// a height or a width, in millimetres
    Mm,
    /// how far something carries, in millimetres, on a scale of ratios like a length's
    Reach,
    /// a part of the whole, as a percentage
    Fraction,
    /// a plain number, as a beam's power
    Number,
    /// a key's switch's place under its cap, in millimetres east or north of
    /// its middle (the path says which), below 0 west or south
    Place,
}

/// One value of the core's "light": where it is (a section's value, or the
/// light's own where the section is ""), what it is called, its slider's
/// range and step. A brightness's range is 0 to Peak nits, whatever `lo` and
/// `hi` say, since nothing above Peak nits shows, and the LED's runs to its
/// switch's edges, since it sits inside the switch. A brightness and a reach
/// keep three significant figures from their sliders, whatever the step
/// says, so a small value moves in small steps.
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

const PARAMS: [Param; 27] = [
    Param { path: ("switches", "led"), label: "North of centre", lo: -7.0, hi: 7.0, step: 0.05, unit: Unit::North, def: 5.08, note: "Where each key's LED sits under its cap, from the switch's centre toward the top of the key; below 0, toward the bottom, as on a keyboard whose LEDs face south. It goes as far as the switch's edge, so Switch width sets how far the slider reaches." },
    Param { path: ("brightness", ""), label: "Brightness", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, def: 600.0, note: "The brightest legend, with every key lit white at full, up to Peak nits (Settings › HDR). Everything else the LEDs light is as bright as the simulation finds it beside that. A colour gives off its own share, as an LED does: a red about a fifth." },
    Param { path: ("switches", "beam"), label: "Beam", lo: 1.0, hi: 200.0, step: 0.5, unit: Unit::Number, def: 24.0, note: "How tight the LED's beam out of the top of the switch is. Higher keeps it to a small bright spot on the legend over the LED; lower spreads it across the cap's top and onto the inside of its skirt." },
    Param { path: ("switches", "scattered"), label: "Scattered", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.7, note: "The share of the LED's light a clear housing scatters, glowing with it all over, rather than sending up in the beam. More lights the legend more evenly, and the deck and the inside of the cap more; less leaves more for the spot over the LED." },
    Param { path: ("switches", "sides"), label: "Through its sides", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.25, note: "The share of the housing's glow that leaves through its sides rather than its top. Its sides light the deck round the switch, out under the skirt; its top lights the back of the legend evenly, its parts far from the LED as well." },
    Param { path: ("switches", "glow"), label: "Glow spread", lo: 0.5, hi: 60.0, step: 0.1, unit: Unit::Reach, def: 4.0, note: "How far the housing's glow carries from the LED before it falls to about a third, as the light fades through the plastic. Short keeps it to the LED's side of the switch, so the deck and the legend are brightest there; long glows evenly all round." },
    Param { path: ("switches", "width"), label: "Switch width", lo: 1.0, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 12.0, note: "How wide the switch's housing is. A black one stands in the way of the light bouncing about inside the cap; a clear one glows with the scattered light over its top and its sides, spreading it across the back of the legend and, out under the skirt, onto the deck round the switch." },
    Param { path: ("switches", "top"), label: "Switch top", lo: 0.1, hi: 14.0, step: 0.1, unit: Unit::Mm, def: 6.5, note: "How high above the deck the switch's housing stands, where the beam leaves it: 5.7 mm on Cherry-style switches. Nearer the cap's top makes the spot on the legend smaller and brighter." },
    Param { path: ("caps", "foot"), label: "Skirt's foot", lo: 0.0, hi: 12.0, step: 0.1, unit: Unit::Mm, def: 5.0, note: "How high above the deck the foot of each cap's skirt is: the gap the light gets out under it by." },
    Param { path: ("caps", "top"), label: "Cap's top", lo: 1.0, hi: 25.0, step: 0.1, unit: Unit::Mm, def: 14.0, note: "How high above the deck each cap's top is, where the legend is." },
    Param { path: ("caps", "legends"), label: "Legends let through", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.3, note: "The share of the light on the back of a cap's top that its legend lets through. With Brightness fixing the legends, less makes everything else brighter beside them." },
    Param { path: ("caps", "inside"), label: "Inside", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.5, note: "The share of the light the inside of a cap gives back. More of it bounces about inside the cap, and more gets out under the skirt onto the deck round it." },
    Param { path: ("caps", "outside"), label: "Outside", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.18, note: "The share of the light the outside of a cap's skirt gives back: what the skirts show of the light round them." },
    Param { path: ("legends", "unlit"), label: "LED off", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.15, note: "How bright a legend looks with its LED off (the dimmer at 0%, or a key the lighting leaves dark), as a share of the grey the art prints it in. A backlit legend with no light behind it is dark." },
    Param { path: ("wells", "depth"), label: "Depth", lo: 0.0, hi: 12.0, step: 0.1, unit: Unit::Mm, def: 2.0, note: "How deep the wells are: how high the chassis round them stands above the deck. Deeper keeps more of the light in the wells." },
    Param { path: ("wells", "deck"), label: "Deck", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.22, note: "The share of the light the deck at the wells' floor gives back." },
    Param { path: ("wells", "chassis"), label: "Chassis", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.4, note: "The share of the light the chassis gives back: its top round the wells, and the wells' walls, which are its own sides." },
    Param { path: ("display", "lit"), label: "Lit dots", lo: 0.0, hi: 0.0, step: 1.0, unit: Unit::Nits, def: 324.0, note: "The display's lit dots with its backlight white at full. A colour gives off its own share of that, as an LED does." },
    Param { path: ("display", "unlit"), label: "Unlit dots", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, def: 4.1, note: "The dots that are off, which let a little of the backlight through, as a real display's do." },
    Param { path: ("display", "background"), label: "Background", lo: 0.0, hi: 0.0, step: 0.1, unit: Unit::Nits, def: 0.8, note: "The ground between the dots." },
    Param { path: ("display", "dot"), label: "Dot size", lo: 0.05, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.84, note: "How much of each cell a dot fills; the rest is the gap between dots." },
    Param { path: ("display", "glass"), label: "Glass sheen", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 1.0, note: "How strongly the glass over the display shows its reflection, as the art draws it at 100%." },
    Param { path: ("caps", "plastic"), label: "Plastic lets through", lo: 0.0, hi: 0.98, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "The share of the light on the inside of a cap that its translucent plastic lets through, glowing on its outside: its sides below the black on pudding caps, all of it but the legends on translucent ones. Plastic with no coating lets through about what a legend does." },
    Param { path: ("caps", "black"), label: "Black down the sides", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "How far down a cap's sides the black of its top comes, as a share of the skirt's height. A pudding cap's black comes part of the way, its milky plastic below; light gets out only below the black." },
    Param { path: ("caps", "milky"), label: "Milky", lo: 0.0, hi: 1.0, step: 0.01, unit: Unit::Fraction, def: 0.0, note: "How milky the translucent plastic is. Milky plastic carries the light it takes some way through itself before it leaves, so it glows evenly round the cap, and looks white with its LED off; clear plastic lets the light out where it went in." },
    Param { path: ("place", "east"), label: "Switch east–west", lo: -60.0, hi: 60.0, step: 0.1, unit: Unit::Place, def: 0.0, note: "Where this key's switch sits under its cap, east or west of the cap's middle: where the plate holds it, which on a long key can be off its middle. Its LED goes with it." },
    Param { path: ("place", "north"), label: "Switch north–south", lo: -30.0, hi: 30.0, step: 0.1, unit: Unit::Place, def: 0.0, note: "Where this key's switch sits under its cap, north or south of the cap's middle, as on a cap that reaches down past its row. Its LED goes with it." },
];

/// One of a section's choices, at the top of its left column, which says
/// what the keyboard is as a whole: what it is called, what it does, and its options.
struct Choice {
    label: &'static str,
    note: &'static str,
    options: &'static [&'static str],
}

const CHOICES: [Choice; 2] = [
    Choice { label: "Switch", note: "What the switch's housing is: black with its LED through a slot in its top, as some older switches have it (Kailh's among them), so all the LED's light leaves in its beam; or clear, glowing with a share of it, over its top as most RGB switches have it, or all over. Each sets the switch's values for that kind; the sliders tune from there.", options: &["LED slot", "Clear top", "Clear housing"] },
    Choice { label: "Keycaps", note: "What the caps let light through: shine-through caps are opaque but for their legends; pudding caps have translucent sides below a black top whose black comes part of the way down them; translucent caps let light through all over. Each fills in Plastic lets through and Black down the sides where they are off; the sliders tune from there.", options: &["Shine-through", "Pudding", "Translucent"] },
];

/// The page's sections: a title, what the part is, its values, and its choices.
const SECTIONS: [(&str, &str, &[usize], &[usize]); 4] = [
    ("LED", "Each key's LED shines up out of its switch in a beam, onto the back of the legend. In a black housing that is all its light; a clear housing scatters a share of it instead, glowing with it over its top and sides, which lights the legend evenly, the inside of the cap and the deck round the switch.", &[0, 1, 2, 3, 4, 5, 7, 6], &[0]),
    ("Keycaps", "Each cap stands over its switch, its skirt from its foot to its top. Light bounces about inside it; some gets out under the skirt, some through the legend, and some through the plastic where it is translucent and not coated black.", &[8, 9, 10, 22, 23, 24, 11, 12, 13], &[1]),
    ("Wells", "The keys sit in wells in the chassis: the deck at their floor, walled round by the chassis. What each gives back of the light falling on it lights the rest.", &[14, 15, 16], &[]),
    ("Display", "The display's backlit dots, and the glass over them.", &[17, 18, 19, 20, 21], &[]),
];

/// A section's sliders that tune this light: a black housing does not
/// glow, so its glow's sliders are not shown with it.
pub(crate) fn sliders_of(section: usize, light: &Value) -> Vec<usize> {
    let slot = Physics::from(light).switch == Switch::Slot;
    // shine-through caps show their plastic only through their legends; translucent ones have no black
    let caps = Physics::from(light).caps;
    SECTIONS[section].2.iter().copied().filter(|&i| {
        let path = PARAMS[i].path;
        !(slot && matches!(path, ("switches", "scattered" | "sides" | "glow"))) && !(caps == Caps::ShineThrough && matches!(path, ("caps", "plastic" | "milky" | "black"))) && !(caps == Caps::Translucent && path == ("caps", "black"))
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
            light["switches"]["kind"] = json!(switch.name());
            // a Cherry-style housing's top, and a clear one's glow as such switches have it
            set(light, ("switches", "top"), 5.7);
            match switch {
                // all the LED's light in its beam: as wide as a diffused LED's, to light the whole cap
                Switch::Slot => set(light, ("switches", "beam"), 3.0),
                Switch::ClearTop => {
                    set(light, ("switches", "beam"), 24.0);
                    set(light, ("switches", "scattered"), 0.7);
                    set(light, ("switches", "sides"), 0.25);
                    set(light, ("switches", "glow"), 4.0);
                }
                Switch::Clear => {
                    set(light, ("switches", "beam"), 24.0);
                    set(light, ("switches", "scattered"), 0.85);
                    set(light, ("switches", "sides"), 0.5);
                    set(light, ("switches", "glow"), 6.0);
                }
            }
        }
        _ => {
            // The caps' kind, and what it needs where it is off: plastic that
            // lets light through (about what a legend does, uncoated), and for
            // pudding caps black some way down the sides. Values already tuned
            // stay, so going from one kind to another and back keeps them.
            let caps = [Caps::ShineThrough, Caps::Pudding, Caps::Translucent][option.min(2)];
            light["caps"]["kind"] = json!(caps.name());
            let plastic = get(light, ("caps", "legends")).unwrap_or(0.3);
            if caps != Caps::ShineThrough && get(light, ("caps", "plastic")).unwrap_or(0.0) <= 0.0 {
                set(light, ("caps", "plastic"), plastic);
            }
            let black = get(light, ("caps", "black")).unwrap_or(0.0);
            if caps == Caps::Pudding && !(black > 0.0 && black < 1.0) {
                set(light, ("caps", "black"), 0.4);
            }
        }
    }
}


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
            Unit::Number | Unit::Reach if self.lo > 0.0 => Scale::Ratio,
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

/// A value in words: its number and its unit, as its box and unit show them.
fn said(p: &Param, v: f64) -> String {
    match reads(p, v) {
        (_, u) if u == "centred" => u,
        (n, u) if u.is_empty() => n,
        (n, u) if u == "%" => format!("{n}%"),
        (n, u) => format!("{n} {u}"),
    }
}

/// A value as its box shows it, and its unit after the box.
fn reads(p: &Param, v: f64) -> (String, String) {
    let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    match p.unit {
        // to three significant figures, as its slider keeps it
        Unit::Nits => (trim(if v >= 100.0 { format!("{v:.0}") } else if v >= 10.0 { format!("{v:.1}") } else if v >= 1.0 { format!("{v:.2}") } else { format!("{v:.3}") }), "nits".into()),
        // the LED's place from the switch's middle, and which way
        Unit::North => (trim(format!("{:.2}", v.abs())), if v.abs() < 0.005 { "centred".into() } else if v > 0.0 { "mm north".into() } else { "mm south".into() }),
        Unit::Mm => (trim(format!("{v:.1}")), "mm".into()),
        Unit::Reach => (trim(format!("{v:.2}")), "mm".into()),
        Unit::Fraction => (trim(format!("{:.1}", v * 100.0)), "%".into()),
        Unit::Number => (trim(format!("{v:.2}")), String::new()),
        // a key's switch's place: how far, and which way
        Unit::Place => {
            let (plus, minus) = if p.path.1 == "east" { ("mm east", "mm west") } else { ("mm north", "mm south") };
            (trim(format!("{:.1}", v.abs())), if v.abs() < 0.05 { "centred".into() } else if v > 0.0 { plus.into() } else { minus.into() })
        }
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

/// What the readout's numbers measure, and the simulation's.
const READOUT_ABOUT: &str = "Measured from the keys' colours and brightness as the keyboard shows them, every part as if each key were lit as the brightest is.";
const WHITE_ABOUT: &str = "How bright this screen shows the desktop's white now; the keyboard's light is drawn beside it.";
const SIM_ABOUT: &str = "The light straight from each LED is worked out exactly at once; the light that has bounced is followed photon by photon, and clears as more come in.";
/// the readout's numbers' ids, after the sliders', the choices', and the sections' tabs'
const READING: i32 = 100;
const CHOICE: i32 = 800;
const TABS: i32 = 900;
/// the keys tuned (all or one), a key on the map, a value's mark that it is
/// the key's own, and the switch pad
const SCOPE: i32 = 940;
const MAP: i32 = 950;
const OWN: i32 = 960;
const PAD: i32 = 970;
/// a slider's reset: this, plus its slider's id
const RESET: i32 = 1000;
/// the switch's place, east and north: values of one key's only
const PLACE_E: usize = 25;
const PLACE_N: usize = 26;
/// what a choice sets, which a key takes as its own when it is chosen for it
const CHOSEN: [&[(&str, &str)]; 2] = [
    &[("switches", "kind"), ("switches", "top"), ("switches", "beam"), ("switches", "scattered"), ("switches", "sides"), ("switches", "glow")],
    &[("caps", "kind"), ("caps", "plastic"), ("caps", "black")],
];
/// what a button is, under the drawing when one is chosen
const BUTTON_ABOUT: &str = "A button stands on the chassis, from its foot to its top, in the light coming out of the keys round it: its sides and top give back their share of what falls on them, and it stands in the way of the rest.";
/// what the switch pad does
const PAD_NOTE: &str = "Drag the switch to where it sits under this key's cap, as you see it with the cap off; its LED goes with it. The numbers below it say where it is from the cap's middle.";
/// how long the pointer rests on something before the line under the drawing tells of it
const REST: Duration = Duration::from_millis(250);
/// the page's rows below the title bar: its head, the stage after a gap, the
/// line under the stage, and the foot's 64
const HEAD_H: f32 = 70.0;
const HELP_H: f32 = 44.0;

/// The device the lab tunes, as its core describes it: what it is called
/// and where its core's description is (Save writes there); which of the
/// light's parts it has: keys with LEDs in them (the LED and Keycaps
/// sections), wells round them (Wells), a display (Display); every cap, each
/// in section as the art has it; and the key drawn for all keys.
pub struct LabDevice {
    pub name: String,
    pub path: std::path::PathBuf,
    pub keys: bool,
    pub wells: bool,
    pub screens: bool,
    pub caps: Vec<(CapShape, Option<KeyGeometry>)>,
    pub typical: Option<String>,
}

impl LabDevice {
    /// A cap's section, by its control's id.
    fn geometry(&self, id: &str) -> Option<&KeyGeometry> {
        self.caps.iter().find(|(c, _)| c.id == id).and_then(|(_, g)| g.as_ref())
    }
}

pub struct LabPage {
    pub ui: LabWindow,
    /// the device tuned
    device: LabDevice,
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
    places_of: Option<(usize, i32, i32, Vec<usize>, Option<usize>)>,
    /// the stage as last drawn, and what it was drawn from: the light, the
    /// section, the stage's size, the window's scale, the slider drawn white
    drawn: Option<(drawing::Stage, slint::Image)>,
    drawn_of: Option<(Value, usize, i32, i32, f32, Option<usize>, Option<usize>)>,
    cells: Rc<VecModel<Cell>>,
    sketches: Rc<VecModel<Sketch>>,
    over: Rc<VecModel<Cell>>,
    rows: Rc<VecModel<LabRow>>,
    foot: Rc<VecModel<Cell>>,
    /// the cap whose own setup the page shows, if one is chosen (its place
    /// among the device's caps), and whether the map to choose one is open
    scope: Option<usize>,
    picking: bool,
    /// the switch pad as last drawn, where it has the key, and what it was drawn from
    pad: Option<(drawing::Stage, drawing::PadMap, slint::Image)>,
    pad_of: Option<(Value, usize, bool, f32)>,
}

/// The value at a path in a light: a section's value, or the light's own
/// where the path names no section ("brightness").
fn at<'a>(light: &'a Value, (a, b): (&str, &str)) -> &'a Value {
    if b.is_empty() { &light[a] } else { &light[a][b] }
}

fn put_at(light: &mut Value, (a, b): (&str, &str), v: Value) {
    if b.is_empty() {
        light[a] = v;
    } else {
        if !light[a].is_object() {
            light[a] = json!({});
        }
        light[a][b] = v;
    }
}

/// A key's own value, under the light's "keys" by its control's id.
fn own_put(light: &mut Value, id: &str, path: (&str, &str), v: Value) {
    if !light["keys"].is_object() {
        light["keys"] = json!({});
    }
    if !light["keys"][id].is_object() {
        light["keys"][id] = json!({});
    }
    put_at(&mut light["keys"][id], path, v);
}

/// A key's own value given back, and with it its entry if that leaves it
/// none, and the light's "keys" if no key has any.
fn own_take(light: &mut Value, id: &str, path: (&str, &str)) {
    if !light["keys"][id].is_object() {
        return;
    }
    remove_at(&mut light["keys"][id], path);
    if light["keys"][id].as_object().is_some_and(|o| o.is_empty()) {
        if let Some(keys) = light["keys"].as_object_mut() {
            keys.remove(id);
        }
    }
    if light["keys"].as_object().is_some_and(|o| o.is_empty()) {
        if let Some(l) = light.as_object_mut() {
            l.remove("keys");
        }
    }
}

/// A value taken out of a light, and its section with it if it leaves it empty.
fn remove_at(light: &mut Value, (a, b): (&str, &str)) {
    let Some(o) = light.as_object_mut() else { return };
    if b.is_empty() {
        o.remove(a);
    } else if let Some(section) = o.get_mut(a).and_then(Value::as_object_mut) {
        section.remove(b);
        if section.is_empty() {
            o.remove(a);
        }
    }
}

/// What a light has that the page tunes: all of it, or one key's own values
/// (by its control's id).
fn tuned<'a>(light: &'a Value, key: Option<&str>) -> &'a Value {
    match key {
        Some(id) => &light["keys"][id],
        None => light,
    }
}

/// Whether a value differs from the saved light's, in what the page tunes.
fn differs(light: &Value, saved: &Value, key: Option<&str>, path: (&str, &str)) -> bool {
    at(tuned(light, key), path) != at(tuned(saved, key), path)
}

/// A value put back as the saved light has it, in what the page tunes: all
/// keys', or one key's own, which follows all keys again where the saved
/// light gives the key none of its own.
fn put_back(light: &mut Value, saved: &Value, key: Option<&str>, path: (&str, &str)) {
    let was = at(tuned(saved, key), path).clone();
    match key {
        Some(id) if was.is_null() => own_take(light, id, path),
        Some(id) => own_put(light, id, path, was),
        None if was.is_null() => remove_at(light, path),
        None => put_at(light, path, was),
    }
}

/// A key's name as short as its cap on the map has room for, where the
/// core gives none: the usual short names of a keyboard's keys.
fn short(label: &str) -> String {
    let s = match label {
        "Escape" => "Esc",
        "Grave accent" => "`",
        "Minus" => "-",
        "Equals" => "=",
        "Backspace" => "Bksp",
        "Left bracket" => "[",
        "Right bracket" => "]",
        "Backslash" => "\\",
        "Caps Lock" => "Caps",
        "Semicolon" => ";",
        "Quote" => "'",
        "Left Shift" | "Right Shift" => "Shift",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Left Ctrl" | "Right Ctrl" => "Ctrl",
        "Left Alt" | "Right Alt" => "Alt",
        "Print Screen" => "PrtSc",
        "Scroll Lock" => "ScrLk",
        "Pause/Break" => "Pause",
        "Insert" => "Ins",
        "Page Up" => "PgUp",
        "Delete" => "Del",
        "Page Down" => "PgDn",
        "Up arrow" => "↑",
        "Down arrow" => "↓",
        "Left arrow" => "←",
        "Right arrow" => "→",
        "Num Lock" => "Num",
        "Keypad Enter" => "Enter",
        _ => return label.strip_prefix("Keypad").map_or(label, str::trim).to_string(),
    };
    s.to_string()
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

/// A light's numbers all as floats, as the lab sets them, so a value read
/// from the core's file (14) and one the lab has set (14.0) are alike.
fn floats(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
        Value::Array(a) => Value::Array(a.into_iter().map(floats).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, floats(v))).collect()),
        v => v,
    }
}

impl LabPage {
    pub fn new(ui: LabWindow, mark: slint::Image, light: Value, device: LabDevice) -> LabPage {
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
        on!(on_pick, |id, index| match id {
            TABS => Ask::Section(index),
            SCOPE => Ask::Scope(index),
            MAP => Ask::Key(index),
            OWN => Ask::Follow(index),
            _ => Ask::Choose(id - CHOICE, index),
        });
        on!(on_pad, |_id, x, y| Ask::Pad(x, y));
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
        let light = floats(light);
        // the section shown when the lab was last open
        let section = crate::uistate::get("lab_section").and_then(|v| v.as_u64()).map_or(0, |n| (n as usize).min(SECTIONS.len() - 1));
        let mut page = LabPage {
            ui,
            device,
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
            scope: None,
            picking: false,
            pad: None,
            pad_of: None,
        };
        // a section the device has
        if !page.sections().contains(&page.section) {
            page.section = page.sections().first().copied().unwrap_or(0);
        }
        page.refresh();
        page
    }

    /// The sections the device has parts for, in order.
    fn sections(&self) -> Vec<usize> {
        let d = &self.device;
        [d.keys, d.keys, d.wells, d.screens].iter().enumerate().filter(|(_, has)| **has).map(|(i, _)| i).collect()
    }

    /// The key the stage draws, in section as its art has it: the one chosen,
    /// or the core's typical key.
    fn drawn_key(&self) -> drawing::Geometry {
        let id = self.key().map(|k| k.id.as_str()).or(self.device.typical.as_deref());
        drawing::Geometry::of(id.and_then(|id| self.device.geometry(id)))
    }

    /// Every cap of the device, each in section, once its board is drawn
    /// (the lab can open before it is).
    pub fn set_caps(&mut self, caps: Vec<(CapShape, Option<KeyGeometry>)>) {
        if self.device.caps != caps {
            self.device.caps = caps;
            self.refresh();
        }
    }

    /// Whether it has the device's caps yet.
    pub fn has_caps(&self) -> bool {
        !self.device.caps.is_empty()
    }

    /// What the lab says of the page, under its title, and more briefly
    /// where the head is narrow: what it tunes, or whose own setup it shows.
    fn intro(&self) -> (String, String) {
        match self.key() {
            None => {
                let name = &self.device.name;
                (format!("Tune how the {name} is lit. Ectodeck's window shows each change at once; Save writes them into its core."), format!("Tune how the {name} is lit; Save writes it into its core."))
            }
            Some(k) if k.button => (format!("{0}'s own setup. A button with no light of its own, standing in the keys' light: how high it stands, and what it gives back of the light.", k.label), format!("{0}'s own setup: a button in the keys' light.", k.label)),
            Some(k) if !k.lit => (format!("{0}'s own setup. It has no LED of its own, so only its cap is set here; the rest follows all keys.", k.label), format!("{}'s own setup: its cap; it has no LED of its own.", k.label)),
            Some(k) => (format!("{0}'s own setup. Changes here are for {0} alone; everything else follows all keys.", k.label), format!("{0}'s own setup: changes here are for {0} alone.", k.label)),
        }
    }

    /// How many of the art's units make a millimetre, as the core's light says.
    fn mm(&self) -> f64 {
        Physics::from(&self.light).units_per_mm as f64
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
                        let f = if p.unit == Unit::North { 1.0 - f } else { f };
                        let v = self.at_along(id.max(0) as usize, f as f64);
                        if get(&self.work(), p.path) != Some(v) {
                            self.remember(id);
                            self.put_num(p.path, v);
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
                                    Unit::Mm if p.path == ("switches", "width") => v.clamp(p.lo, p.hi),
                                    Unit::Nits | Unit::Mm => v.max(0.0),
                                    Unit::Reach => v.max(p.lo),
                                    // on the side it is on unless typed below 0 or "south"/"north"
                                    Unit::North => {
                                        let (lo, hi) = self.range(id.max(0) as usize);
                                        let typed = t.to_lowercase();
                                        let south = if typed.contains("south") || num.starts_with('-') { true } else if typed.contains("north") { false } else { self.value(0) < 0.0 };
                                        (v.abs() * if south { -1.0 } else { 1.0 }).clamp(lo, hi)
                                    }
                                    Unit::Number => v.max(p.lo),
                                    // the other way when typed below 0, or west or south
                                    Unit::Place => {
                                        let (lo, hi) = self.range(id.max(0) as usize);
                                        let typed = t.to_lowercase();
                                        let other = num.starts_with('-') || typed.contains("west") || typed.contains("south");
                                        (v.abs() * if other { -1.0 } else { 1.0 }).clamp(lo, hi)
                                    }
                                };
                                if get(&self.work(), p.path) != Some(v) {
                                    self.remember(id);
                                    self.put_num(p.path, v);
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
                    let work = self.work();
                    if choice < CHOICES.len() && chosen(choice, &work) != option {
                        self.remember(CHOICE + choice as i32);
                        let mut after = work;
                        choose(&mut after, choice, option);
                        if self.key().is_some() {
                            let all = key_light(&self.light, &Value::Null);
                            for &path in CHOSEN[choice] {
                                let v = at(&after, path).clone();
                                if v.is_null() || &v == at(&all, path) {
                                    self.unput(path);
                                } else {
                                    self.put(path, v);
                                }
                            }
                        } else {
                            self.light = after;
                        }
                        self.rev += 1;
                        self.status = (String::new(), 0);
                    }
                }
                Ask::Compare(on) => {
                    if on != self.comparing {
                        self.comparing = on;
                        self.status = if on { ("Showing the saved tuning.".into(), 0) } else { (String::new(), 0) };
                    }
                }
                Ask::Follow(id) if self.key().is_some() => {
                    let path = match id {
                        id if (CHOICE..CHOICE + CHOICES.len() as i32).contains(&id) => CHOSEN[(id - CHOICE) as usize][0],
                        id => PARAMS.get(id.max(0) as usize).map_or(("", ""), |p| p.path),
                    };
                    if !path.0.is_empty() && self.owns(path) {
                        self.remember(id);
                        self.unput(path);
                        self.rev += 1;
                        let what = if id >= CHOICE { CHOICES[(id - CHOICE) as usize].label } else { PARAMS[id as usize].label };
                        self.status = (format!("{what} follows all keys again."), 0);
                    }
                }
                Ask::Follow(_) => {}
                // a slider back to its saved value: all keys', or the chosen key's own as saved
                Ask::Reset(id) => {
                    let i = id.max(0) as usize;
                    if let Some(p) = PARAMS.get(i).filter(|_| self.changed(i)) {
                        self.remember(id);
                        let key = self.key().map(|k| k.id.clone());
                        put_back(&mut self.light, &self.saved, key.as_deref(), p.path);
                        self.rev += 1;
                        self.status = match self.key() {
                            Some(k) => (format!("{} back to {}'s saved value.", p.label, k.label), 0),
                            None => (format!("{} back to its saved value.", p.label), 0),
                        };
                    }
                }
                Ask::Revert => {
                    self.remember(-1);
                    self.light = self.saved.clone();
                    self.rev += 1;
                    self.status = ("Back to the core's saved tuning.".into(), 0);
                }
                Ask::Save => self.save(),
                Ask::Section(i) => {
                    let i = (i.max(0) as usize).min(SECTIONS.len() - 1);
                    if i != self.section && self.sections().contains(&i) && self.section_open(i) {
                        self.section = i;
                        self.hovered = None;
                        self.told = None;
                        self.focused = None;
                        crate::uistate::set("lab_section", json!(i));
                    }
                }
                Ask::Hover(id, on) => {
                    if on {
                        // (a slider's reset is over its slider: the slider's hover never covers its reset's)
                        if self.hovered != Some(id + RESET) {
                            self.hovered = Some(id);
                        }
                    } else if self.hovered == Some(id) {
                        // off a slider's reset, the pointer is on its slider still
                        self.hovered = (id >= RESET).then(|| id - RESET);
                    } else if id < RESET && self.hovered == Some(id + RESET) {
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
                Ask::Scope(0) => {
                    self.scope = None;
                    self.picking = false;
                }
                Ask::Scope(_) => self.picking = !self.picking,
                Ask::Key(i) => {
                    if let Some(lit) = self.device.caps.get(i.max(0) as usize).map(|(c, _)| c.lit) {
                        self.scope = Some(i as usize);
                        self.picking = false;
                        // a key's own sections only: a key with no LED has none of its own
                        if !self.section_open(self.section) {
                            self.section = if lit { 0 } else { 1 };
                        }
                    }
                }
                Ask::Pad(fx, fy) => {
                    if let (true, Some(map)) = (self.key().is_some_and(|k| k.lit), self.pad.as_ref().map(|p| p.1)) {
                        let (px, py, pw, ph) = self.pad_box();
                        let (x, y) = (px + fx * pw, py + fy * ph);
                        let (cx, cy) = (px + map.x, py + map.y);
                        let (e_range, n_range) = (self.range(PLACE_E), self.range(PLACE_N));
                        let east = ((((x - cx) / map.k) as f64 * 10.0).round() / 10.0).clamp(e_range.0, e_range.1);
                        let north = ((((cy - y) / map.k) as f64 * 10.0).round() / 10.0).clamp(n_range.0, n_range.1);
                        let work = self.work();
                        if get(&work, PARAMS[PLACE_E].path) != Some(east) || get(&work, PARAMS[PLACE_N].path) != Some(north) {
                            self.remember(PAD);
                            self.put_num(PARAMS[PLACE_E].path, east);
                            self.put_num(PARAMS[PLACE_N].path, north);
                            self.rev += 1;
                            self.status = (String::new(), 0);
                        }
                    }
                }
            }
        }
        self.refresh();
        window
    }

    /// The key whose own setup the page shows, if one is chosen.
    fn key(&self) -> Option<&CapShape> {
        self.scope.and_then(|i| self.device.caps.get(i)).map(|(c, _)| c)
    }

    /// The chosen key's own values, by its control's id, as the light has them.
    fn own<'a>(&self, light: &'a Value) -> &'a Value {
        self.key().map_or(&Value::Null, |k| &light["keys"][k.id.as_str()])
    }

    /// The light the page works on: all keys', or the chosen key's own values over theirs.
    fn work(&self) -> Value {
        self.work_of(&self.light)
    }

    fn work_of(&self, light: &Value) -> Value {
        match self.key() {
            None => light.clone(),
            Some(_) => key_light(light, self.own(light)),
        }
    }

    /// Whether a slider's value is other than the saved one, in what the
    /// page tunes: all keys', or the chosen key's own. Its reset puts it back.
    fn changed(&self, i: usize) -> bool {
        differs(&self.light, &self.saved, self.key().map(|k| k.id.as_str()), PARAMS[i].path)
    }

    /// Whether the chosen key has a value of its own there.
    fn owns(&self, path: (&str, &str)) -> bool {
        !at(self.own(&self.light), path).is_null()
    }

    /// Set a value: the chosen key's own, or all keys'.
    fn put(&mut self, path: (&str, &str), v: Value) {
        match self.key().map(|k| k.id.clone()) {
            Some(id) => own_put(&mut self.light, &id, path, v),
            None => put_at(&mut self.light, path, v),
        }
    }

    fn put_num(&mut self, path: (&str, &str), v: f64) {
        self.put(path, json!((v * 1000.0).round() / 1000.0));
    }

    /// A value of the chosen key's own given back: it takes all keys' again
    /// (and a key with none of its own left is no longer in the light's keys).
    fn unput(&mut self, path: (&str, &str)) {
        if let Some(id) = self.key().map(|k| k.id.clone()) {
            own_take(&mut self.light, &id, path);
        }
    }

    /// Whether a section is the page's to show: all of them for all keys;
    /// for one key, its LED's (if it has one) and its cap's, the wells and the
    /// display being every key's.
    fn section_open(&self, i: usize) -> bool {
        match self.key() {
            None => true,
            Some(k) => i == 1 || (i == 0 && k.lit),
        }
    }

    /// The sliders the section shows: those that tune the light worked on;
    /// for one key, none of what is every key's (the brightness) nor of a
    /// legend it does not have, and its switch's place.
    fn sliders(&self) -> Vec<usize> {
        let mut ids = sliders_of(self.section, &self.work());
        // a button: how high it stands, and what its outside gives back
        if self.key().is_some_and(|k| k.button) {
            ids.retain(|&i| matches!(PARAMS[i].path, ("caps", "foot" | "top" | "outside")));
            return ids;
        }
        if let Some(k) = self.key() {
            ids.retain(|&i| PARAMS[i].path != ("brightness", "") && (k.legend || !matches!(PARAMS[i].path, ("caps", "legends") | ("legends", "unlit"))));
            if self.section == 0 && k.lit {
                ids.extend([PLACE_E, PLACE_N]);
            }
        }
        ids
    }

    /// The section's choices at the top of its left column: a button's kind
    /// is no choice, a moulded block being what it is.
    fn choices(&self) -> &'static [usize] {
        if self.key().is_some_and(|k| k.button) { &[] } else { SECTIONS[self.section].3 }
    }

    /// The chosen key's cap, in millimetres across and north to south.
    fn key_size(&self) -> Option<(f64, f64)> {
        let mm = self.mm();
        self.key().map(|k| ((k.foot[2] - k.foot[0]) as f64 / mm, (k.foot[3] - k.foot[1]) as f64 / mm))
    }

    /// Where the switch pad is on the page: at the top of the right column,
    /// as tall as the key is for its width, within bounds.
    fn pad_box(&self) -> (f32, f32, f32, f32) {
        let (_, _, sw, sh) = self.sizes();
        let (across, deep) = self.key_size().unwrap_or((17.4, 17.4));
        // (shorter in a short window, to leave the column room for its other sliders)
        let most = (sh - 300.0).clamp(110.0, 200.0);
        let h = (36.0 + (drawing::CTRL_W - 32.0) * (deep / across) as f32).clamp(110.0, most);
        (24.0 + drawing::column_x(false, sw), HEAD_H + drawing::choice_y(0) + 30.0, drawing::CTRL_W, h)
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

    /// Write the tuning into the device's core: each value that changed in
    /// its place in the core's file, the rest of the file as it is.
    fn save(&mut self) {
        let path = self.device.path.clone();
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| {
            let out = saved_into(&s, &self.light)?;
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, &out).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
        });
        self.status = match result {
            Ok(()) => {
                self.saved = self.light.clone();
                (format!("Saved into the {} core.", self.device.name), 1)
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
        self.ui.set_status_text(self.status_shown().into());
        self.ui.set_status_kind(self.status.1);
        self.ui.set_status_room(self.status_room());
        let changed = self.light != self.saved;
        self.ui.set_can_save(changed);
        self.ui.set_can_revert(changed);
    }

    /// The window's size, and the stage's in it.
    fn sizes(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (self.ui.get_window_width().max(900.0), self.ui.get_window_height().max(560.0));
        (w, h, w - 48.0, (h - 36.0 - HEAD_H - 10.0 - HELP_H - 4.0 - 64.0).max(300.0))
    }

    /// The slider drawn white: the one being typed in, or under the pointer.
    fn hot(&self) -> Option<usize> {
        // (a slider's reset is its slider's)
        self.focused.or(self.hovered).map(|i| if i >= RESET { i - RESET } else { i }).filter(|&i| (0..READING).contains(&i)).map(|i| i as usize).filter(|i| self.sliders().contains(i))
    }

    /// Place the section's sliders when it opens or the stage changes size,
    /// and draw the stage again when what it shows has changed.
    fn draw_stage(&mut self) {
        let (_, _, sw, sh) = self.sizes();
        let lens = drawing::Lens::of(self.section);
        let work = self.work();
        // the key drawn: the one chosen, as the art has it, with no LED's
        // light if it has no LED of its own; or the device's typical key
        let mut v = drawing::Values::of(&work, self.drawn_key());
        if self.key().is_some_and(|k| k.button) {
            v = v.button();
        } else if self.key().is_some_and(|k| !k.lit) {
            v = v.unlit();
        }
        let size = (self.section, sw.round() as i32, sh.round() as i32, self.sliders(), self.scope);
        if self.places_of.as_ref() != Some(&size) {
            // one lit key's switch pad and its values at the top of the right column
            let right_top = if self.section == 0 && self.key().is_some_and(|k| k.lit) { self.pad_box().3 + 182.0 } else { 0.0 };
            self.places = drawing::places(&v, lens, sw, sh, self.choices().len(), right_top, Some(&size.3));
            self.places_of = Some(size.clone());
        }
        let scale = slint::ComponentHandle::window(&self.ui).scale_factor();
        let hot = self.hot();
        // the switch pad, for one lit key's LED
        let pad_on = self.section == 0 && self.key().is_some_and(|k| k.lit);
        if pad_on {
            let hot_pad = self.hovered == Some(PAD) || matches!(hot, Some(PLACE_E | PLACE_N));
            let of = (work.clone(), self.scope.unwrap_or(0), hot_pad, scale);
            if self.pad_of.as_ref() != Some(&of) {
                let (_, _, pw, ph) = self.pad_box();
                let (st, map) = drawing::from_above(&v, self.key_size().unwrap_or((17.4, 17.4)), pw, ph, hot_pad);
                let image = slint::Image::from_rgba8_premultiplied(drawing::pixels(&st, scale));
                self.pad = Some((st, map, image));
                self.pad_of = Some(of);
            }
        } else {
            self.pad = None;
            self.pad_of = None;
        }
        let of = (work, self.section, size.1, size.2, scale, hot, self.scope);
        if self.drawn_of.as_ref() == Some(&of) {
            return;
        }
        let same_scale = self.drawn_of.as_ref().is_some_and(|o| o.4 == scale);
        // (lines only to the sliders shown: one key shows fewer)
        let shown: Vec<drawing::Place> = self.places.iter().filter(|p| size.3.contains(&p.i)).copied().collect();
        let st = drawing::stage(&v, lens, sw, sh, &shown, hot);
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
        } else if p.unit == Unit::North {
            let edge = Physics::from(&self.work()).switch_width as f64 / 2.0;
            (-edge, edge)
        } else if p.unit == Unit::Place {
            // the switch's housing inside its cap's foot
            let (across, deep) = self.key_size().unwrap_or((17.4, 17.4));
            let half = Physics::from(&self.work()).switch_width as f64 / 2.0;
            let reach = ((if p.path.1 == "east" { across } else { deep }) / 2.0 - half).max(0.0);
            (-reach, reach)
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
        if PARAMS[i].unit == Unit::North { 1.0 - f } else { f }
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
        self.value_in(&self.work(), i)
    }

    fn value_in(&self, light: &Value, i: usize) -> f64 {
        let p = &PARAMS[i];
        let v = get(light, p.path).unwrap_or(p.def);
        if p.unit == Unit::North {
            let edge = Physics::from(light).switch_width as f64 / 2.0;
            v.clamp(-edge, edge)
        } else {
            v
        }
    }

    /// What the line under the drawing tells of, and what it says.
    fn told(&self) -> (String, String) {
        let r = self.readout;
        match self.focused.or(self.told) {
            Some(i) if (0..READING).contains(&i) && self.sliders().contains(&(i as usize)) => {
                let p = &PARAMS[i as usize];
                match self.key().filter(|_| self.owns(p.path)) {
                    Some(k) => (p.label.into(), format!("{} This is {}'s own value; the dot before the name gives the value back to all keys.", p.note, k.label)),
                    None => (p.label.into(), p.note.into()),
                }
            }
            Some(i) if (RESET..RESET + PARAMS.len() as i32).contains(&i) => {
                let p = &PARAMS[(i - RESET) as usize];
                let was = at(tuned(&self.saved, self.key().map(|k| k.id.as_str())), p.path).as_f64();
                let back = match (self.key(), was) {
                    (Some(k), Some(v)) => format!("Back to {}, {}'s saved value.", said(p, v), k.label),
                    (Some(_), None) => "Back to all keys' value, as saved.".into(),
                    (None, v) => format!("Back to {}, the saved value.", said(p, v.unwrap_or(p.def))),
                };
                (p.label.into(), back)
            }
            Some(i) if i == PAD => ("Switch's place".into(), PAD_NOTE.into()),
            Some(i) if (CHOICE..CHOICE + CHOICES.len() as i32).contains(&i) => (CHOICES[(i - CHOICE) as usize].label.into(), CHOICES[(i - CHOICE) as usize].note.into()),
            Some(i) if i == READING => ("Brightest legend".into(), self.legend_note().unwrap_or_else(|| READOUT_ABOUT.into())),
            Some(i) if i == READING + 1 => ("Brightest deck".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 2 => ("Brightest skirt".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 3 => ("Brightest chassis".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 4 => ("Display's lit dots".into(), READOUT_ABOUT.into()),
            Some(i) if i == READING + 5 => ("The desktop's white".into(), WHITE_ABOUT.into()),
            Some(i) if i == READING + 6 => ("The simulation".into(), if r.photons == 0 { SIM_ABOUT.into() } else { format!("{} photons from each LED so far. {SIM_ABOUT}", text::thousands(r.photons as u64)) }),
            _ if self.key().is_some_and(|k| k.button) => (SECTIONS[self.section].0.into(), BUTTON_ABOUT.into()),
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
        let mut cells = vec![text_cell(style::TITLE, 24.0, 14.0, 300.0, 29.0, "Light lab")];
        let mut bx = 0.0;
        let mut tabs = Vec::new();
        let open = self.sections();
        for (i, (title, _, _, _)) in SECTIONS.iter().enumerate().filter(|(i, _)| open.contains(i)) {
            let bw = text::width(400, 13.0, title) + 24.0;
            tabs.push((i, title, bx, bw));
            bx += bw + 2.0;
        }
        let tabs_w = bx - 2.0 + 6.0;
        let tx = w - 24.0 - tabs_w;
        // the keys tuned, before the sections: all of them, or one, which opens the map to choose it
        let one = self.key().map_or_else(|| "One key".to_string(), |k| k.label.clone());
        let (aw, kw) = (text::width(400, 13.0, "All keys") + 24.0, text::width(400, 13.0, &one) + 24.0 + 16.0);
        let scope_w = aw + 2.0 + kw + 6.0;
        let qx = tx - 12.0 - scope_w;
        // what the page is, or whose own setup it shows; shorter where the head is narrow
        let room = (qx - 48.0).max(200.0);
        let (full, brief) = self.intro();
        let intro = if text::width(400, 12.5, &full) <= room { full } else { brief };
        cells.push(text_cell(style::NOTE, 24.0, 44.0, room, 18.0, &intro));
        cells.push(cell(kind::SEG, qx, 17.0, scope_w, 36.0));
        cells.push(Cell { text: "All keys".into(), on: self.scope.is_none() && !self.picking, id: SCOPE, index: 0, ..cell(kind::SEG_BUTTON, qx + 3.0, 20.0, aw, 30.0) });
        cells.push(Cell { text: one.into(), on: self.scope.is_some() || self.picking, id: SCOPE, index: 1, style: 24, ..cell(kind::SEG_BUTTON, qx + 3.0 + aw + 2.0, 20.0, kw, 30.0) });
        cells.push(cell(kind::SEG, tx, 17.0, tabs_w, 36.0));
        for (i, title, x, bw) in tabs {
            // (for one key, the sections that are every key's are not to be chosen)
            let style = if self.section_open(i) { 0 } else { 26 };
            cells.push(Cell { text: (*title).into(), on: i == self.section, id: TABS, index: i as i32, style, ..cell(kind::SEG_BUTTON, tx + 3.0 + x, 20.0, bw, 30.0) });
        }
        let work = self.work();
        let saved_work = self.work_of(&self.saved);
        // a value of the chosen key's own: a mark before its name, which gives it back to all keys
        let own_mark = |over: &mut Vec<Cell>, x: f32, y: f32, index: i32| {
            over.push(Cell { style: 3, ..cell(kind::DOT, x - 19.0, y + 9.5, 7.0, 7.0) });
            over.push(Cell { id: OWN, index, ..cell(kind::LINK, x - 25.0, y + 4.0, 19.0, 18.0) });
        };
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
        for (k, &c) in self.choices().iter().enumerate() {
            let choice = &CHOICES[c];
            let y = sy + drawing::choice_y(k);
            let id = CHOICE + c as i32;
            over.push(Cell { id, ..cell(kind::ZONE, cx - 10.0, y - 7.0, drawing::CTRL_W + 20.0, drawing::CHOICE_H) });
            over.push(text_cell(style::LABEL, cx, y, drawing::CTRL_W, 26.0, choice.label));
            // its options as wide as their words, the box round them as wide as they are
            let widths: Vec<f32> = choice.options.iter().map(|name| text::width(400, 13.0, name) + 20.0).collect();
            let seg_w = widths.iter().sum::<f32>() + 2.0 * (widths.len() as f32 - 1.0) + 6.0;
            over.push(cell(kind::SEG, cx, y + 30.0, seg_w, 34.0));
            if self.owns(CHOSEN[c][0]) {
                own_mark(&mut over, cx, y, CHOICE + c as i32);
            }
            let on = chosen(c, &work);
            let mut bx = cx + 3.0;
            for (j, (name, bw)) in choice.options.iter().zip(widths).enumerate() {
                over.push(Cell { text: (*name).into(), on: j == on, id, index: j as i32, style: style::TELLS, ..cell(kind::SEG_BUTTON, bx, y + 32.0, bw, 30.0) });
                bx += bw + 2.0;
            }
        }
        // the sliders, each where the section placed it
        let hot = self.hot();
        let mut rows = Vec::new();
        let ids = self.sliders();
        for place in self.places.iter().filter(|_| !self.picking) {
            let i = place.i;
            if !ids.contains(&i) {
                continue;
            }
            let p = &PARAMS[i];
            let (number, mut unit) = reads(p, self.value(i));
            let mut derived = false;
            if let Some((_, extra)) = self.drawn.as_ref().and_then(|(st, _)| st.extras.iter().find(|e| e.0 == i)) {
                unit = extra.clone();
                derived = true;
            }
            let (at, was) = (self.knob(i, self.value(i)), self.knob(i, self.value_in(&saved_work, i)));
            let (rx, ry) = (sx + drawing::column_x(place.left, sw), sy + place.y - drawing::ATTACH);
            if self.owns(p.path) {
                own_mark(&mut over, rx, ry, i as i32);
            }
            rows.push(LabRow {
                x: rx,
                y: ry,
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
                changed: self.changed(i),
            });
        }
        // one lit key's switch's place: the key from above, its switch to drag
        // where it sits, and how far that is from the cap's middle each way
        if let (false, Some((st, _, image))) = (self.picking, &self.pad) {
            let (px, py, pw, ph) = self.pad_box();
            over.push(text_cell(style::LABEL, px, py - 30.0, pw, 26.0, "Switch's place"));
            sketches.push(Sketch { x: px, y: py, w: st.w, h: st.h, image: image.clone() });
            for word in &st.words {
                let top = py + word.y - 1.005 * 11.5 - (16.0 - 1.3 * 11.5) / 2.0;
                over.push(in_colour(style::SMALL_IN, px + word.x, top, text::width(400, 11.5, &word.text) + 2.0, 16.0, &word.text, word.colour));
            }
            over.push(Cell { id: PAD, ..cell(18, px, py, pw, ph) });
            if self.owns(PARAMS[PLACE_E].path) || self.owns(PARAMS[PLACE_N].path) {
                own_mark(&mut over, px, py - 30.0, PLACE_E as i32);
            }
            for (n, i) in [PLACE_E, PLACE_N].into_iter().enumerate() {
                let p = &PARAMS[i];
                let (number, unit) = reads(p, self.value(i));
                let (at, was) = (self.knob(i, self.value(i)), self.knob(i, self.value_in(&saved_work, i)));
                rows.push(LabRow { x: px, y: py + ph + 18.0 + n as f32 * 64.0, w: pw, id: i as i32, name: p.label.into(), hot: hot == Some(i), number: number.into(), unit: unit.into(), derived: false, at: at as f32, was: was as f32, moved: (at - was).abs() > 0.002, changed: self.changed(i) });
            }
        }
        // the map, open to choose a key: every cap where the art has it, the
        // chosen one lit, those with a setup of their own marked
        if self.picking && !self.device.caps.is_empty() {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (c, _) in &self.device.caps {
                (x0, y0, x1, y1) = (x0.min(c.foot[0]), y0.min(c.foot[1]), x1.max(c.foot[2]), y1.max(c.foot[3]));
            }
            let (mx, my, mw) = (24.0, HEAD_H + 4.0, w - 48.0);
            let k = (mw - 48.0) / (x1 - x0);
            let map_h = (y1 - y0) * k;
            // the page behind it dimmed, the map raised over it
            over.push(Cell { style: 28, ..cell(kind::TILE, sx, sy, sw, sh) });
            over.push(Cell { style: 27, ..cell(kind::TILE, mx, my, mw, map_h + 92.0) });
            over.push(text_cell(style::LABEL, mx + 24.0, my + 18.0, 200.0, 26.0, "Choose a key"));
            let note = "Changes you make to a chosen key are for it alone. A dot marks a key with its own setup.";
            over.push(text_cell(style::NOTE, mx + 24.0 + text::width(400, 13.5, "Choose a key") + 14.0, my + 22.0, mw - 200.0, 18.0, note));
            for (i, (c, _)) in self.device.caps.iter().enumerate() {
                let (mut kx, mut ky) = (mx + 24.0 + (c.foot[0] - x0) * k, my + 64.0 + (c.foot[1] - y0) * k);
                let (mut kw, mut kh) = ((c.foot[2] - c.foot[0]) * k, (c.foot[3] - c.foot[1]) * k);
                let name = c.short.clone().unwrap_or_else(|| short(&c.label));
                // a button smaller than a key: its box round its middle, as big as
                // its name needs, with room above it for the dot
                if c.button {
                    let (bw, bh) = (kw.max(text::width(400, 10.5, &name) + 24.0), kh.max(32.0));
                    (kx, ky, kw, kh) = (kx + (kw - bw) / 2.0, ky + (kh - bh) / 2.0, bw, bh);
                }
                over.push(Cell { text: name.into(), on: self.scope == Some(i), id: MAP, index: i as i32, style: 25, ..cell(kind::GHOST, kx + 1.5, ky + 1.5, kw - 3.0, kh - 3.0) });
                if self.light["keys"][c.id.as_str()].is_object() {
                    over.push(Cell { style: 3, ..cell(kind::DOT, kx + kw - 10.0, ky + 4.5, 6.0, 6.0) });
                }
            }
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
    /// of photons, then without its heading (both still told under the
    /// pointer), then without the desktop's white, then without the display.
    /// What was last done takes no more than the room left beside the
    /// shortest, its words wrapping past it.
    fn readout_line(&self) -> Vec<Cell> {
        let status = self.status_shown();
        let room = self.foot_room() - if status.is_empty() { 0.0 } else { (text::width(400, 12.5, status) + 16.0).min(self.status_room() + 16.0) };
        for level in 0..5 {
            let (cells, end) = self.readout_at(level);
            if end - 24.0 <= room || level == 4 {
                return cells;
            }
        }
        unreachable!()
    }

    /// What the foot says was last done: "Changed, not saved." if nothing else.
    fn status_shown(&self) -> &str {
        if self.status.0.is_empty() && self.light != self.saved { "Changed, not saved." } else { self.status.0.as_str() }
    }

    /// The foot's room for its readout and what was last done, beside its buttons.
    fn foot_room(&self) -> f32 {
        let (w, _, _, _) = self.sizes();
        w - 24.0 - 176.0 - 10.0 - 68.0 - 10.0 - 132.0 - 10.0 - 24.0
    }

    /// The most room what was last done takes: what the shortest readout leaves.
    fn status_room(&self) -> f32 {
        self.foot_room() - (self.readout_at(4).1 - 24.0) - 16.0
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
            if k == 4 && level >= 4 {
                continue;
            }
            let c = if k == 0 && held { [0xf0, 0xb3, 0x5a] } else { value_colour };
            item(&mut out, &mut x, READING + k as i32, label, value, c);
        }
        let sep = |out: &mut Vec<Cell>, x: &mut f32| {
            *x += 2.0;
            out.push(Cell { color: colour([0x3a, 0x3a, 0x3a]), ..cell(kind::BOX, *x, 25.0, 1.0, 14.0) });
            *x += 15.0;
        };
        sep(&mut out, &mut x);
        if level < 3 {
            item(&mut out, &mut x, READING + 5, "Desktop white", &format!("{} nits", nits(r.white)), value_colour);
            sep(&mut out, &mut x);
        }
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

/// A core's file with a light saved into it: each of its values that
/// differs from the file's set in its place, each new one added, each the
/// light no longer has taken out, and the rest of the file as it was. A light
/// whose keys have none of their own left leaves the file's keys empty, not
/// gone, so what the file says of them stays.
fn saved_into(text: &str, light: &Value) -> Result<String, String> {
    let mut light = light.clone();
    if light.get("keys").is_none() && jsonc::parse(text)?["light"].get("keys").is_some() {
        light["keys"] = json!({});
    }
    let out = jsonc::update(text, &["light"], &light)?;
    // only a file that reads back as the tuning is written
    if jsonc::update(&out, &["light"], &light)? != out {
        return Err("the tuning would not read back".into());
    }
    Ok(out)
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
        let mut light = json!({ "brightness": 600.0, "switches": { "led": 3.3 }, "caps": { "legends": 0.3 } });
        for (option, switch) in [(1, Switch::ClearTop), (2, Switch::Clear), (0, Switch::Slot)] {
            choose(&mut light, 0, option);
            assert_eq!(chosen(0, &light), option);
            assert_eq!(Physics::from(&light).switch, switch);
        }
        assert_eq!(light["switches"]["kind"], json!("led-slot"));
        assert_eq!(get(&light, ("switches", "top")), Some(5.7));
        assert_eq!(get(&light, ("switches", "beam")), Some(3.0));
        let shown = |light: &Value, section| sliders_of(section, light).iter().map(|&i| PARAMS[i].path.1).collect::<Vec<_>>();
        assert!(shown(&light, 0).iter().all(|k| !matches!(*k, "scattered" | "sides" | "glow")), "a black housing has no glow to tune");
        choose(&mut light, 0, 1);
        assert_eq!(sliders_of(0, &light).len(), SECTIONS[0].2.len(), "a clear one shows them all");
        // the caps
        assert_eq!(chosen(1, &light), 0, "no plastic letting light through: shine-through");
        assert!(shown(&light, 1).iter().all(|k| !matches!(*k, "plastic" | "milky" | "black")), "shine-through caps have none of those");
        choose(&mut light, 1, 1);
        assert_eq!((chosen(1, &light), get(&light, ("caps", "plastic")), get(&light, ("caps", "black"))), (1, Some(0.3), Some(0.4)), "pudding: the plastic lets through what a legend does, below the black");
        assert!(["plastic", "milky", "black"].iter().all(|k| shown(&light, 1).contains(k)));
        set(&mut light, ("caps", "plastic"), 0.0);
        assert_eq!(chosen(1, &light), 1, "pudding caps stay pudding with their plastic dragged to nothing");
        set(&mut light, ("caps", "plastic"), 0.45);
        set(&mut light, ("caps", "black"), 0.3);
        choose(&mut light, 1, 2);
        let p = Physics::from(&light);
        assert_eq!((p.caps, p.skirts, p.tops, p.black), (Caps::Translucent, 0.45, 0.45, 0.0), "a translucent top lets through what the plastic does");
        assert!(!shown(&light, 1).contains(&"black"), "translucent caps have no black");
        choose(&mut light, 1, 0);
        assert_eq!((light["caps"]["kind"].clone(), Physics::from(&light).skirts), (json!("shine-through"), 0.0));
        choose(&mut light, 1, 1);
        assert_eq!((get(&light, ("caps", "plastic")), get(&light, ("caps", "black"))), (Some(0.45), Some(0.3)), "back to pudding as it was tuned");
        // the LED's place in millimetres, and its side
        assert_eq!(reads(&PARAMS[0], 5.08), ("5.08".into(), "mm north".into()));
        assert_eq!(reads(&PARAMS[0], -2.0), ("2".into(), "mm south".into()));
        assert_eq!(Physics::from(&json!({ "scale": 1.38, "switches": { "led": 5.08 } })).led, (5.08f64 * 1.38) as f32, "in the art's units, as the simulation has it");
    }

    /// A key's own values go under the light's "keys" by its control's id,
    /// over all keys' for that key alone; and given back, they leave the
    /// light as it was.
    #[test]
    fn a_key_s_own_values_come_and_go() {
        let light = json!({ "brightness": 600.0, "switches": { "led": 3.3 }, "caps": { "kind": "pudding", "black": 0.4 } });
        let mut l = light.clone();
        own_put(&mut l, "space", ("caps", "kind"), json!("shine-through"));
        own_put(&mut l, "enter", ("place", "north"), json!(9.5));
        own_put(&mut l, "enter", ("switches", "led"), json!(-3.0));
        assert_eq!(l["keys"], json!({ "space": { "caps": { "kind": "shine-through" } }, "enter": { "place": { "north": 9.5 }, "switches": { "led": -3.0 } } }));
        let enter = key_light(&l, &l["keys"]["enter"]);
        assert_eq!((enter["switches"]["led"].clone(), enter["place"]["north"].clone(), enter["caps"]["kind"].clone(), enter["keys"].clone()), (json!(-3.0), json!(9.5), json!("pudding"), Value::Null));
        assert_eq!(key_light(&l, &l["keys"]["space"])["caps"], json!({ "kind": "shine-through", "black": 0.4 }));
        own_take(&mut l, "enter", ("place", "north"));
        assert_eq!(l["keys"]["enter"], json!({ "switches": { "led": -3.0 } }), "what it does not give back stays its own");
        own_take(&mut l, "enter", ("switches", "led"));
        own_take(&mut l, "space", ("caps", "kind"));
        assert_eq!(l, light, "nothing of the keys' own left");
    }

    /// A slider's reset puts back the saved value in what the page tunes:
    /// all keys' (a value set since the save taken out again), or one key's
    /// own (a value of its own set since the save given back, so the key
    /// follows all keys); a change to all keys is no change to a key's own.
    /// And the line under the drawing says the value it goes back to.
    #[test]
    fn a_reset_puts_back_what_was_saved() {
        let saved = json!({ "switches": { "beam": 3.0 }, "caps": { "top": 14.0 }, "keys": { "gpc": { "caps": { "top": 3.0 } } } });
        let mut light = saved.clone();
        set(&mut light, ("switches", "beam"), 27.0);
        set(&mut light, ("switches", "glow"), 6.0);
        own_put(&mut light, "gpc", ("caps", "top"), json!(5.0));
        own_put(&mut light, "enter", ("place", "north"), json!(-3.0));
        let (beam, glow, top, north) = (("switches", "beam"), ("switches", "glow"), ("caps", "top"), ("place", "north"));
        assert!(differs(&light, &saved, None, beam) && differs(&light, &saved, None, glow) && !differs(&light, &saved, None, top));
        assert!(!differs(&light, &saved, Some("gpc"), beam), "all keys' beam is no change to G/PC's own");
        put_back(&mut light, &saved, None, beam);
        put_back(&mut light, &saved, None, glow);
        assert_eq!(light["switches"], saved["switches"], "back as saved, the value set since taken out");
        assert!(differs(&light, &saved, Some("gpc"), top) && differs(&light, &saved, Some("enter"), north));
        put_back(&mut light, &saved, Some("gpc"), top);
        assert_eq!(light["keys"]["gpc"], json!({ "caps": { "top": 3.0 } }), "G/PC's own, as saved");
        put_back(&mut light, &saved, Some("enter"), north);
        assert_eq!(light, saved, "Enter had none of its own: it follows all keys again");
        let p = |label: &str| PARAMS.iter().find(|p| p.label == label).unwrap();
        assert_eq!(said(p("Beam"), 3.0), "3");
        assert_eq!(said(p("North of centre"), 5.08), "5.08 mm north");
        assert_eq!(said(p("Legends let through"), 0.3), "30%");
        assert_eq!(said(p("Cap's top"), 3.0), "3 mm");
        assert_eq!(said(p("Switch north–south"), 0.0), "centred");
        assert_eq!(said(p("Brightness"), 1000.0), "1000 nits");
    }

    /// Save changes the core's file only where the tuning differs from it,
    /// each value in its place, every comment and line round it kept; a key's
    /// own values go in by its id, and given back they leave the file as it was.
    #[test]
    fn the_light_saves_in_its_place() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10/device.json")).unwrap();
        let light = floats(jsonc::parse(&text).unwrap()["light"].clone());
        assert_eq!(saved_into(&text, &light).unwrap(), text, "the tuning as the file has it: nothing changes");
        let mut tuned = light.clone();
        set(&mut tuned, ("caps", "top"), 12.5);
        set(&mut tuned, ("switches", "led"), -2.0);
        own_put(&mut tuned, "enter", ("place", "north"), json!(-3.0));
        let out = saved_into(&text, &tuned).unwrap();
        let back = floats(jsonc::parse(&out).unwrap()["light"].clone());
        assert_eq!(back, tuned, "it reads back as tuned");
        let added: Vec<&str> = out.lines().filter(|l| !text.lines().any(|t| t == *l)).collect();
        let gone: Vec<&str> = text.lines().filter(|l| !out.lines().any(|o| o == *l)).collect();
        assert_eq!((added.len(), gone.len()), (4, 3), "the two values' lines, the last key's with a comma after it, and the new key's: {added:#?} {gone:#?}");
        assert!(out.contains(r#""top": 14,"#.replace("14", "12.5").as_str()) && out.contains(r#""led": -2,           // the LED"#), "{added:#?}");
        assert!(out.contains(r#""enter": { "place": { "north": -3 } }"#), "a key's own, on a line of its own");
        assert_eq!(out.lines().count(), text.lines().count() + 1);
        // and back as it was
        assert_eq!(saved_into(&out, &light).unwrap(), text);
        // a key's own given back and none left: its keys empty, what the file says of them kept
        let mut none = light.clone();
        none.as_object_mut().unwrap().remove("keys");
        let out = saved_into(&text, &none).unwrap();
        assert!(out.contains("    \"keys\": {}\n  },") && out.contains("    // keys set apart from the rest"), "{out}");
    }
}
