//! The Z10's page as the window shows it: what the page asks for, what the
//! keyboard answers, and the models the page is drawn from. Nothing here
//! knows about the window, so the offscreen check drives it the same way.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ectodeck_draw::hdr::Light;
use slint::{Color, Model, ModelRc, SharedString, VecModel};

use crate::device;
use crate::overlays::Overlays;
use crate::z10::{self, Action, Page, View};
use crate::{AppWindow, KeyItem, LayerItem, MenuRow, ProfileItem};

/// What the page asked for, done once its event has been handled.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Move,
    Minimise,
    Maximise,
    Close,
    View(i32),
    Profile(i32),
    Save,
    Discard,
    Key(i32),
    Board,
    ActionMenu(i32, f32, f32, f32),
    Listen(i32),
    Reset,
    MenuChose(i32),
    MenuTyped(String),
    MenuEnter,
    MenuDismiss,
    MenuBack,
    Unpress(u32),
    Go(String),
    Rail(String),
    MacroEditor,
    /// time to read the sliders again, and the display's text
    PollSliders,
    PollLcd,
}

/// What the window itself must do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowAsk {
    Move,
    Minimise,
    Maximise,
    Close,
}

const LAYERS: [(&str, &str); 2] = [("keys", "When pressed"), ("fnkeys", "When Fn is held")];

/// The action menu's pages under "Other": each one's title and its list.
const PAGES: [&str; 5] = ["A key", "A media key", "A mouse button", "Switch profile", "Keyboard function"];

enum Choice {
    Nothing,
    Set([u32; 4]),
    Page(usize),
}

struct Menu {
    layer: usize,
    pos: u32,
    page: Option<usize>,
    query: String,
    anchor: (f32, f32, f32),
    choices: Vec<Choice>,
}

pub struct Front {
    pub ui: AppWindow,
    pub page: Page,
    kb: Option<mpsc::Sender<device::Ask>>,
    pub asks: Rc<RefCell<Vec<Ask>>>,
    menu: Option<Menu>,
    /// "Press a key": the layer listening, and the modifiers held so far
    pub listening: Option<(usize, u32)>,
    /// a menu closed by a press on its own button, which that button's click must not open again
    shut: Option<((f32, f32), Instant)>,
    t0: Instant,
    last_lights: Option<Vec<Light>>,
    /// when the keyboard could not be read, to try again
    failed_at: Option<Instant>,
    controlled: bool,
    /// the knobs, the display and its glass, over the keyboard
    pub overlays: Overlays,
    sliders_asked: bool,
    /// the lowest and highest reading seen for each slider, which widen as the
    /// sliders are moved, so a knob reaches both ends when its slider does
    slider_ends: [[f64; 2]; 2],
    _timers: Vec<slint::Timer>,
}

/// A profile's colour sample: how many colours, and the colours.
fn swatch(cols: &[[f64; 3]]) -> (i32, [Color; 4]) {
    let c = |i: usize| cols.get(i).or(cols.last()).map_or(Color::from_rgb_u8(0, 0, 0), |v| Color::from_rgb_u8(v[0] as u8, v[1] as u8, v[2] as u8));
    (cols.len().clamp(1, 4) as i32, [c(0), c(1), c(2), c(3)])
}

