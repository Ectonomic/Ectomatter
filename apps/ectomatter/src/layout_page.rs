//! The screen layout editor's window: the layouts down its left; the one
//! open, its name (changed in place) and its menu (Duplicate, Delete, which
//! asks first in the header's place); the bar of blocks over the screen, with
//! the layout's backlight and the grid at its end; the screen, its blocks
//! outlined, a block carried from the bar shown where it would land; and the
//! box of the block selected under it. Front keeps the layouts and plays the
//! screen; this page turns the window's asks into changes to the layouts, and
//! shows them.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::front::{color, hsv2rgb, hue_bar, keep_recent, new_rows, recent_colors, rgb2hsv, sv_square, NewFrom, WindowAsk, PRESETS};
use crate::lcd_page::{DateFormat, Page, Readings, Screens, Source, COLS};
use crate::lighting::{ctl, Ctl};
use crate::screen_edit::{self, Editor, Shows};
use crate::{Cell, EdBlock, LayoutRow, LayoutWindow, MenuRow};

pub enum Ask {
    Move,
    Minimize,
    Maximize,
    Close,
    Choose(i32),
    /// + New (or the empty window's New layout): where the button is, and its width
    New(f32, f32, f32, f32),
    Rename,
    NameDone(String),
    NameCancel,
    More(f32, f32, f32),
    ConfirmYes,
    ConfirmNo,
    Chosen(i32),
    Moved(i32, i32, i32),
    /// a block's corner let go: the block, its width and its rows
    Sized(i32, i32, i32),
    Remove,
    ChipAdd(i32),
    ChipCarry(i32, f32, f32),
    ChipDrop(i32, f32, f32),
    ChipCancel,
    LightOwn(bool),
    LightColor(f32, f32, f32),
    Grid,
    CellPick(i32, i32, f32, f32, f32),
    /// a slider in the box moved: an effect's speed
    CellSlide(i32, f32),
    /// an effect's tile chosen
    FxChose(i32),
    CellTyped(i32, String),
    MenuChose(i32),
    MenuDismiss,
    PkSv(f32, f32),
    PkHue(f32),
    PkField(i32, String),
    PkWell(slint::Color),
    PkEnter,
    PkDismiss,
}

/// The menu open: the layout's (Duplicate, Delete), what a block shows, how a date block
/// writes the date, or + New's (From scratch, a copy of a layout, or a ready-made one put
/// back: what each row makes).
enum Menu {
    More,
    Shows(Vec<Option<Shows>>),
    Formats(Vec<Option<DateFormat>>),
    New(Vec<Option<NewFrom>>),
}

/// The color picker, open on the layout's own color: the color it opened
/// on, and the color now as hue, saturation and brightness.
struct Pick {
    start: [u8; 3],
    hsv: [f32; 3],
    rgb: [u8; 3],
}

/// What Front keeps that the window works on.
pub struct Cx<'a> {
    pub screens: &'a mut Screens,
    pub editor: &'a mut Option<Editor>,
    pub readings: &'a Readings,
    /// what this computer can read
    pub avail: &'a [Source],
    /// the profile's default backlight: brightness, red, green, blue
    pub light: [u8; 4],
    /// seconds since the layout open came up, which its scrolling words are timed from
    pub t: f64,
    /// for what deleting a layout takes with it: whether the keyboard keeps a layout by
    /// itself (its onboard layouts), and the profiles' names
    pub onboard: bool,
    pub profiles: [String; 5],
    /// where the chosen block's reading comes from now, for its From line
    pub from: screen_edit::From,
    /// the computer's readings the Shows menu lists, by device
    pub groups: Vec<(String, Vec<(String, Source)>)>,
}

/// What came of the window's asks.
#[derive(Default)]
pub struct Outcome {
    pub window: Vec<WindowAsk>,
    /// the layouts changed: saved, and the player told
    pub edited: bool,
    /// another layout opened: the keyboard's screen held on it
    pub opened: bool,
    /// the window closed: the screen let go
    pub closed: bool,
}

pub struct LayoutPage {
    pub ui: LayoutWindow,
    asks: Rc<RefCell<Vec<Ask>>>,
    menu: Option<Menu>,
    renaming: bool,
    confirm: bool,
    /// a block carried from the bar: its kind, and where it would land (its row, first
    /// column, width and height; none away from the screen and over other blocks)
    carry: Option<(usize, Option<(u8, u8, u8, u8)>)>,
    /// the effects' tiles in the Effect box, and the still pictures of the ones not chosen
    /// as last drawn: for the light, the size and Bounce's word they were drawn for
    tiles: Rc<VecModel<crate::FxTile>>,
    stills: Option<(([u8; 3], u8, u32, String), Vec<slint::Image>)>,
    pick: Option<Pick>,
    /// a menu or the picker a press just closed, so its button's click doesn't open it again
    shut: Option<((f32, f32), Instant)>,
    rows: Rc<VecModel<LayoutRow>>,
    blocks: Rc<VecModel<EdBlock>>,
    cells: Rc<VecModel<Cell>>,
    drawn: Option<(Vec<String>, [u8; 3], u8, u32)>,
    /// the window changed under a resting pointer: what is under it now (its hover
    /// and its cursor) is asked for again, as if the pointer had moved
    rehover: bool,
}

