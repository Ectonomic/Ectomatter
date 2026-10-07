//! Legends set anew from each key's print: the words its cap has printed on
//! it, as the core gives them, set in a typeface, as tall and where on the
//! cap a light's "legends" say (the Light lab's Legends section). They are
//! laid out as the Z10's caps are printed: a key's own legend at the top of
//! its cap, its shifted sign beside it, words of more than one line one under
//! the other, its second function at its foot; a legend the core prints in
//! the middle of its cap stays there. With the core's own typeface, sizes
//! and place, the art's own legends are drawn instead (see hdr::Board).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use resvg::tiny_skia::{Path, PathBuilder, Transform};
use resvg::usvg::{self, fontdb};
use rustybuzz::ttf_parser;
use serde_json::Value;

/// How far apart a key's own legend and its shifted sign stand, in its
/// font size.
const SHIFT_GAP: f32 = 0.261;
/// Lines of words one under the other: from one baseline to the next, in
/// their font size.
const PITCH: f32 = 1.032;
/// Words start a little higher than letters: this share of the inset down.
const WORD_INSET: f32 = 0.852;
/// A second function's baseline above the foot of the cap's top: so much
/// (mm), and so much of its capitals' height.
const FOOT: (f32, f32) = (1.050, 0.311);
/// A legend in the middle of its cap sits this share of its first line's
/// capitals above the middle.
const CENTRE_RISE: f32 = 0.166;
/// A sign printed larger than its kind sits lower on its line, by this share
/// of its kind's capitals.
const MARK_DROP: f32 = 0.0729;
/// The room a legend keeps on either side of its cap's top (mm): a longer
/// one is set smaller to fit, a key's words all alike.
const FIT: f32 = 0.45;
/// The share of capitals' height to font size where a typeface does not say.
const CAP: f32 = 0.7;
/// Ectodeck's own typefaces for a keyboard's glyphs (⌘ ⌥ ⇧ ⎋ ⏻ and the rest
/// of the Unicode blocks they come from), cut from DejaVu Sans Bold, Noto Sans
/// Symbols and Noto Sans Symbols 2 (fonts/README.md), so a legend has them on
/// a computer that has none of those; and their families.
pub const GLYPH_FONTS: [&[u8]; 3] = [
    include_bytes!("../fonts/KeyGlyphs-Bold.ttf"),
    include_bytes!("../fonts/KeyGlyphsSymbols-Regular.ttf"),
    include_bytes!("../fonts/KeyGlyphsSymbols2-Regular.ttf"),
];
const GLYPH_FAMILIES: [(&str, u16); 3] = [("Ectodeck Key Glyphs", 700), ("Ectodeck Key Glyphs Symbols", 400), ("Ectodeck Key Glyphs Symbols 2", 400)];
/// Typefaces for the signs a legend's typeface lacks, after the core's own
/// symbols: Ectodeck's, then this computer's that have more of them, each at
/// the weight nearest the one given.
const FALLBACKS: [(&str, u16); 4] = [("DejaVu Sans", 700), ("Noto Sans Symbols", 700), ("Noto Sans Symbols 2", 400), ("Noto Sans Math", 400)];
/// A sign from those, as tall as so many of the capitals beside it (one
/// that lies flat as wide as a quarter as much again), and the room between two.
const SIGN: f32 = 1.1;
const SIGN_GAP: f32 = 0.15;
/// A key's own text for the parts of its print, as a light's "legends" for
/// the key give them, and the part each is.
pub const TEXTS: [(&str, Role); 3] = [("main", Role::Main), ("shift", Role::Shift), ("second", Role::Second)];

/// Which part of a key's legend a run of its print is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// the key's own legend
    Main,
    /// its shifted sign, beside it
    Shift,
    /// its second function, at the foot of the cap
    Second,
}

impl Role {
    /// How strongly the part is printed: a shifted sign a touch lighter
    /// than the key's own legend, a second function lighter still.
    fn opacity(self) -> f32 {
        match self {
            Role::Main => 1.0,
            Role::Shift => 0.95,
            Role::Second => 0.72,
        }
    }
}

/// A run of a key's print: its words, or the name of one of the core's
/// icons; its part; and how much larger than the rest of its kind the cap
/// prints it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub icon: Option<String>,
    pub role: Role,
    pub scale: f32,
}

