//! The Macro Editor's window, as the web app's second window had it: what
//! its page asks for, and its models, from the editor's state (macros.rs) and
//! the Z10's page, which the two windows share.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::device;
use crate::front::WindowAsk;
use crate::macros::{self, hit, piece, Draft, Editor, Plays, Recording, Timing, MEDIA, PLAYS, ROOM};
use crate::z10::{hid_name, Page};
use crate::{Cell, MacroItem, MacroWindow, MenuRow};

#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Move,
    Minimise,
    Maximise,
    Close,
    Choose(i32),
    RowMenu(i32, f32, f32),
    New,
    Rename,
    NameDone(String),
    NameCancel,
    More(f32, f32, f32),
    ChainClick(i32, i32, f32, f32, f32),
    ChainDrop(i32, f32, f32),
    ChainDrag(i32, f32, f32, f32, f32),
    WaitDone(i32, String),
    WaitCancel,
    Tool(i32, f32, f32, f32),
    Plays(f32, f32, f32),
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
    Plays(u32),
    Rename,
    Duplicate,
    Delete,
}

struct Menu {
    kind: Kind,
    anchor: (f32, f32, f32),
    width: f32,
    /// its right edge on the button's, as the More button's menu hangs
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
    pub ed: Editor,
    asks: Rc<RefCell<Vec<Ask>>>,
    menu: Option<Menu>,
    shut: Option<((f32, f32), Instant, bool)>,
    rec: ectotype_core::Recorder,
    rec_timer: slint::Timer,
    confirm: bool,
    status: (String, i32),
    saving: Option<Saving>,
    chain: Rc<VecModel<Cell>>,
    items: Rc<VecModel<MacroItem>>,
    fake_rec: bool,
    /// the chosen macro is to be scrolled into view in the list
    scroll_to: bool,
}

