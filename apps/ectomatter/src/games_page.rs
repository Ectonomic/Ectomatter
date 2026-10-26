//! The Library's page, the place "library" (games.rs keeps the Library, and
//! steam.rs reads Steam's files): the games and apps on three tabs
//! (Favorites, Games and Apps), each one's own page and Every game's, and the
//! forms that add a game, an app or a script. A game's page holds Ectomatter's
//! own section, Devices: the profile each device switches to while the game
//! is in front. Then each plugin's section, drawn from what the plugin says
//! of it (hosted.rs): its settings as dropdowns, in rows or in a grid (the
//! Monitor Switcher's column for each monitor). Then Custom Scripts,
//! Ectomatter's own too: the person's own commands, each with Run now. Before
//! the sections, where there's room, On this page: each section and what it
//! changes here, on the left, as a page's way around it. The
//! pictures (each cover, and a page's picture with its logo laid on it) are
//! drawn from Steam's files at the size they show, on a thread of their own,
//! and come in as each is ready.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use slint::{ModelRc, VecModel};

use crate::commands::{Ran, Script};
use crate::games::{self, Choice, Entry, From, Kind, Library, Running, EVERY};
use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::tokens::{font, part};
use crate::{AppWindow, Cell, LibApp, LibCover, LibFact, LibToc, MenuRow};

/// A plugin's section as a page shows it: whose it is, its name, and how the
/// plugin describes it (its settings, and the layout they're drawn in).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub plugin: String,
    pub name: String,
    pub desc: Value,
}

/// A device as a page lists it: its key (as the Library keeps its choices),
/// its name, what sort it is (for its glyph: "keyboard" or "mouse"), and its
/// profiles, each by its id and its name; for a device a plugin brought in,
/// its plugin's glyph (by the package's place) and what it is in a few words.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub profiles: Vec<(String, String)>,
    pub glyph: Option<usize>,
    pub sub: String,
    /// each profile's chip, for the Scene row (kind "scene"), whose profiles
    /// are No scene and the scenes
    pub chips: Vec<slint::Image>,
}

/// What each row of the Scene row's menu chooses, Don't switch first, then
/// No scene, the scenes that come with Ectomatter and the person's own, a
/// line (none) between the groups; the Scene row's profiles are No scene,
/// the two built-in scenes, then the person's.
fn scene_rows(d: &Device) -> Vec<Option<Choice>> {
    let p = |i: usize| d.profiles.get(i).map(|(r, _)| Choice::Profile(r.clone()));
    let mut out = vec![Some(Choice::Stay), p(0), None];
    out.extend((1..d.profiles.len().min(3)).map(p));
    if d.profiles.len() > 3 {
        out.push(None);
        out.extend((3..d.profiles.len()).map(p));
    }
    out
}

/// Which games the games' tabs show: every store's, Steam's, or those added by hand.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shop {
    All,
    Steam,
    Hand,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Order {
    Recent,
    Name,
    Most,
}

/// A menu of the Library's, open: the store, the order, a device's on a
/// page (by its row there), a plugin's setting (by its place among the
/// page's), or a script's time limit in its form.
#[derive(Clone, Debug, PartialEq)]
pub enum Menu {
    Shop,
    Order,
    Device(usize),
    Setting(usize),
    Limit,
}

/// A control in a plugin's section other than its dropdowns, by its place
/// among the page's (the piece's index): one of a choice's options, a
/// checkbox, an action, a fold, a value to type, a variable (which opens
/// its form), and Add a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    Option { plugin: String, key: String, value: Value },
    Check { plugin: String, key: String },
    Action { plugin: String, action: Value },
    Fold(String),
    Text { plugin: String, key: String },
    Pair { plugin: String, key: String, name: String },
    PairAdd { plugin: String, key: String },
}

/// A row a game's page changed from Every game's, which its dot puts back:
/// a device's (by its row's key), or a plugin's settings (by their keys);
/// each with its name, for the bar to say.
#[derive(Clone, Debug, PartialEq)]
enum Reset {
    Device { key: String, label: String },
    Settings { plugin: String, keys: Vec<String>, label: String },
}

/// What a dot put back, which Undo puts as it was: the page's own choice
/// for a device, or its own value for each of a plugin's settings.
#[derive(Clone, Debug)]
enum ResetUndo {
    Device { id: String, key: String, was: Option<Choice> },
    Settings { id: String, plugin: String, was: Vec<(String, Option<Value>)> },
}

/// The settings a section's layout parts show, wherever they're laid out:
/// in a row (and nested under it), a grid's row or a list of them (at the
/// section's top, where the page draws those: `top`), or inside a fold,
/// open or shut. (A setting no part shows now, such as one nested under a
/// box that's unticked, does nothing, so it counts as no change.)
fn keys_in(parts: &[Value], top: bool) -> Vec<String> {
    let mut out = Vec::new();
    for part in parts {
        if let Some(row) = part.get("row") {
            out.extend(row["show"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from)));
            out.extend(row["nest"].as_array().into_iter().flatten().filter_map(|n| n["show"].as_str().map(String::from)));
        }
        if top {
            for r in part["grid"]["rows"].as_array().into_iter().flatten() {
                out.extend(r["cells"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)));
            }
            for item in part["show"].as_array().into_iter().flatten() {
                match item {
                    Value::String(k) => out.push(k.clone()),
                    o => out.extend(o["id"].as_str().map(String::from)),
                }
            }
        }
        if let Some(inside) = part["fold"]["parts"].as_array() {
            out.extend(keys_in(inside, false));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The form for a variable in a plugin's section: whose and which setting
/// it's in, and the variable being changed (none while one is added); its
/// words are the window's own.
#[derive(Clone, Debug, PartialEq)]
pub struct VarForm {
    pub plugin: String,
    pub key: String,
    pub editing: Option<String>,
}

/// Whether a variable's name is one a shell can set: letters, digits and
/// underscores, not starting with a digit.
fn var_name_ok(n: &str) -> bool {
    !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !n.starts_with(|c: char| c.is_ascii_digit())
}

/// The form for a script: the page it's on, the script being changed (none
/// while one is added), and its choices; its words are the window's own.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptForm {
    pub page: String,
    pub editing: Option<u64>,
    pub wait: bool,
    pub keep: bool,
    pub limit: u64,
}

/// The time limits a script can have, in seconds.
const LIMITS: [u64; 5] = [5, 10, 30, 60, 120];

fn limit_words(n: u64) -> String {
    match n {
        60 => "1 minute".into(),
        n if n % 60 == 0 => format!("{} minutes", n / 60),
        1 => "1 second".into(),
        n => format!("{n} seconds"),
    }
}

/// The form open over the Library: Add a game (the program chosen), Add an
/// app (the app chosen, by its place in the list), or a script's.
#[derive(Clone, Debug, PartialEq)]
enum Form {
    None,
    Game(Option<PathBuf>),
    App(Option<usize>),
    Script(ScriptForm),
    Var(VarForm),
}

/// A game started or quit, for the window to act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Started(Running),
    Quit(Running),
}

/// A picture to draw: from which files, at which size in pixels, its corners'
/// radius in pixels (and a page's picture, at which scale, times 100).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Pic {
    Cover(PathBuf, u32, u32, u32),
    Hero(PathBuf, Option<PathBuf>, u32, u32, u32),
    Icon(PathBuf, u32, u32),
}

type Drawn = (Pic, Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>);

/// The thread the pictures are drawn on: what is asked of it, and what comes back.
struct Painter {
    ask: mpsc::Sender<Pic>,
    drawn: mpsc::Receiver<Drawn>,
}

impl Painter {
    fn start() -> Painter {
        let (ask, jobs) = mpsc::channel::<Pic>();
        let (tx, drawn) = mpsc::channel();
        let _ = std::thread::Builder::new().name("library pictures".into()).spawn(move || {
            for job in jobs {
                let px = draw(&job);
                if tx.send((job, px)).is_err() {
                    return;
                }
            }
        });
        Painter { ask, drawn }
    }
}

/// A path as an SVG attribute carries it.
fn attr(p: &Path) -> String {
    p.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A picture file's own size, from its header: a PNG's or a JPEG's.
fn size_of(path: &Path) -> Option<(f32, f32)> {
    let b = std::fs::read(path).ok()?;
    if b.starts_with(b"\x89PNG") && b.len() >= 24 {
        let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
        return Some((w as f32, h as f32));
    }
    // a JPEG's frame header (SOF0 to SOF15, less the markers that aren't frames)
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xff {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if (0xc0..=0xcf).contains(&m) && ![0xc4, 0xc8, 0xcc].contains(&m) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]);
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]);
            return Some((w as f32, h as f32));
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        i += 2 + len;
    }
    None
}

/// A picture drawn, its corners rounded: a cover or an icon filling its box,
/// cropped to it; a page's picture as wide as the page, a little above its
/// middle, darkened at its foot and its left for the words on it, the game's
/// logo at its lower left with a shadow under it.
fn draw(p: &Pic) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    use resvg::usvg;
    let filled = |f: &Path, w: u32, h: u32, r: u32| {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs><clipPath id="c"><rect width="{w}" height="{h}" rx="{r}"/></clipPath></defs><image href="{}" width="{w}" height="{h}" preserveAspectRatio="xMidYMid slice" clip-path="url(#c)"/></svg>"#,
            attr(f)
        )
    };
    let (w, h, svg) = match p {
        Pic::Cover(f, w, h, r) => (*w, *h, filled(f, *w, *h, *r)),
        Pic::Icon(f, side, r) => (*side, *side, filled(f, *side, *side, *r)),
        Pic::Hero(f, logo, w, h, s100) => {
            let s = *s100 as f32 / 100.0;
            let (fw, fh) = (*w as f32, *h as f32);
            let (iw, ih) = size_of(f)?;
            // (as CSS's cover, at 35% down)
            let k = (fw / iw).max(fh / ih);
            let (sw, sh) = (iw * k, ih * k);
            let (ix, iy) = ((fw - sw) * 0.5, (fh - sh) * 0.35);
            let logo = logo.as_ref().and_then(|l| Some((l, size_of(l)?))).map(|(l, (lw, lh))| {
                // no larger than 360 by 118, and no larger than itself
                let k = (360.0 / lw).min(118.0 / lh).min(1.0) * s;
                let (lw, lh) = (lw * k, lh * k);
                format!(r#"<image href="{}" x="{}" y="{}" width="{lw}" height="{lh}" preserveAspectRatio="none" filter="url(#s)"/>"#, attr(l), 32.0 * s, fh - 26.0 * s - lh)
            });
            (
                *w,
                *h,
                format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs><clipPath id="c"><rect width="{w}" height="{h}" rx="{}"/></clipPath>
<linearGradient id="d" x1="0" y1="0" x2="0" y2="1"><stop offset="0.4" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.667"/></linearGradient>
<linearGradient id="l" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#000" stop-opacity="0.533"/><stop offset="0.45" stop-color="#000" stop-opacity="0"/></linearGradient>
<filter id="s" x="-30%" y="-60%" width="160%" height="220%"><feDropShadow dx="0" dy="{}" stdDeviation="{}" flood-color="#000" flood-opacity="0.8"/></filter></defs>
<g clip-path="url(#c)"><rect width="{w}" height="{h}" fill="#1f1f23"/><image href="{}" x="{ix}" y="{iy}" width="{sw}" height="{sh}" preserveAspectRatio="none"/><rect width="{w}" height="{h}" fill="url(#d)"/><rect width="{w}" height="{h}" fill="url(#l)"/></g>{}</svg>"##,
                    12.0 * s,
                    4.0 * s,
                    9.0 * s,
                    attr(f),
                    logo.unwrap_or_default()
                ),
            )
        }
    };
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pm = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w.max(1), h.max(1));
    buf.make_mut_bytes().copy_from_slice(pm.data());
    Some(buf)
}

/// The page's measures, a form's (src/form.rs): its margin, its column at most
/// (Size.form), a box's padding, a row's label column (a device's glyph on its
/// square, then its name), the line a row's first piece sits on, and the
/// dropdowns' width.
const X0: f32 = crate::tokens::space::MARGIN;
const PAD_X: f32 = crate::tokens::space::PAD;
const LABEL_W: f32 = crate::tokens::size::LABEL;
const LINE: f32 = crate::form::LINE;
const GLYPH: f32 = crate::form::LINE;
/// where a row's controls start, from the box's edge: after its label's column, the form's
const CTRL_X: f32 = PAD_X + LABEL_W + PAD_X;
const DROP_W: f32 = 300.0;
/// the rhythm down a box: a row's room above and below what it holds, a rule between
const ROW: f32 = crate::tokens::space::ROW;
/// a box's first row's top, and its foot, from its own edges
const BOX_IN: f32 = crate::tokens::space::PAD - crate::tokens::space::ROW;
/// where a page's boxes start: under the line of words at its top
const HEAD_END: f32 = crate::tokens::size::HEAD;
/// On this page: its column on the page's left (it's for getting about the
/// page), at least and at most this wide (about what a docs site's On this page
/// takes; its names and what they change need no more), and the room after it
/// before the sections; it takes what the sections' column leaves, so the
/// sections end where the game's picture does
const TOC_W: f32 = 200.0;
const TOC_W_MAX: f32 = 220.0;
const TOC_GAP: f32 = crate::tokens::space::SECTION;
/// beside On this page, the sections' column at least, and without it, at least
const SECTIONS_LEAST: f32 = 640.0;
const COLUMN_LEAST: f32 = 400.0;
/// the game's picture across the page's top
const HERO_H: f32 = crate::tokens::size::HERO;
/// The pieces down a row's column in a plugin's section, as they're placed:
/// where the next goes, and whether one has gone yet (the first sits on the
/// line of the row's label, its middle on the label's).
struct Column {
    y: f32,
    first: bool,
    /// the line the row's label sits on: a control's, or a line of words where the row's
    /// first piece is words (a status, a value), as a form's rows have it
    line: f32,
}

impl Column {
    /// A piece `h` high placed, `gap` under the one before: where it goes
    /// (the first, its middle on the label's, as the mock's rows have it).
    fn place(&mut self, h: f32, gap: f32) -> f32 {
        let y = if self.first { self.y + (self.line - h).max(0.0) / 2.0 } else { self.y + gap };
        self.y = y + h;
        self.first = false;
        y
    }