impl Run {
    /// A run of one letter or sign, set at the letters' size; words, and
    /// icons that are second functions, at the words'.
    fn letter(&self) -> bool {
        match &self.icon {
            Some(_) => self.role != Role::Second,
            None => self.text.chars().filter(|c| !c.is_whitespace()).count() == 1,
        }
    }
}

/// What a key's cap has printed on it, and whether in the middle of the cap
/// rather than at its top.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Print {
    pub runs: Vec<Run>,
    pub centred: bool,
}

impl Print {
    /// A control's print, as the core gives it: none for a cap printed with nothing.
    pub fn of(control: &Value) -> Option<Print> {
        let runs: Vec<Run> = control["print"]
            .as_array()?
            .iter()
            .filter_map(|r| {
                let role = match r["role"].as_str() {
                    Some("shift") => Role::Shift,
                    Some("second") => Role::Second,
                    _ => Role::Main,
                };
                let icon = r["icon"].as_str().map(str::to_string);
                let text = r["text"].as_str().unwrap_or("").to_string();
                (icon.is_some() || !text.trim().is_empty()).then(|| Run { text, icon, role, scale: r["scale"].as_f64().unwrap_or(1.0) as f32 })
            })
            .collect();
        (!runs.is_empty()).then(|| Print { runs, centred: control["centred"].as_bool() == Some(true) })
    }

    /// The print with a key's own text in place of its parts, as a light's
    /// "legends" for the key give it (see TEXTS): its own legend's lines one
    /// under the other at each line break, "" for a part printed with nothing.
    pub fn with_text(&self, legends: &Value) -> Print {
        if TEXTS.iter().all(|(k, _)| !legends[*k].is_string()) {
            return self.clone();
        }
        let mut runs = Vec::new();
        for (k, role) in TEXTS {
            match legends[k].as_str() {
                Some(text) if role == Role::Main => runs.extend(text.split('\n').filter(|l| !l.trim().is_empty()).map(|l| Run { text: l.trim().to_string(), icon: None, role, scale: 1.0 })),
                Some(text) => runs.extend(Some(text.replace('\n', " ")).filter(|t| !t.trim().is_empty()).map(|t| Run { text: t.trim().to_string(), icon: None, role, scale: 1.0 })),
                None => runs.extend(self.runs.iter().filter(|r| r.role == role).cloned()),
            }
        }
        Print { runs, centred: self.centred }
    }

    /// A part of the print as words: its runs', a line each (an icon by its
    /// name in brackets).
    pub fn text_of(&self, role: Role) -> String {
        self.runs.iter().filter(|r| r.role == role).map(|r| r.icon.as_ref().map_or_else(|| r.text.clone(), |i| format!("[{i}]"))).collect::<Vec<_>>().join("\n")
    }
}

/// Where on its cap a legend sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// where the keyboard prints each legend: at the top, its second
    /// function at the foot, and in the middle on the keys the core prints so
    Printed,
    /// at the top on every key, its second function at the foot
    Top,
    /// in the middle, as many pudding and doubleshot caps print them
    Middle,
    /// at the top left, as classic caps print them
    TopLeft,
}

impl Place {
    pub fn named(name: Option<&str>) -> Place {
        match name {
            Some("middle") => Place::Middle,
            Some("topleft") => Place::TopLeft,
            Some("top") => Place::Top,
            _ => Place::Printed,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Place::Printed => "printed",
            Place::Top => "top",
            Place::Middle => "middle",
            Place::TopLeft => "topleft",
        }
    }
}

/// How legends are set: their typeface and its weight; where on the cap;
/// how tall a legend of one letter or sign is, and a legend of words (a
/// capital's height, mm); and how far down from the top edge of a cap's
/// top a one-letter legend starts (mm), and in from its left at the top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub typeface: String,
    pub weight: u16,
    pub place: Place,
    pub letters: f32,
    pub words: f32,
    pub inset: f32,
    /// how far the whole legend is moved from where its place puts it (mm),
    /// right and down (left and up below 0)
    pub across: f32,
    pub down: f32,
}

