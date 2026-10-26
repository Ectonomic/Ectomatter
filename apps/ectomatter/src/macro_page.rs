//! The Macro Editor's window, as the web app's second window had it: what
//! its page asks for, and its models, from the editor's state (macros.rs) and
//! the Z10's page, which the two windows share.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::tokens::{font, line, part, size, space};

use crate::device;
use crate::front::WindowAsk;
use crate::macros::{self, hit, piece, Draft, Editor, Lib, Plays, Recording, Timing, MEDIA, PLAYS, ROOM};
use crate::z10::Page;
use crate::{Cell, MacroItem, MacroWindow, MenuRow};

#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Move,
    Minimize,
    Maximize,
    Close,
    /// the libraries' drop-down opened, under the list's top
    Libs(f32, f32, f32, f32),
    /// a macro chosen, its menu opened, a new one made: in a library (0 the
    /// keyboard's, 1 Ectomatter's), which then shows
    Choose(i32, i32),
    RowMenu(i32, i32, f32, f32),
    NewIn(i32),
    Rename,
    NameDone(String),
    NameCancel,
    More(f32, f32, f32),
    ChainClick(i32, i32, f32, f32, f32),
    ChainDrop(i32, f32, f32),
    ChainDrag(i32, f32, f32, f32, f32),
    WaitDone(i32, String),
    WaitCancel,
    Tool(i32, f32, f32, f32, f32),
    Plays(f32, f32, f32, f32),
    CountDone(String),
    Revert,
    Save,
    MenuChose(i32),
    MenuTyped(String),
    MenuEnter,
    MenuDismiss,
    MenuField(i32, String),
    ConfirmYes,
    ConfirmNo,
    RecTick,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Insert,
    Step(usize),
    Keys { media: bool, change: Option<usize> },
    Timing,
    Plays,
    Macro,
    Libs,
    /// one of Ectomatter's jobs for a step: a device's profile or a screen job,
    /// or (`macros`) another of its macros; added, or in place of step `change`
    Jobs { macros: bool, change: Option<usize> },
}

#[derive(Clone)]
enum Pick {
    Nothing,
    Tap(Value),
    Change(usize, Value),
    Down(usize, bool),
    Keys(bool, Option<usize>),
    Wait,
    Remove(usize),
    Timing(Timing),
    RecordWaits,
    Plays(macros::How),
    Rename,
    Duplicate,
    Delete,
    Lib(Lib),
    /// a job for a step, by its id; and a step's job to change (the menu of
    /// macros, or of the devices' jobs)
    Job(String),
    ChangeJob(usize, bool),
}

struct Menu {
    kind: Kind,
    anchor: (f32, f32, f32),
    /// how wide what opened it is (a menu is never narrower), and how wide the menu is,
    /// as its rows need (src/menu_size.rs)
    under: f32,
    width: f32,
    /// its right edge on the button's, as the More button's menu hangs: the button's width
    right: f32,
    /// dismissed, nothing in the chain stays chosen
    let_go: bool,
    query: String,
    picks: Vec<Pick>,
}

/// A save under way: the macro, then each profile whose keys play it differently now.
struct Saving {
    n: u32,
    name: String,
    added: Vec<Value>,
    profiles: usize,
    wrote: bool,
}

pub struct MacroPage {
    pub ui: MacroWindow,
    /// the library shown, and the other one, each with its own drafts
    pub ed: Editor,
    pub other: Editor,
    asks: Rc<RefCell<Vec<Ask>>>,
    menu: Option<Menu>,
    shut: Option<((f32, f32), Instant, bool)>,
    rec: evga_z10::Recorder,
    rec_timer: slint::Timer,
    confirm: bool,
    status: (String, i32),
    saving: Option<Saving>,
    chain: Rc<VecModel<Cell>>,
    items: Rc<VecModel<MacroItem>>,
    fake_rec: bool,
    /// the chosen macro is to be scrolled into view in the list
    scroll_to: bool,
    /// whether a mouse is known that keeps one of Ectomatter's macros in its own
    /// memory when the macro is a shortcut (mouse.rs's held_by_mouse)
    pub mouse_holds: bool,
}

/// A heading's note in lines: the first short of the New button beside it,
/// the rest the list's width.
fn note_lines(note: &str) -> Vec<String> {
    let first = crate::text::wrap(font::REGULAR, font::NOTE, note, NOTE_W - new_w());
    let Some(head) = first.first().cloned() else { return Vec::new() };
    let rest = note.get(head.len()..).unwrap_or("").trim_start();
    let mut lines = vec![head];
    if !rest.is_empty() {
        lines.extend(crate::text::wrap(font::REGULAR, font::NOTE, rest, NOTE_W));
    }
    lines
}

/// A library by the number the list gives it: 0 the keyboard's, 1 Ectomatter's.
fn lib_of(l: i32) -> Lib {
    if l == 0 {
        Lib::Keyboard
    } else {
        Lib::Ectomatter
    }
}

/// The keyboard whose memory keeps its macros, by the name its library has.
const KEYBOARD: &str = "EVGA Z10 RGB";

/// The list's measures (ui/app.slint's MacroWindow): where the library's note starts
/// (its first line at the middle of its New button, which takes the button's room of
/// that line), a line of the note and its width (the column less the list's insets,
/// Space.row before the words and Space.tight after), the room under it, and a macro's row.
const NOTE_TOP: f32 = (size::COMPACT - line::NOTE) / 2.0;
const NOTE_LINE: f32 = line::NOTE;
const NOTE_W: f32 = size::SIDEBAR - 2.0 * space::INLINE - space::ROW - space::TIGHT;
const UNDER_NOTE: f32 = space::INLINE;
const ROW_H: f32 = size::ROW + space::LIST;
/// where the list starts in its column, below the window's bar and the libraries' drop-down
const LIST_TOP: f32 = size::TITLE_BAR + space::INLINE + size::BUTTON + space::INLINE;
/// the room the New button takes of the note's first line: the button, and Space.inline before it
fn new_w() -> f32 {
    crate::text::width(font::MEDIUM, font::CONTROL, "+ New").ceil() + 2.0 * part::QUIET_PAD + space::INLINE
}

fn row(kind: i32, label: &str, tick: bool) -> MenuRow {
    MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into(), ..Default::default() }
}

impl MacroPage {
    pub fn new(ui: MacroWindow, mark: slint::Image) -> MacroPage {
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
        on!(on_minimize, Ask::Minimize);
        on!(on_maximize, Ask::Maximize);
        on!(on_close_window, Ask::Close);
        on!(on_libs, |x, y, h, w| Ask::Libs(x, y, h, w));
        on!(on_choose, |l, n| Ask::Choose(l, n));
        on!(on_row_menu, |l, n, x, y| Ask::RowMenu(l, n, x, y));
        on!(on_new_in, |l| Ask::NewIn(l));
        on!(on_rename, Ask::Rename);
        on!(on_name_done, |t| Ask::NameDone(t.to_string()));
        on!(on_name_cancel, Ask::NameCancel);
        on!(on_more, |x, y, h| Ask::More(x, y, h));
        on!(on_chain_click, |id, i, x, y, h| Ask::ChainClick(id, i, x, y, h));
        on!(on_chain_drop, |i, x, y| Ask::ChainDrop(i, x, y));
        on!(on_chain_drag, |i, x, y, ox, oy| Ask::ChainDrag(i, x, y, ox, oy));
        on!(on_wait_done, |i, t| Ask::WaitDone(i, t.to_string()));
        on!(on_wait_cancel, Ask::WaitCancel);
        on!(on_tool, |k, x, y, h, w| Ask::Tool(k, x, y, h, w));
        on!(on_plays, |x, y, h, w| Ask::Plays(x, y, h, w));
        on!(on_count_done, |t| Ask::CountDone(t.to_string()));
        on!(on_revert, Ask::Revert);
        on!(on_save, Ask::Save);
        on!(on_menu_chose, |i| Ask::MenuChose(i));
        on!(on_menu_typed, |t| Ask::MenuTyped(t.to_string()));
        on!(on_menu_enter, Ask::MenuEnter);
        on!(on_menu_dismiss, Ask::MenuDismiss);
        on!(on_menu_field, |i, t| Ask::MenuField(i, t.to_string()));
        on!(on_confirm_yes, Ask::ConfirmYes);
        on!(on_confirm_no, Ask::ConfirmNo);
        ui.set_mark(mark);
        let chain = Rc::new(VecModel::default());
        let items = Rc::new(VecModel::default());
        ui.set_chain(ModelRc::from(chain.clone()));
        ui.set_items(ModelRc::from(items.clone()));
        MacroPage {
            ui,
            ed: Editor::new(Lib::Keyboard),
            asks,
            menu: None,
            shut: None,
            rec: evga_z10::Recorder::new(),
            other: Editor::new(Lib::Ectomatter),
            rec_timer: slint::Timer::default(),
            confirm: false,
            status: (String::new(), 0),
            saving: None,
            chain,
            items,
            fake_rec: false,
            scroll_to: false,
            mouse_holds: false,
        }
    }