fn row(kind: i32, label: &str, tick: bool, sub: &str) -> MenuRow {
    MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() }
}

impl LayoutPage {
    pub fn new(ui: LayoutWindow, mark: slint::Image) -> LayoutPage {
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
        on!(on_choose, |i| Ask::Choose(i));
        on!(on_new_layout, |x, y, w, h| Ask::New(x, y, w, h));
        on!(on_rename, Ask::Rename);
        on!(on_name_done, |t| Ask::NameDone(t.to_string()));
        on!(on_name_cancel, Ask::NameCancel);
        on!(on_more, |x, y, h| Ask::More(x, y, h));
        on!(on_confirm_yes, Ask::ConfirmYes);
        on!(on_confirm_no, Ask::ConfirmNo);
        on!(on_block_chosen, |i| Ask::Chosen(i));
        on!(on_block_moved, |i, r, c| Ask::Moved(i, r, c));
        on!(on_block_sized, |i, w, h| Ask::Sized(i, w, h));
        on!(on_block_remove, Ask::Remove);
        on!(on_chip_add, |k| Ask::ChipAdd(k));
        on!(on_chip_carry, |k, x, y| Ask::ChipCarry(k, x, y));
        on!(on_chip_drop, |k, x, y| Ask::ChipDrop(k, x, y));
        on!(on_chip_cancel, Ask::ChipCancel);
        on!(on_light_own, |on| Ask::LightOwn(on));
        on!(on_light_color, |x, y, h| Ask::LightColor(x, y, h));
        on!(on_grid_toggle, Ask::Grid);
        on!(on_cell_slide, |id, v| Ask::CellSlide(id, v));
        on!(on_fx_chose, |k| Ask::FxChose(k));
        on!(on_cell_pick, |id, i, x, y, h| Ask::CellPick(id, i, x, y, h));
        on!(on_cell_typed, |id, _index, t| Ask::CellTyped(id, t.to_string()));
        on!(on_menu_chose, |i| Ask::MenuChose(i));
        on!(on_menu_dismiss, Ask::MenuDismiss);
        on!(on_pk_sv, |x, y| Ask::PkSv(x, y));
        on!(on_pk_hue_at, |x| Ask::PkHue(x));
        on!(on_pk_field, |i, t| Ask::PkField(i, t.to_string()));
        on!(on_pk_well, |c| Ask::PkWell(c));
        on!(on_pk_enter, Ask::PkEnter);
        on!(on_pk_dismiss, Ask::PkDismiss);
        ui.set_mark(mark);
        ui.set_pk_presets(ModelRc::new(VecModel::from(PRESETS.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        let (rows, blocks, cells) = (Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()));
        ui.set_rows(ModelRc::from(rows.clone()));
        ui.set_blocks(ModelRc::from(blocks.clone()));
        ui.set_box_cells(ModelRc::from(cells.clone()));
        let tiles = Rc::new(VecModel::default());
        ui.set_fx_tiles(ModelRc::from(tiles.clone()));
        LayoutPage { ui, asks, menu: None, renaming: false, confirm: false, carry: None, tiles, stills: None, pick: None, shut: None, rows, blocks, cells, drawn: None, rehover: false }
    }

    /// Whether the window has changed since the pointer last moved over it (see `rehover`).
    pub fn take_rehover(&mut self) -> bool {
        std::mem::take(&mut self.rehover)
    }

    /// The Delete key, while no words are being typed: the block selected goes.
    pub fn delete_key(&self) {
        self.asks.borrow_mut().push(Ask::Remove);
    }

    /// Open on layout `i` of the Library.
    pub fn open(cx: &mut Cx, i: usize) {
        let grid = cx.editor.as_ref().map(|e| e.grid).unwrap_or_else(|| crate::uistate::get("screen_grid").and_then(|v| v.as_bool()).unwrap_or(true));
        *cx.editor = (i < cx.screens.library.len()).then_some(Editor { page: i, chosen: None, grid });
    }

    fn page<'a>(cx: &'a Cx) -> Option<&'a Page> {
        cx.editor.as_ref().and_then(|e| cx.screens.library.get(e.page))
    }