impl Style {
    /// The style `legends` (a light's "legends", or a key's own) asks for,
    /// `under`'s where it says nothing.
    pub fn over(under: &Style, legends: &Value) -> Style {
        let f = |k: &str, d: f32| legends[k].as_f64().map_or(d, |v| v as f32);
        Style {
            typeface: legends["typeface"].as_str().map_or_else(|| under.typeface.clone(), str::to_string),
            weight: legends["weight"].as_f64().map_or(under.weight, |w| w.round() as u16),
            place: if legends["place"].is_string() { Place::named(legends["place"].as_str()) } else { under.place },
            letters: f("letters", under.letters).max(0.2),
            words: f("words", under.words).max(0.2),
            inset: f("inset", under.inset).max(0.0),
            across: f("across", under.across),
            down: f("down", under.down),
        }
    }
}

/// One of the core's icons: drawn on a 24-unit square, filled and stroked
/// (`width` the stroke's), `size` times as tall as its part's capitals would
/// be and `drop` times them below its line.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub fill: String,
    pub stroke: String,
    pub width: f32,
    pub size: f32,
    pub drop: f32,
}

/// How the core's art prints its legends: its style, the typeface for the
/// signs it lacks, its typefaces' files (from its folder), and its icons.
#[derive(Clone, Debug, PartialEq)]
pub struct Own {
    pub style: Style,
    pub symbols: String,
    pub fonts: Vec<String>,
    pub icons: HashMap<String, Icon>,
}

impl Own {
    /// The core's own print, as its device description gives it.
    pub fn of(device: &Value) -> Option<Own> {
        let p = &device["print"];
        let f = |k: &str| p[k].as_f64().map(|v| v as f32);
        let style = Style { typeface: p["typeface"].as_str()?.to_string(), weight: p["weight"].as_f64().map_or(400, |w| w.round() as u16), place: Place::Printed, letters: f("letters")?, words: f("words")?, inset: f("inset")?, across: 0.0, down: 0.0 };
        let icons = p["icons"]
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(name, i)| {
                        let g = |k: &str, d: f32| i[k].as_f64().map_or(d, |v| v as f32);
                        (name.clone(), Icon { fill: i["fill"].as_str().unwrap_or("").into(), stroke: i["stroke"].as_str().unwrap_or("").into(), width: g("width", 2.0), size: g("size", 1.0), drop: g("drop", 0.0) })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fonts = p["fonts"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        Some(Own { style, symbols: p["symbols"].as_str().unwrap_or("").to_string(), fonts, icons })
    }
}

// ---------------------------------------------------------------- typefaces

static FACES: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
static FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Add typefaces' files (a core's own) to those legends can be set in,
/// before they are first loaded.
pub fn add_fonts(files: impl IntoIterator<Item = PathBuf>) {
    if let Ok(mut f) = FILES.lock() {
        f.extend(files);
    }
}

/// The typefaces legends can be set in: this computer's, the cores' added
/// before, and Ectodeck's own for a keyboard's glyphs, loaded once (the
/// first to ask waits for them).
pub fn faces() -> Arc<fontdb::Database> {
    FACES
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            for f in FILES.lock().map(|f| f.clone()).unwrap_or_default() {
                if let Err(e) = db.load_font_file(&f) {
                    eprintln!("ectodeck-draw: the typeface {}: {e}", f.display());
                }
            }
            add_glyph_fonts(&mut db);
            Arc::new(db)
        })
        .clone()
}

/// Ectodeck's own typefaces for a keyboard's glyphs, added to these.
pub fn add_glyph_fonts(db: &mut fontdb::Database) {
    for data in GLYPH_FONTS {
        db.load_font_source(fontdb::Source::Binary(Arc::new(data)));
    }
}

/// Load the typefaces in the background, so they are ready when asked for.
pub fn warm() {
    std::thread::spawn(|| {
        let _ = faces();
    });
}