    fn status(&mut self, t: impl Into<String>, kind: i32) {
        self.status = (t.into(), kind);
    }

    /// Opened on a library (the keyboard's, or Ectomatter's), on a macro, or
    /// on whatever it last showed.
    pub fn open(&mut self, page: &Page, lib: Lib, n: Option<u32>) {
        self.show_lib(lib);
        if let Some(n) = n.filter(|n| self.ed.saved(page, *n).is_some()) {
            self.ed.current = Some(n);
            self.ed.sel_step = None;
            self.ed.ins = None;
            self.scroll_to = true;
        }
        self.refresh(page);
    }

    /// The library shown: its own drafts and choice, kept while the other shows.
    fn show_lib(&mut self, lib: Lib) {
        if self.ed.lib != lib && self.ed.rec.is_none() && !self.ed.busy {
            std::mem::swap(&mut self.ed, &mut self.other);
            self.close_menu(false);
            self.status = (String::new(), 0);
        }
    }

    /// Do what the page asked. Hands back what only the window can do, and
    /// whether the Z10's page changed (a macro saved, a key's plays).
    pub fn act(&mut self, page: &mut Page, kb: &dyn Fn(device::Ask)) -> (Vec<WindowAsk>, bool) {
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        let mut window = Vec::new();
        if asks.is_empty() {
            return (window, false);
        }
        let mut changed = false;
        for ask in asks {
            // (the libraries' drop-down, and a library chosen on it, work while the
            // keyboard's is still being read, so another can always be shown)
            if let Ask::Libs(x, y, h, w) = ask {
                // (the menu at least as wide as the drop-down, which is all its button)
                self.open_menu(page, Kind::Libs, (x, y, h), w, 0.0, false);
                continue;
            }
            if let (Ask::MenuChose(i), Some(Kind::Libs)) = (&ask, self.menu.as_ref().map(|m| m.kind)) {
                self.menu_chose(page, (*i).max(0) as usize);
                continue;
            }
            // (a macro or a New in the library not shown shows it first, unless one
            // is being recorded or saved; then it waits)
            let mut switched = false;
            if let Ask::Choose(l, _) | Ask::RowMenu(l, ..) | Ask::NewIn(l) = ask {
                let lib = lib_of(l);
                switched = self.ed.lib != lib;
                self.show_lib(lib);
                if self.ed.lib != lib {
                    continue;
                }
            }
            // (the keyboard's library waits for the keyboard to be read, all but the
            // window itself; Ectomatter's doesn't wait)
            let waiting = self.ed.lib == Lib::Keyboard && page.dev.is_none();
            if waiting && !matches!(ask, Ask::Move | Ask::Minimize | Ask::Maximize | Ask::Close) {
                continue;
            }
            if self.ed.busy && !matches!(ask, Ask::Move | Ask::Minimize | Ask::Maximize | Ask::Close | Ask::RecTick) {
                continue;
            }
            match ask {
                Ask::Libs(..) => {}
                Ask::Move => window.push(WindowAsk::Move),
                Ask::Minimize => window.push(WindowAsk::Minimize),
                Ask::Maximize => window.push(WindowAsk::Maximize),
                Ask::Close => {
                    if self.ed.rec.is_some() {
                        self.stop_recording(page, false, None);
                    }
                    self.close_menu(false);
                    window.push(WindowAsk::Close);
                }
                Ask::Choose(_, n) => {
                    let n = n as u32;
                    // choosing the open macro again puts it away, back to the blank one
                    // (one last open in the other library is shown, not open)
                    self.ed.current = if self.ed.current == Some(n) && !switched { None } else { Some(n) };
                    self.ed.sel_step = None;
                    self.ed.ins = None;
                    self.ed.edit_name = false;
                }
                Ask::RowMenu(_, n, x, y) => {
                    if self.ed.rec.is_some() {
                        continue;
                    }
                    let n = n as u32;
                    if self.ed.current != Some(n) {
                        self.ed.current = Some(n);
                        self.ed.sel_step = None;
                        self.ed.ins = None;
                        self.ed.edit_name = false;
                    }
                    self.open_menu(page, Kind::Macro, (x, y, 0.0), 0.0, 0.0, false);
                }
                Ask::NewIn(_) => {
                    if self.ed.rec.is_none() {
                        self.ed.new_macro(page);
                        self.scroll_to = true;
                    }
                }
                Ask::Rename => self.start_rename(page),
                Ask::NameDone(t) => self.ed.commit_name(page, &t),
                Ask::NameCancel => self.ed.edit_name = false,
                Ask::More(x, y, h) => self.open_menu(page, Kind::Macro, (x, y, h), 0.0, crate::tokens::size::QUIET, false),
                Ask::ChainClick(id, i, x, y, h) => {
                    let (locked, rec) = (self.locked(page), self.ed.rec.is_some());
                    if locked || rec {
                        continue;
                    }
                    let i = i.max(0) as usize;
                    match id {
                        hit::STEP => {
                            self.ed.sel_step = Some(i);
                            self.ed.ins = Some(i + 1);
                            self.open_menu(page, Kind::Step(i), (x, y, h), 0.0, 0.0, true);
                        }
                        hit::JOINT => {
                            self.ed.ins = Some(i);
                            self.ed.sel_step = None;
                            self.open_menu(page, Kind::Insert, (x, y, h), 0.0, 0.0, true);
                        }
                        hit::WAIT => self.ed.start_wait_edit(page, i),
                        hit::END => self.ed.ins = None,
                        _ => {}
                    }
                }
                Ask::ChainDrag(from, x, y, ox, oy) => {
                    if let Some(mut g) = self.chain.iter().find(|c| c.kind == piece::STEP && c.index == from) {
                        g.x = x - ox;
                        g.y = y - oy;
                        g.id = 0;
                        g.on = false;
                        self.ui.set_ghost(g);
                        self.ui.set_ghost_on(true);
                    }
                    self.ui.set_drop_index(self.step_at(x, y).filter(|&t| t != from).unwrap_or(-1));
                    continue;
                }
                Ask::ChainDrop(from, x, y) => {
                    // onto the step under the pointer, which it goes before
                    self.ui.set_ghost_on(false);
                    self.ui.set_drop_index(-1);
                    if let Some(to) = self.step_at(x, y) {
                        self.ed.move_step(page, from.max(0) as usize, to as usize);
                    }
                }
                Ask::WaitDone(i, t) => self.ed.set_wait(page, i.max(0) as usize, &t),
                Ask::WaitCancel => self.ed.edit_wait = None,
                Ask::Tool(k, x, y, h, w) => match k {
                    0 => self.open_menu(page, Kind::Timing, (x, y, h), w, 0.0, false),
                    1 => {
                        if let Some(d) = self.current_draft(page) {
                            d.steps.clear();
                        }
                        self.ed.ins = None;
                        self.ed.sel_step = None;
                    }
                    2 | 3 => self.open_menu(page, Kind::Keys { media: k == 3, change: None }, (x, y, h), w, 0.0, false),
                    4 => self.ed.insert_wait(page),
                    // (an action, or another macro: Ectomatter's own macros only)
                    6 | 7 if self.ed.lib == Lib::Ectomatter => self.open_menu(page, Kind::Jobs { macros: k == 7, change: None }, (x, y, h), w, 0.0, false),
                    5 => {
                        if self.ed.rec.is_some() {
                            self.stop_recording(page, true, None);
                        } else {
                            self.start_recording(page);
                        }
                    }
                    _ => {}
                },
                Ask::Plays(x, y, h, w) => self.open_menu(page, Kind::Plays, (x, y, h), w, 0.0, false),
                Ask::CountDone(t) => {
                    let c: u32 = t.trim().parse().unwrap_or(2);
                    if let Some(d) = self.current_draft(page) {
                        d.plays = Some((macros::How::Times(c.clamp(2, 255)), c.clamp(2, 255)));
                    }
                }
                Ask::Revert => self.ed.revert(page),
                Ask::Save => {
                    if let Some(plays) = self.save(page, kb, &mut changed) {
                        window.push(plays);
                    }
                }
                Ask::MenuChose(i) => self.menu_chose(page, i.max(0) as usize),
                Ask::MenuTyped(t) => {
                    if let Some(m) = &mut self.menu {
                        m.query = t;
                    }
                    self.build_menu(page);
                }
                Ask::MenuEnter => {
                    let first = self.menu.as_ref().and_then(|m| m.picks.iter().position(|p| matches!(p, Pick::Tap(_) | Pick::Change(..))));
                    if let Some(i) = first {
                        self.menu_chose(page, i);
                    }
                }
                Ask::MenuDismiss => self.close_menu(true),
                Ask::MenuField(_, t) => {
                    let ms: u64 = t.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
                    if let Some(d) = self.current_draft(page) {
                        d.same = ms.min(65535);
                        d.timing = Timing::Same;
                    }
                    self.build_menu(page);
                }
                Ask::ConfirmYes => {
                    self.confirm = false;
                    window.extend(self.delete(page, kb));
                }
                Ask::ConfirmNo => self.confirm = false,
                Ask::RecTick => self.rec_tick(page),
            }
        }
        self.refresh(page);
        (window, changed)
    }