    /// A piece gone onto a line of its own (a line of buttons, wrapped): its bottom.
    fn reach(&mut self, bottom: f32) {
        self.y = self.y.max(bottom);
    }
}

/// a script's card: the column its When it starts and When it quits are in
const SCRIPT_KEY: f32 = 170.0;
/// where On this page scrolls a section to: its heading, Space.row under the page's head
const TOC_ABOVE: f32 = crate::tokens::space::ROW;
/// a nested setting's name's column, and its dropdown's width at most
const NEST_LABEL: f32 = 104.0;
const NEST_DROP: f32 = 330.0;
/// the glyph a plugin's section draws on a button to open a folder (on 14 units, as the mock's)
const FOLDER: &str = "M1.8 3.6c0-.6.5-1.1 1.1-1.1h2.6l1.3 1.4h4.3c.6 0 1.1.5 1.1 1.1v5.4c0 .6-.5 1.1-1.1 1.1H2.9c-.6 0-1.1-.5-1.1-1.1Z";
/// the covers: at least this wide, with a card grid's gaps across and down
const COVER_MIN: f32 = 150.0;
const GAP_X: f32 = crate::tokens::space::MARGIN;
const GAP_Y: f32 = crate::tokens::space::MARGIN;

pub struct Games {
    pub library: Library,
    watch: games::Watch,
    /// the games running now, as last seen; the first look counts what was
    /// already running before Ectomatter started, which isn't a game starting
    pub running: Vec<Running>,
    looked: bool,
    /// Steam read on a thread: its answer to come, when it was last read, and
    /// when to read it again (after a game quits, for the time it was played)
    reading: Option<mpsc::Receiver<Option<crate::steam::Steam>>>,
    read_at: Option<Instant>,
    read_again: Option<Instant>,
    /// the tab (0 Favorites, 1 Games, 2 Apps), what is typed in the find box,
    /// which store's games show, in which order
    pub tab: usize,
    query: String,
    shop: Shop,
    order: Order,
    /// the page of its own open: a game's or app's id, or Every game's
    pub open: Option<String>,
    form: Form,
    /// the desktop's file picker, open for Add a game; the program whose name
    /// was last filled in for the person to change
    picking: Option<mpsc::Receiver<Option<PathBuf>>>,
    named_for: Option<PathBuf>,
    /// what was last done, whether it went wrong, and when
    said: Option<(String, bool, Instant)>,
    pub menu: Option<Menu>,
    /// a game or app taken out of the Library, which Undo puts back
    undo: Option<games::Removed>,
    /// what the covers or the Apps tab's rows show, in order, by id; the
    /// devices the open page's rows are; the apps the Add an app form offers
    shown: Vec<String>,
    rows: Vec<Device>,
    offered: Vec<(String, String)>,
    /// the plugins' sections on the open page; each setting's dropdown there by its
    /// plugin and its key, in the page's order; each script's card by its id; where
    /// each of On this page's entries jumps to, and how many jumps there have been
    sections: Vec<Section>,
    setting_at: Vec<(String, String)>,
    /// the section's other controls, in the page's order; the folds open (by
    /// plugin and fold); what's being typed in a value's box, where, and when
    spots: Vec<Spot>,
    /// the rows a game changed from Every game's, each by its dot's place
    resets: Vec<Reset>,
    folds: HashSet<String>,
    typed: Option<(Spot, String, Instant)>,
    script_at: Vec<u64>,
    toc_at: Vec<f32>,
    jumps: i32,
    /// where the open page's sections start across: after On this page where it has a column
    left: f32,
    /// a script taken off a page, which Undo puts back; a row a dot put back to
    /// Every game's, which Undo puts as it was; the scripts being tried now
    script_undo: Option<games::ScriptRemoved>,
    reset_undo: Option<ResetUndo>,
    pub trying: HashSet<u64>,
    pics: HashMap<Pic, slint::Image>,
    asked: HashSet<Pic>,
    painter: Painter,
    covers: Rc<VecModel<LibCover>>,
    apps: Rc<VecModel<LibApp>>,
    cells: Rc<VecModel<Cell>>,
    facts: Rc<VecModel<LibFact>>,
    seen: Rc<VecModel<LibApp>>,
    toc: Rc<VecModel<LibToc>>,
}

impl Games {
    pub fn new(ui: &AppWindow) -> Games {
        let covers = Rc::new(VecModel::default());
        let apps = Rc::new(VecModel::default());
        let cells = Rc::new(VecModel::default());
        let facts = Rc::new(VecModel::default());
        let seen = Rc::new(VecModel::default());
        let toc = Rc::new(VecModel::default());
        ui.set_gl_toc(ModelRc::from(toc.clone()));
        ui.set_gl_covers(ModelRc::from(covers.clone()));
        ui.set_gl_apps(ModelRc::from(apps.clone()));
        ui.set_gl_cells(ModelRc::from(cells.clone()));
        ui.set_gl_facts(ModelRc::from(facts.clone()));
        ui.set_gl_seen(ModelRc::from(seen.clone()));
        let mut g = Games {
            library: Library::load(),
            watch: games::Watch::start(),
            running: Vec::new(),
            looked: false,
            reading: None,
            read_at: None,
            read_again: None,
            tab: 0,
            query: String::new(),
            shop: Shop::All,
            order: Order::Recent,
            open: None,
            form: Form::None,
            picking: None,
            named_for: None,
            said: None,
            menu: None,
            undo: None,
            shown: Vec::new(),
            rows: Vec::new(),
            offered: Vec::new(),
            sections: Vec::new(),
            setting_at: Vec::new(),
            spots: Vec::new(),
            resets: Vec::new(),
            folds: HashSet::new(),
            typed: None,
            script_at: Vec::new(),
            toc_at: Vec::new(),
            jumps: 0,
            left: X0,
            script_undo: None,
            reset_undo: None,
            trying: HashSet::new(),
            pics: HashMap::new(),
            asked: HashSet::new(),
            painter: Painter::start(),
            covers,
            apps,
            cells,
            facts,
            seen,
            toc,
        };
        g.read_steam();
        g.tell_watch();
        // (offscreen, ECTOMATTER_LIBRARY_PAGE's page open, as if selected: a game's id or Every game's)
        if crate::desk::offscreen() {
            g.open = std::env::var("ECTOMATTER_LIBRARY_PAGE").ok().filter(|p| !p.is_empty());
        }
        g
    }

    /// Read Steam again, on a thread of its own (offscreen, at once: a
    /// snapshot has no later turn to take the answer on).
    fn read_steam(&mut self) {
        if crate::desk::offscreen() {
            self.library.steam = crate::steam::read();
            self.library.seed_favorites();
            self.read_at = Some(Instant::now());
            return;
        }
        if self.reading.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("steam".into()).spawn(move || {
            let _ = tx.send(crate::steam::read());
        });
        self.reading = Some(rx);
        self.read_at = Some(Instant::now());
    }

    /// The watch told which programs are games added by hand.
    fn tell_watch(&self) {
        if let Ok(mut p) = self.watch.programs.lock() {
            *p = self.library.programs();
        }
    }

    /// What came in since last asked: Steam read, the games running, the
    /// pictures drawn, a program chosen. The games started and quit, and
    /// whether the page has anything new to show.
    pub fn poll(&mut self, showing: bool) -> (Vec<Event>, bool) {
        let mut changed = false;
        if let Some(rx) = &self.reading {
            if let Ok(s) = rx.try_recv() {
                self.reading = None;
                self.library.steam = s;
                self.library.seed_favorites();
                changed = true;
            }
        }
        if self.read_again.is_some_and(|t| Instant::now() >= t) || (showing && self.read_at.is_none_or(|t| t.elapsed() > Duration::from_secs(60))) {
            self.read_again = None;
            self.read_steam();
        }
        let mut events = Vec::new();
        while let Ok(now) = self.watch.seen.try_recv() {
            if self.looked {
                for r in &now {
                    if !self.running.iter().any(|b| b.id == r.id) {
                        events.push(Event::Started(r.clone()));
                    }
                }
                for b in &self.running {
                    if !now.iter().any(|r| r.id == b.id) {
                        events.push(Event::Quit(b.clone()));
                        // (Steam writes down the time played as the game quits)
                        self.read_again = Some(Instant::now() + Duration::from_secs(5));
                    }
                }
            }
            self.looked = true;
            self.running = now;
            changed = true;
        }
        while let Ok((pic, px)) = self.painter.drawn.try_recv() {
            self.asked.remove(&pic);
            if let Some(px) = px {
                self.pics.insert(pic, slint::Image::from_rgba8_premultiplied(px));
            }
            changed = true;
        }
        if let Some(rx) = &self.picking {
            if let Ok(chosen) = rx.try_recv() {
                self.picking = None;
                if let (Some(p), Form::Game(_)) = (chosen, &self.form) {
                    self.form = Form::Game(Some(p));
                }
                changed = true;
            }
        }
        if self.said.as_ref().is_some_and(|(_, _, t)| t.elapsed() > Duration::from_secs(if self.undo.is_some() || self.script_undo.is_some() || self.reset_undo.is_some() { 10 } else { 6 })) {
            self.said = None;
            self.undo = None;
            self.script_undo = None;
            self.reset_undo = None;
            changed = true;
        }
        (events, changed)
    }

    /// Whether the watch has looked once, so the games running are known.
    pub fn looked(&self) -> bool {
        self.looked
    }

    /// A picture, drawn already, or asked for (none until it's drawn;
    /// offscreen, drawn at once).
    fn pic(&mut self, p: Pic) -> Option<slint::Image> {
        if let Some(i) = self.pics.get(&p) {
            return Some(i.clone());
        }
        if crate::desk::offscreen() {
            let i = draw(&p).map(slint::Image::from_rgba8_premultiplied);
            if let Some(i) = &i {
                self.pics.insert(p, i.clone());
            }
            return i;
        }
        if self.asked.insert(p.clone()) {
            let _ = self.painter.ask.send(p);
        }
        None
    }

    pub fn say(&mut self, s: impl Into<String>, bad: bool) {
        // (the bar's Undo goes with the words it was beside: one said after sets its own)
        self.undo = None;
        self.script_undo = None;
        self.reset_undo = None;
        self.said = Some((s.into(), bad, Instant::now()));
    }

    // ------------------------------------------------------------ what each is

    /// Every game and app the Library holds, named as the Library names them.
    fn entries(&self, seen: &crate::front_app::Seen) -> Vec<Entry> {
        self.library.entries(&|id| seen.name(id))
    }

    /// A game's or app's name: as the Library names it, or as it was seen in front.
    pub fn name(&self, id: &str, seen: &crate::front_app::Seen) -> String {
        if id == EVERY {
            return "Every game".into();
        }
        self.entries(seen).into_iter().find(|e| e.id == id).map_or_else(|| seen.name(id), |e| e.name)
    }

    /// The id a window in front goes by: a game's window by its game's (the
    /// game whose process it comes from), any other by its app's.
    pub fn front_id(&self, f: &crate::front_app::Front) -> String {
        if f.pid != 0 {
            if let Some(id) = games::game_of(f.pid, &self.running) {
                return id;
            }
        }
        f.app()
    }

    /// What a device's row shows on a page: the choice it has there, whether
    /// the page chose it itself (where a game would otherwise follow Every
    /// game's), and what it follows.
    fn row_value(&self, id: &str, d: &Device) -> (Choice, bool, Choice) {
        let every = if self.library.is_game(id) { self.library.choice(EVERY, &d.key).unwrap_or(Choice::Stay) } else { Choice::Stay };
        if id == EVERY {
            return (every.clone(), true, every);
        }
        match self.library.choice(id, &d.key) {
            Some(c) => (c, true, every),
            None => (every.clone(), false, every),
        }
    }

    // ------------------------------------------------------------ the page

    /// The page as it is now, into the window: `devices` are those a page's
    /// Devices section lists, `scale` the window's pixels to a logical pixel.
    pub fn show(&mut self, ui: &AppWindow, devices: &[Device], sections: Vec<Section>, scale: f32, seen: &crate::front_app::Seen) {
        self.rows = devices.to_vec();
        self.sections = sections;
        let page_w = ui.get_gl_width();
        let all = self.entries(seen);
        // a page of its own that's no longer there goes back to the Library
        if let Some(id) = &self.open {
            if id != EVERY && !all.iter().any(|e| &e.id == id) {
                self.open = None;
            }
        }
        let said = self.said.clone();
        ui.set_gl_said(said.as_ref().map_or_else(Default::default, |(s, _, _)| s.as_str().into()));
        ui.set_gl_said_bad(said.is_some_and(|(_, b, _)| b));
        ui.set_gl_undo_shown(self.undo.is_some() || self.script_undo.is_some() || self.reset_undo.is_some());
        match self.open.clone() {
            None => self.show_tabs(ui, &all, page_w, scale),
            Some(id) => self.show_page(ui, &all, &id, devices, page_w, scale),
        }
        self.show_form(ui, &all, seen, scale);
    }