/// The typefaces there are, each its family's name and the weights it has
/// upright, A to Z; not Ectodeck's own for glyphs, which have no letters.
pub fn families(db: &fontdb::Database) -> Vec<(String, Vec<u16>)> {
    let mut by: std::collections::BTreeMap<String, Vec<u16>> = std::collections::BTreeMap::new();
    for f in db.faces().filter(|f| f.style == fontdb::Style::Normal) {
        if let Some((name, _)) = f.families.first().filter(|(name, _)| !GLYPH_FAMILIES.iter().any(|(g, _)| g == name)) {
            let w = by.entry(name.clone()).or_default();
            if !w.contains(&f.weight.0) {
                w.push(f.weight.0);
            }
        }
    }
    by.into_iter()
        .map(|(n, mut w)| {
            w.sort_unstable();
            (n, w)
        })
        .collect()
}

/// A typeface's face, by its family's name: the weight it has nearest the one asked.
fn query(db: &fontdb::Database, family: &str, weight: u16) -> Option<fontdb::ID> {
    db.query(&fontdb::Query { families: &[fontdb::Family::Name(family)], weight: fontdb::Weight(weight), stretch: fontdb::Stretch::Normal, style: fontdb::Style::Normal })
}

// ------------------------------------------------------------------- layout

/// What a legend is drawn with: a path (in its own units), how it goes onto
/// the art (to its units), how strongly it is printed, and the width of its
/// stroke for a path drawn as a line (in its own units); which part it is.
#[derive(Clone, Debug)]
pub struct Ink {
    pub path: Path,
    pub to_art: Transform,
    pub opacity: f32,
    pub stroke: Option<f32>,
    pub role: Role,
}

/// A line of a legend set: which part it is, whether a letter's (else
/// words'), its capitals' top and its baseline, and where it starts across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub role: Role,
    pub letter: bool,
    pub top: f32,
    pub base: f32,
    pub left: f32,
}

/// A key's legend set: its inks, the top of its second function if it has
/// one, and its lines, in the art's units.
#[derive(Clone, Debug, Default)]
pub struct Set {
    pub inks: Vec<Ink>,
    pub second_top: Option<f32>,
    pub lines: Vec<Line>,
}

/// A run's glyphs in a face: each one's outline (in the face's units, y up)
/// and where along the run it starts; the run's advance; the face's units to
/// the em and its capitals' share of it.
struct Shaped {
    glyphs: Vec<(Path, f32)>,
    advance: f32,
    per_em: f32,
}

impl Set {
    /// The box the legend's inks cover, in the units it was set in: left,
    /// top, right, bottom; none for a legend with nothing to draw.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        for ink in &self.inks {
            let Some(p) = ink.path.clone().transform(ink.to_art) else { continue };
            let r = p.bounds();
            b = Some(match b {
                Some((l, t, rr, bb)) => (l.min(r.left()), t.min(r.top()), rr.max(r.right()), bb.max(r.bottom())),
                None => (r.left(), r.top(), r.right(), r.bottom()),
            });
        }
        b
    }
}

/// Outlines of a face's glyphs, as tiny-skia paths.
struct Outline(PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// The capitals' share of a face's em.
fn cap_share(db: &fontdb::Database, id: fontdb::ID) -> f32 {
    db.with_face_data(id, |data, index| {
        let f = ttf_parser::Face::parse(data, index).ok()?;
        f.capital_height().filter(|h| *h > 0).map(|h| h as f32 / f.units_per_em() as f32)
    })
    .flatten()
    .unwrap_or(CAP)
}

/// Text shaped in a face, the glyphs it has: their outlines and places, in
/// the face's units; and which of its characters it lacks.
fn shape(db: &fontdb::Database, id: fontdb::ID, text: &str) -> Option<Shaped> {
    db.with_face_data(id, |data, index| {
        let face = rustybuzz::Face::from_slice(data, index)?;
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        let out = rustybuzz::shape(&face, &[], buf);
        let mut pen = 0.0;
        let mut glyphs = Vec::new();
        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
            let mut b = Outline(PathBuilder::new());
            if face.outline_glyph(ttf_parser::GlyphId(info.glyph_id as u16), &mut b).is_some() {
                if let Some(p) = b.0.finish() {
                    glyphs.push((p, pen + pos.x_offset as f32));
                }
            }
            pen += pos.x_advance as f32;
        }
        Some(Shaped { glyphs, advance: pen, per_em: face.units_per_em() as f32 })
    })
    .flatten()
}