fn row(kind: i32, label: &str, tick: bool) -> MenuRow {
    MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into() }
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
        on!(on_minimise, Ask::Minimise);
        on!(on_maximise, Ask::Maximise);
        on!(on_close_window, Ask::Close);
        on!(on_choose, |n| Ask::Choose(n));
        on!(on_row_menu, |n, x, y| Ask::RowMenu(n, x, y));
        on!(on_new_macro, Ask::New);
        on!(on_rename, Ask::Rename);
        on!(on_name_done, |t| Ask::NameDone(t.to_string()));
        on!(on_name_cancel, Ask::NameCancel);
        on!(on_more, |x, y, h| Ask::More(x, y, h));
        on!(on_chain_click, |id, i, x, y, h| Ask::ChainClick(id, i, x, y, h));
        on!(on_chain_drop, |i, x, y| Ask::ChainDrop(i, x, y));
        on!(on_chain_drag, |i, x, y, ox, oy| Ask::ChainDrag(i, x, y, ox, oy));
        on!(on_wait_done, |i, t| Ask::WaitDone(i, t.to_string()));
        on!(on_wait_cancel, Ask::WaitCancel);
        on!(on_tool, |k, x, y, h| Ask::Tool(k, x, y, h));
        on!(on_plays, |x, y, h| Ask::Plays(x, y, h));
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
            ed: Editor::new(),
            asks,
            menu: None,
            shut: None,
            rec: ectotype_core::Recorder::new(),
            rec_timer: slint::Timer::default(),
            confirm: false,
            status: (String::new(), 0),
            saving: None,
            chain,
            items,
            fake_rec: false,
            scroll_to: false,
        }
    }

    fn status(&mut self, t: impl Into<String>, kind: i32) {
        self.status = (t.into(), kind);
    }

    /// Opened on a macro, or on whatever it last showed.
    pub fn open(&mut self, page: &Page, n: Option<u32>) {
        if let Some(n) = n.filter(|n| Editor::saved(page, *n).is_some()) {
            self.ed.current = Some(n);
            self.ed.sel_step = None;
            self.ed.ins = None;
            self.scroll_to = true;
        }
        self.refresh(page);
    }

    /// Do what the page asked. Hands back what only the window can do, and
    /// whether the Z10's page changed (a macro saved, a key's plays).
    pub fn act(&mut self, page: &mut Page, kb: &dyn Fn(device::Ask)) -> (Vec<WindowAsk>, bool) {
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        let mut window = Vec::new();
        if asks.is_empty() || page.dev.is_none() {
            return (window, false);
        }
        let mut changed = false;
        for ask in asks {
            if self.ed.busy && !matches!(ask, Ask::Move | Ask::Minimise | Ask::Maximise | Ask::Close | Ask::RecTick) {
                continue;
            }
            match ask {
                Ask::Move => window.push(WindowAsk::Move),
                Ask::Minimise => window.push(WindowAsk::Minimise),
                Ask::Maximise => window.push(WindowAsk::Maximise),
                Ask::Close => {
                    if self.ed.rec.is_some() {
                        self.stop_recording(page, false, None);
                    }
                    self.close_menu(false);
                    window.push(WindowAsk::Close);
                }
                Ask::Choose(n) => {
                    let n = n as u32;
                    // choosing the open macro again puts it away, back to the blank one
                    self.ed.current = if self.ed.current == Some(n) { None } else { Some(n) };
                    self.ed.sel_step = None;
                    self.ed.ins = None;
                    self.ed.edit_name = false;
                }
                Ask::RowMenu(n, x, y) => {
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
                    self.open_menu(page, Kind::Macro, (x, y, 0.0), 220.0, 0.0, false);
                }
                Ask::New => {
                    if self.ed.rec.is_none() {
                        self.ed.new_macro(page);
                        self.scroll_to = true;
                    }
                }
                Ask::Rename => self.start_rename(page),
                Ask::NameDone(t) => self.ed.commit_name(page, &t),
                Ask::NameCancel => self.ed.edit_name = false,
                Ask::More(x, y, h) => self.open_menu(page, Kind::Macro, (x, y, h), 240.0, 26.0, false),
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
                            self.open_menu(page, Kind::Step(i), (x, y, h), 220.0, 0.0, true);
                        }
                        hit::JOINT => {
                            self.ed.ins = Some(i);
                            self.ed.sel_step = None;
                            self.open_menu(page, Kind::Insert, (x, y, h), 220.0, 0.0, true);
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
                Ask::Tool(k, x, y, h) => match k {
                    0 => self.open_menu(page, Kind::Timing, (x, y, h), 290.0, 0.0, false),
                    1 => {
                        if let Some(d) = self.current_draft(page) {
                            d.steps.clear();
                        }
                        self.ed.ins = None;
                        self.ed.sel_step = None;
                    }
                    2 | 3 => self.open_menu(page, Kind::Keys { media: k == 3, change: None }, (x, y, h), 260.0, 0.0, false),
                    4 => self.ed.insert_wait(page),
                    5 => {
                        if self.ed.rec.is_some() {
                            self.stop_recording(page, true, None);
                        } else {
                            self.start_recording(page);
                        }
                    }
                    _ => {}
                },
                Ask::Plays(x, y, h) => self.open_menu(page, Kind::Plays, (x, y, h), 260.0, 0.0, false),
                Ask::CountDone(t) => {
                    let c: u32 = t.trim().parse().unwrap_or(2);
                    if let Some(d) = self.current_draft(page) {
                        d.plays = Some((11, c.clamp(2, 255)));
                    }
                }
                Ask::Revert => self.ed.revert(page),
                Ask::Save => self.save(page, kb, &mut changed),
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
                    self.delete(page, kb);
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
        let (cx, cy) = (290.0 + 18.0, 36.0 + 56.0 + self.ui.get_chain_scroll());
        self.chain
            .iter()
            .find(|c| c.kind == piece::STEP && c.id == hit::STEP && x >= cx + c.x - 6.0 && x < cx + c.x + c.w && y >= cy + c.y && y < cy + c.y + c.h)
            .map(|c| c.index)
    }

    fn locked(&self, page: &Page) -> bool {
        self.ed.current.and_then(|n| Editor::saved(page, n)).is_some_and(|m| m["editable"] == false)
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

    fn open_menu(&mut self, page: &Page, kind: Kind, anchor: (f32, f32, f32), width: f32, right: f32, let_go: bool) {
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
        self.menu = Some(Menu { kind, anchor, width, right, let_go, query: String::new(), picks: Vec::new() });
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
        let (ax, ay, ah) = m.anchor;
        let (h, win) = (self.ui.get_menu_natural(), self.ui.get_window_height());
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        let room = if down { below } else { above };
        self.ui.set_menu_w(m.width);
        self.ui.set_menu_ax(if m.right > 0.0 { ax + m.right - m.width } else { ax });
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
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

    fn build_menu(&mut self, page: &Page) {
        let Some(m) = &self.menu else { return };
        let (kind, q) = (m.kind, m.query.trim().to_lowercase());
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut picks: Vec<Pick> = Vec::new();
        let mut search = false;
        let mut placeholder = String::new();
        let n = self.ed.current.unwrap_or(0);
        let d = self.ed.drafts.get(&n).cloned().or_else(|| {
            let m = Editor::saved(page, n)?;
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
            Kind::Step(i) => {
                let Some(st) = d.steps.get(i) else { return };
                let down = st["down"].as_bool().unwrap_or(false);
                let media = st.get("media").is_some();
                rows.push(row(1, &macros::step_name(st), false));
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
                let current = change.and_then(|i| d.steps.get(i)).map(|st| if media { json!({ "media": st["media"] }) } else { json!({ "code": st["code"] }) });
                let groups: Vec<(&str, Vec<(String, Value)>)> = if media {
                    vec![("Media keys", MEDIA.iter().map(|(c, n)| (n.to_string(), json!({ "media": c }))).collect())]
                } else {
                    vec![
                        ("Modifiers", (112..=119).map(|c| (macros::key_name(c), json!({ "code": c }))).collect()),
                        ("Keys", (1..104).filter_map(|c| hid_name(c).map(|n| (n, json!({ "code": c })))).collect()),
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
                let cur = d.plays.map(|(m, c)| Plays::One(m, c)).or_else(|| Editor::plays_of(page, n));
                rows.push(row(1, "Each time its key is pressed", false));
                picks.push(Pick::Nothing);
                for (v, label) in PLAYS {
                    rows.push(row(0, label, matches!(cur, Some(Plays::One(m, _)) if m == v)));
                    picks.push(Pick::Plays(v));
                }
            }
            Kind::Macro => {
                let saved = Editor::saved(page, n);
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
        let list: f32 = rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum();
        self.ui.set_menu_search(search);
        self.ui.set_menu_back(false);
        self.ui.set_menu_placeholder(placeholder.into());
        self.ui.set_menu_natural(6.0 + if search { 39.0 } else { 0.0 } + list + 6.0);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        if let Some(m) = &mut self.menu {
            m.picks = picks;
        }
    }

    fn menu_chose(&mut self, page: &mut Page, i: usize) {
        let Some(m) = &self.menu else { return };
        let Some(p) = m.picks.get(i).cloned() else { return };
        let (anchor, let_go) = (m.anchor, m.let_go);
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
                        o.remove("code");
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
                self.menu = Some(Menu { kind: Kind::Keys { media, change }, anchor, width: 260.0, right: 0.0, let_go, query: String::new(), picks: Vec::new() });
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
                let cur = self.ed.drafts.get(&n).and_then(|d| d.plays).map(|(m, c)| Plays::One(m, c)).or_else(|| Editor::plays_of(page, n));
                let count = match cur {
                    Some(Plays::One(_, c)) if c > 1 => c,
                    _ => 3,
                };
                if let Some(d) = self.current_draft(page) {
                    d.plays = Some((v, count));
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
                if Editor::saved(page, n).is_none() {
                    self.ed.drafts.remove(&n);
                    self.ed.current = None;
                    return;
                }
                let name = self.current_draft(page).map(|d| d.name.clone()).unwrap_or_default();
                let places = Editor::places(page, n);
                let body = if places.is_empty() {
                    "No key runs this macro.".to_string()
                } else {
                    let list: Vec<String> = places
                        .iter()
                        .map(|(layer, pos, ps)| {
                            let key = page.key(*pos).map(|k| page.key_label(k)).unwrap_or_else(|| format!("Key {pos}"));
                            format!("{}{key} ({})", if layer == "fnkeys" { "Fn + " } else { "" }, macros::profiles_text(ps, |n| page.profile_name(n)))
                        })
                        .collect();
                    format!("{} will do nothing until you give {} something else.", list.join(", "), if places.len() == 1 { "it" } else { "them" })
                };
                self.ui.set_confirm_title(format!("Delete {name}?").into());
                self.ui.set_confirm_body(body.into());
                self.ui.set_confirm_body2("This removes it from the keyboard and cannot be undone.".into());
                self.confirm = true;
            }
        }
    }

    // ------------------------------------------------------------ recording

    fn start_recording(&mut self, page: &Page) {
        let Some(n) = self.ed.current else { return };
        let Some(d) = self.ed.draft(page, n) else { return };
        if d.timing != Timing::Keep {
            d.steps = Editor::effective(d);
            d.timing = Timing::Keep;
        }
        let room = ROOM.saturating_sub(macros::bytes(&d.steps));
        self.close_menu(false);
        self.ed.sel_step = None;
        self.ed.edit_wait = None;
        // ECTODECK_NATIVE_FAKE_REC: steps as JSON, to check how a recording
        // looks offscreen without taking the keyboard
        if let Some(steps) = std::env::var("ECTODECK_NATIVE_FAKE_REC").ok().and_then(|v| serde_json::from_str::<Vec<Value>>(&v).ok()) {
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
            rec.steps = r["steps"].as_array().cloned().unwrap_or_default();
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
        let got = r["steps"].as_array().cloned().unwrap_or_default();
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

    fn save(&mut self, page: &mut Page, kb: &dyn Fn(device::Ask), changed: &mut bool) {
        let Some(n) = self.ed.current else { return };
        let Some(d) = self.ed.draft(page, n).cloned() else { return };
        let (steps, added) = Editor::to_save(&d);
        if macros::bytes(&steps) > ROOM {
            self.status("This macro is too long for the keyboard. Remove some steps and save again.", 2);
            return;
        }
        let m = Editor::saved(page, n).unwrap_or(Value::Null);
        let name = if d.name.is_empty() { format!("Macro {n}") } else { d.name.clone() };
        self.ed.busy = true;
        self.status(format!("Saving {}…", d.name), 0);
        kb(device::Ask::WriteMacro(
            n as u8,
            json!({ "name": name, "steps": steps, "run": m["method"].as_u64().unwrap_or(9), "repeat": m["repeat"].as_u64().unwrap_or(0), "unit": m["unit"].as_u64().unwrap_or(1) }),
        ));
        // how it plays lives on its keys: rewrite those, profile by profile, on
        // top of what each profile has saved, and carry the change into its draft
        let mut profiles = 0;
        if let Some((mode, count)) = d.plays {
            let mut by: BTreeMap<u8, Vec<(String, u32)>> = BTreeMap::new();
            for (layer, pos, ps) in Editor::places(page, n) {
                for p in ps {
                    by.entry(p).or_default().push((layer.clone(), pos));
                }
            }
            let v = json!([3, n, mode, if mode == 11 { count } else { 0 }]);
            for (p, ks) in by {
                let saved = &page.dev.as_ref().unwrap()["profiles"][p.to_string()];
                let mut body = json!({ "keys": saved["keys"].clone(), "fnkeys": saved["fnkeys"].clone() });
                for (layer, pos) in ks {
                    body[&layer][pos as usize] = v.clone();
                    if let Some(dr) = page.drafts.get_mut(&p) {
                        dr[&layer][pos as usize] = v.clone();
                    }
                }
                kb(device::Ask::SaveKeys(p, body));
                profiles += 1;
            }
            *changed = true;
        }
        self.saving = Some(Saving { n, name: d.name.clone(), added, profiles, wrote: false });
    }

    fn delete(&mut self, page: &Page, kb: &dyn Fn(device::Ask)) {
        let Some(n) = self.ed.current else { return };
        let name = self.ed.drafts.get(&n).map(|d| d.name.clone()).or_else(|| Editor::saved(page, n).and_then(|m| m["name"].as_str().map(String::from))).unwrap_or(format!("Macro {n}"));
        self.ed.busy = true;
        self.status(format!("Deleting {name}…"), 0);
        self.saving = Some(Saving { n, name, added: Vec::new(), profiles: 0, wrote: false });
        kb(device::Ask::WriteMacro(n as u8, json!({ "delete": true })));
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

    pub fn refresh(&mut self, page: &Page) {
        let ui = &self.ui;
        if page.dev.is_none() {
            ui.set_status_text("Reading the keyboard…".into());
            return;
        }
        self.ed.settle(page);
        let rec = self.ed.rec.is_some();
        let list = self.ed.list(page);
        let saved_count = Editor::macros_count(page);
        let items: Vec<MacroItem> = list
            .iter()
            .map(|&n| {
                let d = self.ed.drafts.get(&n);
                let m = Editor::saved(page, n);
                let steps = d.map(Editor::effective).unwrap_or_else(|| m.as_ref().and_then(|m| m["steps"].as_array().cloned()).unwrap_or_default());
                let name = d.map(|d| d.name.clone()).or_else(|| m.as_ref().and_then(|m| m["name"].as_str().map(String::from))).filter(|s| !s.is_empty()).unwrap_or(format!("Macro {n}"));
                MacroItem {
                    n: n as i32,
                    name: name.into(),
                    sub: if d.is_some_and(|d| d.is_new) { "Not saved yet".into() } else { macros::describe(&steps).into() },
                    timed: steps.iter().any(|s| s["delay"].as_u64().unwrap_or(0) > 0),
                    current: self.ed.current == Some(n),
                    dirty: self.ed.dirty(page, n),
                }
            })
            .collect();
        // the chosen one brought into view, as the web page scrolled it to the nearest edge
        if std::mem::take(&mut self.scroll_to) {
            if let Some(i) = items.iter().position(|m| m.current) {
                let view = self.ui.get_window_height() - 36.0 - 56.0 - 10.0;
                let (top, bottom) = (i as f32 * 55.0, i as f32 * 55.0 + 53.0);
                let at = -self.ui.get_lib_scroll();
                if top < at {
                    self.ui.set_lib_scroll(-top);
                } else if bottom > at + view {
                    self.ui.set_lib_scroll(-(bottom - view));
                }
            }
        }
        crate::front::set_rows(&self.items, items);
        ui.set_slots_text(format!("{saved_count} of 100 slots").into());
        ui.set_can_new(!self.ed.free_slots(page).is_empty() && !rec);

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
        } else {
            ("Saved on the keyboard", 0)
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
        let hint = if rec { "Type the keys on your Z10. Press Stop to finish." } else { "Press Record and type the keys on your Z10, or add keys, media keys and waits with the buttons below." };
        let lines: Vec<SharedString> = crate::text::wrap(400, 13.0, hint, 440.0).into_iter().map(Into::into).collect();
        ui.set_empty_lines(ModelRc::new(VecModel::from(lines)));

        let used = macros::bytes(&eff) + ed.rec.as_ref().map_or(0, |r| r.used);
        let left = ROOM as i64 - used as i64;
        let count = eff.len() + eff.iter().filter(|s| s["delay"].as_u64().unwrap_or(0) > 0).count();
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
        ui.set_can_record(!locked && left >= 4);
        if let Some(r) = &ed.rec {
            ui.set_rec_clock(macros::clock(r.elapsed).into());
            ui.set_blink((r.elapsed * 2.0).floor() as i64 % 2 == 0);
        }

        let places = Editor::places(page, n);
        let plays = d.plays.map(|(m, c)| Plays::One(m, c)).or_else(|| Editor::plays_of(page, n));
        let label = match (places.is_empty(), plays) {
            (true, _) => "Play once".to_string(),
            (_, Some(Plays::Varies)) => "Varies by key".into(),
            (_, Some(Plays::One(m, c))) => format!("{}{}", PLAYS.iter().find(|(v, _)| *v == m).map_or("Play once", |p| p.1), if m == 11 { format!(" ({c} times)") } else { String::new() }),
            (_, None) => "Play once".into(),
        };
        ui.set_plays_text(label.into());
        ui.set_plays_on(!places.is_empty() && !off);
        match plays {
            Some(Plays::One(11, c)) if !places.is_empty() => {
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
        }
    }
}