    fn show_tabs(&mut self, ui: &AppWindow, all: &[Entry], page_w: f32, scale: f32) {
        ui.set_gl_view(0);
        crate::front::set_rows(&self.toc, Vec::new());
        ui.set_gl_tab(self.tab as i32);
        crate::front::set_rows(&self.cells, Vec::new());
        crate::front::set_rows(&self.facts, Vec::new());
        ui.set_gl_has_hero(false);
        let apps_tab = self.tab == 2;
        ui.set_gl_hint(if apps_tab { "Select an app to choose what changes while the app is in front." } else { "Select a game to choose what changes while you play." }.into());
        ui.set_gl_find_hint(if apps_tab { "Find an app" } else { "Find a game" }.into());
        let shop_word = match self.shop {
            Shop::All => "All stores",
            Shop::Steam => "Steam",
            Shop::Hand => "Added by hand",
        };
        let order_word = match self.order {
            Order::Recent => "Recently played",
            Order::Name => "Name",
            Order::Most => "Most played",
        };
        ui.set_gl_store(shop_word.into());
        ui.set_gl_store_w(crate::form::drop_width(shop_word));
        ui.set_gl_sort(order_word.into());
        ui.set_gl_sort_w(crate::form::drop_width(order_word));
        let q = self.query.trim().to_lowercase();
        let found = |e: &&Entry| q.is_empty() || e.name.to_lowercase().contains(&q);
        if apps_tab {
            let mut list: Vec<&Entry> = all.iter().filter(|e| e.kind == Kind::App).filter(found).collect();
            list.sort_by_key(|e| games::sort_key(&e.name));
            self.shown = list.iter().map(|e| e.id.clone()).collect();
            let rows: Vec<LibApp> = list
                .iter()
                .map(|e| {
                    let icon = self.icon_for(e, crate::tokens::size::APP_ICON as u32, crate::tokens::radius::CONTROL as u32, scale);
                    LibApp {
                        name: e.name.as_str().into(),
                        sub: self.changes_line(&e.id).into(),
                        from: match e.from {
                            From::Steam => "Steam",
                            From::Hand => "Added by hand",
                            From::Tied => "Tied with “Use with an app…”",
                        }
                        .into(),
                        has_icon: icon.is_some(),
                        icon: icon.unwrap_or_default(),
                    }
                })
                .collect();
            crate::front::set_rows(&self.covers, Vec::new());
            let n = rows.len();
            crate::front::set_rows(&self.apps, rows);
            let first_read = self.reading.is_some() && self.library.steam.is_none();
            let (empty, sub) = if n > 0 || first_read {
                (String::new(), String::new())
            } else if !q.is_empty() {
                (format!("No apps match “{}”.", self.query.trim()), "Add an app from the header to give it changes of its own.".to_string())
            } else {
                ("No apps yet.".to_string(), "Add an app from the header to give it changes of its own.".to_string())
            };
            ui.set_gl_empty(empty.into());
            ui.set_gl_empty_sub(sub.into());
            // (the apps' box under the head, its rows Space.inline in from it, the page's margin under it)
            ui.set_gl_page_h(HEAD_END + n as f32 * crate::tokens::size::APP + 2.0 * crate::tokens::space::INLINE + crate::tokens::space::MARGIN);
            return;
        }
        crate::front::set_rows(&self.apps, Vec::new());
        let mut list: Vec<&Entry> = all
            .iter()
            .filter(|e| if self.tab == 0 { self.library.is_favorite(&e.id) } else { e.kind == Kind::Game })
            .filter(|e| match self.shop {
                Shop::All => true,
                Shop::Steam => e.from == From::Steam,
                Shop::Hand => e.from == From::Hand,
            })
            .filter(found)
            .collect();
        match self.order {
            Order::Recent => list.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| games::sort_key(&a.name).cmp(&games::sort_key(&b.name)))),
            Order::Name => list.sort_by_key(|e| games::sort_key(&e.name)),
            Order::Most => list.sort_by(|a, b| b.minutes.cmp(&a.minutes).then_with(|| games::sort_key(&a.name).cmp(&games::sort_key(&b.name)))),
        }
        self.shown = list.iter().map(|e| e.id.clone()).collect();
        // the grid: as many columns as fit at the least width, the covers sharing the room
        let room = (page_w - 2.0 * X0).max(COVER_MIN);
        let cols = (((room + GAP_X) / (COVER_MIN + GAP_X)).floor() as usize).clamp(2, 12);
        let cw = ((room - GAP_X * (cols - 1) as f32) / cols as f32).floor();
        let ch = (cw * 1.5).round();
        // (each cover, its name Space.inline under it, then the grid's gap)
        let pitch = ch + crate::tokens::space::INLINE + crate::tokens::size::COVER_NAME + GAP_Y;
        let top = HEAD_END;
        let (pw, ph, pr) = ((cw * scale).round() as u32, (ch * scale).round() as u32, (crate::tokens::radius::CONTROL * scale).round() as u32);
        let covers: Vec<LibCover> = list
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let pic = e.art.cover.clone().and_then(|f| self.pic(Pic::Cover(f, pw, ph, pr)));
                LibCover {
                    name: e.name.as_str().into(),
                    pic: pic.unwrap_or_default(),
                    x: X0 + (i % cols) as f32 * (cw + GAP_X),
                    y: top + (i / cols) as f32 * pitch,
                    w: cw,
                    h: ch,
                    fav: self.library.is_favorite(&e.id),
                }
            })
            .collect();
        let n = covers.len();
        crate::front::set_rows(&self.covers, covers);
        let any_game = all.iter().any(|e| e.kind == Kind::Game);
        // (while Steam is first read, a moment, nothing is said: it isn't known yet)
        let first_read = self.reading.is_some() && self.library.steam.is_none();
        let (empty, sub): (String, String) = if n > 0 || first_read {
            (String::new(), String::new())
        } else if !q.is_empty() {
            (format!("No games match “{}”.", self.query.trim()), if self.tab == 0 { "Check the spelling, or look in Games if the game isn’t a favorite.".into() } else { "Check the spelling, or select Add a game for a game from outside Steam.".into() })
        } else if self.shop == Shop::Hand {
            ("No games added by hand yet.".into(), "Select Add a game to add one from outside Steam.".into())
        } else if self.tab == 0 && any_game {
            ("No favorites yet.".into(), "In Games, select the star on a game’s cover to add the game here.".into())
        } else {
            ("No games yet.".into(), "Games you install in Steam show here. Select Add a game for a game from anywhere else.".into())
        };
        ui.set_gl_empty(empty.into());
        ui.set_gl_empty_sub(sub.into());
        let rows = n.div_ceil(cols);
        ui.set_gl_page_h(top + rows as f32 * pitch - GAP_Y + crate::tokens::space::MARGIN);
    }

    /// An app's icon: Steam's, or its desktop file's.
    fn icon_for(&mut self, e: &Entry, side: u32, radius: u32, scale: f32) -> Option<slint::Image> {
        let file = e.art.icon.clone().or_else(|| if e.from == From::Steam { None } else { crate::front_app::icon_of(&e.id) })?;
        let (px, r) = ((side as f32 * scale).round() as u32, (radius as f32 * scale).round() as u32);
        self.pic(Pic::Icon(file, px, r))
    }

    /// What an app changes, in a few words, for its row on the Apps tab.
    fn changes_line(&self, id: &str) -> String {
        let chosen: Vec<(String, String)> = self
            .rows
            .iter()
            .filter_map(|d| match self.library.choice(id, &d.key)? {
                Choice::Profile(p) => Some((d.name.clone(), d.profiles.iter().find(|(r, _)| *r == p).map_or_else(|| "a profile no longer there".to_string(), |(_, n)| n.clone()))),
                Choice::Stay => None,
            })
            .collect();
        match chosen.as_slice() {
            [] => "No changes".into(),
            [(d, p)] => format!("{d} switches to {p}"),
            [(d, p), (d2, p2)] => format!("{d} switches to {p}, and {d2} to {p2}"),
            more => format!("{} devices switch profiles", more.len()),
        }
    }

    fn show_page(&mut self, ui: &AppWindow, all: &[Entry], id: &str, devices: &[Device], page_w: f32, scale: f32) {
        ui.set_gl_view(1);
        crate::front::set_rows(&self.covers, Vec::new());
        crate::front::set_rows(&self.apps, Vec::new());
        ui.set_gl_empty("".into());
        self.shown.clear();
        self.resets.clear();
        let every = id == EVERY;
        let entry = all.iter().find(|e| e.id == id).cloned();
        let name = entry.as_ref().map_or_else(|| "Every game".to_string(), |e| e.name.clone());
        let game = every || self.library.is_game(id);
        ui.set_gl_name(name.as_str().into());
        ui.set_gl_every(every);
        ui.set_gl_fav(self.library.is_favorite(id));
        ui.set_gl_playable(entry.as_ref().is_some_and(|e| e.from == From::Steam || e.program.is_some()));
        ui.set_gl_removable(entry.as_ref().is_some_and(|e| e.from != From::Steam));
        ui.set_gl_hint(
            if every {
                "Choose what every game gets. A game’s own page can change any of these.".to_string()
            } else if game {
                format!("Choose what changes while you play {name}. Ectomatter puts everything back when {name} quits.")
            } else {
                format!("Choose what changes while {name} is in front.")
            }
            .into(),
        );
        // On this page has a column of its own at the left where the sections still have
        // room beside it; without, the sections take the page
        let room = page_w - 2.0 * X0;
        let toc_w = (room - crate::tokens::size::FORM - TOC_GAP).clamp(TOC_W, TOC_W_MAX);
        let with_toc = room - (toc_w + TOC_GAP) >= SECTIONS_LEAST;
        self.left = if with_toc { X0 + toc_w + TOC_GAP } else { X0 };
        let w = if with_toc { (page_w - self.left - X0).min(crate::tokens::size::FORM) } else { room.min(crate::tokens::size::FORM).max(COLUMN_LEAST) };
        let x0 = self.left;
        // the game's picture, as wide as the page, with its facts
        let hero = entry.as_ref().and_then(|e| e.art.hero.clone().map(|h| (h, e.art.logo.clone())));
        let mut y = HEAD_END;
        if let Some((hero, logo)) = hero {
            let hw = (page_w - 2.0 * X0).max(1.0);
            let pic = self.pic(Pic::Hero(hero, logo, (hw * scale).round() as u32, (HERO_H * scale).round() as u32, (scale * 100.0).round() as u32));
            ui.set_gl_has_hero(true);
            ui.set_gl_hero(pic.unwrap_or_default());
            let e = entry.as_ref().expect("a hero is an entry's");
            let (runs, w) = facts(e);
            crate::front::set_rows(&self.facts, runs);
            ui.set_gl_facts_w(w);
            y += HERO_H + crate::tokens::space::SECTION;
        } else {
            ui.set_gl_has_hero(false);
            crate::front::set_rows(&self.facts, Vec::new());
        }
        let mut cells = Vec::new();
        // Devices: a row for each, its glyph and name and its dropdown (in gray what
        // the page follows, in ink what it chose), and under them how the switching goes
        let state = if game && !every {
            let n = devices.iter().filter(|d| self.row_value(id, d).1 && self.row_value(id, d).0 != self.row_value(id, d).2).count();
            match n {
                0 => "Same as every game".to_string(),
                n => format!("{n} changed for this game"),
            }
        } else {
            String::new()
        };
        let devices_top = y;
        y = crate::form::heading(&mut cells, x0, y, w, "Devices", "", &state);
        let box_y = y;
        let mut out = Vec::new();
        let (lx, cx) = (x0 + PAD_X, x0 + CTRL_X);
        let mut ry = box_y + BOX_IN;
        for (k, d) in devices.iter().enumerate() {
            if k > 0 {
                out.push(crate::form::rule(lx, ry, w - 2.0 * PAD_X));
            }
            let (c, own, follows) = self.row_value(id, d);
            // (its name on the control's line, or, with what it is under it, Space.tight up)
            let name_top = if d.sub.is_empty() { ry + ROW } else { ry + ROW - crate::tokens::space::TIGHT };
            // (a game's own choice, unlike Every game's: a dot beside its name puts it back)
            if game && !every && own && c != follows {
                let mid = if d.sub.is_empty() { ry + ROW + LINE / 2.0 } else { name_top + crate::tokens::line::TEXT / 2.0 };
                self.reset_dot(&mut out, Reset::Device { key: d.key.clone(), label: d.name.clone() }, lx, mid);
            }
            // (the Scene row's glyph is the scenes' own)
            let glyph = if d.kind == "scene" { -1 } else { d.glyph.map_or((d.kind == "mouse") as i32, |g| 2 + g as i32) };
            out.push(Cell { style: glyph, ..cell(kind::DEVICE, lx, ry + ROW, GLYPH, GLYPH) });
            let mut row_h = LINE;
            // (its name Space.row after its glyph)
            let (nx, nw) = (lx + GLYPH + crate::tokens::space::ROW, LABEL_W - GLYPH - crate::tokens::space::ROW);
            if d.sub.is_empty() {
                out.push(text_cell(style::LABEL, nx, ry + ROW, nw, LINE, &d.name));
            } else {
                // (its name, and what it is under it, on as many lines as it takes)
                out.push(text_cell(style::LABEL, nx, name_top, nw, crate::tokens::line::TEXT, &d.name));
                let end = crate::form::note_lines(&mut out, nx, name_top + crate::tokens::line::TEXT, nw, style::NOTE, &d.sub);
                row_h = (end - (ry + ROW)).max(LINE);
            }
            let word = choice_name(&c, d);
            // (a scene's chip in its dropdown, by its place among the Scene row's chips)
            let chip = match &c {
                Choice::Profile(p) if d.kind == "scene" => d.profiles.iter().position(|(r, _)| r == p).map_or(0.0, |i| i as f32 + 1.0),
                _ => 0.0,
            };
            out.push(Cell { text: word.as_str().into(), style: (!own && !every) as i32, value: chip, on: self.menu == Some(Menu::Device(k)), id: id_of(Ctl::LibChoice), index: k as i32, ..cell(kind::DROP, cx, ry + ROW, DROP_W, crate::tokens::size::CONTROL) });
            ry += ROW + row_h + ROW;
        }
        if devices.iter().all(|d| d.kind == "scene") {
            let end = crate::form::note_lines(&mut out, lx, ry + ROW, w - 2.0 * PAD_X, style::NOTE, "Your devices show here once Ectomatter has found them.");
            ry = end + ROW;
        } else {
            out.push(crate::form::rule(lx, ry, w - 2.0 * PAD_X));
            let who = if every { "a game".to_string() } else { name.clone() };
            let note = if devices.iter().any(|d| d.kind == "scene") {
                format!("The scene and each device’s profile switch when {who} comes to the front, and switch back when another app does.")
            } else {
                format!("Each device switches when {who} comes to the front, and switches back when another app does.")
            };
            let nw = w - PAD_X - (cx - x0);
            ry = crate::form::note_lines(&mut out, cx, ry + ROW, nw, style::NOTE, &note) + ROW;
        }
        let box_h = ry + BOX_IN - box_y;
        cells.push(cell(kind::TILE, x0, box_y, w, box_h));
        cells.extend(out);
        // On this page: Devices first, with the profiles this page switches to
        let mut toc: Vec<LibToc> = vec![self.devices_entry(id, devices, devices_top, game, every)];
        let mut y = box_y + box_h;
        self.setting_at.clear();
        self.spots.clear();
        self.script_at.clear();
        for sec in self.sections.clone() {
            y += crate::tokens::space::SECTION;
            let top = y;
            y = self.draw_section(&mut cells, &sec, id, w, y, game, every);
            toc.push(self.section_entry(&sec, id, top, game, every));
        }
        // then Ectomatter's own Custom Scripts
        y += crate::tokens::space::SECTION;
        toc.push(self.scripts_entry(id, y, game, every));
        y = self.draw_scripts(&mut cells, id, &name, w, y, game, every);
        crate::front::sync_cells(&self.cells, cells);
        // (in its column at the left, where the page has room for one)
        if with_toc {
            self.toc_at = toc.iter().map(|t| t.y.max(0.0)).collect();
            ui.set_gl_toc_x(X0);
            ui.set_gl_toc_w(toc_w);
            ui.set_gl_toc_top(devices_top);
            ui.set_gl_toc_foot(if every { self.every_foot().into() } else { "".into() });
            crate::front::set_rows(&self.toc, toc);
        } else {
            self.toc_at.clear();
            crate::front::set_rows(&self.toc, Vec::new());
        }
        ui.set_gl_page_h(y + crate::tokens::space::MARGIN);
    }

    // ------------------------------------------------------------ the plugins' sections

    /// A setting's value on this page, whether the page chose it itself, and
    /// what it follows (Every game's, for a game; else what it starts at).
    fn setting_value(&self, plugin: &str, key: &str, d: &Value, id: &str) -> (Value, bool, Value) {
        let start = d["default"].clone();
        let every = if self.library.is_game(id) || id == EVERY { self.library.plugin_values(EVERY, plugin)[key].clone() } else { Value::Null };
        let follows = if every.is_null() { start } else { every };
        if id == EVERY {
            return (follows.clone(), true, follows);
        }
        match self.library.plugin_values(id, plugin).get(key) {
            Some(v) if !v.is_null() => (v.clone(), true, follows),
            _ => (follows.clone(), false, follows),
        }
    }

    /// A setting's dropdown: its value's words, in gray where the page follows
    /// another's, open while its menu is.
    fn setting_cell(&mut self, plugin: &str, key: &str, desc: &Value, id: &str, x: f32, y: f32, w: f32) -> Cell {
        let d = &desc["settings"][key];
        let (v, own, _) = self.setting_value(plugin, key, d, id);
        let words = option_label(d, &v);
        let k = self.setting_at.len();
        self.setting_at.push((plugin.to_string(), key.to_string()));
        // (in gray where the page follows another's; outlined in amber where it's to be chosen again)
        let style = if d["warn"] == true { 2 } else { (!own && id != EVERY) as i32 };
        Cell { text: words.as_str().into(), style, on: self.menu == Some(Menu::Setting(k)), id: id_of(Ctl::LibSetting), index: k as i32, ..cell(kind::DROP, x, y, w, crate::tokens::size::CONTROL) }
    }

    /// A dot beside a label that starts at `x`, its line's middle at `y`, on a game's
    /// page for a row the game changed: selecting it puts the row back to Every game's.
    fn reset_dot(&mut self, out: &mut Vec<Cell>, r: Reset, x: f32, y: f32) {
        self.resets.push(r);
        let index = (self.resets.len() - 1) as i32;
        // (the dot in the box's padding, just left of the label, in a place to press round it)
        let r = crate::tokens::size::RESET;
        out.push(Cell { text: "Changed for this game. Select the dot to go back to Every game’s.".into(), id: id_of(Ctl::LibReset), index, ..cell(kind::RESET, x - r, y - r / 2.0, r, r) });
    }

    /// A row's dot in a plugin's section, on a game's page where the game changed
    /// one of the row's settings (`keys`) from Every game's: it puts back every
    /// one of them the game has its own of.
    fn settings_dot(&mut self, out: &mut Vec<Cell>, sec: &Section, id: &str, keys: Vec<String>, label: &str, x: f32, y: f32) {
        if id == EVERY || !self.library.is_game(id) {
            return;
        }
        let mut changed = false;
        let mut own_keys = Vec::new();
        for k in keys {
            let (v, own, follows) = self.setting_value(&sec.plugin, &k, &sec.desc["settings"][k.as_str()], id);
            if own {
                changed |= v != follows;
                own_keys.push(k);
            }
        }
        if changed {
            self.reset_dot(out, Reset::Settings { plugin: sec.plugin.clone(), keys: own_keys, label: label.to_string() }, x, y);
        }
    }

    /// A row's dot selected: the row goes back to Every game's, as the bar
    /// says, with Undo for a moment (the last thing done is the one undone).
    pub fn reset_picked(&mut self, k: usize) {
        self.commit_typed();
        self.menu = None;
        let (Some(r), Some(id)) = (self.resets.get(k).cloned(), self.open.clone()) else { return };
        let (label, undo) = match r {
            Reset::Device { key, label } => {
                let was = self.library.choice(&id, &key);
                self.library.set_choice(&id, &key, None);
                (label, ResetUndo::Device { id, key, was })
            }
            Reset::Settings { plugin, keys, label } => {
                let mine = self.library.plugin_values(&id, &plugin);
                let was = keys.iter().map(|k| (k.clone(), mine.get(k).cloned())).collect();
                for key in &keys {
                    self.library.set_plugin_value(&id, &plugin, key, None);
                }
                (label, ResetUndo::Settings { id, plugin, was })
            }
        };
        self.say(format!("Put {label} back to Every game’s."), false);
        self.reset_undo = Some(undo);
    }

    /// A setting's place among the section's other controls, its piece's index.
    fn spot(&mut self, s: Spot) -> i32 {
        self.spots.push(s);
        (self.spots.len() - 1) as i32
    }

    /// A row of a plugin's section, as the settings language lays one out
    /// (docs/core-protocol.md, section 11): its label, then down its column
    /// what it shows (a status with its dot, a folder's path, a line of
    /// actions, its settings, the settings nested under it) and its note.
    /// Where the row ends.
    #[allow(clippy::too_many_arguments)]
    fn draw_row(&mut self, out: &mut Vec<Cell>, sec: &Section, id: &str, row: &Value, x0: f32, w: f32, ry: f32) -> f32 {
        let (lx, cx) = (x0 + PAD_X, x0 + CTRL_X);
        let cw = w - PAD_X - (cx - x0);
        // (the pieces down the column, Space.inline apart)
        let gap = crate::tokens::space::INLINE;
        let settings = &sec.desc["settings"];
        // (a row whose first piece is words sits on a line of words, as a form's does)
        let words_first = row.get("status").is_some() || row.get("value").is_some();
        let line_h = if words_first { crate::tokens::line::TEXT } else { LINE };
        out.push(text_cell(style::LABEL, lx, ry + ROW, LABEL_W, line_h, row["label"].as_str().unwrap_or("")));
        let shown = row["show"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from));
        let nested = row["nest"].as_array().into_iter().flatten().filter_map(|n| n["show"].as_str().map(String::from));
        self.settings_dot(out, sec, id, shown.chain(nested).collect(), row["label"].as_str().unwrap_or(""), lx, ry + ROW + line_h / 2.0);
        // each piece down the column: the first on the label's line, the rest under it
        let mut col = Column { y: ry + ROW, first: true, line: line_h };
        let text_h = crate::tokens::line::TEXT;
        if let Some(st) = row.get("status") {
            // (its dot and its words on the label's line, as a status is on Settings)
            let y = col.place(text_h, gap);
            let tone = match st["tone"].as_str() {
                Some("good") => 1,
                Some("bad") => 2,
                Some("warn") => 3,
                _ => 0,
            };
            crate::form::status(out, cx, y, text_h, cx + cw, tone, style::VALUE_BRIGHT, st["words"].as_str().unwrap_or(""));
        }
        if let Some(v) = row.get("value") {
            let y = col.place(text_h, gap);
            out.push(text_cell(if v["quiet"] == true { style::VALUE_QUIET } else { style::VALUE_BRIGHT }, cx, y, cw, text_h, v["words"].as_str().unwrap_or("")));
        }
        if let Some(p) = row.get("path") {
            // (a folder's path, as a field holds words to read, and what's beside it at its right)
            let h = crate::tokens::size::CONTROL;
            let pad = crate::tokens::part::FIELD_PAD;
            let y = col.place(h, gap);
            let side = p["side"].as_str().unwrap_or("");
            let sw = if side.is_empty() { 0.0 } else { crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, side).ceil() + 2.0 };
            out.push(Cell { style: 1, ..cell(kind::WBOX, cx, y, cw, h) });
            out.push(text_cell(style::COMMAND, cx + pad, y, cw - 2.0 * pad - if sw > 0.0 { sw + crate::tokens::space::ROW } else { 0.0 }, h, p["words"].as_str().unwrap_or("")));
            if sw > 0.0 {
                out.push(text_cell(style::NOTE, cx + cw - pad - sw, y, sw, h, side));
            }
        }
        if let Some(acts) = row["actions"].as_array() {
            // a line of buttons, as many to a line as fit (quiet ones a TextButton's height)
            let quiet = acts.iter().any(|a| a["quiet"] == true);
            let h = if quiet { crate::tokens::size::COMPACT } else { crate::tokens::size::BUTTON };
            let mut y = col.place(h, gap);
            let mut x = cx;
            for a in acts {
                let label = a["label"].as_str().unwrap_or("");
                let off = a["disabled"] == true;
                let glyph = match a["glyph"].as_str() {
                    Some("folder") => FOLDER,
                    _ => "",
                };
                // (as wide as a TextButton or a WordButton draws its words, ui/system/controls.slint)
                let tw = crate::text::width(font::MEDIUM, font::CONTROL, label).ceil();
                let bw = if quiet {
                    2.0 * part::QUIET_PAD + if glyph.is_empty() { 0.0 } else { part::GLYPH + part::GLYPH_GAP } + tw
                } else {
                    2.0 * part::BUTTON_PAD + tw
                };
                if x > cx && x + bw > cx + cw {
                    x = cx;
                    y += h + gap;
                    col.reach(y + h);
                }
                let index = self.spot(Spot::Action { plugin: sec.plugin.clone(), action: a.clone() });
                out.push(if quiet {
                    Cell { text: label.into(), lines: glyph.into(), style: if off { 2 } else { 1 }, id: id_of(Ctl::LibSpot), index, ..cell(kind::QUIET, x, y, bw, h) }
                } else {
                    Cell { text: label.into(), style: if off { 37 } else { 0 }, id: id_of(Ctl::LibSpot), index, ..cell(kind::GHOST, x, y, bw, h) }
                });
                x += bw + gap;
            }
        }
        // its settings: checkboxes on a line (or down it, stacked), a switch's options, a
        // dropdown, a value to type, or variables
        let keys: Vec<String> = row["show"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from)).collect();
        let bools: Vec<&String> = keys.iter().filter(|k| settings[k.as_str()]["type"] == "bool").collect();
        if !bools.is_empty() {
            let stack = row["stack"] == true;
            let h = crate::tokens::size::QUIET;
            let mut x = cx;
            let mut y = col.place(h, gap);
            for (i, k) in bools.iter().enumerate() {
                let d = &settings[k.as_str()];
                let (v, _, _) = self.setting_value(&sec.plugin, k, d, id);
                let label = d["label"].as_str().unwrap_or(k);
                let bw = crate::form::check_width(label);
                if i > 0 {
                    if stack || x + bw > cx + cw {
                        x = cx;
                        y += h + if stack { crate::tokens::space::TIGHT } else { gap };
                        col.reach(y + h);
                    }
                }
                let (off, shown) = (d["disabled"] == true, d["shown"] == true);
                let style = if off && shown { 2 } else if off { 1 } else { 0 };
                let index = self.spot(Spot::Check { plugin: sec.plugin.clone(), key: k.to_string() }) ;
                out.push(Cell { text: label.into(), on: v.as_bool().unwrap_or(false) || shown, style, id: id_of(Ctl::LibCheck), index, ..cell(kind::CHECKBOX, x, y, bw, h) });
                // (checkboxes on a line Space.margin apart)
                x += bw + crate::tokens::space::MARGIN;
            }
        }
        for k in keys.iter().filter(|k| settings[k.as_str()]["type"] != "bool") {
            let d = &settings[k.as_str()];
            let (v, own, _) = self.setting_value(&sec.plugin, k, d, id);
            let followed = !own && id != EVERY;
            match d["type"].as_str() {
                Some("choice") if d["menu"] == true => {
                    let y = col.place(crate::tokens::size::CONTROL, gap);
                    // (as wide as the plugin says its words need, else as the other sections' are)
                    let dw = d["width"].as_f64().map_or(DROP_W, |w| w as f32);
                    let dc = self.setting_cell(&sec.plugin, k, &sec.desc, id, cx, y, dw.min(cw));
                    out.push(dc);
                }
                Some("choice") => {
                    // a switch, the form's: each option on its well, the one chosen lit
                    let opts = d["options"].as_array().cloned().unwrap_or_default();
                    let names: Vec<(&str, &str)> = opts.iter().map(|o| (o["label"].as_str().unwrap_or(""), "")).collect();
                    let values: Vec<Value> = opts.iter().map(|o| o.get("value").cloned().unwrap_or(Value::Null)).collect();
                    let indexes: Vec<i32> = values.iter().map(|ov| self.spot(Spot::Option { plugin: sec.plugin.clone(), key: k.to_string(), value: ov.clone() })).collect();
                    let y = col.place(crate::tokens::size::CONTROL, gap);
                    let chosen = values.iter().position(|ov| *ov == v);
                    crate::form::switch(out, cx, y, None, &names, chosen, |j| (id_of(Ctl::LibSpot), indexes[j], (opts[j]["disabled"] == true) as i32));
                }
                Some("text") => {
                    let h = crate::tokens::size::CONTROL;
                    let y = col.place(h, gap);
                    let index = self.spot(Spot::Text { plugin: sec.plugin.clone(), key: k.to_string() });
                    out.push(Cell { text: v.as_str().unwrap_or("").into(), fill: d["none"].as_str().unwrap_or("").into(), on: followed, style: 8, id: id_of(Ctl::LibText), index, ..cell(kind::FIELD, cx, y, cw, h) });
                }
                Some("pairs") => {
                    // each variable as a launch line sets it, on a well that opens its form, as a script's card does
                    let (h, pad) = (crate::tokens::size::CONTROL, crate::tokens::part::FIELD_PAD);
                    for (name, value) in v.as_object().into_iter().flatten() {
                        let y = col.place(h, gap);
                        let index = self.spot(Spot::Pair { plugin: sec.plugin.clone(), key: k.to_string(), name: name.clone() });
                        out.push(Cell { text: "".into(), style: 36, id: id_of(Ctl::LibSpot), index, ..cell(kind::LINK, cx, y, cw, h) });
                        out.push(text_cell(style::COMMAND, cx + pad, y, cw - 2.0 * pad, h, &format!("{name}={}", value.as_str().unwrap_or(""))));
                    }
                    let add = d["add"].as_str().unwrap_or("Add");
                    let aw = crate::form::button_width(add, true);
                    let y = col.place(crate::tokens::size::BUTTON, gap);
                    let index = self.spot(Spot::PairAdd { plugin: sec.plugin.clone(), key: k.to_string() });
                    out.push(Cell { text: add.into(), lines: crate::form::ADD.into(), style: 35, id: id_of(Ctl::LibSpot), index, ..cell(kind::GHOST, cx, y, aw, crate::tokens::size::BUTTON) });
                }
                _ => {}
            }
        }
        // the settings nested under it, each with its name, a shade off the box
        if let Some(nest) = row["nest"].as_array() {
            // (a well, Space.row in all round: each nested setting's name in a column of its
            // own, its dropdown Space.row after it, Space.inline between them)
            let (pad, ctl) = (crate::tokens::space::ROW, crate::tokens::size::CONTROL);
            let dw = NEST_DROP.min(cw - 3.0 * pad - NEST_LABEL);
            let h = 2.0 * pad + nest.len() as f32 * ctl + (nest.len().max(1) - 1) as f32 * gap;
            let y = col.place(h, gap);
            let well_w = NEST_LABEL + dw + 3.0 * pad;
            out.push(Cell { style: 2, ..cell(kind::WBOX, cx, y, well_w, h) });
            for (i, n) in nest.iter().enumerate() {
                let ny = y + pad + i as f32 * (ctl + gap);
                out.push(text_cell(style::FACT, cx + pad, ny, NEST_LABEL, ctl, n["label"].as_str().unwrap_or("")));
                if let Some(key) = n["show"].as_str() {
                    let dc = self.setting_cell(&sec.plugin, key, &sec.desc, id, cx + 2.0 * pad + NEST_LABEL, ny, dw);
                    out.push(dc);
                }
            }
        }
        // its note, in amber where one of its settings is to be chosen again
        if let Some(note) = row["note"].as_str().filter(|n| !n.is_empty()) {
            let warn = keys.iter().any(|k| settings[k.as_str()]["warn"] == true);
            let nw = cw.min(crate::form::READ_MOST);
            let lines = crate::text::wrap(font::REGULAR, font::NOTE, note, nw);
            // (a note Space.tight under what it's about)
            let y = col.place(lines.len() as f32 * crate::tokens::line::NOTE, crate::tokens::space::TIGHT);
            if warn {
                crate::form::note_lines_in(out, cx, y, nw, crate::tokens::ink::WARN, note);
            } else {
                crate::form::note_lines(out, cx, y, nw, style::NOTE, note);
            }
        }
        // (a row is as tall as its label's line at least)
        (ry + ROW + line_h).max(col.y) + ROW
    }

    /// The parts of a plugin's section in the settings language's own
    /// shapes: a row, a heading inside the section, or a fold (its parts
    /// drawn under its heading while it's open). Where they end; whether the
    /// last drawn was a row (a row after a row has a line between).
    #[allow(clippy::too_many_arguments)]
    fn draw_parts(&mut self, out: &mut Vec<Cell>, sec: &Section, id: &str, parts: &[Value], x0: f32, w: f32, mut ry: f32, mut after_row: bool) -> (f32, bool) {
        let (lx, inner) = (x0 + PAD_X, w - 2.0 * PAD_X);
        let line = |y: f32| crate::form::rule(lx, y, inner);
        for part in parts {
            if let Some(row) = part.get("row") {
                if after_row {
                    out.push(line(ry));
                }
                ry = self.draw_row(out, sec, id, row, x0, w, ry);
                after_row = true;
            } else if let Some(sub) = part["sub"].as_str() {
                // a heading in capitals inside the section: Space.pad over it, Space.row under
                // it (as a group's in a box)
                out.push(text_cell(style::SUB, lx, ry + crate::tokens::space::PAD, inner, crate::tokens::line::SMALL, &sub.to_uppercase()));
                // (the row after it puts Space.row between)
                ry += crate::tokens::space::PAD + crate::tokens::line::SMALL;
                after_row = false;
            } else if let Some(fold) = part.get("fold") {
                let key = format!("{}/{}", sec.plugin, fold["id"].as_str().unwrap_or(""));
                let open = self.folds.contains(&key);
                out.push(line(ry));
                let index = self.spot(Spot::Fold(key));
                // (the fold's heading: a row's height round a line of words)
                let fh = ROW + crate::tokens::line::TEXT + ROW;
                out.push(Cell { text: fold["label"].as_str().unwrap_or("").into(), on: open, id: id_of(Ctl::LibSpot), index, ..cell(kind::FOLD, lx, ry, inner, fh) });
                // (shut, the changes inside it are under the fold's own dot)
                if !open {
                    let inside = keys_in(fold["parts"].as_array().map_or(&[][..], |v| v.as_slice()), false);
                    self.settings_dot(out, sec, id, inside, fold["label"].as_str().unwrap_or(""), lx, ry + fh / 2.0);
                }
                ry += fh;
                after_row = false;
                if open {
                    let inside = fold["parts"].as_array().cloned().unwrap_or_default();
                    (ry, after_row) = self.draw_parts(out, sec, id, &inside, x0, w, ry, false);
                }
            }
        }
        (ry, after_row)
    }

    /// How many of a section's settings this game changes from Every game's,
    /// of those the section lays out (each under a dot: its row's, or a shut fold's).
    fn changed_here(&self, sec: &Section, id: &str) -> usize {
        let settings = &sec.desc["settings"];
        keys_in(sec.desc["layout"].as_array().map_or(&[][..], |v| v.as_slice()), true)
            .iter()
            .filter(|k| {
                let (v, own, follows) = self.setting_value(&sec.plugin, k, &settings[k.as_str()], id);
                own && v != follows
            })
            .count()
    }

    /// A plugin's section: its heading with its plugin's tag and, on a game's
    /// page, how many of its settings the game changes; then its box, its
    /// parts in the order the plugin lays them out. Where the section ends.
    #[allow(clippy::too_many_arguments)]
    fn draw_section(&mut self, cells: &mut Vec<Cell>, sec: &Section, id: &str, w: f32, y: f32, game: bool, every: bool) -> f32 {
        let x0 = self.left;
        let label = sec.desc["label"].as_str().unwrap_or(&sec.name).to_string();
        let state = if game && !every && sec.desc["settings"].as_object().is_some_and(|m| !m.is_empty()) {
            match self.changed_here(sec, id) {
                0 => "Same as every game".to_string(),
                n => format!("{n} changed for this game"),
            }
        } else {
            String::new()
        };
        let box_y = crate::form::heading(cells, x0, y, w, &label, "Plugin", &state);
        let (lx, cx) = (x0 + PAD_X, x0 + CTRL_X);
        let inner = w - 2.0 * PAD_X;
        let line = |x: f32, y: f32, w: f32| crate::form::rule(x, y, w);
        let mut out: Vec<Cell> = Vec::new();
        let mut ry = box_y + BOX_IN;
        let mut rows_drawn = 0usize;
        // (whether the last part drawn was a row, which the next row has a line under)
        let mut after_row = false;
        let parts = sec.desc["layout"].as_array().cloned().unwrap_or_default();
        for part in &parts {
            if let Some(grid) = part.get("grid") {
                // a column for each, its name and what it is now; a row for each setting
                let cols = grid["columns"].as_array().cloned().unwrap_or_default();
                let n = cols.len().max(1) as f32;
                let cw = ((w - PAD_X - (cx - x0) - PAD_X * (n - 1.0)) / n).floor();
                let col_x = |i: usize| cx + i as f32 * (cw + PAD_X);
                // (each column's name, and what it is under it, Space.inline above and below)
                let (tl, nl) = (crate::tokens::line::TEXT, crate::tokens::line::NOTE);
                ry += crate::tokens::space::INLINE;
                for (i, c) in cols.iter().enumerate() {
                    out.push(text_cell(style::NAME, col_x(i), ry, cw, tl, c["label"].as_str().unwrap_or("")));
                    out.push(text_cell(style::NOTE, col_x(i), ry + tl, cw, nl, c["note"].as_str().unwrap_or("")));
                }
                ry += tl + nl + crate::tokens::space::INLINE;
                for row in grid["rows"].as_array().into_iter().flatten() {
                    out.push(line(lx, ry, inner));
                    out.push(text_cell(style::LABEL, lx, ry + ROW, LABEL_W, LINE, row["label"].as_str().unwrap_or("")));
                    let keys = row["cells"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
                    self.settings_dot(&mut out, sec, id, keys, row["label"].as_str().unwrap_or(""), lx, ry + ROW + LINE / 2.0);
                    for (i, c) in row["cells"].as_array().into_iter().flatten().enumerate() {
                        match c {
                            Value::String(key) => {
                                let dc = self.setting_cell(&sec.plugin, key, &sec.desc, id, col_x(i), ry + ROW, cw);
                                out.push(dc);
                            }
                            other => out.push(text_cell(style::NOTE, col_x(i), ry + ROW, cw, LINE, other["none"].as_str().unwrap_or(""))),
                        }
                    }
                    ry += ROW + LINE + ROW;
                    rows_drawn += 1;
                }
                after_row = true;
            }
            if let Some(show) = part["show"].as_array() {
                for item in show {
                    let (key, label) = match item {
                        Value::String(k) => (k.clone(), sec.desc["settings"][k]["label"].as_str().unwrap_or(k).to_string()),
                        o => (o["id"].as_str().unwrap_or("").to_string(), o["label"].as_str().or(sec.desc["settings"][o["id"].as_str().unwrap_or("")]["label"].as_str()).unwrap_or("").to_string()),
                    };
                    if rows_drawn > 0 {
                        out.push(line(lx, ry, inner));
                    }
                    out.push(text_cell(style::LABEL, lx, ry + ROW, LABEL_W, LINE, &label));
                    self.settings_dot(&mut out, sec, id, vec![key.clone()], &label, lx, ry + ROW + LINE / 2.0);
                    let dc = self.setting_cell(&sec.plugin, &key, &sec.desc, id, cx, ry + ROW, DROP_W);
                    out.push(dc);
                    ry += ROW + LINE;
                    rows_drawn += 1;
                    if let Some(note) = part["note"].as_str().filter(|_| show.last() == Some(item)) {
                        // (Space.tight under the dropdown it's about)
                        let nw = w - PAD_X - (cx - x0);
                        ry = crate::form::note_lines(&mut out, cx, ry + crate::tokens::space::TIGHT, nw, style::NOTE, note);
                    }
                    ry += ROW;
                }
                after_row = true;
                continue;
            }
            if part.get("row").is_some() || part.get("sub").is_some() || part.get("fold").is_some() {
                (ry, after_row) = self.draw_parts(&mut out, sec, id, std::slice::from_ref(part), x0, w, ry, after_row);
                rows_drawn += 1;
                continue;
            }
            if part.get("grid").is_none() {
                if let Some(note) = part["note"].as_str() {
                    // a note alone: under the rows, in the controls' column; with nothing else, across the box
                    let (nx, nw) = if rows_drawn > 0 { (cx, w - PAD_X - (cx - x0)) } else { (lx, inner) };
                    if rows_drawn > 0 {
                        out.push(line(lx, ry, inner));
                    }
                    ry = crate::form::note_lines(&mut out, nx, ry + ROW, nw, style::NOTE, note) + ROW;
                }
            }
        }
        let box_h = ry + BOX_IN - box_y;
        cells.push(cell(kind::TILE, x0, box_y, w, box_h));
        cells.extend(out);
        box_y + box_h
    }

    /// Ectomatter's own Custom Scripts: the person's commands for this page, a
    /// card each, then Add a script and how they run. Where the section ends.
    #[allow(clippy::too_many_arguments)]
    fn draw_scripts(&mut self, cells: &mut Vec<Cell>, id: &str, name: &str, w: f32, y: f32, game: bool, every: bool) -> f32 {
        let x0 = self.left;
        let box_y = crate::form::heading(cells, x0, y, w, "Custom Scripts", "", "");
        let note = if every {
            "Scripts for every game run first when a game starts and last when the game quits.".to_string()
        } else if game {
            format!("Scripts from Every game run first when {name} starts and last when {name} quits.")
        } else {
            format!("Each script runs when {name} starts, and its undo when {name} quits.")
        };
        let mut out = Vec::new();
        let end = self.draw_commands(&mut out, id, name, x0 + PAD_X, box_y + PAD_X, w - 2.0 * PAD_X, &note, every);
        // (Space.pad under Add a script, as a box's padding)
        let box_h = end + PAD_X - box_y;
        cells.push(cell(kind::TILE, x0, box_y, w, box_h));
        cells.extend(out);
        box_y + box_h
    }

    /// The person's commands, a card each (Every game's marked so on a game's
    /// page), then Add a script with the note beside it. Where they end.
    #[allow(clippy::too_many_arguments)]
    fn draw_commands(&mut self, out: &mut Vec<Cell>, id: &str, name: &str, lx: f32, y: f32, inner: f32, note: &str, every: bool) -> f32 {
        let list: Vec<(Script, bool)> = if every { self.library.scripts(EVERY).into_iter().map(|s| (s, false)).collect() } else { self.library.scripts_for(id) };
        let mut ry = y;
        let who = if every { "a game".to_string() } else { name.to_string() };
        // a card: Space.row in all round; its name on a line a TextButton's height (Run now at
        // its right), then When it starts and When it quits, a value's line each, Space.tight
        // apart and Space.inline under the name, and Space.inline under them what it last did
        let (pad, gap) = (crate::tokens::space::ROW, crate::tokens::space::INLINE);
        let (head, tl, nl) = (crate::tokens::size::COMPACT, crate::tokens::line::TEXT, crate::tokens::line::NOTE);
        let card_h = pad + head + gap + tl + crate::tokens::space::TIGHT + tl + gap + nl + pad;
        let (kx, vx) = (lx + pad, lx + pad + SCRIPT_KEY);
        let vw = inner - 2.0 * pad - SCRIPT_KEY;
        for (s, from_every) in &list {
            let k = self.script_at.len();
            self.script_at.push(s.id);
            // the card, which opens its form; on it, its name, Every game's tag, Run now
            out.push(Cell { text: "".into(), style: 36, id: id_of(Ctl::LibScript), index: k as i32, ..cell(kind::LINK, lx, ry, inner, card_h) });
            let shown = if s.name.trim().is_empty() { "A script".to_string() } else { s.name.clone() };
            let run = if self.trying.contains(&s.id) { "Running…" } else { "Run now" };
            let rw = crate::lighting::quiet_width(run, false);
            let nw = (crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::TAB, &shown).ceil() + 2.0).min(inner - 2.0 * pad - rw - SCRIPT_KEY);
            let top = ry + pad;
            out.push(text_cell(style::NAME, kx, top, nw, head, &shown));
            if *from_every {
                crate::form::pill(out, 4, kx + nw + gap, top + (head - tl) / 2.0, "Every game");
            }
            // (its words end at the card's padding)
            crate::form::link(out, lx + inner - pad + crate::tokens::part::QUIET_PAD - rw, top, head, run, id_of(Ctl::LibScriptRun), k as i32);
            let starts = if *from_every || every { "When a game starts".to_string() } else { format!("When {who} starts") };
            let quits = if *from_every || every { "When the game quits".to_string() } else { format!("When {who} quits") };
            let mut cy = top + head + gap;
            out.push(text_cell(style::NOTE, kx, cy, SCRIPT_KEY - crate::tokens::space::TIGHT, tl, &starts));
            out.push(text_cell(style::COMMAND, vx, cy, vw, tl, &s.start));
            cy += tl + crate::tokens::space::TIGHT;
            out.push(text_cell(style::NOTE, kx, cy, SCRIPT_KEY - crate::tokens::space::TIGHT, tl, &quits));
            if s.quit.trim().is_empty() {
                out.push(text_cell(style::NOTE, vx, cy, vw, tl, "Nothing to undo"));
            } else {
                out.push(text_cell(style::COMMAND, vx, cy, vw, tl, &s.quit));
            }
            cy += tl + gap;
            let (said, bad) = result_line(s, &self.library.script_last(s.id), self.trying.contains(&s.id));
            let c = if bad { crate::tokens::color(crate::tokens::ink::BAD) } else { crate::tokens::color(crate::tokens::ink::QUIET) };
            out.push(Cell { color: c, ..text_cell(style::NOTE_IN, kx, cy, inner - 2.0 * pad, nl, &said) });
            ry += card_h + gap;
        }
        // Add a script, and how they run
        crate::form::add_row(out, lx, ry, inner, "Add a script", note, id_of(Ctl::LibScriptAdd), 0) + ry
    }

    /// On this page's entry for Devices: the profiles this page switches to.
    fn devices_entry(&self, id: &str, devices: &[Device], y: f32, game: bool, every: bool) -> LibToc {
        // (the scene last, after what each device switches to)
        let mut order: Vec<&Device> = devices.iter().filter(|d| d.kind != "scene").collect();
        order.extend(devices.iter().filter(|d| d.kind == "scene"));
        let own: Vec<String> = order
            .into_iter()
            .filter_map(|d| {
                let (c, chosen, follows) = self.row_value(id, d);
                (chosen && (every || c != follows)).then(|| match &c {
                    Choice::Profile(p) if d.kind == "scene" && p != "none" => format!("{} scene", choice_name(&c, d)),
                    _ => choice_name(&c, d),
                })
            })
            .filter(|n| n != "Don’t switch" || !every)
            .collect();
        let sub = if !own.is_empty() {
            own.join(", ")
        } else if every || !game {
            if game || every { "Don’t switch".into() } else { "No changes".into() }
        } else {
            "Same as every game".into()
        };
        LibToc { name: "Devices".into(), sub: sub.as_str().into(), set: !own.is_empty(), y: y - TOC_ABOVE }
    }

    /// On this page's entry for a plugin's section: what it changes here (in
    /// the plugin's own words, where it gives them).
    fn section_entry(&self, sec: &Section, id: &str, y: f32, game: bool, every: bool) -> LibToc {
        let name = sec.desc["label"].as_str().unwrap_or(&sec.name);
        if let Some(said) = sec.desc["summary"].as_str() {
            let sub = if !said.is_empty() { said.to_string() } else if every || !game { "No changes".into() } else { "Same as every game".into() };
            return LibToc { name: name.into(), sub: sub.as_str().into(), set: !said.is_empty(), y: y - TOC_ABOVE };
        }
        let mut own: Vec<String> = Vec::new();
        // each setting the page chose itself, by its column's name where it's in a grid
        let columns: Vec<(String, String)> = sec.desc["layout"].as_array().into_iter().flatten().filter_map(|p| p["grid"]["columns"].as_array()).flatten().map(|c| (c["id"].as_str().unwrap_or("").to_string(), c["label"].as_str().unwrap_or("").to_string())).collect();
        // (those the section lays out, as its head counts them)
        let laid = keys_in(sec.desc["layout"].as_array().map_or(&[][..], |v| v.as_slice()), true);
        if let Some(m) = sec.desc["settings"].as_object() {
            for (k, d) in m.iter().filter(|(k, _)| laid.contains(k)) {
                let (v, chosen, follows) = self.setting_value(&sec.plugin, k, d, id);
                let changes = if every { v != d["default"] } else { chosen && v != follows };
                if !changes {
                    continue;
                }
                let words = option_label(d, &v);
                let whose = columns.iter().find(|(c, _)| k.starts_with(&format!("{c}/"))).map(|(_, l)| l.clone());
                own.push(match whose {
                    Some(c) => format!("{c}: {}", words.to_lowercase()),
                    None => format!("{}: {}", d["label"].as_str().unwrap_or(k), words.to_lowercase()),
                });
            }
        }
        let n = own.len();
        let sub = if n > 3 { format!("{}, and {} more", own[..2].join(", "), n - 2) } else if n > 0 { own.join(", ") } else if every || !game { "No changes".into() } else { "Same as every game".into() };
        LibToc { name: sec.desc["label"].as_str().unwrap_or(&sec.name).into(), sub: sub.as_str().into(), set: n > 0, y: y - TOC_ABOVE }
    }

    /// On this page's entry for Custom Scripts: the page's own scripts by name.
    fn scripts_entry(&self, id: &str, y: f32, game: bool, every: bool) -> LibToc {
        let mine: Vec<String> = self.library.scripts(if every { EVERY } else { id }).iter().map(|s| if s.name.trim().is_empty() { "A script".to_string() } else { s.name.clone() }).collect();
        let n = mine.len();
        let sub = if n > 3 {
            format!("{}, and {} more", mine[..2].join(", "), n - 2)
        } else if n > 0 {
            mine.join(", ")
        } else if game && !every && !self.library.scripts(EVERY).is_empty() {
            "Same as every game".into()
        } else {
            "No scripts".into()
        };
        LibToc { name: "Custom Scripts".into(), sub: sub.as_str().into(), set: n > 0, y: y - TOC_ABOVE }
    }

    /// Under Every game's On this page: how many games it's for, and how many
    /// change something of their own.
    fn every_foot(&self) -> String {
        let all = self.library.entries(&|id| id.to_string());
        let games: Vec<&Entry> = all.iter().filter(|e| e.kind == Kind::Game).collect();
        let own = games.iter().filter(|e| self.library.has_own(&e.id)).count();
        let n = games.len();
        let used = match n {
            0 => "Used by every game you add".to_string(),
            1 => "Used by your one game".to_string(),
            n => format!("Used by all {n} games"),
        };
        let mine = match own {
            0 => "none has changes of its own".to_string(),
            1 => "1 has changes of its own".to_string(),
            k => format!("{k} have changes of their own"),
        };
        format!("{used}; {mine}. Every game is for games only: an app’s page starts empty.")
    }

    /// On this page's entry selected: its section scrolled to.
    pub fn toc_jump(&mut self, i: usize, ui: &AppWindow) {
        if let Some(y) = self.toc_at.get(i) {
            self.jumps += 1;
            ui.set_gl_jump_y(*y);
            ui.set_gl_jump_n(self.jumps);
        }
    }

    fn show_form(&mut self, ui: &AppWindow, all: &[Entry], seen: &crate::front_app::Seen, scale: f32) {
        match self.form.clone() {
            Form::None => {
                ui.set_gl_modal(0);
            }
            Form::Game(program) => {
                ui.set_gl_modal(1);
                let shown = program.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                ui.set_gl_program(shown.into());
                // (the program's name filled in once it's chosen, to keep or change)
                if let Some(p) = &program {
                    if self.named_for.as_ref() != Some(p) {
                        self.named_for = Some(p.clone());
                        ui.set_gl_name_typed(name_from(p).into());
                    }
                }
                ui.set_gl_name_hint("Filled in from the program".into());
                ui.set_gl_can_add(program.is_some());
            }
            Form::App(chosen) => {
                ui.set_gl_modal(2);
                // the apps seen in front that the Library doesn't hold yet, and aren't games
                self.offered = seen.list.iter().filter(|a| !all.iter().any(|e| e.id == a.id) && games::steam_number(&a.id).is_none()).map(|a| (a.id.clone(), a.name.clone())).collect();
                let rows: Vec<LibApp> = self
                    .offered
                    .clone()
                    .iter()
                    .map(|(id, name)| {
                        let icon = crate::front_app::icon_of(id).and_then(|f| self.pic(Pic::Icon(f, (crate::tokens::size::PICK_ICON * scale).round() as u32, (crate::tokens::radius::SMALL * scale).round() as u32)));
                        LibApp { name: name.as_str().into(), sub: "".into(), from: "".into(), has_icon: icon.is_some(), icon: icon.unwrap_or_default() }
                    })
                    .collect();
                crate::front::set_rows(&self.seen, rows);
                ui.set_gl_seen_chosen(chosen.map_or(-1, |c| c as i32));
                ui.set_gl_can_add(chosen.is_some_and(|c| c < self.offered.len()));
            }
            Form::Script(f) => {
                ui.set_gl_modal(3);
                let every = f.page == EVERY;
                let n = if every { "a game".to_string() } else { self.name(&f.page, seen) };
                let (starts, quits) = if every { ("a game starts".to_string(), "the game quits".to_string()) } else { (format!("{n} starts"), format!("{n} quits")) };
                let opened_from = self.open.clone().unwrap_or_default();
                ui.set_gl_script_title(
                    match (&f.editing, every && opened_from != EVERY) {
                        (Some(_), true) => "Edit Every game’s script",
                        (Some(_), false) => "Edit the script",
                        (None, _) => "Add a script",
                    }
                    .into(),
                );
                ui.set_gl_script_sub(format!("A command to run when {starts}, and one to undo the command when {quits}.").into());
                ui.set_gl_script_start_label(format!("When {starts}").into());
                ui.set_gl_script_quit_label(format!("When {quits}").into());
                ui.set_gl_script_wait(f.wait);
                ui.set_gl_script_keep(f.keep);
                ui.set_gl_script_limit(limit_words(f.limit).into());
                // (the game starts through Steam, so Ectomatter can hold up only its own next change)
                ui.set_gl_script_wait_label("Wait for the command to finish before Ectomatter goes on".into());
                ui.set_gl_script_keep_label(format!("Keep the command running until {quits}").into());
                ui.set_gl_script_limit_note("Ectomatter stops a command that runs longer than this.".into());
                ui.set_gl_script_editing(f.editing.is_some());
                // (a command that would be an administrator's is said no to here, before it's added)
                let warn = crate::commands::refused(&ui.get_gl_script_start()).or_else(|| crate::commands::refused(&ui.get_gl_script_quit()));
                ui.set_gl_script_warning(warn.clone().unwrap_or_default().into());
                ui.set_gl_can_add(!ui.get_gl_script_start().trim().is_empty() && warn.is_none());
            }
            Form::Var(f) => {
                ui.set_gl_modal(5);
                // (its words are its plugin's, as the setting describes the pairs)
                let d = self.sections.iter().find(|s| s.plugin == f.plugin).map(|s| s.desc["settings"][f.key.as_str()].clone()).unwrap_or_default();
                let words = |k: &str, or: &str| d[k].as_str().unwrap_or(or).to_string();
                ui.set_gl_var_title(if f.editing.is_some() { words("change", "Change it") } else { words("add", "Add") }.into());
                ui.set_gl_var_sub(words("note", "").into());
                ui.set_gl_var_name_label(words("name", "Name").into());
                ui.set_gl_var_value_label(words("value", "Value").into());
                ui.set_gl_var_name_hint(d["hints"][0].as_str().unwrap_or("").into());
                ui.set_gl_var_value_hint(d["hints"][1].as_str().unwrap_or("").into());
                ui.set_gl_var_editing(f.editing.is_some());
                let (ok, warn) = self.var_check(&f, ui);
                ui.set_gl_var_warning(warn.into());
                ui.set_gl_can_add(ok);
            }
        }
    }

    // ------------------------------------------------------------ what the page asks

    pub fn tab_chosen(&mut self, i: i32, ui: &AppWindow) {
        let i = i.clamp(0, 2) as usize;
        if i != self.tab {
            self.tab = i;
            self.query.clear();
            ui.set_gl_query("".into());
        }
    }

    pub fn find(&mut self, t: &str) {
        self.query = t.to_string();
    }

    /// A menu's rows, for the menu being opened.
    pub fn menu_rows(&self, m: &Menu, all_counts: (usize, usize)) -> Vec<MenuRow> {
        let row = |label: &str, tick: bool, sub: String| MenuRow { kind: 0, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() };
        match m {
            Menu::Shop => vec![
                row("All stores", self.shop == Shop::All, String::new()),
                row("Steam", self.shop == Shop::Steam, all_counts.0.to_string()),
                row("Added by hand", self.shop == Shop::Hand, all_counts.1.to_string()),
            ],
            Menu::Order => vec![row("Recently played", self.order == Order::Recent, String::new()), row("Name", self.order == Order::Name, String::new()), row("Most played", self.order == Order::Most, String::new())],
            Menu::Setting(k) => {
                let Some((plugin, key)) = self.setting_at.get(*k).cloned() else { return Vec::new() };
                let Some(sec) = self.sections.iter().find(|x| x.plugin == plugin) else { return Vec::new() };
                let d = &sec.desc["settings"][&key];
                let id = self.open.clone().unwrap_or_default();
                let (v, _, follows) = self.setting_value(&plugin, &key, d, &id);
                let game = id != EVERY && self.library.is_game(&id);
                d["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| {
                        let ov = o.get("value").unwrap_or(o);
                        let label = o["label"].as_str().map(String::from).unwrap_or_else(|| ov.as_str().unwrap_or("").to_string());
                        // (on a game's page, Every game's is said: choosing it follows Every game again)
                        row(&label, *ov == v, if game && *ov == follows { "Every game’s".into() } else { String::new() })
                    })
                    .collect()
            }
            Menu::Limit => {
                let now = if let Form::Script(f) = &self.form { f.limit } else { 10 };
                LIMITS.iter().map(|n| row(&limit_words(*n), *n == now, String::new())).collect()
            }
            Menu::Device(k) => {
                let Some(d) = self.rows.get(*k) else { return Vec::new() };
                let id = self.open.clone().unwrap_or_default();
                let (c, _, follows) = self.row_value(&id, d);
                // (on a game's page, what Every game has is said: choosing it follows Every game again)
                let every = |x: &Choice| if id != EVERY && self.library.is_game(&id) && *x == follows { "Every game’s".to_string() } else { String::new() };
                if d.kind == "scene" {
                    // (each with its chip, Don't switch a chip's room on its own, a line between groups)
                    return scene_rows(d)
                        .into_iter()
                        .map(|x| match x {
                            None => MenuRow { kind: 2, ..Default::default() },
                            Some(Choice::Stay) => MenuRow { chipped: true, ..row("Don’t switch", c == Choice::Stay, every(&Choice::Stay)) },
                            Some(Choice::Profile(p)) => {
                                let i = d.profiles.iter().position(|(r, _)| *r == p).unwrap_or(0);
                                let ch = Choice::Profile(p.clone());
                                MenuRow { chip: d.chips.get(i).cloned().unwrap_or_default(), chipped: true, ..row(&d.profiles[i].1, c == ch, every(&ch)) }
                            }
                        })
                        .collect();
                }
                std::iter::once(row("Don’t switch", c == Choice::Stay, every(&Choice::Stay)))
                    .chain(d.profiles.iter().map(|(r, n)| {
                        let p = Choice::Profile(r.clone());
                        row(n, c == p, every(&p))
                    }))
                    .collect()
            }
        }
    }

    /// How many games each store has, for the store menu.
    pub fn counts(&self, seen: &crate::front_app::Seen) -> (usize, usize) {
        let all = self.entries(seen);
        let games = || all.iter().filter(|e| e.kind == Kind::Game);
        (games().filter(|e| e.from == From::Steam).count(), games().filter(|e| e.from == From::Hand).count())
    }

    /// A row of the open menu chosen.
    pub fn chose(&mut self, i: usize) {
        self.commit_typed();
        let Some(m) = self.menu.take() else { return };
        match m {
            Menu::Shop => self.shop = [Shop::All, Shop::Steam, Shop::Hand].get(i).copied().unwrap_or(self.shop),
            Menu::Order => self.order = [Order::Recent, Order::Name, Order::Most].get(i).copied().unwrap_or(self.order),
            Menu::Setting(k) => {
                let (Some((plugin, key)), Some(id)) = (self.setting_at.get(k).cloned(), self.open.clone()) else { return };
                let Some(sec) = self.sections.iter().find(|x| x.plugin == plugin).cloned() else { return };
                let d = &sec.desc["settings"][&key];
                let Some(o) = d["options"].as_array().and_then(|a| a.get(i)) else { return };
                let want = o.get("value").unwrap_or(o).clone();
                let (_, _, follows) = self.setting_value(&plugin, &key, d, &id);
                // what the page follows anyway is no choice of its own
                let keep = if id == EVERY { (want != d["default"]).then_some(want) } else { (want != follows).then_some(want) };
                self.library.set_plugin_value(&id, &plugin, &key, keep);
            }
            Menu::Limit => {
                if let (Form::Script(f), Some(n)) = (&mut self.form, LIMITS.get(i)) {
                    f.limit = *n;
                }
            }
            Menu::Device(k) => {
                let (Some(d), Some(id)) = (self.rows.get(k).cloned(), self.open.clone()) else { return };
                let want = if d.kind == "scene" {
                    let Some(Some(w)) = scene_rows(&d).get(i).cloned() else { return };
                    w
                } else if i == 0 {
                    Choice::Stay
                } else if let Some((r, _)) = d.profiles.get(i - 1) {
                    Choice::Profile(r.clone())
                } else {
                    return;
                };
                let (_, _, follows) = self.row_value(&id, &d);
                let keep = if id == EVERY {
                    // (Every game's own Don't switch is no choice at all)
                    (want != Choice::Stay).then_some(want)
                } else if self.library.is_game(&id) {
                    // choosing what Every game has goes back to following it
                    (want != follows).then_some(want)
                } else {
                    (want != Choice::Stay).then_some(want)
                };
                self.library.set_choice(&id, &d.key, keep);
            }
        }
    }

    // ------------------------------------------------------------ the sections' other controls

    /// A value set on the open page in a plugin's section (what the page
    /// follows anyway is no choice of its own, as with a dropdown's).
    fn set_value(&mut self, plugin: &str, key: &str, want: Value) {
        let Some(id) = self.open.clone() else { return };
        let Some(sec) = self.sections.iter().find(|x| x.plugin == plugin).cloned() else { return };
        let d = &sec.desc["settings"][key];
        let (_, _, follows) = self.setting_value(plugin, key, d, &id);
        let keep = if id == EVERY { (want != d["default"]).then_some(want) } else { (want != follows).then_some(want) };
        self.library.set_plugin_value(&id, plugin, key, keep);
    }

    /// A setting's variables as the page has them now: its own, or those it follows.
    fn pairs_of(&self, plugin: &str, key: &str) -> serde_json::Map<String, Value> {
        let id = self.open.clone().unwrap_or_default();
        let d = self.sections.iter().find(|x| x.plugin == plugin).map(|s| s.desc["settings"][key].clone()).unwrap_or(Value::Null);
        self.setting_value(plugin, key, &d, &id).0.as_object().cloned().unwrap_or_default()
    }

    /// One of a section's controls used (LibSpot, by its place): an option
    /// chosen, a fold opened or shut, a variable's form opened. An action is
    /// the window's to make: its plugin and its id.
    pub fn spot_picked(&mut self, k: usize, ui: &AppWindow) -> Option<(String, Value)> {
        self.commit_typed();
        let s = self.spots.get(k)?.clone();
        self.menu = None;
        match s {
            Spot::Option { plugin, key, value } => self.set_value(&plugin, &key, value),
            Spot::Fold(f) => {
                if !self.folds.remove(&f) {
                    self.folds.insert(f);
                }
            }
            Spot::Action { plugin, action } => return Some((plugin, action)),
            Spot::PairAdd { plugin, key } => {
                self.form = Form::Var(VarForm { plugin, key, editing: None });
                ui.set_gl_var_name("".into());
                ui.set_gl_var_value("".into());
            }
            Spot::Pair { plugin, key, name } => {
                let value = self.pairs_of(&plugin, &key).get(&name).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
                ui.set_gl_var_name(name.as_str().into());
                ui.set_gl_var_value(value.as_str().into());
                self.form = Form::Var(VarForm { plugin, key, editing: Some(name) });
            }
            Spot::Check { .. } | Spot::Text { .. } => {}
        }
        None
    }

    /// A checkbox checked or unchecked (LibCheck: its place twice, and one for checked).
    pub fn check_picked(&mut self, index: usize) {
        self.commit_typed();
        if let Some(Spot::Check { plugin, key }) = self.spots.get(index / 2).cloned() {
            self.menu = None;
            self.set_value(&plugin, &key, Value::Bool(index % 2 == 1));
        }
    }

    /// What's typed in a value's box, kept until the box is left, Enter is
    /// pressed, the typing pauses, or another control is used.
    pub fn typing(&mut self, k: usize, t: String) {
        if let Some(s) = self.spots.get(k).cloned() {
            self.typed = Some((s, t, Instant::now()));
        }
    }

    /// A value's box left (`how` 1), or put back with Escape (2).
    pub fn text_done(&mut self, how: u8) {
        if how == 2 {
            self.typed = None;
        } else {
            self.commit_typed();
        }
    }

    /// What's typed set, once the typing has paused a moment. True if it was.
    pub fn typed_settled(&mut self) -> bool {
        if self.typed.as_ref().is_some_and(|(_, _, t)| t.elapsed() > Duration::from_millis(800)) {
            self.commit_typed();
            return true;
        }
        false
    }

    /// What's typed in a value's box set, if anything is.
    fn commit_typed(&mut self) {
        if let Some((Spot::Text { plugin, key }, text, _)) = self.typed.take() {
            self.set_value(&plugin, &key, Value::String(text.trim().to_string()));
        }
    }

    /// Add or Save in a variable's form: whether it can be, and the warning
    /// that says why not (none, for a name not yet typed).
    fn var_check(&self, f: &VarForm, ui: &AppWindow) -> (bool, String) {
        let name = ui.get_gl_var_name().trim().to_string();
        if name.is_empty() {
            return (false, String::new());
        }
        if !var_name_ok(&name) {
            return (false, "A name has only letters, numbers and underscores, and doesn’t start with a number.".into());
        }
        if f.editing.as_deref() != Some(name.as_str()) && self.pairs_of(&f.plugin, &f.key).contains_key(&name) {
            return (false, format!("{name} is set already. Select it in the list to change it."));
        }
        (true, String::new())
    }

    /// The variable being changed taken off.
    pub fn remove_var(&mut self) {
        let Form::Var(f) = self.form.clone() else { return };
        let Some(name) = f.editing else { return };
        let mut m = self.pairs_of(&f.plugin, &f.key);
        m.remove(&name);
        self.set_value(&f.plugin, &f.key, Value::Object(m));
        self.form = Form::None;
        self.say(format!("Removed the variable {name}."), false);
    }

    /// A cover, or a row on the Apps tab, selected: its own page.
    pub fn open_at(&mut self, i: usize) {
        if let Some(id) = self.shown.get(i).cloned() {
            self.left_page();
            self.open = Some(id);
            self.menu = None;
        }
    }

    /// A page left: what was typed on it set first.
    fn left_page(&mut self) {
        self.commit_typed();
    }

    /// A cover's star: a favorite, or no longer.
    pub fn star_at(&mut self, i: usize) {
        if let Some(id) = self.shown.get(i).cloned() {
            let on = !self.library.is_favorite(&id);
            self.library.set_favorite(&id, on);
        }
    }

    pub fn every(&mut self) {
        self.left_page();
        self.open = Some(EVERY.to_string());
    }

    pub fn back(&mut self) {
        self.left_page();
        self.open = None;
        self.menu = None;
    }

    pub fn star_page(&mut self) {
        if let Some(id) = self.open.clone().filter(|id| id != EVERY) {
            let on = !self.library.is_favorite(&id);
            self.library.set_favorite(&id, on);
        }
    }

    /// Play: Steam asked to start the game, as a desktop shortcut does; a game
    /// added by hand started by its program.
    pub fn play(&mut self, seen: &crate::front_app::Seen) {
        // (what was just typed on the page set first, for the game's start to have it)
        self.commit_typed();
        let Some(id) = self.open.clone() else { return };
        let Some(e) = self.entries(seen).into_iter().find(|e| e.id == id) else { return };
        let done = if let Some(n) = games::steam_number(&e.id) {
            games::open(&format!("steam://rungameid/{n}")).map(|_| format!("Asked Steam to start {}.", e.name))
        } else if let Some(p) = &e.program {
            games::start(p).map(|_| format!("Started {}.", e.name))
        } else {
            return;
        };
        match done {
            Ok(s) => self.say(s, false),
            Err(why) => self.say(format!("{} didn’t start: {why}", e.name), true),
        }
    }

    /// Remove, on the page of a game or app added by hand (or there for its
    /// ties): taken out of the Library, with Undo for a moment.
    pub fn remove(&mut self, seen: &crate::front_app::Seen) {
        let Some(id) = self.open.clone().filter(|id| id != EVERY) else { return };
        let name = self.name(&id, seen);
        let removed = self.library.remove(&id);
        self.tell_watch();
        self.open = None;
        self.say(format!("Removed {name} from the Library."), false);
        self.undo = Some(removed);
    }

    /// Undo, after Remove: the script back on its page, or the game or app
    /// back, and its page.
    pub fn undo(&mut self) {
        if let Some(r) = self.reset_undo.take() {
            // (each as it was, unless it's been chosen again since: that choice stays)
            match r {
                ResetUndo::Device { id, key, was } => {
                    if self.library.choice(&id, &key).is_none() {
                        self.library.set_choice(&id, &key, was);
                    }
                }
                ResetUndo::Settings { id, plugin, was } => {
                    let now = self.library.plugin_values(&id, &plugin);
                    for (key, v) in was {
                        if now.get(&key).is_none_or(|n| n.is_null()) {
                            self.library.set_plugin_value(&id, &plugin, &key, v);
                        }
                    }
                }
            }
            self.said = None;
            return;
        }
        if let Some(r) = self.script_undo.take() {
            self.library.restore_script(r);
            self.said = None;
            return;
        }
        let Some(r) = self.undo.take() else { return };
        let id = r.id.clone();
        self.library.restore(r);
        self.tell_watch();
        self.open = Some(id);
        self.said = None;
    }

    /// The header's Add: Add a game on the games' tabs, Add an app on Apps.
    pub fn add(&mut self, ui: &AppWindow) {
        self.form = if self.tab == 2 { Form::App(None) } else { Form::Game(None) };
        ui.set_gl_name_typed("".into());
        self.named_for = None;
        self.menu = None;
    }

    pub fn choose_program(&mut self) {
        if self.picking.is_some() {
            return;
        }
        // (offscreen, ECTOMATTER_PROGRAM's file, as if chosen there)
        let rx = match std::env::var_os("ECTOMATTER_PROGRAM").filter(|_| crate::desk::offscreen()) {
            Some(f) => {
                let (tx, rx) = mpsc::channel();
                let _ = tx.send(Some(PathBuf::from(f)));
                rx
            }
            None => crate::picker::open_file("Choose the game’s program", "Programs", &["*"]),
        };
        self.picking = Some(rx);
        if crate::desk::offscreen() {
            self.poll(false);
        }
    }

    pub fn seen_pick(&mut self, i: i32) {
        if let Form::App(_) = self.form {
            // (selected again, the chosen app lets go)
            let i = usize::try_from(i).ok();
            self.form = Form::App(if i == self.form_chosen() { None } else { i });
        }
    }

    fn form_chosen(&self) -> Option<usize> {
        match self.form {
            Form::App(c) => c,
            _ => None,
        }
    }

    /// Add, in the form: the game or app joins the Library, and its page opens.
    pub fn add_done(&mut self, ui: &AppWindow) {
        match self.form.clone() {
            Form::Game(Some(p)) => {
                let typed = ui.get_gl_name_typed().trim().to_string();
                let name = if typed.is_empty() { name_from(&p) } else { typed };
                let id = self.library.add_game(&p, &name);
                self.tell_watch();
                self.open = Some(id);
                self.say(format!("Added {name} to Games."), false);
            }
            Form::App(Some(i)) => {
                let Some((id, name)) = self.offered.get(i).cloned() else { return };
                self.library.add_app(&id, &name);
                self.open = Some(id);
                self.say(format!("Added {name} to Apps."), false);
            }
            Form::Script(f) => {
                let start = ui.get_gl_script_start().trim().to_string();
                if start.is_empty() || crate::commands::refused(&start).is_some() || crate::commands::refused(&ui.get_gl_script_quit()).is_some() {
                    return;
                }
                let typed = ui.get_gl_script_name().trim().to_string();
                let name = if typed.is_empty() { name_of_command(&start) } else { typed };
                let s = Script { id: f.editing.unwrap_or(0), name: name.clone(), start, quit: ui.get_gl_script_quit().trim().to_string(), wait: f.wait && !f.keep, keep: f.keep, limit: f.limit };
                self.library.put_script(&f.page, s);
                self.say(if f.editing.is_some() { format!("Saved the script {name}.") } else { format!("Added the script {name}.") }, false);
            }
            Form::Var(f) => {
                if !self.var_check(&f, ui).0 {
                    return;
                }
                let name = ui.get_gl_var_name().trim().to_string();
                let mut m = self.pairs_of(&f.plugin, &f.key);
                if let Some(old) = &f.editing {
                    m.remove(old);
                }
                m.insert(name.clone(), Value::String(ui.get_gl_var_value().to_string()));
                self.set_value(&f.plugin, &f.key, Value::Object(m));
                self.say(if f.editing.is_some() { format!("Saved the variable {name}.") } else { format!("Added the variable {name}.") }, false);
            }
            _ => return,
        }
        self.form = Form::None;
    }

    // ------------------------------------------------------------ the scripts

    /// Add a script, on the page open.
    pub fn add_script(&mut self, ui: &AppWindow) {
        let Some(page) = self.open.clone() else { return };
        self.form = Form::Script(ScriptForm { page, editing: None, wait: true, keep: false, limit: 10 });
        ui.set_gl_script_name("".into());
        ui.set_gl_script_start("".into());
        ui.set_gl_script_quit("".into());
        self.menu = None;
    }

    /// A script's card selected: its form, to change it.
    pub fn edit_script(&mut self, k: usize, ui: &AppWindow) {
        let Some(n) = self.script_at.get(k).copied() else { return };
        let Some(s) = self.library.script(n) else { return };
        let page = self.library.script_page(n).unwrap_or_else(|| self.open.clone().unwrap_or_default());
        self.form = Form::Script(ScriptForm { page, editing: Some(n), wait: s.wait, keep: s.keep, limit: s.limit });
        ui.set_gl_script_name(s.name.as_str().into());
        ui.set_gl_script_start(s.start.as_str().into());
        ui.set_gl_script_quit(s.quit.as_str().into());
        self.menu = None;
    }

    /// The script being changed taken off its page, with Undo for a moment.
    pub fn remove_script(&mut self) {
        let Form::Script(f) = self.form.clone() else { return };
        let Some(n) = f.editing else { return };
        let name = self.library.script(n).map(|s| s.name).unwrap_or_default();
        let removed = self.library.remove_script(n);
        self.form = Form::None;
        self.say(format!("Removed the script {name}."), false);
        self.script_undo = removed;
    }

    pub fn script_wait(&mut self) {
        if let Form::Script(f) = &mut self.form {
            f.wait = !f.wait;
            if f.wait {
                f.keep = false;
            }
        }
    }

    pub fn script_keep(&mut self) {
        if let Form::Script(f) = &mut self.form {
            f.keep = !f.keep;
            if f.keep {
                f.wait = false;
            }
        }
    }

    /// Run now on a script's card: the script to try (none while it's being tried).
    pub fn run_now(&mut self, k: usize) -> Option<Script> {
        let n = *self.script_at.get(k)?;
        if !self.trying.insert(n) {
            return None;
        }
        self.library.script(n)
    }

    /// A script ran: kept with it, and no longer being tried once its undo has
    /// run too (or its start failed, or it has none).
    pub fn script_ran(&mut self, n: u64, game: &str, quit: bool, ran: &Ran, at: u64) {
        self.library.script_ran(n, game, quit, ran, at);
        if game.is_empty() && (quit || !ran.ok() || self.library.script(n).is_none_or(|s| s.quit.trim().is_empty())) {
            self.trying.remove(&n);
        }
    }

    pub fn close_form(&mut self) -> bool {
        if self.form == Form::None {
            return false;
        }
        self.form = Form::None;
        true
    }

    /// Escape on the Library: the form shut, else a page of its own back to the Library.
    pub fn escape(&mut self) -> bool {
        if self.close_form() {
            return true;
        }
        if self.open.is_some() {
            self.back();
            return true;
        }
        false
    }
}

