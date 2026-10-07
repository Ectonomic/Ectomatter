//! Legends set anew from each key's print: the words its cap has printed on
//! it, as the core gives them, set in a typeface, as tall and where on the
//! cap a light's "legends" say (the Key lab's Legends section). They are
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
    /// Its place among a key's legend's parts, as TEXTS has them.
    pub fn index(self) -> usize {
        match self {
            Role::Main => 0,
            Role::Shift => 1,
            Role::Second => 2,
        }
    }

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
    pub fn letter(&self) -> bool {
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
    /// under the other at each line break, "" for a part printed with nothing;
    /// a line that is one of the core's `icons` by its name in brackets
    /// ("[mute]", as text_of gives it) that icon. A part may be a picture of
    /// the person's own in place of its text, by its file's name
    /// ("main_image"), one of the images added (add_image); a key whose own
    /// legend is a picture has it in the middle of its cap, whatever the
    /// legends' place (see set).
    pub fn with_text(&self, legends: &Value, icons: &HashMap<String, Icon>) -> Print {
        let image = |k: &str| legends[format!("{k}_image")].as_str().filter(|n| !n.is_empty()).map(str::to_string);
        if TEXTS.iter().all(|(k, _)| !legends[*k].is_string() && image(k).is_none()) {
            return self.clone();
        }
        let run = |t: &str, role: Role| {
            let t = t.trim();
            match t.strip_prefix('[').and_then(|n| n.strip_suffix(']')).filter(|n| icons.contains_key(*n)) {
                Some(n) => Run { text: String::new(), icon: Some(n.to_string()), role, scale: 1.0 },
                None => Run { text: t.to_string(), icon: None, role, scale: 1.0 },
            }
        };
        let mut runs = Vec::new();
        for (k, role) in TEXTS {
            if let Some(name) = image(k) {
                runs.push(Run { text: String::new(), icon: Some(format!("{IMAGE}{name}")), role, scale: 1.0 });
                continue;
            }
            match legends[k].as_str() {
                Some(text) if role == Role::Main => runs.extend(text.split('\n').filter(|l| !l.trim().is_empty()).map(|l| run(l, role))),
                Some(text) => runs.extend(Some(text.replace('\n', " ")).filter(|t| !t.trim().is_empty()).map(|t| run(&t, role))),
                None => runs.extend(self.runs.iter().filter(|r| r.role == role).cloned()),
            }
        }
        Print { runs, centred: self.centred || image("main").is_some() }
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
    /// each part's own (Main, Shifted, Bottom, as TEXTS has them): how tall
    /// its capitals are (mm), in place of its kind's size (letters or words),
    /// and how far it is moved from where the rest puts it
    pub parts: [Part; 3],
}

/// A part of a legend's own size and place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    /// none where it takes its kind's size, as the rest of the legend does
    pub size: Option<f32>,
    pub across: f32,
    pub down: f32,
}

impl Default for Part {
    fn default() -> Part {
        Part { size: None, across: 0.0, down: 0.0 }
    }
}

/// A part's values in a light's "legends": its name in TEXTS and the value's.
pub fn part_key(part: usize, value: &str) -> String {
    format!("{}_{value}", TEXTS[part].0)
}