/// Whether a face has a character.
fn has(db: &fontdb::Database, id: fontdb::ID, c: char) -> bool {
    db.with_face_data(id, |data, index| ttf_parser::Face::parse(data, index).ok().and_then(|f| f.glyph_index(c)).is_some()).unwrap_or(false)
}

/// A run laid out: its glyphs or icon, each as drawn in its own units and
/// how it goes onto the face's top (mm, from its top left corner, y down);
/// its advance (mm) and capitals' height.
struct Laid {
    run: Run,
    parts: Vec<(Path, Transform, Option<f32>)>,
    advance: f32,
    cap: f32,
    /// how far below its line's baseline it sits (mm)
    drop: f32,
}

/// The icons as paths on their 24-unit squares, filled or stroked.
fn icon_paths(icon: &Icon) -> Vec<(Path, Option<f32>)> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{}"/><path d="{}" fill="none" stroke="black" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
        icon.fill, icon.stroke, icon.width
    );
    let Ok(tree) = usvg::Tree::from_str(&svg, &usvg::Options::default()) else { return Vec::new() };
    let mut out = Vec::new();
    for n in tree.root().children() {
        if let usvg::Node::Path(p) = n {
            let stroke = p.stroke().map(|s| s.width().get());
            if p.fill().is_some() || stroke.is_some() {
                out.push((p.data().clone(), stroke));
            }
        }
    }
    out
}

/// A run set in `face` (and the first of `fallbacks` that has each sign it
/// lacks) at a capitals' height `cap` (mm): its glyphs placed along it from
/// its start, on its baseline. A sign from a typeface other than the
/// legends' and the core's own symbols (`symbols`), whose signs are drawn as
/// the art draws them, is sized to the capitals and stands on the line, so
/// signs from different typefaces look alike.
fn lay_run(db: &fontdb::Database, run: &Run, cap: f32, face: fontdb::ID, fallbacks: &[fontdb::ID], symbols: Option<fontdb::ID>, own: &Own) -> Laid {
    if let Some(name) = &run.icon {
        let Some(icon) = own.icons.get(name) else { return Laid { run: run.clone(), parts: Vec::new(), advance: 0.0, cap, drop: 0.0 } };
        let side = icon.size * cap;
        let s = side / 24.0;
        // its square's foot `drop` below the line
        let drop = icon.drop * cap;
        let parts = icon_paths(icon).into_iter().map(|(p, w)| (p, Transform::from_row(s, 0.0, 0.0, s, 0.0, drop - side), w)).collect();
        return Laid { run: run.clone(), parts, advance: side, cap, drop: 0.0 };
    }
    // its font size: the capitals as tall as asked, larger signs larger
    let share = cap_share(db, face);
    let size = cap * run.scale / share;
    // the text in pieces, each in the face that has it
    let mut pieces: Vec<(fontdb::ID, String)> = Vec::new();
    for c in run.text.chars() {
        let id = if c.is_whitespace() || has(db, face, c) { face } else { fallbacks.iter().copied().find(|&f| has(db, f, c)).unwrap_or(face) };
        match pieces.last_mut() {
            Some((last, s)) if *last == id => s.push(c),
            _ => pieces.push((id, c.to_string())),
        }
    }
    let mut parts = Vec::new();
    let mut pen = 0.0;
    for (id, text) in pieces {
        if id != face && Some(id) != symbols {
            let mut first = true;
            for c in text.chars() {
                let Some(sh) = shape(db, id, &c.to_string()) else { continue };
                // its outline's box, in the typeface's units (y up)
                let (mut l, mut lo, mut r, mut hi) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (p, x) in &sh.glyphs {
                    let b = p.bounds();
                    (l, lo, r, hi) = (l.min(b.left() + x), lo.min(b.top()), r.max(b.right() + x), hi.max(b.bottom()));
                }
                if l > r {
                    continue;
                }
                let tall = cap * run.scale * SIGN;
                let k = (tall / (hi - lo).max(1e-3)).min(tall * 1.25 / (r - l).max(1e-3));
                if !first {
                    pen += SIGN_GAP * cap;
                }
                first = false;
                for (p, x) in sh.glyphs {
                    parts.push((p, Transform::from_row(k, 0.0, 0.0, -k, pen + (x - l) * k, lo * k), None));
                }
                pen += (r - l) * k;
            }
            continue;
        }
        let Some(sh) = shape(db, id, &text) else { continue };
        let k = size / sh.per_em;
        for (p, x) in sh.glyphs {
            parts.push((p, Transform::from_row(k, 0.0, 0.0, -k, pen + x * k, 0.0), None));
        }
        pen += sh.advance * k;
    }
    let drop = if run.scale > 1.0 { MARK_DROP * cap } else { 0.0 };
    Laid { run: run.clone(), parts, advance: pen, cap, drop }
}