/// A Ctl's id, as the pieces carry it.
fn id_of(c: Ctl) -> i32 {
    id(c)
}

/// A setting's value in its own words: its option's, or what it starts at.
fn option_label(d: &Value, v: &Value) -> String {
    let opts = d["options"].as_array().cloned().unwrap_or_default();
    let of = |want: &Value| opts.iter().find(|o| o.get("value").unwrap_or(o) == want).map(|o| o["label"].as_str().map(String::from).unwrap_or_else(|| o.as_str().unwrap_or("").to_string()));
    if let Some(l) = of(v) {
        return l;
    }
    if v.is_null() {
        if let Some(l) = of(&d["default"]) {
            return l;
        }
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// When something happened, as a script's line says it: "today at 3:04 PM",
/// or "Oct 21 at 3:04 PM".
fn when_words(secs: u64) -> String {
    let t = crate::lcd_page::Moment::at(secs as libc::time_t);
    let now = crate::lcd_page::Moment::now();
    let clock = crate::lcd_page::twelve_hour(t.hour, t.minute);
    if (t.year, t.month, t.day) == (now.year, now.month, now.day) {
        return format!("today at {clock}");
    }
    let d = format!("{} {}", MONTHS[t.month as usize % 12], t.day);
    if t.year == now.year { format!("{d} at {clock}") } else { format!("{d}, {} at {clock}", t.year) }
}

/// A sentence's first letter made a capital.
fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// The line under a script: how it last ran, in red where it went wrong; or,
/// before it has run, what Ectomatter does with it.
fn result_line(s: &Script, last: &Value, trying: bool) -> (String, bool) {
    if trying {
        return ("Running now…".into(), false);
    }
    let Some(start) = Ran::of(&last["start"]) else {
        let how = if s.keep {
            "Ectomatter keeps it running until the game quits.".to_string()
        } else if s.wait {
            format!("Ectomatter waits up to {} for it to finish.", limit_words(s.limit))
        } else {
            "Ectomatter doesn’t wait for it to finish.".to_string()
        };
        return (format!("Not run yet. {how}"), false);
    };
    if !start.ok() {
        return (format!("{}.", capital(&start.words())), true);
    }
    let game = last["game"].as_str().unwrap_or("");
    let when = last["at"].as_u64().map(when_words).unwrap_or_default();
    let lead = if game.is_empty() { format!("Tried {when}") } else { format!("Ran as {game} started, {when}") };
    let mut said = format!("{lead}: {}.", start.words());
    let mut bad = false;
    if let Some(q) = Ran::of(&last["quit"]) {
        said.push_str(&format!(" Its undo {}.", q.words()));
        bad = !q.ok();
    }
    (said, bad)
}

/// A choice as a dropdown shows it.
fn choice_name(c: &Choice, d: &Device) -> String {
    match c {
        Choice::Stay => "Don’t switch".into(),
        Choice::Profile(p) => d.profiles.iter().find(|(r, _)| r == p).map_or_else(|| "A profile no longer there".to_string(), |(_, n)| n.clone()),
    }
}

/// A script's name where none is typed: its program's, as a game's is.
fn name_of_command(cmd: &str) -> String {
    let first = cmd.split_whitespace().find(|w| !w.contains('=')).unwrap_or(cmd);
    name_from(Path::new(first.trim_start_matches('~')))
}

/// A game's name from its program: the file's name, its separators spaces,
/// its first letter a capital.
pub fn name_from(p: &Path) -> String {
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let spaced: String = stem.chars().map(|c| if c == '_' || c == '-' || c == '.' { ' ' } else { c }).collect();
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// The facts under a game's picture, each a number standing out in ink and
/// its words: where it's from, how long it's been played, and when it was
/// last played. Each run placed where it starts, and the width of them all.
fn facts(e: &Entry) -> (Vec<LibFact>, f32) {
    let mut said: Vec<(String, String)> = vec![(String::new(), if e.from == From::Steam { "Steam" } else { "Added by hand" }.to_string())];
    let m = e.minutes;
    if e.from == From::Steam {
        said.push(match m {
            0 => (String::new(), "Not played yet".into()),
            1 => ("1 minute".into(), "played".into()),
            2..=59 => (format!("{m} minutes"), "played".into()),
            60..=119 => ("1 hour".into(), "played".into()),
            _ => (format!("{} hours", (m as f64 / 60.0).round()), "played".into()),
        });
    }
    if e.last > 0 {
        said.push((String::new(), last_played(e.last, crate::lcd_page::Moment::now())));
    }
    let mut runs = Vec::new();
    let mut x = 0.0f32;
    for (k, (strong, words)) in said.iter().enumerate() {
        // (the facts Space.pad apart, a number Space.tight before its words)
        if k > 0 {
            x += crate::tokens::space::PAD;
        }
        if !strong.is_empty() {
            runs.push(LibFact { text: strong.as_str().into(), strong: true, x });
            x += crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::NOTE, strong).ceil() + crate::tokens::space::TIGHT;
        }
        runs.push(LibFact { text: words.as_str().into(), strong: false, x });
        x += crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, words).ceil();
    }
    (runs, x)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Last played today", "Last played Aug 23", or with its year when not this year's.
pub fn last_played(secs: u64, now: crate::lcd_page::Moment) -> String {
    let t = crate::lcd_page::Moment::at(secs as libc::time_t);
    if (t.year, t.month, t.day) == (now.year, now.month, now.day) {
        return "Last played today".into();
    }
    let d = format!("{} {}", MONTHS[t.month as usize % 12], t.day);
    if t.year == now.year { format!("Last played {d}") } else { format!("Last played {d}, {}", t.year) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_programs_name_is_its_files() {
        assert_eq!(name_from(Path::new("/home/u/Desktop/Xbox 360/xenia_canary.exe")), "Xenia canary");
        assert_eq!(name_from(Path::new("/games/PSVR2Toolkit.UnitySetup.x86_64")), "PSVR2Toolkit UnitySetup");
    }

    #[test]
    fn last_played_says_the_day() {
        let now = crate::lcd_page::Moment { hour: 12, minute: 0, weekday: 2, day: 21, month: 9, year: 2026 };
        // (the moments are read in this computer's time zone, as the page reads them)
        let at = |y: i32, mo: u32, d: u32| {
            let mut t: libc::tm = unsafe { std::mem::zeroed() };
            t.tm_year = y - 1900;
            t.tm_mon = mo as i32;
            t.tm_mday = d as i32;
            t.tm_hour = 12;
            t.tm_isdst = -1;
            unsafe { libc::mktime(&mut t) as u64 }
        };
        assert_eq!(last_played(at(2026, 9, 21), now), "Last played today");
        assert_eq!(last_played(at(2026, 7, 23), now), "Last played Aug 23");
        assert_eq!(last_played(at(2025, 11, 2), now), "Last played Dec 2, 2025");
    }

    #[test]
    fn a_pictures_size_is_read_from_its_header() {
        // a 3 by 2 PNG's first bytes, and a JPEG's frame header after another segment
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&3u32.to_be_bytes());
        png.extend_from_slice(&2u32.to_be_bytes());
        let jpg: Vec<u8> = [&[0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0][..], &[0xff, 0xc0, 0, 17, 8, 0, 200, 1, 44, 3][..], &[0u8; 16][..]].concat();
        let dir = std::env::temp_dir().join(format!("ectomatter-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), &png).unwrap();
        std::fs::write(dir.join("a.jpg"), &jpg).unwrap();
        assert_eq!(size_of(&dir.join("a.png")), Some((3.0, 2.0)));
        assert_eq!(size_of(&dir.join("a.jpg")), Some((300.0, 200.0)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