impl Front {
    pub fn new(ui: AppWindow, kb: Option<mpsc::Sender<device::Ask>>, controlled: bool) -> Front {
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
        on!(on_view_changed, |i| Ask::View(i));
        on!(on_choose_profile, |n| Ask::Profile(n));
        on!(on_save, Ask::Save);
        on!(on_discard, Ask::Discard);
        on!(on_key_clicked, |p| Ask::Key(p));
        on!(on_board_clicked, Ask::Board);
        on!(on_action_menu, |l, x, y, h| Ask::ActionMenu(l, x, y, h));
        on!(on_listen, |l| Ask::Listen(l));
        on!(on_reset_key, Ask::Reset);
        on!(on_menu_chose, |i| Ask::MenuChose(i));
        on!(on_menu_typed, |t| Ask::MenuTyped(t.to_string()));
        on!(on_menu_enter, Ask::MenuEnter);
        on!(on_menu_dismiss, Ask::MenuDismiss);
        on!(on_menu_back_to, Ask::MenuBack);
        on!(on_go, |p| Ask::Go(p.to_string()));
        on!(on_rail, |p| Ask::Rail(p.to_string()));
        on!(on_macro_editor, Ask::MacroEditor);
        let page = Page::new();
        ui.set_units(page.view_box.2 as f32);
        ui.set_units_high(page.view_box.3 as f32);
        // the sliders ten times a second, the display's text every two seconds, as the web app read them
        let timers = [(Duration::from_millis(100), Ask::PollSliders), (Duration::from_secs(2), Ask::PollLcd)]
            .into_iter()
            .map(|(every, ask)| {
                let t = slint::Timer::default();
                let a = asks.clone();
                t.start(slint::TimerMode::Repeated, every, move || a.borrow_mut().push(ask.clone()));
                t
            })
            .collect();
        let slider_ends = crate::uistate::get("slider_ends").and_then(|v| serde_json::from_value(v).ok()).unwrap_or([[400.0, 3700.0], [400.0, 3700.0]]);
        let mut f = Front {
            ui,
            page,
            kb,
            asks,
            menu: None,
            listening: None,
            shut: None,
            t0: Instant::now(),
            last_lights: None,
            failed_at: None,
            controlled,
            overlays: Overlays::new(),
            sliders_asked: false,
            slider_ends,
            _timers: timers,
        };
        f.poll_lcd();
        if controlled {
            f.ask_kb(device::Ask::State);
        } else {
            f.ui.set_conn_state(3);
            f.ui.set_conn_text("Not controlled by Ectodeck".into());
            f.ui.set_boot(3);
            f.ui.set_boot_title("Control is off".into());
            f.ui.set_boot_text("Ectodeck leaves this keyboard to other programs.".into());
        }
        f.refresh();
        f
    }

    fn ask_kb(&self, a: device::Ask) {
        if let Some(kb) = &self.kb {
            let _ = kb.send(a);
        }
    }

    fn status(&self, text: impl Into<SharedString>, kind: i32) {
        self.ui.set_status_text(text.into());
        self.ui.set_status_kind(kind);
    }

    /// The keyboard answered.
    pub fn heard(&mut self, h: device::Heard) {
        match h {
            device::Heard::State(Ok(dev)) => {
                self.failed_at = None;
                self.page.loaded(dev);
                let (line, chip) = self.page.firmware_line();
                self.ui.set_conn_state(1);
                self.ui.set_conn_text(line.into());
                self.ui.set_conn_chip(chip.unwrap_or_default().into());
                self.ui.set_boot(0);
                if let Ok(dir) = std::env::var("ECTODECK_NATIVE_DUMP") {
                    let _ = std::fs::write(std::path::Path::new(&dir).join("state.json"), serde_json::to_vec(self.page.dev.as_ref().unwrap()).unwrap_or_default());
                }
            }
            device::Heard::State(Err(e)) => {
                self.failed_at = Some(Instant::now());
                if self.page.dev.is_none() {
                    self.ui.set_conn_state(2);
                    self.ui.set_conn_text("Keyboard not connected".into());
                    self.ui.set_boot(2);
                    self.ui.set_boot_title("The editor cannot reach the keyboard".into());
                    self.ui.set_boot_text(format!("{e}. Check that it is plugged in; Ectodeck tries again every few seconds.").into());
                }
            }
            device::Heard::Saved(n, r) => {
                self.page.busy = false;
                match r {
                    Ok(p) => {
                        self.page.saved_profile(n, p);
                        self.status(format!("Saved profile {n} to the keyboard and read it back to check."), 1);
                    }
                    Err(e) => self.status(format!("Profile {n} was not saved: {e}"), 2),
                }
            }
            device::Heard::Sliders(r) => {
                self.sliders_asked = false;
                if let Ok(v) = r {
                    self.move_knobs(&v);
                }
                return;
            }
            device::Heard::Active(n, r) => match r {
                Ok(active) => {
                    self.page.set_active(active.unwrap_or(n));
                    self.status(format!("Profile {n} is now active on the keyboard."), 1);
                }
                Err(e) => self.status(format!("The keyboard did not switch profiles: {e}"), 2),
            },
        }
        self.refresh();
    }