/// A key's print set in a style, on its cap's top `face` (in the art's
/// units, `per_mm` of them to the millimetre): its inks in the art's units.
pub fn set(db: &fontdb::Database, print: &Print, style: &Style, own: &Own, face: [f32; 4], per_mm: f32) -> Set {
    let Some(id) = query(db, &style.typeface, style.weight).or_else(|| query(db, &own.style.typeface, own.style.weight)) else { return Set::default() };
    let symbols = (!own.symbols.is_empty()).then(|| query(db, &own.symbols, 700)).flatten();
    let fallbacks: Vec<fontdb::ID> = symbols.into_iter().chain(GLYPH_FAMILIES.iter().chain(&FALLBACKS).filter_map(|&(f, w)| query(db, f, w))).collect();
    let (fw, fh) = ((face[2] - face[0]) / per_mm, (face[3] - face[1]) / per_mm);
    // each run at its kind's size, a word too long for its cap set smaller,
    // a key's words all alike (PRINT over SCREEN the same size)
    let cap_of = |r: &Run| if r.letter() { style.letters } else { style.words };
    let mut laid: Vec<Laid> = print.runs.iter().map(|r| lay_run(db, r, cap_of(r), id, &fallbacks, symbols, own)).collect();
    let room = fw - 2.0 * FIT;
    let words = laid.iter().filter(|l| !l.run.letter() && l.run.icon.is_none() && l.advance > room && room > 0.0).map(|l| room / l.advance).fold(1.0f32, f32::min);
    for l in laid.iter_mut() {
        let k = if l.run.letter() || l.run.icon.is_some() { if l.advance > room && room > 0.0 { room / l.advance } else { 1.0 } } else { words };
        if k < 1.0 {
            for (_, t, _) in l.parts.iter_mut() {
                *t = t.post_scale(k, k);
            }
            l.advance *= k;
            l.cap *= k;
            l.drop *= k;
        }
    }
    // its lines: its own legend with its shifted sign beside it, the rest of
    // its own legend's lines under it, its second function on a line of its own
    let mains: Vec<usize> = (0..laid.len()).filter(|&i| laid[i].run.role == Role::Main).collect();
    let shifts: Vec<usize> = (0..laid.len()).filter(|&i| laid[i].run.role == Role::Shift).collect();
    let seconds: Vec<usize> = (0..laid.len()).filter(|&i| laid[i].run.role == Role::Second).collect();
    let mut lines: Vec<Vec<usize>> = Vec::new();
    if let Some(&first) = mains.first() {
        lines.push(std::iter::once(first).chain(shifts.iter().copied()).collect());
        lines.extend(mains[1..].iter().map(|&i| vec![i]));
    } else if !shifts.is_empty() {
        lines.push(shifts.clone());
    }
    // each line's baseline (mm, down from the top edge of the cap's top),
    // as at the top; and the second function's at the foot
    let unscaled = |i: usize| if laid[i].run.letter() { style.letters } else { style.words };
    let mut bases: Vec<f32> = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let lead = line[0];
        let cap = laid[lead].cap.min(unscaled(lead));
        let base = match n {
            0 => style.inset * if laid[lead].run.letter() { 1.0 } else { WORD_INSET } + cap,
            _ => bases[n - 1] + PITCH * cap / CAP,
        };
        bases.push(base);
    }
    let first_cap = lines.first().map_or(style.words, |l| laid[l[0]].cap.min(unscaled(l[0])));
    let second_base = seconds.first().map(|&i| fh - (FOOT.0 + FOOT.1 * laid[i].cap));
    // a legend the core prints in the middle of its cap, there at the top;
    // in the middle, all of it there; at the top left, all at the top
    let centre_of = |top: f32, foot: f32| fh / 2.0 - CENTRE_RISE * first_cap - (top + foot) / 2.0;
    let (main_shift, second_shift) = match (style.place, bases.first(), bases.last()) {
        (Place::Printed, Some(&b0), Some(&bn)) if print.centred => (centre_of(b0 - first_cap, bn), 0.0),
        (Place::Middle, Some(&b0), Some(&bn)) => {
            let foot = second_base.unwrap_or(bn).max(bn);
            let d = centre_of(b0 - first_cap, foot);
            (d, d)
        }
        _ => (0.0, 0.0),
    };
    // each line across the cap: in its middle, or from its left at the top left
    let line_width = |line: &[usize]| {
        let mut w = 0.0;
        for (k, &i) in line.iter().enumerate() {
            if k > 0 {
                w += SHIFT_GAP * laid[line[0]].cap / CAP;
            }
            w += laid[i].advance;
        }
        w
    };
    let start = |w: f32| if style.place == Place::TopLeft { style.inset } else { (fw - w) / 2.0 };
    let mut placed: Vec<(usize, f32, f32)> = Vec::new();
    let mut set_lines: Vec<Line> = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let mut x = start(line_width(line));
        let lead = &laid[line[0]];
        let cap = lead.cap.min(unscaled(line[0]));
        set_lines.push(Line { role: lead.run.role, letter: lead.run.letter(), top: bases[n] + main_shift - cap, base: bases[n] + main_shift, left: x });
        for (k, &i) in line.iter().enumerate() {
            if k > 0 {
                x += SHIFT_GAP * laid[line[0]].cap / CAP;
            }
            placed.push((i, x, bases[n] + main_shift + laid[i].drop));
            x += laid[i].advance;
        }
    }
    let mut second_top = None;
    if let Some(base) = second_base {
        let w = seconds.iter().map(|&i| laid[i].advance).sum::<f32>() + SHIFT_GAP * style.words / CAP * seconds.len().saturating_sub(1) as f32;
        let mut x = start(w);
        if let Some(&i) = seconds.first() {
            set_lines.push(Line { role: Role::Second, letter: laid[i].run.letter(), top: base + second_shift - laid[i].cap, base: base + second_shift, left: x });
        }
        for &i in &seconds {
            let y = base + second_shift + laid[i].drop;
            let top = if laid[i].run.icon.is_some() { y - own.icons.get(laid[i].run.icon.as_deref().unwrap_or("")).map_or(0.0, |ic| (ic.size - ic.drop) * laid[i].cap) } else { y - laid[i].cap };
            second_top = Some(second_top.map_or(top, |t: f32| t.min(top)));
            placed.push((i, x, y));
            x += laid[i].advance + SHIFT_GAP * style.words / CAP;
        }
    }
    // onto the art: mm from the cap top's corner, moved as the style says, to its units
    let (ax, ay) = (style.across, style.down);
    let to_face = Transform::from_row(per_mm, 0.0, 0.0, per_mm, face[0], face[1]);
    let mut inks = Vec::new();
    for (i, x, y) in placed {
        let l = &laid[i];
        for (p, t, w) in &l.parts {
            inks.push(Ink { path: p.clone(), to_art: to_face.pre_translate(x + ax, y + ay).pre_concat(*t), opacity: l.run.role.opacity(), stroke: *w, role: l.run.role });
        }
    }
    let lines = set_lines.into_iter().map(|l| Line { top: face[1] + (l.top + ay) * per_mm, base: face[1] + (l.base + ay) * per_mm, left: face[0] + (l.left + ax) * per_mm, ..l }).collect();
    Set { inks, second_top: second_top.map(|t| face[1] + (t + ay) * per_mm), lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Value {
        crate::jsonc::parse(&crate::fixture::device()).unwrap()
    }

    /// The core's print reads as it is written: its 110 printed keys, each
    /// part its own, its seven centred, its own typeface and sizes.
    #[test]
    fn the_core_s_print_reads() {
        let d = device();
        let prints: Vec<(String, Print)> = d["controls"].as_array().unwrap().iter().filter_map(|c| Some((c["id"].as_str()?.to_string(), Print::of(c)?))).collect();
        assert_eq!(prints.len(), 110);
        assert_eq!(prints.iter().filter(|(_, p)| p.centred).count(), 7);
        let two = &prints.iter().find(|(id, _)| id == "2").unwrap().1;
        assert_eq!(two.runs.iter().map(|r| (r.text.as_str(), r.role)).collect::<Vec<_>>(), [("2", Role::Main), ("@", Role::Shift), ("PF2", Role::Second)]);
        let f1 = &prints.iter().find(|(id, _)| id == "f1").unwrap().1;
        assert_eq!(f1.runs[1].icon.as_deref(), Some("mute"));
        let own = Own::of(&d).unwrap();
        assert_eq!((own.style.typeface.as_str(), own.style.weight, own.symbols.as_str()), ("Barlow Semi Condensed", 600, "Z10 Symbols"));
        assert!(own.icons.contains_key("mute"));
        let s = Style::over(&own.style, &serde_json::json!({ "typeface": "Montserrat", "place": "middle", "words": 2.2 }));
        assert_eq!((s.typeface.as_str(), s.place, s.words, s.letters), ("Montserrat", Place::Middle, 2.2, own.style.letters));
    }

    /// A key's own text takes the place of the parts it names: its own
    /// legend's lines at each line break, a part of none taken away, the
    /// parts it does not name as printed; with none, the print as it is.
    #[test]
    fn a_key_s_own_text_takes_the_place_of_its_print() {
        let d = device();
        let control = |id: &str| d["controls"].as_array().unwrap().iter().find(|c| c["id"] == id).cloned().unwrap();
        let two = Print::of(&control("2")).unwrap();
        assert_eq!(two.with_text(&serde_json::json!({})), two, "no text of its own");
        let parts = |p: &Print| p.runs.iter().map(|r| (r.text.clone(), r.role)).collect::<Vec<_>>();
        let mac = two.with_text(&serde_json::json!({ "main": "⌘", "second": "" }));
        assert_eq!(parts(&mac), [("⌘".to_string(), Role::Main), ("@".to_string(), Role::Shift)], "the main its own, the second gone, the shift as printed");
        let caps = Print::of(&control("caps-lock")).unwrap();
        assert_eq!(caps.text_of(Role::Main), "CAPS\nLOCK", "lines at line breaks");
        assert_eq!(parts(&caps.with_text(&serde_json::json!({ "main": "CAPS\nLOCK" }))), parts(&caps), "the same text, the same print");
        let f1 = Print::of(&control("f1")).unwrap();
        assert_eq!(f1.text_of(Role::Second), "[mute]", "an icon by its name");
        assert!(f1.with_text(&serde_json::json!({ "main": "F1" })).runs.iter().any(|r| r.icon.as_deref() == Some("mute")), "an icon kept where no text takes its place");
        assert!(caps.with_text(&serde_json::json!({ "main": "⇪" })).centred, "a centred key stays centred");
    }

    /// A keyboard's glyphs are set from Ectodeck's own typefaces on a
    /// computer that has no fonts for them, each the same as the font it was
    /// cut from draws it; and those typefaces are not offered for legends.
    #[test]
    fn a_keyboard_s_glyphs_need_no_fonts_on_the_computer() {
        let mut db = fontdb::Database::new();
        db.load_font_file(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cores/evga-z10/fonts/BarlowSemiCondensed-SemiBold.ttf")).unwrap();
        add_glyph_fonts(&mut db);
        let face = query(&db, "Barlow Semi Condensed", 600).unwrap();
        let ours: Vec<fontdb::ID> = GLYPH_FAMILIES.iter().filter_map(|&(f, w)| query(&db, f, w)).collect();
        assert_eq!(ours.len(), 3, "each found by its family");
        for c in "⌘⌥⌃⇧⇪⎋⇥⌫⌦↵⏎␣←↑→↓⇞⇟⏯⏮⏭⏏⏻⊞≡⎙☀▲◀▶▼".chars() {
            assert!(!has(&db, face, c) && ours.iter().any(|&id| has(&db, id, c)), "{c}: from Ectodeck's own");
        }
        assert_eq!(families(&db).iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>(), ["Barlow Semi Condensed"]);
    }
}