    /// The window's asks, carried out.
    pub fn act(&mut self, cx: &mut Cx) -> Outcome {
        let mut out = Outcome::default();
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        self.rehover |= !asks.is_empty();
        for a in asks {
            match a {
                Ask::Move => out.window.push(WindowAsk::Move),
                Ask::Minimize => out.window.push(WindowAsk::Minimize),
                Ask::Maximize => out.window.push(WindowAsk::Maximize),
                Ask::Close => {
                    self.close_menu();
                    self.close_picker();
                    self.renaming = false;
                    self.confirm = false;
                    out.closed = true;
                    out.window.push(WindowAsk::Close);
                }
                Ask::Choose(i) => {
                    self.settle();
                    LayoutPage::open(cx, i.max(0) as usize);
                    out.opened = true;
                }
                // (with no layout to copy, a blank one at once)
                Ask::New(x, y, w, h) if !cx.screens.library.is_empty() => self.open_new((x, y, h), w, cx),
                Ask::New(..) => {
                    self.settle();
                    let i = cx.screens.new_page();
                    LayoutPage::open(cx, i);
                    out.edited = true;
                    out.opened = true;
                }
                Ask::Rename => {
                    if let Some(p) = LayoutPage::page(cx) {
                        self.close_menu();
                        self.ui.set_name_text(p.name.as_str().into());
                        self.renaming = true;
                    }
                }
                Ask::NameDone(t) => {
                    self.ui.global::<crate::Typing>().set_on(false);
                    if std::mem::take(&mut self.renaming) {
                        if let Some(ed) = cx.editor.as_ref() {
                            cx.screens.rename_page(ed.page, &t);
                            out.edited = true;
                        }
                    }
                }
                Ask::NameCancel => {
                    self.renaming = false;
                    // (the name's field goes with its focus, which then never says it went)
                    self.ui.global::<crate::Typing>().set_on(false);
                }
                Ask::More(x, y, h) => self.open_more((x, y, h), cx),
                Ask::ConfirmNo => self.confirm = false,
                Ask::ConfirmYes => {
                    self.confirm = false;
                    if let Some(i) = cx.editor.as_ref().map(|e| e.page) {
                        if cx.screens.delete_page(i) {
                            out.edited = true;
                            // the layout after it opens (the one before it, when it was the
                            // last), or, with none left, the window shows none
                            let n = cx.screens.library.len();
                            if n == 0 {
                                *cx.editor = None;
                            } else {
                                LayoutPage::open(cx, i.min(n - 1));
                            }
                            out.opened = true;
                        }
                    }
                }
                Ask::Chosen(i) => {
                    self.close_menu();
                    self.close_picker();
                    if let Some(ed) = cx.editor.as_mut() {
                        ed.chosen = (i >= 0).then_some(i as usize);
                    }
                }
                Ask::Moved(i, r, c) => {
                    if let Some(ed) = cx.editor.as_mut() {
                        if let Some(p) = cx.screens.library.get_mut(ed.page) {
                            let i = i.max(0) as usize;
                            let least = p.pieces.get(i).map_or(1, |q| crate::lcd_page::least(q, cx.readings));
                            screen_edit::move_to(p, i, r, c, least);
                            ed.chosen = Some(i);
                            out.edited = true;
                        }
                    }
                    // the drag let go, even where the layout keeps the block as it was
                    self.ui.set_settled(self.ui.get_settled().wrapping_add(1));
                }
                Ask::Sized(i, w, h) => {
                    if let Some(ed) = cx.editor.as_ref() {
                        if let Some(p) = cx.screens.library.get_mut(ed.page) {
                            let i = i.max(0) as usize;
                            // (how narrow it can be at each height it can have)
                            let q = p.pieces.get(i).cloned();
                            screen_edit::size_to(p, i, w, h, |rows| q.as_ref().map_or(1, |q| crate::lcd_page::least_at(q, cx.readings, rows)));
                            out.edited = true;
                        }
                    }
                    self.ui.set_settled(self.ui.get_settled().wrapping_add(1));
                }
                Ask::Remove => {
                    if let Some(ed) = cx.editor.as_mut() {
                        if let (Some(_), Some(p)) = (ed.chosen, cx.screens.library.get_mut(ed.page)) {
                            screen_edit::act(p, ed, Ctl::ScrRemove, 0, None);
                            out.edited = true;
                        }
                    }
                }
                Ask::ChipAdd(k) => {
                    let first = cx.avail.first().copied().unwrap_or(Source::CpuTemp);
                    if let Some(ed) = cx.editor.as_mut() {
                        if let Some(p) = cx.screens.library.get_mut(ed.page) {
                            out.edited |= screen_edit::add(p, ed, k.clamp(0, 3) as usize, first, cx.readings, None);
                        }
                    }
                }
                Ask::ChipCarry(k, x, y) => {
                    let kind = k.clamp(0, 3) as usize;
                    let at = self.landing_at(cx, kind, x, y);
                    self.carry = Some((kind, at));
                    self.ui.set_carry_on(true);
                    self.ui.set_carry_kind(k);
                    self.ui.set_carry_name(["Text", "Data", "Bar", "Effect"][kind].into());
                    self.ui.set_carry_x(x);
                    self.ui.set_carry_y(y);
                    self.drawn = None;
                }
                Ask::ChipDrop(k, x, y) => {
                    let kind = k.clamp(0, 3) as usize;
                    let at = self.landing_at(cx, kind, x, y);
                    self.carry = None;
                    self.ui.set_carry_on(false);
                    self.drawn = None;
                    if let Some(at) = at {
                        let first = cx.avail.first().copied().unwrap_or(Source::CpuTemp);
                        if let Some(ed) = cx.editor.as_mut() {
                            if let Some(p) = cx.screens.library.get_mut(ed.page) {
                                out.edited |= screen_edit::add(p, ed, kind, first, cx.readings, Some(at));
                            }
                        }
                    }
                }
                Ask::ChipCancel => {
                    self.carry = None;
                    self.ui.set_carry_on(false);
                    self.drawn = None;
                }
                Ask::LightOwn(on) => {
                    let default = [cx.light[1], cx.light[2], cx.light[3]];
                    if let Some(p) = cx.editor.as_ref().and_then(|e| cx.screens.library.get_mut(e.page)) {
                        if on {
                            // the color it had before, or to start, the profile's own: nothing changes until another is picked
                            p.light = Some(p.kept_light.unwrap_or(default));
                        } else {
                            p.kept_light = p.light.or(p.kept_light);
                            p.light = None;
                            self.close_picker();
                        }
                        out.edited = true;
                    }
                }
                Ask::LightColor(x, y, h) => self.open_picker(cx, (x, y, h)),
                Ask::Grid => {
                    if let Some(ed) = cx.editor.as_mut() {
                        ed.grid = !ed.grid;
                        crate::uistate::set("screen_grid", serde_json::json!(ed.grid));
                    }
                }
                Ask::CellPick(id, index, x, y, h) => {
                    let c = ctl(id);
                    match c {
                        Ctl::ScrSource => self.open_shows(cx, (x, y, h)),
                        Ctl::OpenSettings => out.window.push(WindowAsk::ShowSettings),
                        Ctl::ScrFormat => self.open_formats(cx, (x, y, h)),
                        Ctl::ScrAlign | Ctl::ScrUnits | Ctl::ScrHours | Ctl::ScrFit => {
                            if let Some(ed) = cx.editor.as_mut() {
                                if let Some(p) = cx.screens.library.get_mut(ed.page) {
                                    screen_edit::act(p, ed, c, index.max(0) as usize, None);
                                    out.edited = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ask::CellSlide(id, v) => {
                    if ctl(id) == Ctl::ScrSpeed {
                        if let Some(ed) = cx.editor.as_ref() {
                            if let Some(p) = cx.screens.library.get_mut(ed.page) {
                                screen_edit::set_speed(p, ed, v);
                                out.edited = true;
                            }
                        }
                    }
                }
                Ask::FxChose(k) => {
                    if let (Some(fx), Some(ed)) = (crate::lcd_fx::FXS.get(k.max(0) as usize).copied(), cx.editor.as_ref()) {
                        if let Some(p) = cx.screens.library.get_mut(ed.page) {
                            screen_edit::set_fx(p, ed, fx);
                            out.edited = true;
                        }
                    }
                }
                Ask::CellTyped(id, t) => {
                    if ctl(id) == Ctl::ScrText {
                        if let Some(ed) = cx.editor.as_mut() {
                            if let Some(p) = cx.screens.library.get_mut(ed.page) {
                                screen_edit::act(p, ed, Ctl::ScrText, 0, Some(&t));
                                out.edited = true;
                            }
                        }
                    }
                }
                Ask::MenuChose(i) => {
                    let i = i.max(0) as usize;
                    match self.menu.take() {
                        Some(Menu::More) => {
                            self.ui.set_menu_open(false);
                            if i == 0 {
                                if let Some(c) = cx.editor.as_ref().and_then(|e| cx.screens.duplicate_page(e.page)) {
                                    LayoutPage::open(cx, c);
                                    out.edited = true;
                                    out.opened = true;
                                }
                            } else if i == 1 {
                                self.confirm = true;
                            }
                        }
                        Some(Menu::New(made)) => {
                            self.ui.set_menu_open(false);
                            let at = match made.get(i).cloned().flatten() {
                                Some(NewFrom::Scratch) => Some(cx.screens.new_page()),
                                Some(NewFrom::Copy(id)) => cx.screens.index_of(&id).and_then(|k| cx.screens.duplicate_page(k)),
                                Some(NewFrom::Ready(key)) => cx.screens.add_built_in(&key),
                                None => None,
                            };
                            if let Some(at) = at {
                                self.settle();
                                LayoutPage::open(cx, at);
                                out.edited = true;
                                out.opened = true;
                            }
                        }
                        Some(Menu::Formats(choices)) => {
                            self.ui.set_menu_open(false);
                            if let (Some(Some(f)), Some(ed)) = (choices.get(i).copied(), cx.editor.as_ref()) {
                                if let Some(p) = cx.screens.library.get_mut(ed.page).and_then(|pg| pg.pieces.get_mut(ed.chosen?)) {
                                    p.date_format = f;
                                    out.edited = true;
                                }
                            }
                        }
                        Some(Menu::Shows(choices)) => {
                            self.ui.set_menu_open(false);
                            if choices.get(i).copied().flatten() == Some(screen_edit::Shows::MoreSensors) {
                                out.window.push(WindowAsk::ShowSettings);
                            } else if let (Some(Some(s)), Some(ed)) = (choices.get(i).copied(), cx.editor.as_ref()) {
                                if let Some(p) = cx.screens.library.get_mut(ed.page) {
                                    screen_edit::set_shows(p, ed, s);
                                    out.edited = true;
                                }
                            }
                        }
                        None => {}
                    }
                }
                Ask::MenuDismiss => self.close_menu(),
                Ask::PkSv(x, y) => {
                    if let Some(p) = &self.pick {
                        let h = p.hsv[0];
                        out.edited |= self.pick_set(cx, [h, x, 1.0 - y], None, None);
                    }
                }
                Ask::PkHue(x) => {
                    if let Some(p) = &self.pick {
                        let [_, sv, v] = p.hsv;
                        out.edited |= self.pick_set(cx, [(x * 360.0).min(359.9), sv, v], None, None);
                    }
                }
                Ask::PkField(i, t) => out.edited |= self.pick_field(cx, i, &t),
                Ask::PkWell(c) => {
                    let rgb = [c.red(), c.green(), c.blue()];
                    out.edited |= self.pick_set(cx, rgb2hsv(rgb), Some(rgb), None);
                }
                Ask::PkEnter | Ask::PkDismiss => self.close_picker(),
            }
        }
        out
    }

    /// A press anywhere in the window: away from an open menu or the
    /// picker, it closes them (and a press on their own button doesn't open them again).
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        if self.menu.is_some() {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                self.shut = Some(((x, y), Instant::now()));
                self.close_menu();
            }
        }
        if self.pick.is_some() {
            let (px, py, ph) = (self.ui.get_picker_x().max(8.0).min(self.ui.get_window_width() - 272.0), self.ui.get_picker_top(), self.ui.get_picker_height());
            if !(x >= px && x < px + 264.0 && y >= py && y < py + ph) {
                self.shut = Some(((x, y), Instant::now()));
                self.close_picker();
            }
        }
    }

    /// Whether a press a moment ago closed a menu at this button (so its click opens nothing).
    fn just_shut(&mut self, anchor: (f32, f32, f32)) -> bool {
        let Some((at, when)) = self.shut.take() else { return false };
        when.elapsed() < Duration::from_millis(800) && at.0 >= anchor.0 - 40.0 && at.0 <= anchor.0 + 400.0 && at.1 >= anchor.1 && at.1 <= anchor.1 + anchor.2
    }

    /// Renaming, asking, a menu or the picker: each let go, as another layout opens.
    fn settle(&mut self) {
        self.close_menu();
        self.close_picker();
        self.renaming = false;
        self.confirm = false;
    }

    /// Where a new block of `kind` carried to `x`, `y` in the window would land: in the
    /// free cells under the pointer (see [screen_edit::fit]); None away from the screen and
    /// over other blocks.
    fn landing_at(&self, cx: &Cx, kind: usize, x: f32, y: f32) -> Option<(u8, u8, u8, u8)> {
        let (sx, sy, cw, ch) = (self.ui.get_cells_x(), self.ui.get_cells_y(), self.ui.get_cell_w(), self.ui.get_cell_h());
        if x < sx || y < sy || x >= sx + cw * 20.0 || y >= sy + ch * 4.0 {
            return None;
        }
        let page = LayoutPage::page(cx)?;
        let first = cx.avail.first().copied().unwrap_or(Source::CpuTemp);
        let (k, width, right) = screen_edit::new_block(kind, first);
        let least = crate::lcd_page::least(&crate::lcd_page::Piece { kind: k, row: 0, col: 0, width, right, center: false, gap: 1, height: 1, scroll: true, date_format: Default::default() }, cx.readings).max(screen_edit::NEW_LEAST).min(width);
        let (row, col) = (((y - sy) / ch).floor().clamp(0.0, 3.0) as u8, ((x - sx) / cw).floor().clamp(0.0, 19.0) as u8);
        // (an effect: the whole screen, or what other effects leave round the cell)
        if kind == 3 {
            return screen_edit::effect_spot(page, Some((row, col)));
        }
        screen_edit::fit(page, row, col, width, 1, least, None).map(|(r, c, w)| (r, c, w, 1))
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.ui.set_menu_open(false);
        }
    }

    fn place_menu(&mut self, rows: Vec<MenuRow>, w: f32, anchor: (f32, f32, f32)) {
        let h = crate::menu_size::height(&rows, false, false, f32::MAX);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        // (a column for ticks only when something in the menu is ticked)
        self.ui.set_menu_marks(rows.iter().any(|r| r.tick));
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        let room = if down { below } else { above };
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
        self.ui.set_menu_open(true);
    }

    /// "···" in the header: Duplicate the layout, or Delete it (which asks first).
    fn open_more(&mut self, anchor: (f32, f32, f32), cx: &Cx) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(p) = LayoutPage::page(cx) else { return };
        self.close_picker();
        let rows = vec![row(0, &format!("Duplicate {}", p.name), false, ""), row(4, &format!("Delete {}…", p.name), false, "")];
        let w = rows.iter().map(|r| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::TEXT, &r.label) + 64.0).fold(200.0f32, f32::max).min(320.0).ceil();
        // (its right edge at the button's)
        self.place_menu(rows, w, (anchor.0 + 30.0 - w, anchor.1, anchor.2));
        self.menu = Some(Menu::More);
    }