    /// The step under a point in the window, by its index.
    fn step_at(&self, x: f32, y: f32) -> Option<i32> {
        let (cx, cy) = (crate::tokens::size::SIDEBAR + crate::tokens::space::MARGIN, crate::tokens::size::TITLE_BAR + crate::tokens::size::BAR + self.ui.get_chain_scroll());
        self.chain
            .iter()
            .find(|c| c.kind == piece::STEP && c.id == hit::STEP && x >= cx + c.x - 6.0 && x < cx + c.x + c.w && y >= cy + c.y && y < cy + c.y + c.h)
            .map(|c| c.index)
    }

    fn locked(&self, page: &Page) -> bool {
        self.ed.current.and_then(|n| self.ed.saved(page, n)).is_some_and(|m| m["editable"] == false)
    }

    fn current_draft<'a>(&'a mut self, page: &Page) -> Option<&'a mut Draft> {
        let n = self.ed.current?;
        self.ed.draft(page, n)
    }

    fn start_rename(&mut self, page: &Page) {
        if self.locked(page) || self.ed.rec.is_some() {
            return;
        }
        let name = self.current_draft(page).map(|d| d.name.clone()).unwrap_or_default();
        self.ui.set_name_text(name.into());
        self.ed.edit_name = true;
    }

    // ------------------------------------------------------------ menus

    fn open_menu(&mut self, page: &Page, kind: Kind, anchor: (f32, f32, f32), under: f32, right: f32, let_go: bool) {
        // the second click on the button whose menu a press just closed
        if let Some((at, when, go)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                if go {
                    self.ed.ins = None;
                    self.ed.sel_step = None;
                }
                return;
            }
        }
        self.close_menu(false);
        self.menu = Some(Menu { kind, anchor, under, width: under, right, let_go, query: String::new(), picks: Vec::new() });
        self.ui.set_menu_query("".into());
        self.build_menu(page);
        self.place_menu();
        self.ui.set_menu_open(true);
        if matches!(kind, Kind::Keys { .. }) {
            self.ui.invoke_focus_menu();
        }
    }

    /// Below the button when it fits, or when there is a fair amount of room
    /// there anyway (the menu then scrolls); above it only when below is cramped.
    fn place_menu(&mut self) {
        let Some(m) = &self.menu else { return };
        let (_, ay, ah) = m.anchor;
        let (h, win) = (self.ui.get_menu_natural(), self.ui.get_window_height());
        let (top, most) = crate::menu_size::hang(win, (ay, ah), h);
        self.fit_menu_width();
        self.ui.set_menu_top(top);
        self.ui.set_menu_max(most);
        // (turned to what's chosen, where that's out of view)
        let rows: Vec<MenuRow> = self.ui.get_menu_rows().iter().collect();
        let head = crate::menu_size::height(&[], self.ui.get_menu_search(), self.ui.get_menu_back(), 0.0);
        self.ui.invoke_scroll_menu(crate::menu_size::reveal(&rows, h.min(most) - head));
    }

    /// The open menu as wide as its rows say, its right edge kept where it hangs from the
    /// More button (a menu's rows can change while it's open; where it hangs doesn't).
    fn fit_menu_width(&self) {
        let Some(m) = &self.menu else { return };
        let ax = m.anchor.0;
        self.ui.set_menu_w(m.width);
        self.ui.set_menu_ax(if m.right > 0.0 { ax + m.right - m.width } else { ax });
    }

    fn close_menu(&mut self, dismissed: bool) {
        if let Some(m) = self.menu.take() {
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
            if dismissed && m.let_go {
                self.ed.ins = None;
                self.ed.sel_step = None;
            }
        }
    }

    /// A press in the window: away from the open menu closes it, and a press
    /// on the button it hangs from keeps that button's click from opening it again.
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        let Some(m) = &self.menu else { return };
        let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
        if x >= mx && x < mx + mw && y >= my && y < my + mh {
            return;
        }
        let (ax, ay, ah) = m.anchor;
        let aw = if m.right > 0.0 { m.right } else { 120.0 };
        if ah > 0.0 && x >= ax && x < ax + aw && y >= ay && y < ay + ah {
            self.shut = Some(((ax, ay), Instant::now(), m.let_go));
            self.menu.take();
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
            return;
        }
        self.close_menu(true);
        self.refresh_chain_only();
    }

    /// the page drawn again at the next look, for a change made outside its asks
    fn refresh_chain_only(&mut self) {
        self.asks.borrow_mut().push(Ask::RecTick);
    }

    /// The libraries' menu: each library by its name, the one shown ticked,
    /// how many macros it keeps on its right; while a macro is recorded or
    /// saved, the others can't be chosen.
    fn build_libs_menu(&mut self, page: &Page) {
        let still = self.ed.rec.is_some() || self.ed.busy;
        let mut rows = Vec::new();
        let mut picks = Vec::new();
        for (lib, name, count) in self.libraries(page) {
            let shown = lib == self.ed.lib;
            rows.push(MenuRow { sub: count.into(), ..row(if still && !shown { 5 } else { 0 }, &name, shown) });
            picks.push(if still && !shown { Pick::Nothing } else { Pick::Lib(lib) });
        }
        self.show_rows(rows, picks, false, String::new());
    }

    /// The open menu's rows, and what each does, shown as every menu shows them
    /// (src/menu_size.rs): one with a search box Size.menu-search wide, so it keeps its width
    /// as it's searched, the rest as wide as their rows need; never narrower than what
    /// opened it.
    fn show_rows(&mut self, mut rows: Vec<MenuRow>, picks: Vec<Pick>, search: bool, placeholder: String) {
        let under = self.menu.as_ref().map_or(0.0, |m| m.under);
        let w = if search { crate::tokens::size::MENU_SEARCH.max(under) } else { crate::menu_size::width(&rows, self.ui.get_menu_marks(), under) };
        crate::menu_size::fit_notes(&mut rows, w);
        self.ui.set_menu_search(search);
        self.ui.set_menu_back(false);
        self.ui.set_menu_placeholder(placeholder.into());
        self.ui.set_menu_natural(crate::menu_size::height(&rows, search, false, f32::MAX));
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        if let Some(m) = &mut self.menu {
            m.picks = picks;
            m.width = w;
        }
    }

    /// The libraries, in the menu's order: Ectomatter's, then the keyboard's
    /// once it has been read (or while it shows, being read), each with its
    /// name and how many macros it keeps.
    fn libraries(&self, page: &Page) -> Vec<(Lib, String, String)> {
        let ed = |lib: Lib| if self.ed.lib == lib { &self.ed } else { &self.other };
        let mut out = vec![(Lib::Ectomatter, "Ectomatter".to_string(), format!("{} saved", ed(Lib::Ectomatter).macros_count(page)))];
        if page.dev.is_some() {
            let k = ed(Lib::Keyboard);
            out.push((Lib::Keyboard, KEYBOARD.to_string(), format!("{} of {} used", k.macros_count(page), k.room().unwrap_or(0))));
        } else if self.ed.lib == Lib::Keyboard {
            out.push((Lib::Keyboard, KEYBOARD.to_string(), "Reading…".to_string()));
        }
        out
    }

    fn build_menu(&mut self, page: &Page) {
        let Some(m) = &self.menu else { return };
        let (kind, q) = (m.kind, m.query.trim().to_lowercase());
        if kind == Kind::Libs {
            return self.build_libs_menu(page);
        }
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut picks: Vec<Pick> = Vec::new();
        let mut search = false;
        let mut placeholder = String::new();
        let n = self.ed.current.unwrap_or(0);
        let d = self.ed.drafts.get(&n).cloned().or_else(|| {
            let m = self.ed.saved(page, n)?;
            Some(Draft { name: m["name"].as_str().unwrap_or("").into(), blank_name: None, steps: m["steps"].as_array().cloned().unwrap_or_default(), is_new: false, starter: false, timing: Timing::Keep, same: 20, plays: None })
        });
        let Some(d) = d else { return };
        match kind {
            Kind::Insert => {
                rows.push(row(1, "Insert here", false));
                picks.push(Pick::Nothing);
                for (label, p) in [("Key", Pick::Keys(false, None)), ("Media key", Pick::Keys(true, None)), ("Wait", Pick::Wait)] {
                    rows.push(row(0, label, false));
                    picks.push(p);
                }
            }
            Kind::Step(i) if d.steps.get(i).is_some_and(macros::is_job) => {
                let st = &d.steps[i];
                let other = macros::macro_of(st).is_some();
                rows.push(row(1, &macros::step_words(st, &self.ed.jobs), false));
                picks.push(Pick::Nothing);
                rows.push(row(0, if other { "Change macro ›" } else { "Change action ›" }, false));
                picks.push(Pick::ChangeJob(i, other));
                rows.push(row(2, "", false));
                picks.push(Pick::Nothing);
                rows.push(row(4, "Remove", false));
                picks.push(Pick::Remove(i));
            }
            Kind::Jobs { macros: true, change } => {
                // Ectomatter's other macros, none that plays this one (it would play itself)
                let current = change.and_then(|i| d.steps.get(i)).and_then(macros::macro_of);
                rows.push(row(1, "Ectomatter’s macros", false));
                picks.push(Pick::Nothing);
                let mut any = false;
                for (m, name) in &self.ed.jobs.macros {
                    if *m == n || macros::plays_macro(*m, n, &mut Vec::new()) {
                        continue;
                    }
                    any = true;
                    let j = crate::jobs::Job::Macro { n: *m, plays: ectomatter_protocol::Plays::Once };
                    rows.push(row(0, name, current == Some(*m)));
                    picks.push(Pick::Job(j.id()));
                }
                if !any {
                    rows.push(row(3, "No other macros yet.", false));
                    picks.push(Pick::Nothing);
                }
            }
            Kind::Jobs { macros: false, change } => {
                // each device's jobs under its name, as they sit on the desk: its profiles, the
                // next and the one before, and the keyboard's screen jobs; then the scenes'
                use crate::jobs::Job;
                let current = change.and_then(|i| d.steps.get(i)).and_then(macros::job_of);
                let names = &self.ed.jobs;
                let push = |rows: &mut Vec<MenuRow>, picks: &mut Vec<Pick>, label: &str, j: Job| {
                    rows.push(row(0, label, current.as_ref() == Some(&j)));
                    picks.push(Pick::Job(j.id()));
                };
                let groups = names.devices.iter().filter(|dev| dev.here).map(|dev| (dev.name.clone(), dev.jobs())).chain(std::iter::once(("Scenes".to_string(), names.scene_jobs())));
                for (head, jobs) in groups.filter(|(_, jobs)| !jobs.is_empty()) {
                    if !rows.is_empty() {
                        rows.push(row(2, "", false));
                        picks.push(Pick::Nothing);
                    }
                    rows.push(row(1, &head, false));
                    picks.push(Pick::Nothing);
                    for j in jobs {
                        let words = names.short(&j);
                        push(&mut rows, &mut picks, &words, j);
                    }
                }
                if rows.is_empty() {
                    rows.push(row(3, "No device is here to act on.", false));
                    picks.push(Pick::Nothing);
                }
            }
            Kind::Step(i) => {
                let Some(st) = d.steps.get(i) else { return };
                let down = st["down"].as_bool().unwrap_or(false);
                let media = st.get("media").is_some();
                rows.push(row(1, &macros::step_words(st, &self.ed.jobs), false));
                picks.push(Pick::Nothing);
                rows.push(row(0, "Press ↓", down));
                picks.push(Pick::Down(i, true));
                rows.push(row(0, "Release ↑", !down));
                picks.push(Pick::Down(i, false));
                rows.push(row(2, "", false));
                picks.push(Pick::Nothing);
                rows.push(row(0, "Change key ›", false));
                picks.push(Pick::Keys(media, Some(i)));
                rows.push(row(4, "Remove", false));
                picks.push(Pick::Remove(i));
            }
            Kind::Keys { media, change } => {
                search = true;
                placeholder = if media { "Search media keys" } else { "Search keys" }.into();
                let current = change.and_then(|i| d.steps.get(i)).map(|st| if media { json!({ "media": st["media"] }) } else { json!({ "key": st["key"] }) });
                let groups: Vec<(&str, Vec<(String, Value)>)> = if media {
                    vec![("Media keys", MEDIA.iter().map(|(c, n)| (n.to_string(), json!({ "media": ectomatter_protocol::media::name(*c as u16).map_or(json!(c), |m| json!(m)) }))).collect())]
                } else {
                    let (mods, keys) = macros::step_keys();
                    vec![
                        ("Modifiers", mods.into_iter().map(|(l, k)| (l, json!({ "key": k }))).collect()),
                        ("Keys", keys.into_iter().map(|(l, k)| (l, json!({ "key": k }))).collect()),
                    ]
                };
                let mut hits = 0;
                for (head, items) in groups {
                    let hit: Vec<&(String, Value)> = items.iter().filter(|(l, _)| q.is_empty() || l.to_lowercase().contains(&q)).collect();
                    if hit.is_empty() {
                        continue;
                    }
                    rows.push(row(1, head, false));
                    picks.push(Pick::Nothing);
                    for (label, v) in hit.into_iter().take(if q.is_empty() { 400 } else { 60 }) {
                        hits += 1;
                        rows.push(row(0, label, current.as_ref() == Some(v)));
                        picks.push(match change {
                            Some(i) => Pick::Change(i, v.clone()),
                            None => Pick::Tap(v.clone()),
                        });
                    }
                }
                if hits == 0 {
                    rows.push(row(3, "Nothing matches.", false));
                    picks.push(Pick::Nothing);
                }
            }
            Kind::Timing => {
                rows.push(row(1, "Timing", false));
                picks.push(Pick::Nothing);
                rows.push(row(0, "Keep the waits as they are", d.timing == Timing::Keep));
                picks.push(Pick::Timing(Timing::Keep));
                rows.push(row(0, "No waits", d.timing == Timing::None));
                picks.push(Pick::Timing(Timing::None));
                rows.push(MenuRow { tag: d.same.to_string().into(), ..row(6, "Same wait everywhere", d.timing == Timing::Same) });
                picks.push(Pick::Timing(Timing::Same));
                rows.push(row(2, "", false));
                picks.push(Pick::Nothing);
                rows.push(row(0, "Record waits while recording", self.ed.record_waits));
                picks.push(Pick::RecordWaits);
            }
            Kind::Plays => {
                let cur = d.plays.map(|(m, c)| Plays::One(m, c)).or_else(|| self.ed.plays_of(page, n));
                rows.push(row(1, "Each time its key is pressed", false));
                picks.push(Pick::Nothing);
                for (v, label) in PLAYS {
                    rows.push(row(0, label, matches!(cur, Some(Plays::One(m, _)) if macros::same_choice(m, v))));
                    picks.push(Pick::Plays(v));
                }
            }
            Kind::Libs => {}
            Kind::Macro => {
                let saved = self.ed.saved(page, n);
                let locked = saved.as_ref().is_some_and(|m| m["editable"] == false);
                rows.push(row(if locked { 5 } else { 0 }, "Rename", false));
                picks.push(if locked { Pick::Nothing } else { Pick::Rename });
                let free = !self.ed.free_slots(page).is_empty();
                rows.push(row(if free { 0 } else { 5 }, "Duplicate", false));
                picks.push(if free { Pick::Duplicate } else { Pick::Nothing });
                rows.push(row(4, if saved.is_some() { "Delete macro…" } else { "Discard this new macro" }, false));
                picks.push(Pick::Delete);
            }
        }
        self.show_rows(rows, picks, search, placeholder);
    }

    fn menu_chose(&mut self, page: &mut Page, i: usize) {
        let Some(m) = &self.menu else { return };
        let Some(p) = m.picks.get(i).cloned() else { return };
        let (anchor, under, let_go) = (m.anchor, m.under, m.let_go);
        match p {
            Pick::Nothing => return,
            Pick::Tap(v) => {
                self.close_menu(false);
                self.ed.insert_tap(page, &v);
            }
            Pick::Change(i, v) => {
                self.close_menu(false);
                if let Some(st) = self.current_draft(page).and_then(|d| d.steps.get_mut(i)) {
                    if let Some(o) = st.as_object_mut() {
                        o.remove("key");
                        o.remove("media");
                        if let Some(nv) = v.as_object() {
                            for (k, x) in nv {
                                o.insert(k.clone(), x.clone());
                            }
                        }
                    }
                }
            }
            Pick::Down(i, down) => {
                self.close_menu(false);
                if let Some(st) = self.current_draft(page).and_then(|d| d.steps.get_mut(i)) {
                    st["down"] = json!(down);
                }
            }
            Pick::Keys(media, change) => {
                // the next menu hangs from the same place
                self.menu.take();
                self.menu = Some(Menu { kind: Kind::Keys { media, change }, anchor, under, width: under, right: 0.0, let_go, query: String::new(), picks: Vec::new() });
                self.ui.set_menu_query("".into());
                self.build_menu(page);
                self.place_menu();
                self.ui.invoke_focus_menu();
            }
            Pick::Wait => {
                self.close_menu(false);
                self.ed.insert_wait(page);
            }
            Pick::Remove(i) => {
                self.close_menu(false);
                if let Some(d) = self.current_draft(page) {
                    if i < d.steps.len() {
                        d.steps.remove(i);
                    }
                }
                self.ed.sel_step = None;
                self.ed.ins = None;
            }
            Pick::Timing(t) => {
                self.close_menu(false);
                if let Some(d) = self.current_draft(page) {
                    d.timing = t;
                }
            }
            Pick::RecordWaits => {
                self.ed.record_waits = !self.ed.record_waits;
                crate::uistate::set("record_waits", json!(self.ed.record_waits));
                self.build_menu(page);
            }
            Pick::Plays(v) => {
                self.close_menu(false);
                let n = self.ed.current.unwrap_or(0);
                let cur = self.ed.drafts.get(&n).and_then(|d| d.plays).map(|(m, c)| Plays::One(m, c)).or_else(|| self.ed.plays_of(page, n));
                let count = match cur {
                    Some(Plays::One(_, c)) if c > 1 => c,
                    _ => 3,
                };
                if let Some(d) = self.current_draft(page) {
                    d.plays = Some((macros::with_count(v, count), count));
                }
            }
            Pick::Job(id) => {
                let change = match m.kind {
                    Kind::Jobs { change, .. } => change,
                    _ => None,
                };
                self.close_menu(false);
                match change {
                    Some(i) => {
                        if let Some(st) = self.current_draft(page).and_then(|d| d.steps.get_mut(i)) {
                            st["job"] = json!(id);
                        }
                    }
                    None => self.ed.insert_job(page, &id),
                }
            }
            Pick::ChangeJob(i, macros) => {
                // the next menu hangs from the same place
                self.menu.take();
                self.menu = Some(Menu { kind: Kind::Jobs { macros, change: Some(i) }, anchor, under, width: under, right: 0.0, let_go, query: String::new(), picks: Vec::new() });
                self.ui.set_menu_query("".into());
                self.build_menu(page);
                self.place_menu();
            }
            Pick::Lib(lib) => {
                self.close_menu(false);
                if self.ed.lib != lib {
                    self.show_lib(lib);
                    self.ui.set_lib_scroll(0.0);
                    self.scroll_to = true;
                }
            }
            Pick::Rename => {
                self.close_menu(false);
                self.start_rename(page);
            }
            Pick::Duplicate => {
                self.close_menu(false);
                match self.ed.duplicate(page) {
                    Ok(()) => self.scroll_to = true,
                    Err(e) => self.status(e, 2),
                }
            }
            Pick::Delete => {
                self.close_menu(false);
                let n = self.ed.current.unwrap_or(0);
                if self.ed.saved(page, n).is_none() {
                    self.ed.drafts.remove(&n);
                    self.ed.current = None;
                    return;
                }
                let name = self.current_draft(page).map(|d| d.name.clone()).unwrap_or_default();
                let places = self.ed.places(page, n);
                let body = if places.is_empty() {
                    if self.ed.lib == Lib::Keyboard { "No key runs this macro." } else { "Nothing runs this macro." }.to_string()
                } else {
                    let names = self.ed.names.clone();
                    let list: Vec<String> = places
                        .iter()
                        .map(|(layer, pos, ps)| match self.ed.lib {
                            Lib::Keyboard => {
                                let key = page.key(*pos).map(|k| page.key_label(k)).unwrap_or_else(|| format!("Key {pos}"));
                                format!("{}{key} ({})", if layer == "fn" { "Fn + " } else { "" }, macros::profiles_text(ps, |n| page.profile_name(n)))
                            }
                            // (a device's control, by its words, on its profiles by their names)
                            Lib::Ectomatter => {
                                let on: Vec<String> = ps.iter().filter_map(|p| names.get(*p as usize).cloned()).collect();
                                format!("{layer} ({})", on.join(", "))
                            }
                        })
                        .collect();
                    format!("{} will do nothing until you give {} something else.", list.join(", "), if places.len() == 1 { "it" } else { "them" })
                };
                self.ui.set_confirm_title(format!("Delete {name}?").into());
                self.ui.set_confirm_body(body.into());
                self.ui.set_confirm_body2(if self.ed.lib == Lib::Keyboard { "This removes it from the keyboard and cannot be undone." } else { "This removes it from Ectomatter and cannot be undone." }.into());
                self.confirm = true;
            }
        }
    }

    // ------------------------------------------------------------ recording

    fn start_recording(&mut self, page: &Page) {
        let Some(n) = self.ed.current else { return };
        let keyboard = self.ed.lib == Lib::Keyboard;
        let Some(d) = self.ed.draft(page, n) else { return };
        if d.timing != Timing::Keep {
            d.steps = Editor::effective(d);
            d.timing = Timing::Keep;
        }
        // (Ectomatter's own macros hold any number of steps)
        let room = if keyboard { ROOM.saturating_sub(macros::bytes(&d.steps)) } else { 1 << 24 };
        self.close_menu(false);
        self.ed.sel_step = None;
        self.ed.edit_wait = None;
        // ECTOMATTER_FAKE_REC: steps as JSON, to check how a recording
        // looks offscreen without taking the keyboard
        if let Some(steps) = std::env::var("ECTOMATTER_FAKE_REC").ok().and_then(|v| serde_json::from_str::<Vec<Value>>(&v).ok()) {
            self.ed.rec = Some(Recording { used: macros::bytes(&steps), steps, elapsed: 3.4 });
            self.status("Recording. The Z10\u{2019}s keys go only to the recording until you press Stop.", 0);
            self.fake_rec = true;
            return;
        }
        match self.rec.start(room, self.ed.record_waits) {
            Ok(_) => {
                self.ed.rec = Some(Recording { steps: Vec::new(), used: 0, elapsed: 0.0 });
                self.status("Recording. The Z10\u{2019}s keys go only to the recording until you press Stop.", 0);
                let a = self.asks.clone();
                self.rec_timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || a.borrow_mut().push(Ask::RecTick));
            }
            Err(e) => self.status(format!("Recording did not start: {e}"), 2),
        }
    }

    fn rec_tick(&mut self, page: &Page) {
        if self.ed.rec.is_none() || self.fake_rec {
            return;
        }
        let r = self.rec.status();
        if let Some(rec) = &mut self.ed.rec {
            // (the keyboard's recorder gives its own steps: as the protocol's)
            rec.steps = evga_z10::shared::steps_of(&r["steps"].as_array().cloned().unwrap_or_default());
            rec.used = r["used"].as_u64().unwrap_or(0) as usize;
            rec.elapsed = r["elapsed"].as_f64().unwrap_or(0.0);
        }
        if r["recording"] != true {
            let reason = r["reason"].as_str().map(String::from);
            self.stop_recording(page, true, reason);
        }
    }

    fn stop_recording(&mut self, page: &Page, keep: bool, reason: Option<String>) {
        if self.ed.rec.take().is_none() {
            return;
        }
        self.rec_timer.stop();
        let r = self.rec.stop(keep);
        let got = evga_z10::shared::steps_of(&r["steps"].as_array().cloned().unwrap_or_default());
        if !got.is_empty() {
            self.ed.insert_at(page, got.clone());
        }
        let why = reason.filter(|r| !r.is_empty() && r != "stopped").map_or(String::new(), |r| format!(" It stopped because {r}."));
        let skipped = r["skipped"].as_u64().unwrap_or(0);
        let text = if got.is_empty() {
            format!("Nothing was recorded.{why}")
        } else {
            let mut t = format!("Recorded {} step{}.{why}", got.len(), if got.len() == 1 { "" } else { "s" });
            if skipped > 0 {
                t += &format!(" {skipped} key{} a macro cannot hold were left out.", if skipped == 1 { "" } else { "s" });
            }
            t
        };
        self.status(text, 0);
    }

    /// Stop recording without keeping it, as the window goes.
    pub fn cancel_recording(&mut self, page: &Page) {
        self.stop_recording(page, false, None);
    }

    // ------------------------------------------------------------ save, delete

    /// Save the macro open. Ectomatter's is kept at once, and how it plays goes
    /// to the controls that run it (handed back for the page to do); the
    /// keyboard's is written to it, and its keys rewritten for how it plays.
    fn save(&mut self, page: &mut Page, kb: &dyn Fn(device::Ask), changed: &mut bool) -> Option<WindowAsk> {
        let n = self.ed.current?;
        let d = self.ed.draft(page, n).cloned()?;
        let (steps, added) = Editor::to_save(&d);
        if self.ed.lib == Lib::Ectomatter {
            let name = if d.name.is_empty() { format!("Macro {n}") } else { d.name.clone() };
            crate::library::set(n, &name, steps);
            self.ed.drafts.remove(&n);
            let mut t = format!("Saved {name}.");
            if !added.is_empty() {
                let names: Vec<String> = added.iter().map(macros::step_name).collect();
                t += &format!(" {} {} let go at the end so nothing stays held down.", names.join(" and "), if added.len() == 1 { "was" } else { "were" });
            }
            self.status(t, 1);
            // (how it plays, if that changed, else its steps, go to the controls that run it)
            return Some(d.plays.map_or(WindowAsk::MacroSaved(n), |(how, count)| WindowAsk::MacroPlays(n, macros::with_count(how, count))));
        }
        if macros::bytes(&steps) > ROOM {
            self.status("This macro is too long for the keyboard. Remove some steps and save again.", 2);
            return None;
        }
        let m = self.ed.saved(page, n).unwrap_or(Value::Null);
        let name = if d.name.is_empty() { format!("Macro {n}") } else { d.name.clone() };
        self.ed.busy = true;
        self.status(format!("Saving {}…", d.name), 0);
        kb(device::Ask::WriteMacro(
            n as u8,
            json!({ "name": name, "steps": steps, "run": m["run"].as_u64().unwrap_or(9), "repeat": m["repeat"].as_u64().unwrap_or(0), "unit": m["unit"].as_u64().unwrap_or(1) }),
        ));
        // how it plays lives on its keys: rewrite those, profile by profile, on
        // top of what each profile has saved, and carry the change into its draft
        let mut profiles = 0;
        if let Some((how, count)) = d.plays {
            let mut by: BTreeMap<u8, Vec<(String, u32)>> = BTreeMap::new();
            for (layer, pos, ps) in self.ed.places(page, n) {
                for p in ps {
                    by.entry(p).or_default().push((layer.clone(), pos));
                }
            }
            let v = ectomatter_protocol::Action::Macro { n, plays: macros::with_count(how, count) }.write();
            for (p, ks) in by {
                let saved = &page.dev.as_ref().unwrap()["profiles"][p.to_string()];
                let mut body = json!({ "keys": saved["keys"].clone() });
                for (layer, pos) in ks {
                    let id = page.id(pos).to_string();
                    body["keys"][&layer][&id] = v.clone();
                    if let Some(dr) = page.drafts.get_mut(&p) {
                        dr["keys"][&layer][&id] = v.clone();
                    }
                }
                kb(device::Ask::SaveKeys(p, body));
                profiles += 1;
            }
            *changed = true;
        }
        self.saving = Some(Saving { n, name: d.name.clone(), added, profiles, wrote: false });
        None
    }

    /// Delete the macro open. Ectomatter's goes at once, and the controls that
    /// ran it are handed back for the page to clear.
    fn delete(&mut self, page: &Page, kb: &dyn Fn(device::Ask)) -> Option<WindowAsk> {
        let n = self.ed.current?;
        let name = self.ed.drafts.get(&n).map(|d| d.name.clone()).or_else(|| self.ed.saved(page, n).and_then(|m| m["name"].as_str().map(String::from))).unwrap_or(format!("Macro {n}"));
        if self.ed.lib == Lib::Ectomatter {
            crate::library::delete(n);
            self.ed.drafts.remove(&n);
            self.ed.current = None;
            self.status(format!("Deleted {name}. Any button set to it now does nothing."), 1);
            return Some(WindowAsk::MacroGone(n));
        }
        self.ed.busy = true;
        self.status(format!("Deleting {name}…"), 0);
        self.saving = Some(Saving { n, name, added: Vec::new(), profiles: 0, wrote: false });
        kb(device::Ask::WriteMacro(n as u8, json!({ "delete": true })));
        None
    }

    /// The keyboard answered a write; true when the Z10's page should show it.
    pub fn heard(&mut self, page: &mut Page, h: device::Heard) {
        let Some(s) = &mut self.saving else { return };
        match h {
            device::Heard::Macros(delete, Ok(macros)) => {
                if let Some(dev) = &mut page.dev {
                    dev["macros"] = macros;
                }
                if delete {
                    let (n, name) = (s.n, s.name.clone());
                    self.saving = None;
                    self.ed.busy = false;
                    self.ed.drafts.remove(&n);
                    self.ed.current = None;
                    self.status(format!("Deleted {name}. Any key set to it now does nothing."), 1);
                } else {
                    s.wrote = true;
                    self.finish_save();
                }
            }
            device::Heard::KeysSaved(p, Ok(mut prof)) => {
                crate::z10::lit_screen(&mut prof);
                if let Some(dev) = &mut page.dev {
                    dev["profiles"][p.to_string()] = prof;
                }
                s.profiles = s.profiles.saturating_sub(1);
                self.finish_save();
            }
            device::Heard::Macros(delete, Err(e)) => {
                let name = s.name.clone();
                self.saving = None;
                self.ed.busy = false;
                self.status(format!("{name} was not {}: {e}", if delete { "deleted" } else { "saved" }), 2);
            }
            device::Heard::KeysSaved(_, Err(e)) => {
                let name = s.name.clone();
                self.saving = None;
                self.ed.busy = false;
                self.status(format!("{name} was not saved: {e}"), 2);
            }
            _ => {}
        }
        self.refresh(page);
    }

    fn finish_save(&mut self) {
        let Some(s) = &self.saving else { return };
        if !s.wrote || s.profiles > 0 {
            return;
        }
        let s = self.saving.take().unwrap();
        self.ed.busy = false;
        self.ed.drafts.remove(&s.n);
        let mut t = format!("Saved {} to the keyboard.", s.name);
        if !s.added.is_empty() {
            let names: Vec<String> = s.added.iter().map(macros::step_name).collect();
            t += &format!(" {} {} let go at the end so nothing stays held down.", names.join(" and "), if s.added.len() == 1 { "was" } else { "were" });
        }
        self.status(t, 1);
    }

    // ------------------------------------------------------------ the page's models

    /// The list: the shown library's macros (its drop-down, above, names it), under
    /// a note saying where they're kept and who plays them, beside a way to
    /// make a new one there. The keyboard's waits while it is being read.
    fn sections(&self, page: &Page) -> Vec<MacroItem> {
        let (lib, ed) = (self.ed.lib, &self.ed);
        let still = ed.rec.is_some() || ed.busy;
        let read = lib == Lib::Ectomatter || page.dev.is_some();
        let count = ed.macros_count(page);
        let note = match lib {
            Lib::Ectomatter if self.mouse_holds => format!("Kept in Ectomatter. Shortcuts on your mouse play even with Ectomatter closed. {count} saved."),
            Lib::Ectomatter => format!("Kept in Ectomatter, so they play while Ectomatter is running. {count} saved."),
            Lib::Keyboard if !read => "Reading the keyboard…".to_string(),
            Lib::Keyboard => format!("Kept on the keyboard, so they play even with Ectomatter closed. {count} of {} slots used.", ed.room().unwrap_or(0)),
        };
        let note = note_lines(&note);
        let l = if lib == Lib::Keyboard { 0 } else { 1 };
        let h = NOTE_TOP + note.len() as f32 * NOTE_LINE + UNDER_NOTE;
        let mut out = vec![MacroItem {
            kind: 1,
            lib: l,
            n: 0,
            name: "".into(),
            sub: note.join("\n").into(),
            lines: ModelRc::new(VecModel::from(note.iter().map(|l| SharedString::from(l.as_str())).collect::<Vec<_>>())),
            timed: false,
            current: false,
            dirty: false,
            can_new: read && !still && !ed.free_slots(page).is_empty(),
            y: 0.0,
            h,
        }];
        if !read {
            return out;
        }
        let mut y = h;
        for n in ed.list(page) {
            let d = ed.drafts.get(&n);
            let m = ed.saved(page, n);
            let steps = d.map(Editor::effective).unwrap_or_else(|| m.as_ref().and_then(|m| m["steps"].as_array().cloned()).unwrap_or_default());
            let name = d.map(|d| d.name.clone()).or_else(|| m.as_ref().and_then(|m| m["name"].as_str().map(String::from))).filter(|s| !s.is_empty()).unwrap_or(format!("Macro {n}"));
            out.push(MacroItem {
                kind: 0,
                lib: l,
                n: n as i32,
                name: name.into(),
                sub: if d.is_some_and(|d| d.is_new) { "Not saved yet".into() } else { macros::describe(&steps, &ed.jobs).into() },
                lines: ModelRc::default(),
                timed: steps.iter().any(|s| s["after"].as_u64().unwrap_or(0) > 0),
                current: ed.current == Some(n),
                dirty: ed.dirty(page, n),
                can_new: false,
                y,
                h: ROW_H,
            });
            y += ROW_H;
        }
        out
    }

    pub fn refresh(&mut self, page: &Page) {
        let keyboard = self.ed.lib == Lib::Keyboard;
        // (Ectomatter's own macros belong to no one device)
        self.ui.set_lib_title(if keyboard { KEYBOARD.into() } else { "".into() });
        let read = !keyboard || page.dev.is_some();
        if read {
            self.ed.settle(page);
        }
        // the library shown, on its drop-down
        self.ui.set_lib_name(if keyboard { KEYBOARD } else { "Ectomatter" }.into());
        let items = self.sections(page);
        let end = items.last().map_or(0.0, |m| m.y + m.h);
        // the chosen one brought into view, as the web page scrolled it to the
        // nearest edge, with its section's heading when it's the first under it
        if std::mem::take(&mut self.scroll_to) {
            if let Some(i) = items.iter().position(|m| m.current) {
                let view = self.ui.get_window_height() - LIST_TOP - 10.0;
                let top = if i > 0 && items[i - 1].kind == 1 { items[i - 1].y } else { items[i].y };
                let bottom = items[i].y + ROW_H - 2.0;
                let at = -self.ui.get_lib_scroll();
                if top < at {
                    self.ui.set_lib_scroll(-top);
                } else if bottom > at + view {
                    self.ui.set_lib_scroll(-(bottom - view));
                }
            }
        }
        crate::front::set_rows(&self.items, items);
        self.ui.set_items_h(end);
        let ui = &self.ui;
        if !read {
            ui.set_status_text("Reading the keyboard…".into());
            return;
        }
        let rec = self.ed.rec.is_some();

        let Some(n) = self.ed.current else { return };
        let locked = self.locked(page);
        let dirty = self.ed.dirty(page, n);
        let chain_w = ui.get_chain_width();
        let Some(d) = self.ed.draft(page, n).cloned() else { return };
        let ed = &self.ed;
        let ui = &self.ui;
        ui.set_macro_name(d.name.clone().into());
        ui.set_editing_name(ed.edit_name);
        ui.set_can_rename(!locked && !rec);
        let (state, kind) = if rec {
            ("Recording", 2)
        } else if d.is_new {
            ("Not saved yet", if dirty { 1 } else { 0 })
        } else if dirty {
            ("Changed, not saved yet", 1)
        } else if keyboard {
            ("Saved on the keyboard", 0)
        } else {
            ("Saved in Ectomatter", 0)
        };
        ui.set_state_text(state.into());
        ui.set_state_kind(kind);
        ui.set_recording(rec);

        let eff = Editor::effective(&d);
        let live = ed.rec.as_ref().map_or(0, |r| r.steps.len());
        let (cells, h) = macros::chain(ed, &d, locked, chain_w);
        crate::front::sync_cells(&self.chain, cells);
        ui.set_chain_h(h);
        ui.set_chain_empty(if eff.is_empty() && live == 0 { if rec { 2 } else { 1 } } else { 0 });
        // (a recording is typed on the keyboard, for either library, while there's one)
        // (Ectomatter's own macros also take actions and other macros)
        let hint = if rec {
            "Type the keys on your Z10. Press Stop to finish."
        } else if page.dev.is_some() && keyboard {
            "Press Record and type the keys on your Z10, or add keys, media keys and waits with the buttons below."
        } else if page.dev.is_some() {
            "Press Record and type the keys on your Z10, or add keys, media keys, waits, actions and other macros with the buttons below."
        } else {
            "Add keys, media keys, waits, actions and other macros with the buttons below."
        };
        let lines: Vec<SharedString> = crate::text::wrap(font::REGULAR, font::CONTROL, hint, size::EMPTY).into_iter().map(Into::into).collect();
        ui.set_empty_lines(ModelRc::new(VecModel::from(lines)));

        // (the keyboard's macros have room for so many steps; Ectomatter's, any number)
        let used = macros::bytes(&eff) + ed.rec.as_ref().map_or(0, |r| r.used);
        let left = if keyboard { ROOM as i64 - used as i64 } else { i64::MAX };
        let count = eff.len() + eff.iter().filter(|s| s["after"].as_u64().unwrap_or(0) > 0).count();
        let (meter, warn) = if left < 60 {
            (if left < 4 { "Full".to_string() } else { format!("Nearly full: room for about {} more key presses", left / 4) }, true)
        } else {
            (if count > 0 { format!("{count} step{}", if count == 1 { "" } else { "s" }) } else { String::new() }, false)
        };
        let off = locked || rec;
        ui.set_meter_text(meter.into());
        ui.set_meter_warn(warn);
        ui.set_tools_off(off);
        ui.set_can_clear(!off && !d.steps.is_empty());
        ui.set_can_key(!off && left >= 8);
        ui.set_can_media(!off && left >= 12);
        ui.set_can_wait(!off && !d.steps.is_empty());
        ui.set_jobs_shown(!keyboard);
        ui.set_can_jobs(!off);
        ui.set_can_record(!locked && left >= 4 && page.dev.is_some());
        if let Some(r) = &ed.rec {
            ui.set_rec_clock(macros::clock(r.elapsed).into());
            ui.set_blink((r.elapsed * 2.0).floor() as i64 % 2 == 0);
        }

        let places = self.ed.places(page, n);
        let plays = d.plays.map(|(m, c)| Plays::One(m, c)).or_else(|| self.ed.plays_of(page, n));
        let label = match (places.is_empty(), plays) {
            (true, _) => "Play once".to_string(),
            (_, Some(Plays::Varies)) => "Varies by key".into(),
            (_, Some(Plays::One(m, c))) => format!("{}{}", PLAYS.iter().find(|(v, _)| macros::same_choice(*v, m)).map_or("Play once", |p| p.1), if matches!(m, macros::How::Times(_)) { format!(" ({c} times)") } else { String::new() }),
            (_, None) => "Play once".into(),
        };
        ui.set_plays_text(label.into());
        ui.set_plays_on(!places.is_empty() && !off);
        match plays {
            Some(Plays::One(macros::How::Times(_), c)) if !places.is_empty() => {
                ui.set_count_shown(true);
                ui.set_count_text(c.to_string().into());
            }
            _ => ui.set_count_shown(false),
        }
        ui.set_locked(locked);
        ui.set_status_text(SharedString::from(self.status.0.as_str()));
        ui.set_status_kind(self.status.1);
        let can = dirty && !rec && !ed.busy;
        ui.set_can_revert(can);
        ui.set_can_save(can && !d.steps.is_empty());
        ui.set_confirm_open(self.confirm);
        if self.menu.is_some() {
            self.build_menu(page);
            self.fit_menu_width();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Slint drawing nowhere, for a window made in a test.
    struct Nowhere;
    impl slint::platform::Platform for Nowhere {
        fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(slint::platform::software_renderer::MinimalSoftwareWindow::new(slint::platform::software_renderer::RepaintBufferType::NewBuffer))
        }
    }

    /// One of Ectomatter's macros takes actions and other macros as steps:
    /// + Action lists each device's profiles and the screen's jobs, + Macro
    /// the other macros (not this one, nor one that plays it), each chosen
    /// goes in as a step, the chain shows them as jobs, and a job's step has
    /// its own menu. The keyboard's macros, which it plays itself, offer neither.
    #[test]
    fn ectomatters_macros_take_actions_and_other_macros() {
        let _one = crate::library::TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let _ = slint::platform::set_platform(Box::new(Nowhere));
        crate::library::set(81, "Gaming on", vec![]);
        crate::library::set(82, "Plays it", vec![json!({ "job": "macro:81:once", "after": 0 })]);
        crate::library::set(83, "Another", vec![json!({ "key": "A", "down": true, "after": 0 }), json!({ "key": "A", "down": false, "after": 0 })]);
        let mut page = Page::new();
        page.dev = Some(json!({ "macros": {} }));
        let mut mp = MacroPage::new(MacroWindow::new().unwrap(), slint::Image::default());
        let device = |key: &str, name: &str, profiles: &[(&str, &str)], screen: bool| crate::jobs::Profiled {
            key: key.into(),
            name: name.into(),
            profiles: profiles.iter().map(|(id, n)| (id.to_string(), n.to_string())).collect(),
            steps: true,
            screen,
            here: true,
            kind: String::new(),
        };
        let names = crate::jobs::Names {
            devices: vec![device("z10", "EVGA Z10 RGB", &[("1", "Studio"), ("2", "FPS Gaming")], true), device("sensei310", "SteelSeries Sensei 310", &[("pa", "Studio"), ("pb", "Gaming")], false)],
            macros: vec![(81, "Gaming on".into()), (82, "Plays it".into()), (83, "Another".into())],
            scenes: vec![("wave".into(), "Wave across the desk".into()), ("scene-1".into(), "DOOM".into())],
        };
        mp.ed.jobs = names.clone();
        mp.other.jobs = names;
        mp.open(&page, Lib::Ectomatter, Some(81));
        assert!(mp.ui.get_jobs_shown() && mp.ui.get_can_jobs());
        let menu = |mp: &MacroPage| mp.ui.get_menu_rows().iter().map(|r| (r.kind, r.label.to_string())).collect::<Vec<_>>();
        let choose = |mp: &mut MacroPage, page: &mut Page, label: &str, after: usize| {
            let i = mp.ui.get_menu_rows().iter().enumerate().skip(after).find(|(_, r)| r.label == label).map(|(i, _)| i).unwrap();
            mp.ui.invoke_menu_chose(i as i32);
            mp.act(page, &|_| {});
        };
        // + Action: each device's under its own name, the keyboard's profiles and screen jobs, then the mouse's
        mp.ui.invoke_tool(6, 0.0, 0.0, 30.0, 0.0);
        mp.act(&mut page, &|_| {});
        let rows = menu(&mp);
        assert_eq!(rows[..5], [(1, "EVGA Z10 RGB".into()), (0, "Studio".into()), (0, "FPS Gaming".into()), (0, "Next profile".into()), (0, "Previous profile".into())]);
        let mouse = rows.iter().position(|r| *r == (1, "SteelSeries Sensei 310".to_string())).unwrap();
        assert!(rows[..mouse].contains(&(0, "Screen: next layout".to_string())));
        assert_eq!(rows[mouse + 1..mouse + 3], [(0, "Studio".into()), (0, "Gaming".into())]);
        // then the scenes, each by its name, the next, the one before and none
        let scenes = rows.iter().position(|r| *r == (1, "Scenes".to_string())).unwrap();
        assert!(scenes > mouse);
        assert_eq!(rows[scenes + 1..], [(0, "Wave across the desk".into()), (0, "DOOM".into()), (0, "Next scene".into()), (0, "Previous scene".into()), (0, "Scene off".into())]);
        choose(&mut mp, &mut page, "FPS Gaming", 0);
        mp.ui.invoke_tool(6, 0.0, 0.0, 30.0, 0.0);
        mp.act(&mut page, &|_| {});
        choose(&mut mp, &mut page, "Gaming", mouse);
        mp.ui.invoke_tool(6, 0.0, 0.0, 30.0, 0.0);
        mp.act(&mut page, &|_| {});
        choose(&mut mp, &mut page, "DOOM", scenes);
        let steps = mp.ed.drafts[&81].steps.clone();
        assert_eq!(steps, [json!({ "job": "profile:z10:2", "after": 0 }), json!({ "job": "profile:sensei310:pb", "after": 0 }), json!({ "job": "scene:scene-1", "after": 0 })]);
        // the chain shows them as jobs, in their words
        let chips: Vec<(i32, String)> = mp.chain.iter().filter(|c| c.kind == piece::STEP).map(|c| (c.style, c.text.to_string())).collect();
        assert_eq!(chips, [(2, "EVGA Z10 RGB: FPS Gaming".into()), (2, "SteelSeries Sensei 310: Gaming".into()), (2, "Scene: DOOM".into())]);
        // + Macro: the other macros, not this one nor the one that plays it
        mp.ui.invoke_tool(7, 0.0, 0.0, 30.0, 0.0);
        mp.act(&mut page, &|_| {});
        assert_eq!(menu(&mp), [(1, "Ectomatter’s macros".to_string()), (0, "Another".to_string())]);
        choose(&mut mp, &mut page, "Another", 0);
        assert_eq!(mp.ed.drafts[&81].steps.last(), Some(&json!({ "job": "macro:83:once", "after": 0 })));
        // a job's step has its own menu: change it, or take it out
        mp.ui.invoke_chain_click(hit::STEP, 0, 0.0, 0.0, 29.0);
        mp.act(&mut page, &|_| {});
        assert_eq!(menu(&mp), [(1, "EVGA Z10 RGB: FPS Gaming".into()), (0, "Change action ›".into()), (2, "".into()), (4, "Remove".into())]);
        choose(&mut mp, &mut page, "Change action ›", 0);
        choose(&mut mp, &mut page, "Studio", 0);
        assert_eq!(mp.ed.drafts[&81].steps[0], json!({ "job": "profile:z10:1", "after": 0 }));
        // the keyboard's macros, which it plays itself, offer neither
        mp.open(&page, Lib::Keyboard, None);
        assert!(!mp.ui.get_jobs_shown());
        for n in 81..=83 {
            crate::library::delete(n);
        }
    }

    /// The library shown is chosen from a drop-down: its menu lists every
    /// library, Ectomatter's then the keyboard's, the one shown ticked, how many
    /// macros each keeps beside it; the list holds the shown one's macros
    /// under its note. A library chosen shows with the macro last open there
    /// still open; a macro chosen again is put away; New makes one in the
    /// library shown; while one is recorded or saved, the others can't be
    /// chosen; with no keyboard read, Ectomatter's is alone, unless the
    /// keyboard's shows, being read.
    #[test]
    fn each_library_is_chosen_from_a_menu_and_its_macros_open_in_it() {
        let _one = crate::library::TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let _ = slint::platform::set_platform(Box::new(Nowhere));
        let step = |k: &str, down: bool| json!({ "key": k, "down": down, "after": 0 });
        crate::library::set(77, "Ectomatter's own", vec![step("A", true), step("A", false)]);
        let mut page = Page::new();
        page.dev = Some(json!({ "macros": { "2": { "name": "The keyboard's", "steps": [step("B", true), step("B", false)] } } }));
        let mut mp = MacroPage::new(MacroWindow::new().unwrap(), slint::Image::default());
        mp.open(&page, Lib::Keyboard, Some(2));
        let rows = |mp: &MacroPage| mp.items.iter().map(|m| (m.kind, m.lib, m.n, m.current)).collect::<Vec<_>>();
        // the libraries' menu, opened, and what it lists: each name, ticked, and its count
        let menu = |mp: &mut MacroPage, page: &mut Page| {
            mp.ui.invoke_libs(10.0, 47.0, 32.0, 0.0);
            mp.act(page, &|_| {});
            mp.ui.get_menu_rows().iter().map(|r| (r.kind, r.label.to_string(), r.tick, r.sub.to_string())).collect::<Vec<_>>()
        };
        let choose = |mp: &mut MacroPage, page: &mut Page, i: i32| {
            mp.ui.invoke_menu_chose(i);
            mp.act(page, &|_| {});
        };
        assert_eq!(mp.ui.get_lib_name(), "EVGA Z10 RGB");
        // the keyboard's alone: its note, under no name (the drop-down names it), then its macros
        assert_eq!(rows(&mp), [(1, 0, 0, false), (0, 0, 2, true)]);
        let head = mp.items.row_data(0).unwrap();
        assert!(head.name.is_empty() && head.sub.starts_with("Kept on the keyboard"), "{}", head.sub);
        let ys: Vec<f32> = mp.items.iter().map(|m| m.y).collect();
        assert!(ys.windows(2).all(|w| w[1] > w[0]), "{ys:?}");
        assert_eq!(mp.ui.get_items_h(), mp.items.iter().last().map_or(0.0, |m| m.y + m.h));
        assert_eq!(menu(&mut mp, &mut page), [(0, "Ectomatter".into(), false, "1 saved".into()), (0, "EVGA Z10 RGB".into(), true, "1 of 100 used".into())]);

        // Ectomatter's: its macros alone, and one chosen opens there
        choose(&mut mp, &mut page, 0);
        assert_eq!((mp.ed.lib, mp.ui.get_lib_name().as_str(), mp.ui.get_menu_open()), (Lib::Ectomatter, "Ectomatter", false));
        assert!(rows(&mp).contains(&(0, 1, 77, false)) && rows(&mp).iter().all(|r| r.1 == 1), "{:?}", rows(&mp));
        mp.ui.invoke_choose(1, 77);
        mp.act(&mut page, &|_| {});
        assert_eq!((mp.ed.lib, mp.ed.current), (Lib::Ectomatter, Some(77)));
        // back to the keyboard's: its macro still open there
        menu(&mut mp, &mut page);
        choose(&mut mp, &mut page, 1);
        assert_eq!((mp.ed.lib, mp.ed.current), (Lib::Keyboard, Some(2)));
        // chosen again, it's put away, as before
        mp.ui.invoke_choose(0, 2);
        mp.act(&mut page, &|_| {});
        assert_ne!(mp.ed.current, Some(2));

        // New in Ectomatter's makes one there
        menu(&mut mp, &mut page);
        choose(&mut mp, &mut page, 0);
        mp.ui.invoke_new_in(1);
        mp.act(&mut page, &|_| {});
        assert_eq!(mp.ed.lib, Lib::Ectomatter);
        let n = mp.ed.current.unwrap();
        assert!(mp.items.iter().any(|m| m.kind == 0 && m.lib == 1 && m.n == n as i32 && m.current && m.sub == "Not saved yet"));

        // while one is being saved, the others can't be chosen
        mp.ed.busy = true;
        mp.refresh(&page);
        assert!(mp.items.iter().filter(|m| m.kind == 1).all(|m| !m.can_new));
        let busy = menu(&mut mp, &mut page);
        assert_eq!((busy[0].0, busy[1].0), (0, 5), "{busy:?}");
        choose(&mut mp, &mut page, 1);
        assert_eq!(mp.ed.lib, Lib::Ectomatter);
        mp.ed.busy = false;

        // no keyboard read: Ectomatter's alone, unless the keyboard's shows, being read
        page.dev = None;
        mp.refresh(&page);
        assert_eq!(menu(&mut mp, &mut page).len(), 1);
        mp.ui.invoke_menu_dismiss();
        mp.act(&mut page, &|_| {});
        mp.open(&page, Lib::Keyboard, None);
        let reading = menu(&mut mp, &mut page);
        assert_eq!(reading[1], (0, "EVGA Z10 RGB".into(), true, "Reading…".into()));
        // and Ectomatter's can be chosen while it is read
        choose(&mut mp, &mut page, 0);
        assert_eq!(mp.ed.lib, Lib::Ectomatter);
        mp.open(&page, Lib::Keyboard, None);
        let head = mp.items.row_data(0).unwrap();
        assert_eq!((mp.items.row_count(), head.kind, head.lib, head.sub.as_str(), head.can_new), (1, 1, 0, "Reading the keyboard…", false));
    }
}