    /// Try the keyboard again a few seconds after it could not be read.
    pub fn retry(&mut self) {
        if self.failed_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3)) {
            self.failed_at = None;
            self.ask_kb(device::Ask::State);
        }
    }

    pub fn waiting(&self) -> Option<Duration> {
        self.failed_at.map(|t| Duration::from_secs(3).saturating_sub(t.elapsed()) + Duration::from_millis(10))
    }

    /// Do what the page asked; what only the window can do is handed back.
    pub fn act(&mut self) -> Vec<WindowAsk> {
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        let mut window = Vec::new();
        let mut changed = false;
        for ask in asks {
            changed = true;
            match ask {
                Ask::Move => window.push(WindowAsk::Move),
                Ask::Minimise => window.push(WindowAsk::Minimise),
                Ask::Maximise => window.push(WindowAsk::Maximise),
                Ask::Close => window.push(WindowAsk::Close),
                Ask::View(i) => {
                    // still reading the keyboard
                    if self.page.dev.is_none() {
                        continue;
                    }
                    self.close_menu();
                    self.page.view = [View::Keys, View::Lighting, View::Display][i.clamp(0, 2) as usize];
                }
                Ask::Profile(n) => self.choose_profile(n as u8),
                Ask::Save => self.save(),
                Ask::Discard => {
                    self.page.discard();
                    self.status(format!("Discarded the unsaved changes to profile {}.", self.page.editing), 0);
                }
                Ask::Key(pos) => self.select_key(pos as u32),
                Ask::Board => {
                    if self.page.view == View::Keys && self.page.sel.is_some() {
                        self.page.sel = None;
                        self.close_menu();
                    }
                }
                Ask::ActionMenu(l, x, y, h) => self.open_menu(l as usize, (x, y, h)),
                Ask::Listen(l) => {
                    if self.listening.is_none() {
                        self.close_menu();
                        self.listening = Some((l as usize, 0));
                    }
                }
                Ask::Reset => {
                    if let Some(pos) = self.page.sel {
                        self.page.reset_key(pos);
                    }
                }
                Ask::MenuChose(i) => self.menu_chose(i as usize),
                Ask::MenuTyped(t) => {
                    if let Some(m) = &mut self.menu {
                        m.query = t;
                    }
                    self.build_menu();
                }
                Ask::MenuEnter => {
                    let first = self.menu.as_ref().and_then(|m| m.choices.iter().position(|c| matches!(c, Choice::Set(_) | Choice::Page(_))));
                    if let Some(i) = first {
                        self.menu_chose(i);
                    }
                }
                Ask::MenuDismiss => self.close_menu(),
                Ask::MenuBack => {
                    if let Some(m) = &mut self.menu {
                        m.page = None;
                        m.query.clear();
                    }
                    self.build_menu();
                }
                Ask::Unpress(pos) => {
                    self.page.pressed.remove(&pos);
                }
                Ask::Go(place) => {
                    self.close_menu();
                    self.ui.set_place(place.into());
                }
                // the item already chosen opens the rail into its blade, or closes it
                Ask::Rail(place) => {
                    self.close_menu();
                    if self.ui.get_place() == place.as_str() {
                        let open = !self.ui.get_rail_open();
                        self.ui.set_rail_open(open);
                        crate::uistate::set("rail_open", serde_json::json!(open));
                    } else {
                        self.ui.set_place(place.into());
                    }
                }
                Ask::PollSliders => {
                    changed = false;
                    if self.page.reads_sliders() && self.ui.get_place() == "z10" && !self.sliders_asked && self.kb.is_some() {
                        self.sliders_asked = true;
                        self.ask_kb(device::Ask::Sliders);
                    }
                }
                Ask::PollLcd => {
                    changed = false;
                    self.poll_lcd();
                }
                Ask::MacroEditor => self.status("The Macro Editor is not in the native window yet; it comes after the Z10's three pages.", 0),
            }
        }
        if changed {
            self.refresh();
        }
        window
    }

    /// The display's text: the keyboard will not read its display back, so the
    /// z10-lcd service writes each frame it sends to a file; if nothing has
    /// written it for ten seconds, the panel is drawn unlit rather than stale.
    fn poll_lcd(&mut self) {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let f = dir.join("z10-lcd/frame");
        let fresh = std::fs::metadata(&f).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() < 10);
        let rows: Vec<String> = std::fs::read_to_string(&f).map(|s| s.split('\n').take(4).map(|r| format!("{:<20}", r.chars().take(20).collect::<String>())).collect()).unwrap_or_default();
        let lcd = (fresh && !rows.is_empty(), rows, self.page.lcd_color());
        if lcd != self.overlays.lcd {
            self.overlays.lcd = lcd;
            self.ui.set_overlays(ModelRc::new(VecModel::from(self.overlays.model())));
        }
    }

    /// Where the sliders are: each reading becomes a knob's place between the
    /// lowest and highest readings seen for its slider.
    fn move_knobs(&mut self, v: &serde_json::Value) {
        let raws = [v["dimmer_raw"].as_f64(), v["volume_raw"].as_f64()];
        let mut wider = false;
        for (i, raw) in raws.iter().enumerate() {
            let Some(raw) = *raw else { continue };
            let e = &mut self.slider_ends[i];
            if raw < e[0] {
                e[0] = raw;
                wider = true;
            }
            if raw > e[1] {
                e[1] = raw;
                wider = true;
            }
            self.overlays.fracs[i] = Some(((raw - e[0]) / (e[1] - e[0]).max(1.0)).clamp(0.0, 1.0) as f32);
        }
        if wider {
            crate::uistate::set("slider_ends", serde_json::json!(self.slider_ends));
        }
        self.ui.set_overlays(ModelRc::new(VecModel::from(self.overlays.model())));
    }

    /// The keyboard's size on the page, in the buffer's pixels: the knobs and
    /// the display's glass are drawn again for a new one.
    pub fn board_size(&mut self, w: u32, h: u32, scale: f32) {
        if self.overlays.board(w, h, scale) {
            self.ui.set_overlays(ModelRc::new(VecModel::from(self.overlays.model())));
        }
    }

    fn choose_profile(&mut self, n: u8) {
        if self.page.dev.is_none() {
            return;
        }
        self.close_menu();
        self.page.choose(n);
        if n != self.page.active() {
            self.ask_kb(device::Ask::Activate(n));
        }
    }

    fn save(&mut self) {
        if self.page.dev.is_none() || self.page.busy || !self.page.is_dirty(self.page.editing) {
            return;
        }
        let n = self.page.editing;
        self.page.busy = true;
        self.status(format!("Saving profile {n}…"), 0);
        self.ask_kb(device::Ask::Save(n, self.page.save_body()));
    }

    /// A key clicked on the board: chosen for editing on the Keys page; on
    /// the Lighting page it lights as pressing it would.
    fn select_key(&mut self, pos: u32) {
        if self.page.dev.is_none() {
            return;
        }
        let now = self.t0.elapsed().as_secs_f64() * 1000.0;
        if self.page.view == View::Lighting {
            self.page.flash(pos, now);
            self.press(pos);
            return;
        }
        if self.page.view != View::Keys {
            self.page.view = View::Keys;
        } else if self.page.sel == Some(pos) {
            // pressing it again lets it go
            self.page.sel = None;
            self.close_menu();
            return;
        }
        self.close_menu();
        self.listening = None;
        self.page.sel = Some(pos);
        self.press(pos);
    }

    /// Show a key pressed: it darkens where it stands for a moment.
    pub fn press(&mut self, pos: u32) {
        self.page.pressed.insert(pos, 0.0);
        let a = self.asks.clone();
        slint::Timer::single_shot(Duration::from_millis(90), move || a.borrow_mut().push(Ask::Unpress(pos)));
    }

    /// A key typed on the keyboard, as a Linux key code: "Press a key" takes
    /// it, or the board shows it pressed. True if it was taken.
    pub fn typed(&mut self, code: u32, down: bool) -> bool {
        let Some((mods, hid)) = z10::evdev_to_hid(code) else { return self.listening.is_some() };
        if let Some((layer, held)) = self.listening {
            if down {
                if hid == 0 {
                    self.listening = Some((layer, held | mods));
                } else {
                    self.assign(layer, [0, held, hid, 0]);
                }
            } else if hid == 0 {
                // a lone modifier pressed and let go assigns that modifier
                if held == mods {
                    self.assign(layer, [0, held, 0, 0]);
                } else {
                    self.listening = Some((layer, held & !mods));
                }
            }
            return true;
        }
        if down && self.menu.is_none() {
            if let Some(pos) = self.page.pos_for(mods, hid) {
                if self.page.key(pos).is_some() {
                    self.press(pos);
                    let now = self.t0.elapsed().as_secs_f64() * 1000.0;
                    self.page.flash(pos, now);
                    self.refresh();
                }
            }
        }
        false
    }

    fn assign(&mut self, layer: usize, d: [u32; 4]) {
        self.listening = None;
        if let Some(pos) = self.page.sel {
            self.page.set_def(LAYERS[layer].0, pos, d);
        }
        self.refresh();
    }

    // ------------------------------------------------------------ the action menu

    fn open_menu(&mut self, layer: usize, anchor: (f32, f32, f32)) {
        let Some(pos) = self.page.sel else { return };
        // the second click on the button whose menu a press just closed
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        if self.menu.as_ref().is_some_and(|m| m.layer == layer) {
            self.close_menu();
            return;
        }
        self.listening = None;
        self.menu = Some(Menu { layer, pos, page: None, query: String::new(), anchor, choices: Vec::new() });
        self.ui.set_menu_query("".into());
        self.build_menu();
        // under the button when it fits there, or when there is a fair amount of
        // room there anyway (the menu then scrolls); over it only when below is
        // cramped. Where it hangs is fixed now, so it stays put as it is searched.
        let (_, ay, ah) = anchor;
        let (h, win) = (self.ui.get_menu_natural(), self.ui.get_window_height());
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        if std::env::var_os("ECTODECK_NATIVE_DEBUG").is_some() {
            eprintln!("menu: natural {h} window {win} anchor {ay}+{ah} below {below} above {above} down {down}");
        }
        let room = if down { below } else { above };
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
        self.ui.set_menu_open(true);
        self.ui.invoke_focus_menu();
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
        }
    }

    /// A press somewhere in the window, in its logical pixels: a press away
    /// from the open menu closes it, and a press on the button it hangs from
    /// keeps that button's click from opening it again.
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        let Some(m) = &self.menu else { return };
        let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
        if x >= mx && x < mx + mw && y >= my && y < my + mh {
            return;
        }
        let (ax, ay, ah) = m.anchor;
        if x >= ax && x < ax + 300.0 && y >= ay && y < ay + ah {
            self.shut = Some(((ax, ay), Instant::now()));
        }
        self.close_menu();
    }

    fn menu_chose(&mut self, i: usize) {
        let Some(m) = &mut self.menu else { return };
        match m.choices.get(i) {
            Some(Choice::Set(d)) => {
                let (layer, pos, d) = (m.layer, m.pos, *d);
                self.close_menu();
                // a macro keeps playing the way it already plays on its other keys
                let d = self.page.with_macro_play(d);
                if self.page.sel == Some(pos) {
                    self.page.set_def(LAYERS[layer].0, pos, d);
                }
            }
            Some(Choice::Page(p)) => {
                m.page = Some(*p);
                m.query.clear();
                self.ui.set_menu_query("".into());
                self.build_menu();
            }
            _ => {}
        }
    }

    fn build_menu(&mut self) {
        let Some(m) = &self.menu else { return };
        let def = self.page.def(LAYERS[m.layer].0, m.pos);
        let g = self.page.groups();
        let lists: [&Vec<Action>; 5] = [&g.keys, &g.media, &g.mouse, &g.profiles, &g.board];
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut choices: Vec<Choice> = Vec::new();
        let row = |kind: i32, label: &str, tag: &str, tick: bool| MenuRow { kind, label: label.into(), tag: tag.into(), tick };
        let q = m.query.trim().to_lowercase();
        // a searchable list, as pickList draws it
        let mut pick = |rows: &mut Vec<MenuRow>, choices: &mut Vec<Choice>, groups: &[(&str, Vec<&Action>)]| {
            let mut n = 0;
            for (head, items) in groups {
                let hit: Vec<&&Action> = items.iter().filter(|a| q.is_empty() || a.label.to_lowercase().contains(&q) || a.tag.to_lowercase().contains(&q)).collect();
                if hit.is_empty() {
                    continue;
                }
                if !head.is_empty() {
                    rows.push(row(1, head, "", false));
                    choices.push(Choice::Nothing);
                }
                for a in hit.into_iter().take(if q.is_empty() { 400 } else { 60 }) {
                    n += 1;
                    rows.push(row(0, &a.label, if q.is_empty() { "" } else { a.tag }, a.def == def));
                    choices.push(Choice::Set(a.def));
                }
            }
            if n == 0 {
                rows.push(row(3, "Nothing matches.", "", false));
                choices.push(Choice::Nothing);
            }
        };
        let placeholder;
        match m.page {
            Some(p) => {
                let title = PAGES[p];
                let what = title.trim_start_matches("A ").trim_start_matches("An ").to_lowercase();
                placeholder = format!("Search {what}s");
                pick(&mut rows, &mut choices, &[(title, lists[p].iter().collect())]);
            }
            None => {
                placeholder = "Search keys, media, macros".to_string();
                if !q.is_empty() {
                    let all: Vec<&Action> = g.macros.iter().chain(&g.keys).chain(&g.media).chain(&g.mouse).chain(&g.profiles).chain(&g.board).collect();
                    pick(&mut rows, &mut choices, &[("", all)]);
                } else {
                    if !g.macros.is_empty() {
                        rows.push(row(1, "Macros", "", false));
                        choices.push(Choice::Nothing);
                        for a in &g.macros {
                            rows.push(row(0, &a.label, "", def[0] == 3 && def[1] == a.def[1]));
                            choices.push(Choice::Set(a.def));
                        }
                        rows.push(row(2, "", "", false));
                        choices.push(Choice::Nothing);
                    }
                    rows.push(row(1, "Other", "", false));
                    choices.push(Choice::Nothing);
                    let kind = z10::kind_of(def);
                    for (p, title) in PAGES.iter().enumerate() {
                        let tick = lists[p].first().is_some_and(|a| a.tag == kind) && def[0] != 3;
                        rows.push(row(0, &format!("{title} ›"), "", tick));
                        choices.push(Choice::Page(p));
                    }
                    rows.push(row(0, "Nothing", "", z10::is_nothing(def)));
                    choices.push(Choice::Set([0, 0, 0, 0]));
                }
            }
        }
        let (ax, ay, ah) = m.anchor;
        let back = m.page.is_some();
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_back(back);
        self.ui.set_menu_placeholder(placeholder.into());
        // its height as the page lays its rows out: Slint lays a list's rows out
        // only as it draws them, so the window works it out
        let list: f32 = rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum();
        self.ui.set_menu_natural(6.0 + if back { 33.0 } else { 0.0 } + 33.0 + 6.0 + list + 6.0);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        if let Some(m) = &mut self.menu {
            m.choices = choices;
        }
    }

    // ------------------------------------------------------------ the page's models

    /// Draw the page from the page's state.
    pub fn refresh(&mut self) {
        let ui = &self.ui;
        let p = &self.page;
        ui.set_tab(match p.view {
            View::Keys => 0,
            View::Lighting => 1,
            View::Display => 2,
        });
        let loaded = p.dev.is_some();
        if loaded {
            let profiles: Vec<ProfileItem> = z10::PROFILE_NUMBERS
                .iter()
                .map(|&n| {
                    let (colors, [c1, c2, c3, c4]) = swatch(&p.profile_swatch(n));
                    ProfileItem {
                        n: n as i32,
                        name: p.profile_name(n).into(),
                        sub: p.profile_effect(n).into(),
                        active: n == p.active(),
                        current: n == p.editing,
                        dirty: p.is_dirty(n),
                        colors,
                        c1,
                        c2,
                        c3,
                        c4,
                    }
                })
                .collect();
            ui.set_profiles(ModelRc::new(VecModel::from(profiles)));
        }
        let dirty = loaded && p.is_dirty(p.editing);
        ui.set_can_save(dirty && !p.busy);
        ui.set_save_label(format!("Save profile {}", p.editing).into());

        // the keys over the keyboard
        let (x0, y0, _, _) = p.view_box;
        let names = p.board_macro_names();
        let keys: Vec<KeyItem> = p
            .layout
            .iter()
            .map(|k| KeyItem {
                pos: k.pos as i32,
                x: (k.x - x0 + z10::GAP / 2.0) as f32,
                y: (k.y - y0 + z10::GAP / 2.0) as f32,
                w: (k.w - z10::GAP) as f32,
                h: (k.h - z10::GAP) as f32,
                molded: k.id.starts_with("Multimedia"),
                remap: p.remap(k.pos, &names).map(|s| s.to_uppercase()).unwrap_or_default().into(),
                selected: p.view == View::Keys && p.sel == Some(k.pos),
                pressed: p.pressed.contains_key(&k.pos),
            })
            .collect();
        let same = {
            let old = ui.get_keys();
            old.row_count() == keys.len() && keys.iter().enumerate().all(|(i, k)| old.row_data(i).as_ref() == Some(k))
        };
        if !same {
            ui.set_keys(ModelRc::new(VecModel::from(keys)));
        }

        // the chosen key's editor
        let chosen = if p.view == View::Keys { p.sel.and_then(|pos| p.key(pos).cloned()) } else { None };
        ui.set_key_open(chosen.is_some());
        if let Some(k) = chosen {
            let fac = p.factory("keys", k.pos);
            let name = p.key_label(&k);
            ui.set_key_name(name.clone().into());
            ui.set_key_factory(format!("Factory: {}", if z10::is_nothing(fac) { "does nothing".to_string() } else { p.describe(fac) }).into());
            ui.set_reset_label(format!("Reset {name} to factory").into());
            let layers: Vec<LayerItem> = LAYERS
                .iter()
                .enumerate()
                .map(|(i, (layer, title))| {
                    let d = p.def(layer, k.pos);
                    let is_macro = d[0] == 3;
                    let label = if is_macro { p.macro_label(d[1]) } else if z10::is_nothing(d) { "Nothing".into() } else { p.describe(d) };
                    let held: Vec<SharedString> = if is_macro { p.macro_keys_held(d[1]).into_iter().map(Into::into).collect() } else { Vec::new() };
                    LayerItem {
                        title: (*title).into(),
                        kind: z10::kind_of(d).into(),
                        label: label.into(),
                        nothing: z10::is_nothing(d),
                        held: ModelRc::new(VecModel::from(held)),
                        r#macro: is_macro,
                        listening: self.listening.is_some_and(|(l, _)| l == i),
                    }
                })
                .collect();
            ui.set_layers(ModelRc::new(VecModel::from(layers)));
        }
        if self.menu.is_some() {
            self.build_menu();
        }
        let back = self.page.lcd_color();
        if back != self.overlays.lcd.2 {
            self.overlays.lcd.2 = back;
            self.ui.set_overlays(ModelRc::new(VecModel::from(self.overlays.model())));
        }
    }

    /// The keys' light now, if it has changed since it was last asked for.
    pub fn lights(&mut self) -> Option<Vec<Light>> {
        let t = self.t0.elapsed().as_secs_f64() * 1000.0;
        let lights = self.page.lights(t);
        if self.last_lights.as_ref() == Some(&lights) {
            return None;
        }
        self.last_lights = Some(lights.clone());
        Some(lights)
    }

    pub fn animated(&self) -> bool {
        self.page.animated()
    }

}
