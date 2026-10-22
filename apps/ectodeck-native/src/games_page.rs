//! The Library's page, the place "library" (games.rs keeps the Library, and
//! steam.rs reads Steam's files): the games and apps on three tabs
//! (Favorites, Games and Apps), each one's own page and Every game's, and the
//! forms that add a game or an app. A game's page holds Ectodeck's own
//! section, Devices: the profile each device switches to while the game is in
//! front. The pictures (each cover, and a page's picture with its logo laid on
//! it) are drawn from Steam's files at the size they show, on a thread of
//! their own, and come in as each is ready.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::{ModelRc, VecModel};

use crate::games::{self, Choice, Entry, From, Kind, Library, Running, EVERY};
use crate::lighting::{cell, id, kind, style, text_cell, Ctl};
use crate::{AppWindow, Cell, LibApp, LibCover, LibFact, MenuRow};

/// A device as a page lists it: its key (as the Library keeps its choices),
/// its name, and its profiles, each by its id and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub key: String,
    pub name: String,
    pub profiles: Vec<(String, String)>,
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

/// A menu of the Library's, open: the store, the order, or a device's on a
/// page (by its row there).
#[derive(Clone, Debug, PartialEq)]
pub enum Menu {
    Shop,
    Order,
    Device(usize),
}

/// The form open over the Library: Add a game (the program chosen) or Add an
/// app (the app chosen, by its place in the list).
#[derive(Clone, Debug, PartialEq)]
enum Form {
    None,
    Game(Option<PathBuf>),
    App(Option<usize>),
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

/// The page's measures: its gutter, a page's column at most, and on a page the
/// label's room in a row and the dropdowns' width.
const X0: f32 = 24.0;
const COLUMN: f32 = 960.0;
const PAD_X: f32 = 24.0;
const LABEL_W: f32 = 196.0;
const DROP_W: f32 = 300.0;
/// where a page's boxes start: under the line of words at its top
const HEAD_END: f32 = 12.0 + 36.0 + 6.0;
/// the covers: at least this wide, with these gaps across and down
const COVER_MIN: f32 = 150.0;
const GAP_X: f32 = 24.0;
const GAP_Y: f32 = 22.0;

pub struct Games {
    pub library: Library,
    watch: games::Watch,
    /// the games running now, as last seen; the first look counts what was
    /// already running before Ectodeck started, which isn't a game starting
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
    pics: HashMap<Pic, slint::Image>,
    asked: HashSet<Pic>,
    painter: Painter,
    covers: Rc<VecModel<LibCover>>,
    apps: Rc<VecModel<LibApp>>,
    cells: Rc<VecModel<Cell>>,
    facts: Rc<VecModel<LibFact>>,
    seen: Rc<VecModel<LibApp>>,
}

impl Games {
    pub fn new(ui: &AppWindow) -> Games {
        let covers = Rc::new(VecModel::default());
        let apps = Rc::new(VecModel::default());
        let cells = Rc::new(VecModel::default());
        let facts = Rc::new(VecModel::default());
        let seen = Rc::new(VecModel::default());
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
            pics: HashMap::new(),
            asked: HashSet::new(),
            painter: Painter::start(),
            covers,
            apps,
            cells,
            facts,
            seen,
        };
        g.read_steam();
        g.tell_watch();
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
        if self.said.as_ref().is_some_and(|(_, _, t)| t.elapsed() > Duration::from_secs(if self.undo.is_some() { 10 } else { 6 })) {
            self.said = None;
            self.undo = None;
            changed = true;
        }
        (events, changed)
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