impl Style {
    /// A part's own size and place.
    pub fn part(&self, role: Role) -> Part {
        self.parts[role.index()]
    }

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
            parts: [0, 1, 2].map(|i| {
                let u = under.parts[i];
                let size = legends[part_key(i, "size").as_str()].as_f64().map(|v| (v as f32).max(0.2)).or(u.size);
                Part { size, across: f(&part_key(i, "across"), u.across), down: f(&part_key(i, "down"), u.down) }
            }),
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
        let style = Style { typeface: p["typeface"].as_str()?.to_string(), weight: p["weight"].as_f64().map_or(400, |w| w.round() as u16), place: Place::Printed, letters: f("letters")?, words: f("words")?, inset: f("inset")?, across: 0.0, down: 0.0, parts: [Part::default(); 3] };
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

// ------------------------------------------------------------------ images

/// The start of an icon's name that is a picture of the person's own.
const IMAGE: &str = "image:";
/// A picture, as tall as so many of its part's capitals (its part's size
/// scaling it), its middle level with theirs.
const IMAGE_SIZE: f32 = 2.4;
/// The most a picture's file may hold, and the most pieces of outline it may
/// have, before it is too much for a key's legend.
pub const IMAGE_BYTES: usize = 1 << 20;
const IMAGE_PIECES: usize = 100_000;
static IMAGES: Mutex<Option<HashMap<String, Icon>>> = Mutex::new(None);

/// Add a picture of the person's own, by its file's name: its outline on a
/// 24-unit square, as image_from_svg makes it.
pub fn add_image(name: &str, fill: String) {
    if let Ok(mut m) = IMAGES.lock() {
        m.get_or_insert_with(HashMap::new).insert(format!("{IMAGE}{name}"), Icon { fill, stroke: String::new(), width: 0.0, size: IMAGE_SIZE, drop: (IMAGE_SIZE - 1.0) / 2.0 });
    }
}

fn image(name: &str) -> Option<Icon> {
    IMAGES.lock().ok()?.as_ref()?.get(name).cloned()
}

/// One outline of a shape: its start, then each piece to its end ('L' a
/// line; 'Q' and 'C' curves, their control points first).
type Contour = Vec<(char, Vec<(f32, f32)>)>;

/// A picture's file (SVG) as one outline, as a legend is one ink: its
/// shapes, filled and stroked, in one colour, fitted to a 24-unit square
/// and in its middle. Only shapes are kept: text not turned into outlines,
/// pictures within it and its colours are left out. Each shape's holes stay
/// holes (its outlines wound as filling them all as one needs: outer ones
/// one way, the holes in them the other).
pub fn image_from_svg(svg: &str) -> Result<String, String> {
    if svg.len() > IMAGE_BYTES {
        return Err("larger than 1 MB".into());
    }
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    fn walk(g: &usvg::Group, out: &mut Vec<(Path, bool)>) {
        use resvg::tiny_skia::{LineCap, LineJoin, Stroke};
        for n in g.children() {
            match n {
                usvg::Node::Group(g) => walk(g, out),
                usvg::Node::Path(p) if p.is_visible() => {
                    let t = p.abs_transform();
                    if let Some(d) = p.fill().and_then(|_| p.data().clone().transform(t)) {
                        out.push((d, p.fill().is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd)));
                    }
                    if let Some(s) = p.stroke() {
                        let st = Stroke {
                            width: s.width().get(),
                            line_cap: match s.linecap() {
                                usvg::LineCap::Round => LineCap::Round,
                                usvg::LineCap::Square => LineCap::Square,
                                _ => LineCap::Butt,
                            },
                            line_join: match s.linejoin() {
                                usvg::LineJoin::Round => LineJoin::Round,
                                usvg::LineJoin::Bevel => LineJoin::Bevel,
                                _ => LineJoin::Miter,
                            },
                            ..Stroke::default()
                        };
                        if let Some(d) = p.data().stroke(&st, 1.0).and_then(|d| d.transform(t)) {
                            out.push((d, false));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut shapes: Vec<(Path, bool)> = Vec::new();
    walk(tree.root(), &mut shapes);
    let area = |p: &[(f32, f32)]| p.iter().zip(p.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum::<f32>() / 2.0;
    let inside = |pt: (f32, f32), poly: &[(f32, f32)]| {
        let mut c = false;
        for (a, b) in poly.iter().zip(poly.iter().cycle().skip(1)) {
            if (a.1 > pt.1) != (b.1 > pt.1) && pt.0 < (b.0 - a.0) * (pt.1 - a.1) / (b.1 - a.1) + a.0 {
                c = !c;
            }
        }
        c
    };
    let mut outlines: Vec<Contour> = Vec::new();
    let mut pieces = 0;
    for (path, even_odd) in &shapes {
        let mut contours: Vec<Contour> = Vec::new();
        for seg in path.segments() {
            use resvg::tiny_skia::PathSegment as S;
            pieces += 1;
            let last = contours.last_mut();
            match (seg, last) {
                (S::MoveTo(p), _) => contours.push(vec![('M', vec![(p.x, p.y)])]),
                (S::LineTo(p), Some(c)) => c.push(('L', vec![(p.x, p.y)])),
                (S::QuadTo(a, p), Some(c)) => c.push(('Q', vec![(a.x, a.y), (p.x, p.y)])),
                (S::CubicTo(a, b, p), Some(c)) => c.push(('C', vec![(a.x, a.y), (b.x, b.y), (p.x, p.y)])),
                _ => {}
            }
        }
        if pieces > IMAGE_PIECES {
            return Err("too detailed".into());
        }
        contours.retain(|c| c.len() > 1);
        let polys: Vec<Vec<(f32, f32)>> = contours.iter().map(|c| flatten(c)).collect();
        // a shape filled as wound keeps its outlines' turns, all turned the
        // way its largest outline should go if that goes the other way
        let largest = (0..polys.len()).max_by(|&a, &b| area(&polys[a]).abs().partial_cmp(&area(&polys[b]).abs()).unwrap_or(std::cmp::Ordering::Equal));
        let flip = largest.is_some_and(|o| area(&polys[o]) < 0.0);
        for (i, c) in contours.into_iter().enumerate() {
            let positive = area(&polys[i]) >= 0.0;
            let want = if *even_odd {
                // as deep inside the shape's other outlines as it is: the outermost
                // one way (inside another where most of its points are, as a
                // hole touching its shape's edge at a point still is)
                let within = |j: usize| 2 * polys[i].iter().filter(|&&p| inside(p, &polys[j])).count() > polys[i].len();
                (0..polys.len()).filter(|&j| j != i && within(j)).count() % 2 == 0
            } else {
                positive != flip
            };
            outlines.push(if want == positive { c } else { reversed(&c) });
        }
    }
    // fitted to the square, in its middle
    let all: Vec<(f32, f32)> = outlines.iter().flat_map(|c| flatten(c)).collect();
    if all.is_empty() {
        return Err("no shapes".into());
    }
    let (x0, y0) = all.iter().fold((f32::MAX, f32::MAX), |m, p| (m.0.min(p.0), m.1.min(p.1)));
    let (x1, y1) = all.iter().fold((f32::MIN, f32::MIN), |m, p| (m.0.max(p.0), m.1.max(p.1)));
    let k = 24.0 / (x1 - x0).max(y1 - y0).max(1e-6);
    let (ox, oy) = ((24.0 - (x1 - x0) * k) / 2.0, (24.0 - (y1 - y0) * k) / 2.0);
    let mut d = String::new();
    for c in &outlines {
        for (cmd, pts) in c {
            d.push(*cmd);
            d.push_str(&pts.iter().map(|p| format!("{:.3} {:.3}", ox + (p.0 - x0) * k, oy + (p.1 - y0) * k)).collect::<Vec<_>>().join(" "));
        }
        d.push('Z');
    }
    Ok(d)
}

/// An outline as points along it, its curves in steps.
fn flatten(c: &[(char, Vec<(f32, f32)>)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut at = (0.0, 0.0);
    for (cmd, p) in c {
        for i in 1..=8 {
            let (t, u) = (i as f32 / 8.0, 1.0 - i as f32 / 8.0);
            match cmd {
                'Q' => out.push((u * u * at.0 + 2.0 * u * t * p[0].0 + t * t * p[1].0, u * u * at.1 + 2.0 * u * t * p[0].1 + t * t * p[1].1)),
                'C' => {
                    let b = |a: f32, b: f32, c: f32, d: f32| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
                    out.push((b(at.0, p[0].0, p[1].0, p[2].0), b(at.1, p[0].1, p[1].1, p[2].1)));
                }
                _ => {
                    out.push(p[0]);
                    break;
                }
            }
        }
        at = *p.last().unwrap_or(&at);
    }
    out
}

/// An outline wound the other way.
fn reversed(c: &[(char, Vec<(f32, f32)>)]) -> Contour {
    let ends: Vec<(f32, f32)> = c.iter().map(|(_, p)| *p.last().unwrap_or(&(0.0, 0.0))).collect();
    let mut out = vec![('M', vec![*ends.last().unwrap_or(&(0.0, 0.0))])];
    for i in (1..c.len()).rev() {
        let to = ends[i - 1];
        let (cmd, p) = &c[i];
        out.push(match cmd {
            'Q' => ('Q', vec![p[0], to]),
            'C' => ('C', vec![p[1], p[0], to]),
            _ => ('L', vec![to]),
        });
    }
    out
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

    /// The box one part's inks cover, as bounds gives the whole's.
    pub fn bounds_of(&self, role: Role) -> Option<(f32, f32, f32, f32)> {
        Set { inks: self.inks.iter().filter(|i| i.role == role).cloned().collect(), second_top: None, lines: Vec::new() }.bounds()
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
        let Some(icon) = own.icons.get(name).cloned().or_else(|| image(name)) else { return Laid { run: run.clone(), parts: Vec::new(), advance: 0.0, cap, drop: 0.0 } };
        let icon = &icon;
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
    let cap_of = |r: &Run| style.part(r.role).size.unwrap_or(if r.letter() { style.letters } else { style.words });
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
    let unscaled = |i: usize| style.part(laid[i].run.role).size.unwrap_or(if laid[i].run.letter() { style.letters } else { style.words });
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
    // a key whose own legend is a picture has it in the middle, however the rest are placed
    let place = if print.runs.iter().any(|r| r.role == Role::Main && r.icon.as_deref().is_some_and(|i| i.starts_with(IMAGE))) { Place::Middle } else { style.place };
    let (main_shift, second_shift) = match (place, bases.first(), bases.last()) {
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
    let start = |w: f32| if place == Place::TopLeft { style.inset } else { (fw - w) / 2.0 };
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
            let top = if let Some(name) = laid[i].run.icon.as_deref() { y - own.icons.get(name).cloned().or_else(|| image(name)).map_or(0.0, |ic| (ic.size - ic.drop) * laid[i].cap) } else { y - laid[i].cap };
            second_top = Some(second_top.map_or(top, |t: f32| t.min(top)));
            placed.push((i, x, y));
            x += laid[i].advance + SHIFT_GAP * style.words / CAP;
        }
    }
    // onto the art: mm from the cap top's corner, moved as the style says, to its units
    let (ax, ay) = (style.across, style.down);
    let to_face = Transform::from_row(per_mm, 0.0, 0.0, per_mm, face[0], face[1]);
    // (each part moved on from there as its own values say)
    let mut inks = Vec::new();
    for (i, x, y) in placed {
        let l = &laid[i];
        let pt = style.part(l.run.role);
        for (p, t, w) in &l.parts {
            inks.push(Ink { path: p.clone(), to_art: to_face.pre_translate(x + ax + pt.across, y + ay + pt.down).pre_concat(*t), opacity: l.run.role.opacity(), stroke: *w, role: l.run.role });
        }
    }
    let lines = set_lines
        .into_iter()
        .map(|l| {
            let pt = style.part(l.role);
            Line { top: face[1] + (l.top + ay + pt.down) * per_mm, base: face[1] + (l.base + ay + pt.down) * per_mm, left: face[0] + (l.left + ax + pt.across) * per_mm, ..l }
        })
        .collect();
    let second = style.part(Role::Second);
    Set { inks, second_top: second_top.map(|t| face[1] + (t + ay + second.down) * per_mm), lines }
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
        let icons = Own::of(&d).unwrap().icons;
        let control = |id: &str| d["controls"].as_array().unwrap().iter().find(|c| c["id"] == id).cloned().unwrap();
        let two = Print::of(&control("2")).unwrap();
        assert_eq!(two.with_text(&serde_json::json!({}), &icons), two, "no text of its own");
        let parts = |p: &Print| p.runs.iter().map(|r| (r.text.clone(), r.role)).collect::<Vec<_>>();
        let mac = two.with_text(&serde_json::json!({ "main": "⌘", "second": "" }), &icons);
        assert_eq!(parts(&mac), [("⌘".to_string(), Role::Main), ("@".to_string(), Role::Shift)], "the main its own, the second gone, the shift as printed");
        let caps = Print::of(&control("caps-lock")).unwrap();
        assert_eq!(caps.text_of(Role::Main), "CAPS\nLOCK", "lines at line breaks");
        assert_eq!(parts(&caps.with_text(&serde_json::json!({ "main": "CAPS\nLOCK" }), &icons)), parts(&caps), "the same text, the same print");
        let f1 = Print::of(&control("f1")).unwrap();
        assert_eq!(f1.text_of(Role::Second), "[mute]", "an icon by its name");
        assert!(f1.with_text(&serde_json::json!({ "main": "F1" }), &icons).runs.iter().any(|r| r.icon.as_deref() == Some("mute")), "an icon kept where no text takes its place");
        assert!(caps.with_text(&serde_json::json!({ "main": "⇪" }), &icons).centred, "a centred key stays centred");
        // the core's icon by its name, as the text box shows it, is the icon; a name it has none of, words
        let muted = f1.with_text(&serde_json::json!({ "second": "[mute]" }), &icons);
        assert_eq!(muted, f1, "[mute] the icon, as printed");
        let none = f1.with_text(&serde_json::json!({ "second": "[volume]" }), &icons);
        assert_eq!(none.runs.iter().find(|r| r.role == Role::Second).map(|r| (r.text.as_str(), r.icon.is_none())), Some(("[volume]", true)));
    }

    /// Each part of a legend sizes and moves on its own: the bottom legend
    /// at half its size, the main legend raised a millimetre, the rest where
    /// it was; with no part's values, the legend as it was.
    #[test]
    fn a_legend_s_parts_size_and_move_on_their_own() {
        let d = device();
        let db = crate::fixture::faces();
        let own = Own::of(&d).unwrap();
        let control = d["controls"].as_array().unwrap().iter().find(|c| c["id"] == "keypad-8").cloned().unwrap();
        let print = Print::of(&control).unwrap();
        let face = [0.0, 0.0, 13.0, 13.0];
        let plain = set(&db, &print, &own.style, &own, face, 1.0);
        let same = set(&db, &print, &Style::over(&own.style, &serde_json::json!({})), &own, face, 1.0);
        assert_eq!(same.bounds(), plain.bounds(), "no part's values: as it was");
        // (the bottom legend, keypad 8's arrow, set at half the letters' size in mm)
        let style = Style::over(&own.style, &serde_json::json!({ "second_size": own.style.letters / 2.0, "main_down": -1.0 }));
        let moved = set(&db, &print, &style, &own, face, 1.0);
        let tall = |s: &Set, r: Role| s.bounds_of(r).map(|b| b.3 - b.1).unwrap();
        assert!((tall(&moved, Role::Second) / tall(&plain, Role::Second) - 0.5).abs() < 0.02, "the bottom legend half as tall");
        let (was, now) = (plain.bounds_of(Role::Main).unwrap(), moved.bounds_of(Role::Main).unwrap());
        assert!((was.1 - now.1 - 1.0).abs() < 1e-3 && (was.0 - now.0).abs() < 1e-3, "the main legend a millimetre up, no further across");
        assert_eq!(tall(&moved, Role::Main), tall(&plain, Role::Main), "the main legend its own size");
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

    /// A picture of the person's own becomes one outline on a 24-unit
    /// square: a ring drawn with even-odd filling keeps its hole when its
    /// outline is filled as legends are, a stroked line comes as its
    /// outline, and its colours go; and more than the file may hold is refused.
    #[test]
    fn a_picture_s_shapes_keep_their_holes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <path fill="#e4572e" fill-rule="evenodd" d="M50 4a46 46 0 1 0 0.01 0ZM50 12a38 38 0 1 1-0.01 0Z"/>
            <path d="M30 50H70" stroke="#123" stroke-width="6" fill="none"/></svg>"##;
        let d = image_from_svg(svg).unwrap();
        let pix = resvg::tiny_skia::Pixmap::new(48, 48).unwrap();
        let mut pix = pix;
        let path = usvg::Tree::from_str(&format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{d}"/></svg>"#), &usvg::Options::default()).unwrap();
        let usvg::Node::Path(p) = &path.root().children()[0] else { panic!("a path") };
        let mut paint = resvg::tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        pix.fill_path(p.data(), &paint, resvg::tiny_skia::FillRule::Winding, Transform::from_scale(2.0, 2.0), None);
        let at = |x: u32, y: u32| pix.pixel(x, y).unwrap().alpha();
        assert_eq!(at(24, 16), 0, "the ring's hole stays a hole");
        assert!(at(24, 1) > 200 && at(1, 24) > 200, "the ring itself, filled, as wide as the square");
        assert!(at(24, 24) > 200, "the stroked line, as its outline");
        assert!(image_from_svg(&" ".repeat(IMAGE_BYTES + 1)).is_err());
        assert!(image_from_svg(r#"<svg xmlns="http://www.w3.org/2000/svg"><text>hi</text></svg>"#).is_err(), "no shapes");
    }
}