    /// + New: From scratch, a copy of a layout, or a ready-made one put back, its right edge
    /// at the button's (`bw` wide).
    fn open_new(&mut self, anchor: (f32, f32, f32), bw: f32, cx: &Cx) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_picker();
        let (rows, made) = new_rows(&cx.screens.library);
        let w = rows.iter().map(|r| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::TEXT, &r.label) + 64.0).fold(200.0f32, f32::max).min(320.0).ceil();
        self.place_menu(rows, w, (anchor.0 + bw - w, anchor.1, anchor.2));
        self.menu = Some(Menu::New(made));
    }

    /// Format, in the box of a Data block showing the date: each format as the date reads today.
    fn open_formats(&mut self, cx: &Cx, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(piece) = cx.editor.as_ref().and_then(|e| cx.screens.library.get(e.page)?.pieces.get(e.chosen?)) else { return };
        self.close_picker();
        let listed = screen_edit::format_menu(piece.date_format, cx.readings);
        let rows: Vec<MenuRow> = listed.iter().map(|(k, label, sub, tick, _)| row(*k, label, *tick, sub)).collect();
        let choices = listed.into_iter().map(|r| r.4).collect();
        self.place_menu(rows, 250.0, anchor);
        self.menu = Some(Menu::Formats(choices));
    }

    /// Shows, in the box of a Data block or a bar: what it can show, each with what it reads now.
    fn open_shows(&mut self, cx: &Cx, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(piece) = cx.editor.as_ref().and_then(|e| cx.screens.library.get(e.page)?.pieces.get(e.chosen?)) else { return };
        let Some(current) = screen_edit::shows_of(piece) else { return };
        let bars = matches!(piece.kind, crate::lcd_page::Kind::Bar(_));
        self.close_picker();
        let listed = screen_edit::source_menu(current, bars, piece.date_format, &cx.groups, cx.avail, cx.readings);
        let rows: Vec<MenuRow> = listed.iter().map(|(k, label, sub, tick, _)| row(*k, label, *tick, sub)).collect();
        let choices = listed.into_iter().map(|r| r.4).collect();
        self.place_menu(rows, 330.0, anchor);
        self.menu = Some(Menu::Shows(choices));
    }

    // ------------------------------------------------------------ the layout's own color

    fn open_picker(&mut self, cx: &Cx, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(rgb) = LayoutPage::page(cx).and_then(|p| p.light) else { return };
        self.close_menu();
        self.pick = Some(Pick { start: rgb, hsv: rgb2hsv(rgb), rgb });
        self.ui.set_pk_recent(ModelRc::new(VecModel::from(recent_colors().iter().map(|c| color(*c)).collect::<Vec<_>>())));
        let (ax, ay, ah) = anchor;
        self.ui.set_picker_x(ax);
        self.ui.set_picker_top(ay + ah + 6.0);
        let scale = self.scale();
        self.ui.set_pk_hue_img(hue_bar(scale));
        self.show_pick(None);
        self.ui.set_picker_open(true);
    }

    fn close_picker(&mut self) {
        if let Some(p) = self.pick.take() {
            self.ui.set_picker_open(false);
            if p.rgb != p.start {
                keep_recent(p.rgb);
            }
        }
    }

    fn scale(&self) -> f32 {
        self.ui.window().scale_factor()
    }

    fn show_pick(&self, skip: Option<i32>) {
        let Some(p) = &self.pick else { return };
        let [h, sv, v] = p.hsv;
        let rgb = p.rgb;
        self.ui.set_pk_hue(h);
        self.ui.set_pk_sat(sv);
        self.ui.set_pk_val(v);
        self.ui.set_pk_now(color(rgb));
        self.ui.set_pk_pure(color(hsv2rgb([h, 1.0, 1.0])));
        self.ui.set_pk_sv_img(sv_square(h, self.scale()));
        let fields = [
            crate::lighting::hex_of(rgb),
            rgb[0].to_string(),
            rgb[1].to_string(),
            rgb[2].to_string(),
            (h.round() as i32).to_string(),
            ((sv * 100.0).round() as i32).to_string(),
            ((v * 100.0).round() as i32).to_string(),
        ];
        let setters: [fn(&LayoutWindow, SharedString); 7] = [
            LayoutWindow::set_pk_hex,
            LayoutWindow::set_pk_r,
            LayoutWindow::set_pk_g,
            LayoutWindow::set_pk_b,
            LayoutWindow::set_pk_h,
            LayoutWindow::set_pk_s,
            LayoutWindow::set_pk_v,
        ];
        for (i, (set, t)) in setters.iter().zip(fields).enumerate() {
            if skip != Some(i as i32) {
                set(&self.ui, t.into());
            }
        }
    }

    /// A new color in the picker: shown, and the layout's own color. Whether the layout changed.
    fn pick_set(&mut self, cx: &mut Cx, hsv: [f32; 3], rgb: Option<[u8; 3]>, skip: Option<i32>) -> bool {
        let Some(p) = &mut self.pick else { return false };
        p.hsv = hsv;
        p.rgb = rgb.unwrap_or_else(|| hsv2rgb(hsv));
        let rgb = p.rgb;
        self.show_pick(skip);
        match cx.editor.as_ref().and_then(|e| cx.screens.library.get_mut(e.page)) {
            Some(page) if page.light.is_some() => {
                page.light = Some(rgb);
                true
            }
            _ => false,
        }
    }

    /// A field of the picker typed in: a hex code, or one of its numbers.
    fn pick_field(&mut self, cx: &mut Cx, i: i32, t: &str) -> bool {
        let Some(p) = &self.pick else { return false };
        let (rgb, hsv) = (p.rgb, p.hsv);
        let v = t.trim().parse::<f32>().ok();
        match i {
            0 => {
                let t = t.trim().trim_start_matches('#');
                let full: String = if t.len() == 3 { t.chars().flat_map(|c| [c, c]).collect() } else { t.to_string() };
                match (full.len() == 6).then(|| u32::from_str_radix(&full, 16).ok()).flatten() {
                    Some(n) => {
                        let c = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
                        self.pick_set(cx, rgb2hsv(c), Some(c), Some(0))
                    }
                    None => false,
                }
            }
            1..=3 => match v {
                Some(v) => {
                    let mut c = rgb;
                    c[(i - 1) as usize] = v.round().clamp(0.0, 255.0) as u8;
                    self.pick_set(cx, rgb2hsv(c), Some(c), Some(i))
                }
                None => false,
            },
            4..=6 => match v {
                Some(v) => {
                    let mut n = hsv;
                    match i {
                        4 => n[0] = v.clamp(0.0, 359.0),
                        5 => n[1] = v.clamp(0.0, 100.0) / 100.0,
                        _ => n[2] = v.clamp(0.0, 100.0) / 100.0,
                    }
                    self.pick_set(cx, n, None, Some(i))
                }
                None => false,
            },
            _ => false,
        }
    }

    // ------------------------------------------------------------ what the window shows

    /// The window as the layouts are now: the list, the layout open, its
    /// picture (with a block carried from the bar where it would land), its
    /// blocks, its backlight and grid, and the box of the block selected.
    pub fn show(&mut self, cx: &Cx) {
        let ed = cx.editor.clone();
        let rows: Vec<LayoutRow> = cx.screens.library.iter().enumerate().map(|(k, p)| LayoutRow { name: p.name.as_str().into(), about: p.about().into(), on: ed.as_ref().is_some_and(|e| e.page == k) }).collect();
        crate::front::set_rows(&self.rows, rows);
        let page = ed.as_ref().and_then(|e| cx.screens.library.get(e.page));
        self.ui.set_editing_name(self.renaming);
        self.ui.set_confirm_open(self.confirm);
        let default = [cx.light[1], cx.light[2], cx.light[3]];
        self.ui.set_default_light(color(default));
        self.ui.set_has_layout(page.is_some());
        let Some(page) = page else {
            self.close_menu();
            self.close_picker();
            self.carry = None;
            self.ui.set_carry_on(false);
            crate::front::set_rows(&self.blocks, Vec::new());
            crate::front::set_rows(&self.cells, Vec::new());
            return;
        };
        let ed = ed.unwrap();
        self.ui.set_name(page.name.as_str().into());
        let says = cx.screens.delete_says(ed.page, cx.onboard, |n| cx.profiles[n as usize - 1].clone());
        self.ui.set_confirm_title(format!("Delete {}?", page.name).into());
        self.ui.set_confirm_body(says.into());
        self.ui.set_grid(ed.grid);
        self.ui.set_own_light(page.light.is_some());
        let own = page.light.unwrap_or(default);
        self.ui.set_own_color(color(own));
        self.ui.set_own_hex(crate::lighting::hex_of(own).into());
        self.ui.set_picking(self.pick.is_some());
        // a block carried from the bar is drawn on the screen where it would land, with
        // what it would say there
        let mut shown = page.clone();
        if let Some((kind, Some((r, c, width, height)))) = self.carry {
            let first = cx.avail.first().copied().unwrap_or(Source::CpuTemp);
            let (k, _, right) = screen_edit::new_block(kind, first);
            shown.pieces.push(crate::lcd_page::Piece { kind: k, row: r, col: c, width, right, center: false, gap: 1, height, scroll: true, date_format: Default::default() });
        }
        // each block outlined where it lands (a reading that runs long pushes the rest of
        // its row along), with how far its corner can reach at each height it can have
        let spans = crate::lcd_page::spans(&shown, cx.readings);
        let outline = |(row, start, len): (u8, usize, usize)| {
            let col = start.min(COLS as usize - 1);
            (row, col, len.clamp(1, COLS as usize - col))
        };
        let blocks: Vec<EdBlock> = page
            .pieces
            .iter()
            .zip(spans.iter().copied())
            .enumerate()
            .map(|(i, (p, sp))| {
                let (row, col, width) = outline(sp);
                let least = |h| crate::lcd_page::least_at(p, cx.readings, h);
                // for each height it can have: how far its corner reaches, and how near it may
                // pull the block in, on one row never past where the block lands (a reading
                // that runs long), on more rows as narrow as the block can be there
                let reach: Vec<u8> = screen_edit::room(page, i, least).iter().map_while(|e| *e).collect();
                let ends: Vec<i32> = reach.iter().enumerate().map(|(k, e)| if k == 0 { (*e as i32).max((col + width) as i32) } else { *e as i32 }).collect();
                let lows: Vec<i32> = (1..=reach.len() as u8).map(|h| (col + if h == 1 { (screen_edit::floor(p, 1, least) as usize).min(width) } else { screen_edit::floor(p, h, least) as usize }) as i32).collect();
                EdBlock {
                    row: row as i32,
                    col: col as i32,
                    width: width as i32,
                    height: p.height.max(1) as i32,
                    on: ed.chosen == Some(i),
                    fx: screen_edit::behind(p),
                    // (taller or shorter from its corner, where there is room for that)
                    tall: reach.len() > 1 || p.height > 1,
                    lows: ModelRc::new(VecModel::from(lows)),
                    ends: ModelRc::new(VecModel::from(ends)),
                }
            })
            .collect();
        // (the selected block's corner and × are drawn over every block, from its place here)
        self.ui.set_sel_block(blocks.iter().position(|b| b.on).map_or(-1, |i| i as i32));
        crate::front::set_rows(&self.blocks, blocks);
        // the cells the carried block would take
        match self.carry {
            Some((kind, Some((_, _, _, height)))) => {
                let (row, col, width) = outline(spans.last().copied().unwrap_or_default());
                self.ui.set_ghost_on(true);
                self.ui.set_ghost(EdBlock { row: row as i32, col: col as i32, width: width as i32, height: height as i32, on: false, fx: kind == 3, tall: false, lows: ModelRc::default(), ends: ModelRc::default() });
                self.ui.set_lit_chip(kind as i32);
            }
            Some((kind, None)) => {
                self.ui.set_ghost_on(false);
                self.ui.set_lit_chip(kind as i32);
            }
            None => {
                self.ui.set_ghost_on(false);
                self.ui.set_lit_chip(-1);
            }
        }
        let rows = crate::lcd_page::render_at(&shown, cx.readings, cx.t);
        self.draw(&rows, page.light.unwrap_or(default), cx.light[0]);
        // the box of the block selected
        let rig_w = self.ui.get_window_width() - crate::tokens::size::SIDEBAR - 2.0 * crate::tokens::space::MARGIN;
        let open = match self.menu {
            Some(Menu::Shows(_)) => Some(Ctl::ScrSource),
            Some(Menu::Formats(_)) => Some(Ctl::ScrFormat),
            _ => None,
        };
        let b = screen_edit::block_box(page, ed.chosen, cx.readings, cx.avail, open, &cx.from, (rig_w - crate::tokens::space::GAP) / 2.0);
        self.ui.set_box_kind(b.kind);
        self.ui.set_box_title(b.title.into());
        self.ui.set_box_h(b.h);
        self.ui.set_has_blocks(!page.pieces.is_empty());
        // (room for the least a new block starts in)
        self.ui.set_bar_hint(if screen_edit::has_room(page, screen_edit::NEW_LEAST) { "Drag a block onto the screen." } else { "No room for another block. Narrow a block or remove one." }.into());
        crate::front::set_rows(&self.cells, b.cells);
        // an effect selected: the effects' tiles, its own lit and moving
        let chosen = ed.chosen.and_then(|i| page.pieces.get(i));
        if let Some(crate::lcd_page::Kind::Effect { fx, speed, word }) = chosen.map(|p| &p.kind) {
            let tile_w = (((rig_w - 12.0) / 2.0 - 32.0 - 24.0) / 4.0 - 12.0).max(20.0);
            let sc = self.scale().max(1.0);
            let (w, h) = ((tile_w * sc).round() as u32, (tile_w / 5.0 * sc).round() as u32);
            let rgb = crate::z10::as_lit(page.light.unwrap_or(default));
            let lv = cx.light[0] as f32 / 255.0;
            let word = evga_z10::lcd::lcd_text(word);
            let rows_of = |f: [[char; 20]; 4]| -> Vec<String> { f.iter().map(|r| r.iter().collect()).collect() };
            let key = (rgb, cx.light[0], w, word.clone());
            if self.stills.as_ref().is_none_or(|(k, _)| *k != key) {
                // (each as the mock's tiles stand still: four seconds in)
                let sample = crate::lcd_fx::Live::sample();
                let pics = crate::lcd_fx::FXS.iter().map(|f| crate::lcd_draw::picture(&rows_of(crate::lcd_fx::frame(*f, crate::lcd_fx::still(*f), &word, &sample)), rgb, lv, w, h)).collect();
                self.stills = Some((key, pics));
            }
            let stills = &self.stills.as_ref().unwrap().1;
            let live = crate::lcd_fx::live(*fx, *speed, cx.readings.cpu_load.unwrap_or(0.0));
            let moving = crate::lcd_draw::picture(&rows_of(crate::lcd_fx::frame(*fx, crate::lcd_fx::tenths(*fx, cx.t, *speed), &word, &live)), rgb, lv, w, h);
            let tiles: Vec<crate::FxTile> = crate::lcd_fx::FXS
                .iter()
                .enumerate()
                .map(|(k, f)| crate::FxTile { name: f.name().into(), pic: if f == fx { moving.clone() } else { stills[k].clone() }, on: f == fx })
                .collect();
            crate::front::set_rows(&self.tiles, tiles);
        }
    }

    /// The screen's picture, as the keyboard shows it, drawn at the window's own pixels.
    fn draw(&mut self, rows: &[String], light: [u8; 3], level: u8) {
        let sc = self.scale().max(1.0);
        let (w, h) = ((780.0 * sc).round() as u32, (156.0 * sc).round() as u32);
        let rgb = crate::z10::as_lit(light);
        let key = (rows.to_vec(), rgb, level, w);
        if self.drawn.as_ref() == Some(&key) {
            return;
        }
        let lv = level as f32 / 255.0;
        self.ui.set_picture(crate::lcd_draw::picture(rows, rgb, lv, w, h));
        self.ui.set_field(crate::lcd_draw::field(rgb, lv, w));
        self.drawn = Some(key);
    }

    /// The window's layouts list scrolled to the one open (after a new one is made).
    pub fn reveal(&self, i: usize) {
        let h = self.ui.get_window_height() - 36.0 - 52.0 - 10.0;
        let pitch = crate::tokens::size::ROW + crate::tokens::space::LIST;
        let y = i as f32 * pitch;
        let at = -self.ui.get_list_scroll();
        if y < at || y + pitch > at + h {
            self.ui.set_list_scroll(-(y + pitch - h).max(0.0));
        }
    }
}