    fn say(&mut self, s: impl Into<String>, bad: bool) {
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
    pub fn show(&mut self, ui: &AppWindow, devices: &[Device], scale: f32, seen: &crate::front_app::Seen) {
        self.rows = devices.to_vec();
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
        ui.set_gl_undo_shown(self.undo.is_some());
        match self.open.clone() {
            None => self.show_tabs(ui, &all, page_w, scale),
            Some(id) => self.show_page(ui, &all, &id, devices, page_w, scale),
        }
        self.show_form(ui, &all, seen, scale);
    }

    fn show_tabs(&mut self, ui: &AppWindow, all: &[Entry], page_w: f32, scale: f32) {
        ui.set_gl_view(0);
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
        ui.set_gl_store_w(crate::text::width(400, 13.0, shop_word).ceil() + 48.0);
        ui.set_gl_sort(order_word.into());
        ui.set_gl_sort_w(crate::text::width(400, 13.0, order_word).ceil() + 48.0);
        let q = self.query.trim().to_lowercase();
        let found = |e: &&Entry| q.is_empty() || e.name.to_lowercase().contains(&q);
        if apps_tab {
            let mut list: Vec<&Entry> = all.iter().filter(|e| e.kind == Kind::App).filter(found).collect();
            list.sort_by_key(|e| games::sort_key(&e.name));
            self.shown = list.iter().map(|e| e.id.clone()).collect();
            let rows: Vec<LibApp> = list
                .iter()
                .map(|e| {
                    let icon = self.icon_for(e, 32, 7, scale);
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
            ui.set_gl_page_h(HEAD_END + 6.0 + n as f32 * 60.0 + 12.0 + 40.0);
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
        let pitch = ch + 9.0 + 34.0 + GAP_Y;
        let top = HEAD_END + 6.0;
        let (pw, ph, pr) = ((cw * scale).round() as u32, (ch * scale).round() as u32, (8.0 * scale).round() as u32);
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
        ui.set_gl_page_h(top + rows as f32 * pitch + 40.0);
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
                format!("Choose what changes while you play {name}. Ectodeck puts everything back when {name} quits.")
            } else {
                format!("Choose what changes while {name} is in front.")
            }
            .into(),
        );
        let w = (page_w - 2.0 * X0).min(COLUMN).max(400.0);
        // the game's picture, as wide as the page, with its facts
        let hero = entry.as_ref().and_then(|e| e.art.hero.clone().map(|h| (h, e.art.logo.clone())));
        let mut y = HEAD_END;
        if let Some((hero, logo)) = hero {
            let hw = (page_w - 2.0 * X0).max(1.0);
            let pic = self.pic(Pic::Hero(hero, logo, (hw * scale).round() as u32, (240.0 * scale).round() as u32, (scale * 100.0).round() as u32));
            ui.set_gl_has_hero(true);
            ui.set_gl_hero(pic.unwrap_or_default());
            let e = entry.as_ref().expect("a hero is an entry's");
            let (runs, w) = facts(e);
            crate::front::set_rows(&self.facts, runs);
            ui.set_gl_facts_w(w);
            y += 240.0 + 28.0;
        } else {
            ui.set_gl_has_hero(false);
            crate::front::set_rows(&self.facts, Vec::new());
            y += 6.0;
        }
        let mut cells = Vec::new();
        // Devices: a row for each, its label and its dropdown, a dot where the
        // page chose for itself, and under them how the switching goes
        cells.push(text_cell(style::HEAD, X0 + 2.0, y, w / 2.0, 18.0, "Devices"));
        if game && !every {
            let n = devices.iter().filter(|d| self.row_value(id, d).1 && self.row_value(id, d).0 != self.row_value(id, d).2).count();
            let state = match n {
                0 => "Same as every game".to_string(),
                n => format!("{n} changed for this game"),
            };
            let sw = crate::text::width(400, 12.5, &state).ceil() + 2.0;
            cells.push(text_cell(style::NOTE, X0 + w - sw, y, sw, 18.0, &state));
        }
        y += 18.0 + 9.0;
        let box_y = y;
        let mut out = Vec::new();
        let (lx, cx) = (X0 + PAD_X, X0 + PAD_X + LABEL_W + 24.0);
        let mut ry = box_y + 8.0;
        for (k, d) in devices.iter().enumerate() {
            if k > 0 {
                out.push(Cell { color: slint::Color::from_argb_u8(9, 255, 255, 255), ..cell(kind::BOX, lx, ry, w - 2.0 * PAD_X, 1.0) });
            }
            let (c, own, follows) = self.row_value(id, d);
            out.push(text_cell(style::LABEL, lx, ry + 13.0, LABEL_W, 36.0, &d.name));
            if own && !every && (c != follows || !game) {
                out.push(Cell { style: 4, id: id_of(Ctl::LibReset), index: k as i32, ..cell(kind::DOT, lx - 13.0, ry + 13.0 + 15.0, 6.0, 6.0) });
            }
            let word = choice_name(&c, d);
            out.push(Cell { text: word.as_str().into(), style: (!own && !every) as i32, on: self.menu == Some(Menu::Device(k)), id: id_of(Ctl::LibChoice), index: k as i32, ..cell(kind::DROP, cx, ry + 13.0, DROP_W, 36.0) });
            ry += 13.0 + 36.0 + 13.0;
        }
        if devices.is_empty() {
            out.push(text_cell(style::NOTE, lx, ry + 13.0, w - 2.0 * PAD_X, 18.0, "Your devices show here once Ectodeck has found them."));
            ry += 13.0 + 18.0 + 13.0;
        } else {
            out.push(Cell { color: slint::Color::from_argb_u8(9, 255, 255, 255), ..cell(kind::BOX, lx, ry, w - 2.0 * PAD_X, 1.0) });
            let who = if every { "a game".to_string() } else { name.clone() };
            let note = format!("Each device switches when {who} comes to the front, and switches back when another app does.");
            let nw = w - PAD_X - (cx - X0);
            let lines = crate::text::wrap(400, 12.5, &note, nw);
            for (i, l) in lines.iter().enumerate() {
                out.push(text_cell(style::NOTE, cx, ry + 13.0 + i as f32 * 18.0, nw, 18.0, l));
            }
            ry += 13.0 + lines.len() as f32 * 18.0 + 13.0;
        }
        let box_h = ry + 10.0 - box_y;
        cells.push(cell(kind::TILE, X0, box_y, w, box_h));
        cells.extend(out);
        crate::front::sync_cells(&self.cells, cells);
        ui.set_gl_page_h(box_y + box_h + 60.0);
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
                        let icon = crate::front_app::icon_of(id).and_then(|f| self.pic(Pic::Icon(f, (24.0 * scale).round() as u32, (5.0 * scale).round() as u32)));
                        LibApp { name: name.as_str().into(), sub: "".into(), from: "".into(), has_icon: icon.is_some(), icon: icon.unwrap_or_default() }
                    })
                    .collect();
                crate::front::set_rows(&self.seen, rows);
                ui.set_gl_seen_chosen(chosen.map_or(-1, |c| c as i32));
                ui.set_gl_can_add(chosen.is_some_and(|c| c < self.offered.len()));
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
        let row = |label: &str, tick: bool, sub: String| MenuRow { kind: 0, label: label.into(), tag: "".into(), tick, sub: sub.into() };
        match m {
            Menu::Shop => vec![
                row("All stores", self.shop == Shop::All, String::new()),
                row("Steam", self.shop == Shop::Steam, all_counts.0.to_string()),
                row("Added by hand", self.shop == Shop::Hand, all_counts.1.to_string()),
            ],
            Menu::Order => vec![row("Recently played", self.order == Order::Recent, String::new()), row("Name", self.order == Order::Name, String::new()), row("Most played", self.order == Order::Most, String::new())],
            Menu::Device(k) => {
                let Some(d) = self.rows.get(*k) else { return Vec::new() };
                let id = self.open.clone().unwrap_or_default();
                let (c, _, _) = self.row_value(&id, d);
                std::iter::once(row("Don’t switch", c == Choice::Stay, String::new())).chain(d.profiles.iter().map(|(r, n)| row(n, c == Choice::Profile(r.clone()), String::new()))).collect()
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
        let Some(m) = self.menu.take() else { return };
        match m {
            Menu::Shop => self.shop = [Shop::All, Shop::Steam, Shop::Hand].get(i).copied().unwrap_or(self.shop),
            Menu::Order => self.order = [Order::Recent, Order::Name, Order::Most].get(i).copied().unwrap_or(self.order),
            Menu::Device(k) => {
                let (Some(d), Some(id)) = (self.rows.get(k).cloned(), self.open.clone()) else { return };
                let want = if i == 0 { Choice::Stay } else if let Some((r, _)) = d.profiles.get(i - 1) { Choice::Profile(r.clone()) } else { return };
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

    /// A dot beside a row: back to following Every game (a game's), or to no change (an app's).
    pub fn reset_row(&mut self, k: usize) {
        let (Some(d), Some(id)) = (self.rows.get(k).cloned(), self.open.clone()) else { return };
        self.library.set_choice(&id, &d.key, None);
    }

    /// A cover, or a row on the Apps tab, selected: its own page.
    pub fn open_at(&mut self, i: usize) {
        if let Some(id) = self.shown.get(i).cloned() {
            self.open = Some(id);
            self.menu = None;
        }
    }

    /// A cover's star: a favorite, or no longer.
    pub fn star_at(&mut self, i: usize) {
        if let Some(id) = self.shown.get(i).cloned() {
            let on = !self.library.is_favorite(&id);
            self.library.set_favorite(&id, on);
        }
    }

    pub fn every(&mut self) {
        self.open = Some(EVERY.to_string());
    }

    pub fn back(&mut self) {
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
        self.undo = Some(self.library.remove(&id));
        self.tell_watch();
        self.open = None;
        self.say(format!("Removed {name} from the Library."), false);
    }

    /// Undo, after Remove: the game or app back, and its page.
    pub fn undo(&mut self) {
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
        // (offscreen, ECTODECK_NATIVE_PROGRAM's file, as if chosen there)
        let rx = match std::env::var_os("ECTODECK_NATIVE_PROGRAM").filter(|_| crate::desk::offscreen()) {
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
            _ => return,
        }
        self.form = Form::None;
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

/// A choice as a dropdown shows it.
fn choice_name(c: &Choice, d: &Device) -> String {
    match c {
        Choice::Stay => "Don’t switch".into(),
        Choice::Profile(p) => d.profiles.iter().find(|(r, _)| r == p).map_or_else(|| "A profile no longer there".to_string(), |(_, n)| n.clone()),
    }
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
        if k > 0 {
            x += 18.0;
        }
        if !strong.is_empty() {
            runs.push(LibFact { text: strong.as_str().into(), strong: true, x });
            x += crate::text::width(500, 12.5, strong).ceil() + 4.0;
        }
        runs.push(LibFact { text: words.as_str().into(), strong: false, x });
        x += crate::text::width(400, 12.5, words).ceil();
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
        let dir = std::env::temp_dir().join(format!("ectodeck-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), &png).unwrap();
        std::fs::write(dir.join("a.jpg"), &jpg).unwrap();
        assert_eq!(size_of(&dir.join("a.png")), Some((3.0, 2.0)));
        assert_eq!(size_of(&dir.join("a.jpg")), Some((300.0, 200.0)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
